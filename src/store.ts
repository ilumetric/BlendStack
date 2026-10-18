import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { api, errorText } from "./lib/api";
import { buildName } from "./lib/naming";
import { resolveLanguage, translate } from "./i18n/core";
import type {
  AppNotification,
  BuildList,
  DownloadTask,
  InstalledBuild,
  NewsList,
  RecentFile,
  RemoteBuild,
  RemoteChannel,
  Settings,
  SystemInfo,
} from "./lib/types";

export type Tab = "library" | "builds" | "news" | "chat";
export type ToastTone = "info" | "success" | "error";

export interface Toast {
  id: number;
  text: string;
  tone: ToastTone;
  action?: { label: string; run: () => void };
}

interface Remote {
  list: BuildList | null;
  loading: boolean;
}

type SeenKey = RemoteChannel | "news";

interface Store {
  booted: boolean;
  system: SystemInfo | null;
  settings: Settings | null;
  lang: string;
  installed: InstalledBuild[];
  remote: Record<RemoteChannel, Remote>;
  news: { list: NewsList | null; loading: boolean };
  downloads: Record<string, DownloadTask>;
  notifications: AppNotification[];
  recent: RecentFile[] | null;
  thumbs: Record<string, string | null>;
  tab: Tab;
  buildsChannel: RemoteChannel;
  settingsOpen: boolean;
  openFiles: string[];
  toasts: Toast[];
  seen: Record<SeenKey, number>;
  /** Value of `seen` before the current visit; newer items get a "New" badge. */
  prevSeen: Record<SeenKey, number>;
  focusBuild: string | null;

  boot(): Promise<void>;
  setTab(tab: Tab): void;
  setBuildsChannel(channel: RemoteChannel): void;
  setSettingsOpen(open: boolean): void;
  refreshInstalled(): Promise<void>;
  loadBuilds(channel: RemoteChannel, force?: boolean): Promise<void>;
  loadNews(force?: boolean): Promise<void>;
  loadRecent(): Promise<void>;
  loadThumb(path: string): void;
  patchSettings(patch: Partial<Settings>): Promise<void>;
  download(build: RemoteBuild, replaces?: string | null): Promise<void>;
  cancelDownload(id: string): Promise<void>;
  launch(build: InstalledBuild, file?: string | null): Promise<void>;
  removeBuild(build: InstalledBuild): Promise<void>;
  importBuild(path: string): Promise<void>;
  queueFiles(paths: string[]): void;
  toast(text: string, tone?: ToastTone, action?: Toast["action"]): void;
  dismissToast(id: number): void;
  markSeen(key: SeenKey): void;
  showBuild(channel: RemoteChannel, buildId: string | null): void;
}

const SEEN_KEY = "blendstack.seen";

function loadSeen(): Record<SeenKey, number> {
  const now = Date.now() / 1000;
  const fresh = { stable: now, daily: now, experimental: now, news: now };
  try {
    const raw = localStorage.getItem(SEEN_KEY);
    if (raw) return { ...fresh, ...JSON.parse(raw) };
    localStorage.setItem(SEEN_KEY, JSON.stringify(fresh));
  } catch {
    // Storage unavailable: badges simply start empty.
  }
  return fresh;
}

let toastId = 0;
let booting = false;
const thumbRequests = new Set<string>();

