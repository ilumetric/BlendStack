import { useEffect } from "react";
import { create } from "zustand";

/**
 * Counts open overlays (sheets, menus, popovers). The Chat tab is a native
 * WebView drawn above the page, so it steps aside while any overlay is open.
 */
export const useOverlays = create<{ count: number }>(() => ({ count: 0 }));

export function useOverlay(active = true) {
  useEffect(() => {
    if (!active) return;
    useOverlays.setState((s) => ({ count: s.count + 1 }));
    return () => useOverlays.setState((s) => ({ count: Math.max(0, s.count - 1) }));
  }, [active]);
}
