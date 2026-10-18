import { useLayoutEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useOverlays } from "../lib/overlay";

/**
 * Blender Chat in a native WebView placed edge to edge over `.chat-host`
 * (see chat.rs), so the site reads as part of the window.
 */
export function ChatView() {
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

  return <div ref={host} className="view chat-host" />;
}
