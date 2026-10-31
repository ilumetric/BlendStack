import { useEffect, useState } from "react";
import { ArrowUpCircle, BellOff, Download, Package, RefreshCw, Sparkles } from "lucide-react";
import { useStore } from "../store";
import { useT } from "../i18n";
import { api, errorText } from "../lib/api";
import { relativeTime } from "../lib/format";
import { Popover } from "../components/Popover";
import type { AppNotification, RemoteChannel } from "../lib/types";

function icon(n: AppNotification) {
  switch (n.kind) {
    case "release":
      return { tone: "orange", el: <Sparkles size={16} /> };
    case "app":
      return { tone: "gray", el: <Download size={16} /> };
    case "update":
      return { tone: "blue", el: <ArrowUpCircle size={16} /> };
    default:
      return { tone: n.channel === "experimental" ? "pink" : "purple", el: <Package size={16} /> };
  }
}

export function NotificationsPopover({ anchor, onClose }: { anchor: HTMLElement | null; onClose: () => void }) {
  const tr = useT();
  const lang = useStore((s) => s.lang);
  const items = useStore((s) => s.notifications);
  const showBuild = useStore((s) => s.showBuild);
  const toast = useStore((s) => s.toast);
  const [checking, setChecking] = useState(false);

  // Opening the panel counts as reading it.
  useEffect(() => {
    return () => {
      if (useStore.getState().notifications.some((n) => !n.read)) void api.markNotificationsRead();
    };
  }, []);

  const check = async () => {
    setChecking(true);
    try {
      const fresh = await api.checkNow();
      toast(fresh.length ? tr("toast.newBuilds", { count: fresh.length }) : tr("toast.upToDate"), "info");
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      setChecking(false);
    }
  };

  return (
    <Popover anchor={anchor} onClose={onClose}>
      <div className="popover-head">
        <h3>{tr("notifications.title")}</h3>
        <button className="btn plain" onClick={check} disabled={checking}>
          <RefreshCw size={13} className={checking ? "spin" : undefined} />
          {tr("notifications.check")}
        </button>
        {items.length > 0 && (
          <button className="btn plain" onClick={() => void api.clearNotifications()}>
            {tr("notifications.clear")}
          </button>
        )}
      </div>
      <div className="popover-body">
        {items.length === 0 && (
          <div className="popover-empty">
            <BellOff size={28} strokeWidth={1.5} className="faint" />
            <div style={{ marginTop: 8 }}>{tr("notifications.empty")}</div>
          </div>
        )}
        {items.map((n) => {
          const ic = icon(n);
          const clickable = n.kind === "app" || (n.channel && n.channel !== "custom");
          return (
            <button
              key={n.id}
              className="row"
              style={{ width: "100%", textAlign: "start", paddingInlineStart: 14 }}
              onClick={() => {
                if (!clickable) return;
                if (n.kind === "app") useStore.getState().setSettingsOpen(true);
                else showBuild(n.channel as RemoteChannel, n.buildId);
                onClose();
              }}
            >
              {!n.read && <span className="notif-unread" />}
              <div className={`notif-icon tone-${ic.tone}`}>{ic.el}</div>
              <div className="row-main">
                <div className="row-title" style={{ fontSize: 13 }}>
                  <span className="name">{n.title}</span>
                </div>
                <div className="row-sub">
                  <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{n.body}</span>
                </div>
              </div>
              <span className="faint" style={{ fontSize: 11, alignSelf: "flex-start", marginTop: 2 }}>
                {relativeTime(n.date, lang)}
              </span>
            </button>
          );
        })}
      </div>
    </Popover>
  );
}
