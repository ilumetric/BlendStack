import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FileBox } from "lucide-react";
import { useStore } from "./store";
import { languageDirection, t, useT } from "./i18n";
import { api, errorText } from "./lib/api";
import { TitleBar } from "./components/TitleBar";
import { MenuHost } from "./components/Menu";
import { DialogHost } from "./components/Dialogs";
import { Toasts } from "./components/bits";
import { TooltipHost } from "./components/Tooltip";
import { LibraryView } from "./views/LibraryView";
import { BuildsView } from "./views/BuildsView";
import { NewsView } from "./views/NewsView";
import { ChatView } from "./views/ChatView";
import { useOverlay } from "./lib/overlay";
import { SettingsSheet } from "./views/SettingsSheet";
import { OpenWithSheet } from "./views/OpenWithSheet";

function useDocumentClasses() {
  const system = useStore((s) => s.system);
  const theme = useStore((s) => s.settings?.theme);
  const lang = useStore((s) => s.lang);
  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("translucent", !!system?.translucent);
    root.classList.forEach((c) => c.startsWith("os-") && root.classList.remove(c));
    if (system) root.classList.add(`os-${system.os}`);
    if (theme && theme !== "system") root.dataset.theme = theme;
    else delete root.dataset.theme;
    root.lang = lang;
    root.dir = languageDirection(lang);
  }, [system, theme, lang]);
}

function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      const s = useStore.getState();
      // Block browser accelerators that make no sense in an app window.
      if (e.key === "F5" || (mod && ["r", "p", "f", "g", "u", "j"].includes(e.key.toLowerCase()))) {
        e.preventDefault();
      }
      if (!mod) return;
      if (e.key === "1") s.setTab("library");
      if (e.key === "2") s.setTab("builds");
      if (e.key === "3") s.setTab("news");
      if (e.key === "4") s.setTab("chat");
      if (e.key === ",") s.setSettingsOpen(true);
      if (e.key.toLowerCase() === "r") {
        if (s.tab === "builds") void s.loadBuilds(s.buildsChannel, true);
        if (s.tab === "news") void s.loadNews(true);
        if (s.tab === "library") {
          void s.refreshInstalled();
          void s.loadRecent();
        }
      }
    };
    const onContext = (e: MouseEvent) => {
      const el = e.target as HTMLElement;
      if (!el.closest("input, textarea")) e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("contextmenu", onContext);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("contextmenu", onContext);
    };
  }, []);
}

function useFileDrop() {
  const [over, setOver] = useState(false);
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === "enter" || p.type === "over") setOver(true);
      else if (p.type === "leave") setOver(false);
      else if (p.type === "drop") {
        setOver(false);
        const files = p.paths.filter((f) => f.toLowerCase().endsWith(".blend"));
        if (files.length) useStore.getState().queueFiles(files);
        else useStore.getState().toast(t("toast.notBlend"), "error");
      }
    });
    return () => void un.then((f) => f());
  }, []);
  return over;
}

export default function App() {
  const tr = useT();
  const booted = useStore((s) => s.booted);
  const tab = useStore((s) => s.tab);
  const settingsOpen = useStore((s) => s.settingsOpen);
  const installedCount = useStore((s) => s.installed.length);
  const [fatal, setFatal] = useState<string | null>(null);
  const dropping = useFileDrop();
  useOverlay(dropping);

  useDocumentClasses();
  useShortcuts();

  useEffect(() => {
    useStore
      .getState()
      .boot()
      .catch((e) => setFatal(errorText(e)))
      .finally(() => requestAnimationFrame(() => void api.appReady()));
  }, []);

  if (fatal) {
    return (
      <div className="placeholder" style={{ height: "100%" }}>
        <h2>Blend Stack failed to start</h2>
        <code className="mono">{fatal}</code>
      </div>
    );
  }

  return (
    <div className="app">
      <TitleBar />
      <div className="content">
        {booted && tab === "library" && <LibraryView />}
        {booted && tab === "builds" && <BuildsView />}
        {booted && tab === "news" && <NewsView />}
        {booted && tab === "chat" && <ChatView />}
      </div>
      {settingsOpen && <SettingsSheet />}
      <OpenWithSheet />
      <DialogHost />
      <MenuHost />
      <TooltipHost />
      <Toasts />
      {dropping && installedCount > 0 && (
        <div className="dropzone">
          <div>
            <FileBox size={40} strokeWidth={1.4} color="var(--accent)" />
            <h2>{tr("drop.title")}</h2>
            <div className="muted">{tr("drop.subtitle")}</div>
          </div>
        </div>
      )}
    </div>
  );
}
