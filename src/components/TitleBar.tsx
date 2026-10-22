import { useEffect, useRef, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Bell,
  Boxes,
  Copy,
  Download,
  Heart,
  LibraryBig,
  MessagesSquare,
  Minus,
  Newspaper,
  Settings as Gear,
  Square,
  X,
} from "lucide-react";
import { api } from "../lib/api";
import logo from "../assets/logo.svg";
import { useStore, type Tab } from "../store";
import { useT } from "../i18n";
import { Segmented } from "./Segmented";
import { Ring } from "./bits";
import { DownloadsPopover } from "../views/DownloadsPopover";
import { NotificationsPopover } from "../views/NotificationsPopover";
import type { RemoteChannel } from "../lib/types";

/** The Blender Foundation's official donation page. */
const DEVELOPMENT_FUND = "https://fund.blender.org/";
const REPOSITORY = "https://github.com/ilumetric/BlendStack";

/** GitHub's mark (lucide has no brand icons). */
function GitHubMark({ size }: { size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="currentColor" aria-hidden>
      <path d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z" />
    </svg>
  );
}

/** Text label; in a narrow window only the icon is shown (see styles.css). */
function tabLabel(icon: ReactNode, text: string) {
  return (
    <>
      <span className="seg-icon">{icon}</span>
      <span className="seg-label">{text}</span>
    </>
  );
}

type Panel = "downloads" | "notifications" | null;

export function TitleBar() {
  const tr = useT();
  const tab = useStore((s) => s.tab);
  const setTab = useStore((s) => s.setTab);
  const os = useStore((s) => s.system?.os);
  const downloads = useStore((s) => s.downloads);
  const notifications = useStore((s) => s.notifications);
  const remote = useStore((s) => s.remote);
  const news = useStore((s) => s.news.list);
  const seen = useStore((s) => s.seen);
  const chatUnread = useStore((s) => s.chatUnread);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);
  const [panel, setPanel] = useState<Panel>(null);
  const [maximized, setMaximized] = useState(false);
  const downloadsBtn = useRef<HTMLButtonElement>(null);
  const bellBtn = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (os === "macos") return;
    const win = getCurrentWindow();
    const update = () => void win.isMaximized().then(setMaximized);
    update();
    const un = win.onResized(update);
    return () => void un.then((f) => f());
  }, [os]);

  const tasks = Object.values(downloads);
  const active = tasks.filter((d) => ["queued", "downloading", "verifying", "extracting"].includes(d.state));
  const totals = active.reduce(
    (acc, d) => ({ received: acc.received + d.received, total: acc.total + (d.total || d.build.fileSize) }),
    { received: 0, total: 0 },
  );
  const unread = notifications.filter((n) => !n.read).length;

  const newIn = (c: RemoteChannel) => (remote[c].list?.builds ?? []).filter((b) => b.date > seen[c]).length;
  const buildsNew = tab === "builds" ? 0 : newIn("stable") + newIn("daily");
  const newsNew = tab !== "news" && (news?.items ?? []).some((n) => n.date > seen.news);

  const toggle = (p: Panel) => setPanel((cur) => (cur === p ? null : p));
  const win = getCurrentWindow();

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <img src={logo} alt="" draggable={false} />
        <span className="brand-name">Blend Stack</span>
      </div>

      <div className="titlebar-center">
        <Segmented<Tab>
          large
          value={tab}
          onChange={setTab}
          segments={[
            { value: "library", label: tabLabel(<LibraryBig size={16} />, tr("tab.library")), title: tr("tab.library") },
            { value: "builds", label: tabLabel(<Boxes size={16} />, tr("tab.builds")), title: tr("tab.builds"), dot: buildsNew > 0 },
            { value: "news", label: tabLabel(<Newspaper size={16} />, tr("tab.news")), title: tr("tab.news"), dot: newsNew },
            {
              value: "chat",
              label: tabLabel(<MessagesSquare size={16} />, tr("tab.chat")),
              title: tr("tab.chat"),
              badge: tab === "chat" ? 0 : chatUnread,
            },
          ]}
        />
      </div>

      <div className="titlebar-actions" data-tauri-drag-region>
        <button
          className="icon-btn heart"
          title={tr("fund.tooltip")}
          onClick={() => void api.openUrl(DEVELOPMENT_FUND)}
        >
          <Heart size={17} />
        </button>
        <button className="icon-btn github" title={tr("github.tooltip")} onClick={() => void api.openUrl(REPOSITORY)}>
          <GitHubMark size={16} />
        </button>
        <button
          ref={downloadsBtn}
          className={`icon-btn${panel === "downloads" ? " active" : ""}`}
          title={tr("downloads.title")}
          onClick={() => toggle("downloads")}
        >
          {active.length > 0 ? (
            <span className="ring-btn" style={{ width: 22, height: 22 }}>
              <Ring value={totals.total ? totals.received / totals.total : null} size={22} />
              <Download size={11} strokeWidth={2.6} />
            </span>
          ) : (
            <Download size={17} />
          )}
        </button>
        <button
          ref={bellBtn}
          className={`icon-btn${panel === "notifications" ? " active" : ""}`}
          title={tr("notifications.title")}
          onClick={() => toggle("notifications")}
        >
          <Bell size={17} />
          {unread > 0 && <span className="dot" />}
        </button>
        <button className="icon-btn" title={`${tr("settings.title")} (Ctrl+,)`} onClick={() => setSettingsOpen(true)}>
          <Gear size={17} />
        </button>

        {os !== "macos" && (
          <div className="window-controls">
            <button aria-label={tr("window.minimize")} onClick={() => void win.minimize()}>
              <Minus size={15} />
            </button>
            <button aria-label={tr("window.maximize")} onClick={() => void win.toggleMaximize()}>
              {maximized ? <Copy size={12} style={{ transform: "scaleX(-1)" }} /> : <Square size={12} />}
            </button>
            <button className="close" aria-label={tr("window.close")} onClick={() => void win.close()}>
              <X size={16} />
            </button>
          </div>
        )}
      </div>

      {panel === "downloads" && <DownloadsPopover anchor={downloadsBtn.current} onClose={() => setPanel(null)} />}
      {panel === "notifications" && <NotificationsPopover anchor={bellBtn.current} onClose={() => setPanel(null)} />}
    </header>
  );
}
