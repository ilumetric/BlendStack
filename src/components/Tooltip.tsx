import { useEffect, useRef, useState } from "react";

const DELAY = 450;
/** Matches the icon-only tab breakpoint in styles.css. */
const COMPACT_WIDTH = 860;

/**
 * Replaces native `title` tooltips app-wide with tooltips in the app's style.
 * Elements keep using `title`; on first hover it moves to `data-tip`.
 */
export function TooltipHost() {
  const [tip, setTip] = useState<{ text: string; x: number; y: number; below: boolean } | null>(null);
  const timer = useRef<number>(0);
  const target = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const hide = () => {
      window.clearTimeout(timer.current);
      target.current = null;
      setTip(null);
    };
    const onOver = (e: PointerEvent) => {
      const el = (e.target as HTMLElement).closest?.<HTMLElement>("[title], [data-tip], [data-compact-tip]");
      if (!el) return;
      const title = el.getAttribute("title");
      if (title !== null) {
        el.dataset.tip = title;
        el.removeAttribute("title");
      }
      if (el === target.current) return;
      hide();
      // Icon-only tabs explain themselves only while the window is narrow.
      const text = el.dataset.tip ?? (window.innerWidth <= COMPACT_WIDTH ? el.dataset.compactTip : undefined);
      if (!text) return;
      target.current = el;
      timer.current = window.setTimeout(() => {
        if (target.current !== el || !el.isConnected) return;
        const r = el.getBoundingClientRect();
        const below = r.bottom + 40 < window.innerHeight;
        setTip({ text, x: r.left + r.width / 2, y: below ? r.bottom + 6 : r.top - 6, below });
      }, DELAY);
    };
    const onOut = (e: PointerEvent) => {
      const el = target.current;
      if (el && !el.contains(e.relatedTarget as Node)) hide();
    };
    window.addEventListener("pointerover", onOver);
    window.addEventListener("pointerout", onOut);
    window.addEventListener("pointerdown", hide, true);
    window.addEventListener("wheel", hide, true);
    window.addEventListener("keydown", hide, true);
    window.addEventListener("blur", hide);
    return () => {
      window.removeEventListener("pointerover", onOver);
      window.removeEventListener("pointerout", onOut);
      window.removeEventListener("pointerdown", hide, true);
      window.removeEventListener("wheel", hide, true);
      window.removeEventListener("keydown", hide, true);
      window.removeEventListener("blur", hide);
    };
  }, []);

  const ref = useRef<HTMLDivElement>(null);
  const [left, setLeft] = useState(0);
  useEffect(() => {
    if (!tip || !ref.current) return;
    const w = ref.current.offsetWidth;
    setLeft(Math.max(8, Math.min(tip.x - w / 2, window.innerWidth - w - 8)));
  }, [tip]);

  if (!tip) return null;
  return (
    <div
      ref={ref}
      className="tooltip"
      role="tooltip"
      style={{ left, top: tip.y, transform: tip.below ? undefined : "translateY(-100%)" }}
    >
      {tip.text}
    </div>
  );
}