export const useStore = create<Store>((set, get) => {
  const tr = (key: Parameters<typeof translate>[1], vars?: Record<string, string | number>) =>
    translate(get().lang, key, vars);
  const nameOf = (b: InstalledBuild | RemoteBuild) => buildName(b, tr, get().settings?.labels);
  const fail = (e: unknown) => get().toast(errorText(e), "error");

  function upsertTask(task: DownloadTask) {
    const previous = get().downloads[task.id];
    set((s) => ({ downloads: { ...s.downloads, [task.id]: task } }));
    if (previous?.state === task.state) return;
    if (task.state === "done") get().toast(tr("toast.installed", { name: nameOf(task.build) }), "success");
    if (task.state === "failed")
      get().toast(`${tr("toast.failed", { name: nameOf(task.build) })}: ${task.error ?? ""}`, "error");
  }

  return {
    booted: false,
    system: null,
    settings: null,
    lang: resolveLanguage(undefined),
    installed: [],
    remote: {
      stable: { list: null, loading: false },
      daily: { list: null, loading: false },
      experimental: { list: null, loading: false },
    },
    news: { list: null, loading: false },
    downloads: {},
    notifications: [],
    recent: null,
    thumbs: {},
    tab: "library",
    buildsChannel: "stable",
    settingsOpen: false,
    openFiles: [],
    toasts: [],
    seen: loadSeen(),
    prevSeen: loadSeen(),
    focusBuild: null,

    async boot() {
      // React StrictMode mounts twice in development; listeners must register once.
      if (booting) return;
      booting = true;
      const boot = await api.bootstrap();
      set({
        booted: true,
        system: boot.system,
        settings: boot.settings,
        lang: resolveLanguage(boot.settings.language),
        installed: boot.installed,
        downloads: Object.fromEntries(boot.downloads.map((d) => [d.id, d])),
        notifications: boot.notifications,
      });

      // Instant first paint from disk caches, then refresh in the background.
      const channels: RemoteChannel[] = ["stable", "daily", "experimental"];
      await Promise.all(
        channels.map(async (c) => {
          const cached = await api.cachedBuilds(c);
          if (cached.fetchedAt) set((s) => ({ remote: { ...s.remote, [c]: { ...s.remote[c], list: cached } } }));
        }),
      );
      const news = await api.cachedNews();
      if (news.fetchedAt) set((s) => ({ news: { ...s.news, list: news } }));

      void listen<DownloadTask>("download:progress", (e) => upsertTask(e.payload));
      void listen("library:changed", () => void get().refreshInstalled());
      void listen<Settings>("settings:changed", (e) =>
        set({ settings: e.payload, lang: resolveLanguage(e.payload.language) }),
      );
      void listen("notifications:changed", async () => set({ notifications: await api.notifications() }));
      void listen<RemoteChannel>("builds:updated", async (e) => {
        const list = await api.cachedBuilds(e.payload);
        set((s) => ({ remote: { ...s.remote, [e.payload]: { ...s.remote[e.payload], list } } }));
      });
      void listen("files:opened", async () => get().queueFiles(await api.takePendingFiles()));

      get().queueFiles(await api.takePendingFiles());
      void get().loadBuilds("stable");
      void get().loadBuilds("daily");
      void get().loadRecent();
    },

    setTab(tab) {
      set({ tab });
      if (tab === "news") {
        void get().loadNews();
        get().markSeen("news");
      }
      if (tab === "builds") get().markSeen(get().buildsChannel);
      if (tab === "library") void get().loadRecent();
    },

    setBuildsChannel(channel) {
      set({ buildsChannel: channel });
      get().markSeen(channel);
      if (!get().remote[channel].list || get().remote[channel].list?.error) void get().loadBuilds(channel);
    },

    setSettingsOpen(open) {
      set({ settingsOpen: open });
    },

    async refreshInstalled() {
      try {
        set({ installed: await api.installed() });
      } catch (e) {
        fail(e);
      }
    },

    async loadBuilds(channel, force = false) {
      set((s) => ({ remote: { ...s.remote, [channel]: { ...s.remote[channel], loading: true } } }));
      try {
        const list = await api.builds(channel, force);
        set((s) => ({ remote: { ...s.remote, [channel]: { list, loading: false } } }));
      } catch (e) {
        set((s) => ({ remote: { ...s.remote, [channel]: { ...s.remote[channel], loading: false } } }));
        fail(e);
      }
    },

    async loadNews(force = false) {
      set((s) => ({ news: { ...s.news, loading: true } }));
      try {
        const list = await api.news(force);
        set({ news: { list, loading: false } });
      } catch (e) {
        set((s) => ({ news: { ...s.news, loading: false } }));
        fail(e);
      }
    },

    async loadRecent() {
      try {
        set({ recent: await api.recentFiles() });
      } catch {
        set({ recent: [] });
      }
    },

    loadThumb(path) {
      if (thumbRequests.has(path)) return;
      thumbRequests.add(path);
      api
        .thumbnail(path)
        .then((data) => set((s) => ({ thumbs: { ...s.thumbs, [path]: data } })))
        .catch(() => set((s) => ({ thumbs: { ...s.thumbs, [path]: null } })));
    },

    async patchSettings(patch) {
      const before = get().settings;
      if (before) set({ settings: { ...before, ...patch } });
      if (patch.language !== undefined) set({ lang: resolveLanguage(patch.language) });
      try {
        const saved = await api.updateSettings(patch);
        set({ settings: saved });
        if (patch.theme !== undefined || patch.translucency !== undefined || patch.libraryPath !== undefined) {
          set({ system: await api.systemInfo() });
        }
      } catch (e) {
        if (before) set({ settings: before });
        fail(e);
      }
    },

    async download(build, replaces = null) {
      try {
        upsertTask(await api.startDownload(build, replaces));
      } catch (e) {
        fail(e);
      }
    },

    async cancelDownload(id) {
      await api.cancelDownload(id).catch(fail);
    },

    async launch(build, file = null) {
      try {
        await api.launch(build.id, file);
        get().toast(tr("toast.launching", { name: nameOf(build) }), "info");
      } catch (e) {
        fail(e);
      }
    },

    async removeBuild(build) {
      try {
        await api.deleteBuild(build.id);
        get().toast(tr("toast.deleted", { name: nameOf(build) }), "info");
      } catch (e) {
        fail(e);
      }
    },

    async importBuild(path) {
      try {
        const b = await api.importBuild(path);
        get().toast(tr("toast.imported", { name: nameOf(b) }), "success");
      } catch (e) {
        fail(e);
      }
    },

    queueFiles(paths) {
      if (paths.length) set((s) => ({ openFiles: [...s.openFiles, ...paths] }));
    },

    toast(text, tone = "info", action) {
      const id = ++toastId;
      set((s) => ({ toasts: [...s.toasts.slice(-3), { id, text, tone, action }] }));
      window.setTimeout(() => get().dismissToast(id), tone === "error" ? 6500 : 3500);
    },

    dismissToast(id) {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    },

    markSeen(key) {
      const prevSeen = { ...get().prevSeen, [key]: get().seen[key] };
      const seen = { ...get().seen, [key]: Date.now() / 1000 };
      set({ seen, prevSeen });
      try {
        localStorage.setItem(SEEN_KEY, JSON.stringify(seen));
      } catch {
        // ignore
      }
    },

    showBuild(channel, buildId) {
      set({ tab: "builds", buildsChannel: channel, focusBuild: buildId });
      get().markSeen(channel);
      void get().loadBuilds(channel);
    },
  };
});

// The store owns the IPC listeners; hot-swapping it would leave the UI un-booted.
if (import.meta.hot) import.meta.hot.accept(() => window.location.reload());
