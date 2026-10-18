import { useLayoutEffect, useRef, useState, type ReactNode } from "react";

export interface Segment<T extends string> {
  value: T;
  label: ReactNode;
  badge?: number;
  dot?: boolean;
  /** Tooltip shown when the label is reduced to an icon (narrow window). */
  title?: string;
}

interface Props<T extends string> {
  value: T;
  segments: Segment<T>[];
  onChange: (value: T) => void;
  large?: boolean;
}

/** macOS-style segmented control with a sliding thumb. */
export function Segmented<T extends string>({ value, segments, onChange, large }: Props<T>) {
  const refs = useRef<Record<string, HTMLButtonElement | null>>({});
  const [thumb, setThumb] = useState<{ left: number; width: number } | null>(null);

  useLayoutEffect(() => {
    const measure = () => {
      const el = refs.current[value];
      if (el) setThumb({ left: el.offsetLeft, width: el.offsetWidth });
    };
    measure();
    const ro = new ResizeObserver(measure);
    Object.values(refs.current).forEach((el) => el && ro.observe(el));
    return () => ro.disconnect();
  }, [value, segments.length]);

  return (
    <div className={`segmented${large ? " large" : ""}`} role="tablist">
      {thumb && <div className="segmented-thumb" style={{ left: thumb.left, width: thumb.width }} />}
      {segments.map((s) => (
        <button
          key={s.value}
          ref={(el) => {
            refs.current[s.value] = el;
          }}
          role="tab"
          aria-selected={s.value === value}
          className={s.value === value ? "on" : ""}
          data-compact-tip={s.title}
          onClick={() => onChange(s.value)}
        >
          {s.label}
          {!!s.badge && <span className="seg-badge">{s.badge > 99 ? "99+" : s.badge}</span>}
          {!s.badge && s.dot && <span className="seg-dot" />}
        </button>
      ))}
    </div>
  );
}
