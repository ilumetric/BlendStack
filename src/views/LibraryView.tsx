import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ChevronDown,
  Download,
  FileBox,
  FolderOpen,
  FolderPlus,
  MoreHorizontal,
  PanelLeftClose,
  PanelLeftOpen,
  Play,
  Search,
  Star,
} from "lucide-react";
import { useStore } from "../store";
import { useT, type Translate } from "../i18n";
import { api } from "../lib/api";
import { bestForSeries, findUpdate, resolveDefault, sortInstalled, tone } from "../lib/builds";
import { relativeTime } from "../lib/format";
import { buildName, cycleLabel } from "../lib/naming";
import type { Channel, FileSection, InstalledBuild, RecentFile, RemoteBuild, Settings } from "../lib/types";
import { Segmented } from "../components/Segmented";
import { Glyph } from "../components/bits";
import { openContextMenu, openMenuAt, type MenuEntry } from "../components/Menu";
import { installedMenu, pickBlendFile, revealLabel } from "./buildMenu";
import logo from "../assets/logo.svg";

const CHANNEL_ORDER: Channel[] = ["stable", "daily", "experimental", "custom"];
const RAIL_KEY = "blendstack.libraryCollapsed";

/** Collapsed sidebar: the default build first, every installed build after it.
    A vertical rail beside the files; a horizontal strip above them in a narrow window. */
function Rail({ builds, settings }: { builds: InstalledBuild[]; settings: Settings | null }) {
  const tr = useT();
  const launch = useStore((s) => s.launch);
  const setTab = useStore((s) => s.setTab);
  const updates = useUpdates(builds);
  const current = resolveDefault(builds, settings);
  if (!current) {
    return (
      <div className="rail">
        <button className="rail-item" title={tr("library.empty.browse")} onClick={() => setTab("builds")}>
          <Download size={18} />
        </button>
      </div>
    );
  }
  const others = sortInstalled(builds).filter((b) => b.id !== current.id);
  const item = (b: InstalledBuild, main = false) => (
    <button
      key={b.id}
      className={`rail-item${main ? " main" : ""}`}
      title={`${tr("common.launch")} ${buildName(b, tr, settings?.labels)}`}
      onClick={() => launch(b)}
      onContextMenu={(e) => openContextMenu(e, installedMenu(b, updates[b.id] ?? null))}
    >
      <Glyph build={b} size={main ? undefined : "sm"} />
      <span className="rail-play">
        <Play size={main ? 14 : 11} fill="currentColor" />
      </span>
    </button>
  );
  return (
    <div className="rail">
      {item(current, true)}
      {others.length > 0 && <div className="rail-sep" />}
      {others.map((b) => item(b))}
    </div>
  );
}

function useUpdates(installed: InstalledBuild[]) {
  const remote = useStore((s) => s.remote);
  return useMemo(() => {
    const lists: Partial<Record<Channel, RemoteBuild[]>> = {
      stable: remote.stable.list?.builds,
      daily: remote.daily.list?.builds,
      experimental: remote.experimental.list?.builds,
    };
    const map: Record<string, RemoteBuild | null> = {};
    for (const b of installed) map[b.id] = findUpdate(b, lists, installed);
    return map;
  }, [remote, installed]);
}

function subtitle(b: InstalledBuild, tr: Translate, lang: string): string[] {
  const parts: string[] = [];
  if (b.channel === "stable") parts.push(tr("channel.stable"));
  else if (b.channel === "custom") parts.push(tr("channel.custom"));
  else parts.push(b.version);
  if (b.channel === "daily" && b.branch) parts.push(b.branch);
  if (b.hash && b.channel !== "stable") parts.push(b.hash.slice(0, 7));
  if (b.date && b.channel !== "stable") parts.push(tr("library.built", { time: relativeTime(b.date, lang) }));
  else if (b.installedAt && b.channel !== "custom") parts.push(tr("library.installedAgo", { time: relativeTime(b.installedAt, lang) }));
  return parts;
}

