import { useLayoutEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ExternalLink, MessagesSquare, RefreshCw } from "lucide-react";
import { useT } from "../i18n";
import { api } from "../lib/api";
import { useOverlays } from "../lib/overlay";

const CHAT_URL = "https://chat.blender.org/";

/** Blender Chat in a native WebView placed over `.chat-host` (see chat.rs). */
export function ChatView() {
  const tr = useT();
  const host = useRef<HTMLDivElement>(null);
  const overlays = useOverlays((s) => s.count);

  useLayoutEffect(() => {
    const el = host.current;
    if (!el) return;
    if (overlays > 0) {
      void invoke("chat_hide");
      return;
    }
    const place = () => {
      const r = el.getBoundingClientRect();
      void invoke("chat_show", { x: r.left, y: r.top, width: r.width, height: r.height });
    };
    place();
    const ro = new ResizeObserver(place);
    ro.observe(el);
    window.addEventListener("resize", place);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", place);
    };
  }, [overlays]);

  useLayoutEffect(() => () => void invoke("chat_hide"), []);

  return (
    <div className="view chat">
      <div className="toolbar">
        <MessagesSquare size={16} className="muted" />
        <span className="chat-title">{tr("chat.title")}</span>
        <span className="toolbar-status">{tr("chat.hint")}</span>
        <span style={{ flex: 1 }} />
        <button className="icon-btn" title={tr("common.refresh")} onClick={() => void invoke("chat_reload")}>
          <RefreshCw size={16} />
        </button>
        <button className="icon-btn" title={tr("chat.openBrowser")} onClick={() => void api.openUrl(CHAT_URL)}>
          <ExternalLink size={16} />
        </button>
      </div>
      <div ref={host} className="chat-host" />
    </div>
  );
}
