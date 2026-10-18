import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { create } from "zustand";

export type MenuEntry =
  | {
      label: string;
      icon?: ReactNode;
      hint?: string;
      checked?: boolean;
      danger?: boolean;
      disabled?: boolean;
      onSelect: () => void;
    }
  | { separator: true }
  | { heading: string };

interface MenuState {
  open: { x: number; y: number; items: MenuEntry[]; alignRight?: boolean } | null;
  show(x: number, y: number, items: MenuEntry[], alignRight?: boolean): void;
  close(): void;
}

const useMenu = create<MenuState>((set) => ({
  open: null,
  show: (x, y, items, alignRight) => set({ open: { x, y, items, alignRight } }),
  close: () => set({ open: null }),
}));

/** Context menu at the pointer. */
export function openContextMenu(e: React.MouseEvent, items: MenuEntry[]) {
  e.preventDefault();
  e.stopPropagation();
  useMenu.getState().show(e.clientX, e.clientY, items);
}

/** Dropdown below an element. */
export function openMenuAt(el: HTMLElement, items: MenuEntry[], alignRight = false) {
  const r = el.getBoundingClientRect();
  useMenu.getState().show(alignRight ? r.right : r.left, r.bottom + 4, items, alignRight);
}

export function MenuHost() {
  const open = useMenu((s) => s.open);
  const close = useMenu((s) => s.close);
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  useLayoutEffect(() => {
    if (!open || !ref.current) {
      setPos(null);
      return;
    }
    const { offsetWidth: w, offsetHeight: h } = ref.current;
    let left = open.alignRight ? open.x - w : open.x;
    let top = open.y;
    left = Math.max(8, Math.min(left, window.innerWidth - w - 8));
    if (top + h > window.innerHeight - 8) top = Math.max(8, open.y - h - (open.alignRight ? 36 : 0));
    setPos({ left, top });
    ref.current.querySelector<HTMLButtonElement>(".menu-item")?.focus({ preventScroll: true });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const items = Array.from(ref.current?.querySelectorAll<HTMLButtonElement>(".menu-item") ?? []);
        const i = items.indexOf(document.activeElement as HTMLButtonElement);
        const next = e.key === "ArrowDown" ? (i + 1) % items.length : (i - 1 + items.length) % items.length;
        items[next]?.focus();
      }
    };
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
    };
  }, [open, close]);

  if (!open) return null;
  return (
    <div
      ref={ref}
      className="menu"
      role="menu"
      style={pos ? { left: pos.left, top: pos.top } : { left: -9999, top: -9999 }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {open.items.map((item, i) => {
        if ("separator" in item) return <div key={i} className="menu-sep" />;
        if ("heading" in item)
          return (
            <div key={i} className="menu-label">
              {item.heading}
            </div>
          );
        return (
          <button
            key={i}
            role="menuitem"
            className={`menu-item${item.danger ? " danger" : ""}`}
            disabled={item.disabled}
            onClick={() => {
              close();
              item.onSelect();
            }}
          >
            {item.icon}
            <span>{item.label}</span>
            {item.hint && <span className="hint">{item.hint}</span>}
            {item.checked && <span className="check">✓</span>}
          </button>
        );
      })}
    </div>
  );
}