function Hero({ builds, settings }: { builds: InstalledBuild[]; settings: Settings | null }) {
  const tr = useT();
  const launch = useStore((s) => s.launch);
  const patchSettings = useStore((s) => s.patchSettings);
  const updates = useUpdates(builds);
  const current = resolveDefault(builds, settings);
  if (!current) return null;

  const choose = (el: HTMLElement) => {
    const items: MenuEntry[] = [{ heading: tr("library.chooseBuild") }];
    for (const b of sortInstalled(builds)) {
      items.push({
        label: buildName(b, tr, settings?.labels),
        hint: b.channel === "stable" ? undefined : cycleLabel(b.releaseCycle, tr, true),
        checked: b.id === current.id,
        onSelect: () => patchSettings({ defaultBuild: b.id }),
      });
    }
    openMenuAt(el, items, true);
  };

  return (
    <div
      className={`card hero tone-${tone(current)}`}
      onContextMenu={(e) => openContextMenu(e, installedMenu(current, updates[current.id] ?? null))}
    >
      <div className="hero-top">
        <Glyph build={current} size="xl" />
        <div style={{ minWidth: 0 }}>
          <div className="hero-caption">{tr("library.ready")}</div>
          <div className="hero-title">{buildName(current, tr, settings?.labels)}</div>
          <div className="hero-meta">
            <span className={`tag tone-${tone(current)}`}>
              {current.channel === "stable" ? tr("channel.stable") : current.channel === "custom" ? tr("channel.custom") : cycleLabel(current.releaseCycle, tr)}
            </span>
            {current.lts && <span className="tag tone-green">{tr("badge.lts")}</span>}
            {current.channel === "daily" && <span className="tag">{tr("channel.daily")}</span>}
          </div>
        </div>
      </div>
      <div className="launch-split">
        <button
          className="launch-main"
          title="Ctrl+Enter"
          onClick={() => void launch(current)}
        >
          <Play size={16} fill="currentColor" />
          {tr("library.launchDefault")}
        </button>
        <button className="launch-more" aria-label={tr("library.chooseBuild")} onClick={(e) => choose(e.currentTarget)}>
          <ChevronDown size={18} />
        </button>
      </div>
    </div>
  );
}

function EmptyHero() {
  const tr = useT();
  const setTab = useStore((s) => s.setTab);
  const importBuild = useStore((s) => s.importBuild);
  return (
    <div className="card empty-hero" style={{ marginRight: 8 }}>
      <img src={logo} width={64} height={64} alt="" />
      <h2>{tr("library.empty.title")}</h2>
      <p>{tr("library.empty.body")}</p>
      <div className="actions">
        <button className="btn primary lg" onClick={() => setTab("builds")}>
          {tr("library.empty.browse")}
        </button>
        <button
          className="btn plain"
          onClick={async () => {
            const dir = await open({ directory: true });
            if (typeof dir === "string") await importBuild(dir);
          }}
        >
          {tr("library.import")}
        </button>
      </div>
    </div>
  );
}

function InstalledRow({ build, update }: { build: InstalledBuild; update: RemoteBuild | null }) {
  const tr = useT();
  const lang = useStore((s) => s.lang);
  const settings = useStore((s) => s.settings);
  const launch = useStore((s) => s.launch);
  const download = useStore((s) => s.download);
  const updating = useStore((s) => (update ? s.downloads[update.id] : undefined));
  const favorite = settings?.favorites.includes(build.id);
  const isDefault = settings?.defaultBuild === build.id;
  const busy = updating && ["queued", "downloading", "verifying", "extracting"].includes(updating.state);

  return (
    <div
      className="row"
      onDoubleClick={() => launch(build)}
      onContextMenu={(e) => openContextMenu(e, installedMenu(build, update))}
    >
      <Glyph build={build} />
      <div className="row-main">
        <div className="row-title">
          <span className="name">{buildName(build, tr, settings?.labels)}</span>
          {favorite && <Star size={12} className="star" fill="currentColor" />}
          {build.lts && <span className="tag tone-green">{tr("badge.lts")}</span>}
          {isDefault && <span className="tag">{tr("badge.default")}</span>}
        </div>
        <div className="row-sub">
          {subtitle(build, tr, lang).map((p, i) => (
            <span key={i} className={i ? "sep" : undefined}>
              {i ? " " : ""}
              {p}
            </span>
          ))}
        </div>
      </div>
      <div className="row-actions">
        {update && !busy && (
          <button
            className="update-chip"
            title={tr("library.updateTo", { version: update.version })}
            onClick={() => download(update, build.id)}
          >
            {tr("common.update")}
          </button>
        )}
        {busy && <span className="task-status">{Math.round((updating.state === "downloading" && updating.total ? updating.received / updating.total : updating.progress) * 100)}%</span>}
        <button className="icon-btn hover-only" title={tr("common.launch")} onClick={() => launch(build)}>
          <Play size={15} fill="currentColor" />
        </button>
        <button
          className="icon-btn hover-only"
          aria-label="More"
          onClick={(e) => openMenuAt(e.currentTarget, installedMenu(build, update), true)}
        >
          <MoreHorizontal size={17} />
        </button>
      </div>
    </div>
  );
}

