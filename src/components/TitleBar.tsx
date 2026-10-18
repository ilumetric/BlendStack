import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Bell, Copy, Download, Heart, Minus, Settings as Gear, Square, X } from "lucide-react";
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
        Blend Stack
      </div>

      <div className="titlebar-center">
        <Segmented<Tab>
          large
          value={tab}
          onChange={setTab}
          segments={[
            { value: "library", label: tr("tab.library") },
            { value: "builds", label: tr("tab.builds"), dot: buildsNew > 0 },
            { value: "news", label: tr("tab.news"), dot: newsNew },
            { value: "chat", label: tr("tab.chat") },
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