function RecentCard({
  file,
  builds,
  fallback,
}: {
  file: RecentFile;
  builds: InstalledBuild[];
  fallback: InstalledBuild | null;
}) {
  const tr = useT();
  const lang = useStore((s) => s.lang);
  const thumb = useStore((s) => s.thumbs[file.path]);
  const loadThumb = useStore((s) => s.loadThumb);
  const launch = useStore((s) => s.launch);
  const labels = useStore((s) => s.settings?.labels);
  const target = bestForSeries(builds, file.series, fallback);

  useEffect(() => {
    if (thumb === undefined) loadThumb(file.path);
  }, [file.path, thumb, loadThumb]);

  const menu = (e: React.MouseEvent) => {
    const items: MenuEntry[] = [{ heading: tr("menu.openWith") }];
    for (const b of sortInstalled(builds)) {
      items.push({
        label: buildName(b, tr, labels),
        checked: b.id === target?.id,
        onSelect: () => launch(b, file.path),
      });
    }
    items.push({ separator: true });
    items.push({ label: revealLabel(tr), icon: <FolderOpen size={14} />, onSelect: () => api.reveal(file.path) });
    openContextMenu(e, items);
  };

  const caption =
    file.kind === "session"
      ? tr("library.kind.session")
      : file.kind === "autosave"
        ? tr("library.kind.autosave")
        : file.kind === "backup"
          ? tr("library.kind.backup", { n: file.backup ?? 1 })
          : file.folder;
  const name = file.name || (file.kind === "session" ? tr("library.kind.session") : tr("library.untitled"));

  return (
    <button
      className="file-card"
      title={file.path}
      disabled={!target}
      onClick={() => target && launch(target, file.path)}
      onContextMenu={menu}
    >
      <div className="thumb">
        {thumb ? <img src={thumb} alt="" draggable={false} /> : <FileBox size={30} strokeWidth={1.4} />}
        {file.series && <span className="series">{file.series}</span>}
        {file.kind === "backup" && <span className="file-kind">.blend{file.backup}</span>}
      </div>
      <div className="file-name">{name}</div>
      <div className="file-meta">
        {relativeTime(file.modified, lang)} · {caption}
      </div>
    </button>
  );
}

export function LibraryView() {
  const tr = useT();
  const installed = useStore((s) => s.installed);
  const settings = useStore((s) => s.settings);
  const section = useStore((s) => s.fileSection);
  const setSection = useStore((s) => s.setFileSection);
  const files = useStore((s) => s.files[s.fileSection]);
  const launch = useStore((s) => s.launch);
  const importBuild = useStore((s) => s.importBuild);
  const [query, setQuery] = useState("");
  const updates = useUpdates(installed);
  const fallback = resolveDefault(installed, settings);
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return localStorage.getItem(RAIL_KEY) === "1";
    } catch {
      return false;
    }
  });
  const toggleRail = () => {
    const next = !collapsed;
    setCollapsed(next);
    try {
      localStorage.setItem(RAIL_KEY, next ? "1" : "0");
    } catch {
      // ignore
    }
  };

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const visible = installed.filter((b) => {
      if (!q) return true;
      const hay = `${buildName(b, tr, settings?.labels)} ${b.version} ${b.branch} ${b.hash ?? ""} ${b.patch ?? ""}`;
      return hay.toLowerCase().includes(q);
    });
    const favorites = new Set(settings?.favorites ?? []);
    const out: { key: string; title: string; builds: InstalledBuild[] }[] = [];
    const fav = visible.filter((b) => favorites.has(b.id));
    if (fav.length) out.push({ key: "fav", title: tr("library.favorites"), builds: sortInstalled(fav) });
    for (const c of CHANNEL_ORDER) {
      const list = visible.filter((b) => b.channel === c && !favorites.has(b.id));
      if (list.length) out.push({ key: c, title: tr(`channel.${c}`), builds: sortInstalled(list) });
    }
    return out;
  }, [installed, settings, query, tr]);

  const openFile = async () => {
    const file = await pickBlendFile();
    if (!file) return;
    useStore.getState().queueFiles([file]);
  };

  const addExisting = async () => {
    const dir = await open({ directory: true });
    if (typeof dir === "string") await importBuild(dir);
  };

  return (
    <div className={`view library${collapsed ? " collapsed" : ""}`}>
      <aside className="library-side">
        {collapsed && <Rail builds={installed} settings={settings} />}
        {!collapsed && (installed.length ? <Hero builds={installed} settings={settings} /> : <EmptyHero />)}

        {!collapsed && installed.length > 0 && (
          <>
            <div className="section-title">
              {tr("library.installed")}
              <span className="count">{installed.length}</span>
              <span className="spacer" />
              <button className="icon-btn" title={tr("library.import")} onClick={addExisting} style={{ width: 26, height: 24 }}>
                <FolderPlus size={15} />
              </button>
            </div>
            {installed.length > 6 && (
              <div className="search" style={{ margin: "0 8px 8px 0" }}>
                <Search size={13} />
                <input
                  className="input"
                  placeholder={tr("library.filter")}
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                />
              </div>
            )}
            <div className="installed-list">
              {groups.map((g) => (
                <div key={g.key} className="group">
                  {(groups.length > 1 || g.key === "fav") && (
                    <div className="section-title" style={{ marginBottom: 2, fontSize: 11, color: "var(--text-3)" }}>
                      {g.title}
                    </div>
                  )}
                  {g.builds.map((b) => (
                    <InstalledRow key={b.id} build={b} update={updates[b.id] ?? null} />
                  ))}
                </div>
              ))}
              {groups.length === 0 && <div className="placeholder">{tr("library.noMatch", { query })}</div>}
            </div>
          </>
        )}
      </aside>

      <main className="library-main">
        <div className="page-header">
          <button
            className="icon-btn sidebar-toggle"
            title={collapsed ? tr("library.expand") : tr("library.collapse")}
            onClick={toggleRail}
          >
            {collapsed ? <PanelLeftOpen size={17} /> : <PanelLeftClose size={17} />}
          </button>
          <Segmented<FileSection>
            value={section}
            onChange={setSection}
            segments={[
              { value: "recent", label: tr("library.section.recent") },
              { value: "recovery", label: tr("library.section.recovery") },
              { value: "backups", label: tr("library.section.backups") },
            ]}
          />
          <span className="spacer" />
          <button className="btn" onClick={openFile} disabled={!installed.length}>
            <FileBox size={14} />
            {tr("library.openFile")}
          </button>
        </div>
        {files && files.length === 0 && (
          <div className="placeholder">
            <FileBox size={36} strokeWidth={1.3} />
            {tr(section === "recovery" ? "library.recovery.empty" : section === "backups" ? "library.backups.empty" : "library.recent.empty")}
          </div>
        )}
        <div className="files-grid" key={section}>
          {(files ?? []).map((f) => (
            <RecentCard key={f.path} file={f} builds={installed} fallback={fallback} />
          ))}
        </div>
      </main>

      {/* Ctrl+Enter launches the default build. */}
      <KeyLaunch onLaunch={() => fallback && launch(fallback)} />
    </div>
  );
}

function KeyLaunch({ onLaunch }: { onLaunch: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "Enter") onLaunch();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onLaunch]);
  return null;
}

