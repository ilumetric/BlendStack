import { CheckCircle2, Info, XCircle } from "lucide-react";
import { useStore } from "../store";
import { series, tone } from "../lib/builds";
import type { Channel } from "../lib/types";
import { BlenderMark } from "./BlenderMark";

export function Glyph({
  build,
  size,
}: {
  build: { version: string; releaseCycle: string; channel: Channel; patch: string | null };
  size?: "sm" | "xl";
}) {
  return (
    <div className={`glyph tone-${tone(build)}${size ? ` ${size}` : ""}`} aria-hidden>
      <BlenderMark className="glyph-mark" />
      <span>{build.version ? series(build.version) : "?"}</span>
    </div>
  );
}

export function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label?: string }) {
  return (
    <button
      role="switch"
      aria-checked={on}
      aria-label={label}
      className={`switch${on ? " on" : ""}`}
      onClick={() => onChange(!on)}
    />
  );
}

/** Circular progress; `value` null means indeterminate. */
export function Ring({ value, size = 30 }: { value: number | null; size?: number }) {
  const r = size / 2 - 2;
  const c = 2 * Math.PI * r;
  const v = value === null ? 0.25 : Math.max(0.02, Math.min(1, value));
  return (
    <svg className="ring" width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      <circle className="ring-track" cx={size / 2} cy={size / 2} r={r} fill="none" strokeWidth="2.5" />
      <g className={value === null ? "ring-indeterminate" : undefined}>
        <circle
          className="ring-value"
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          strokeWidth="2.5"
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={c * (1 - v)}
        />
      </g>
    </svg>
  );
}

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismissToast);
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast ${toast.tone}`} onClick={() => dismiss(toast.id)}>
          {toast.tone === "success" ? (
            <CheckCircle2 size={17} />
          ) : toast.tone === "error" ? (
            <XCircle size={17} />
          ) : (
            <Info size={17} />
          )}
          <span>{toast.text}</span>
          {toast.action && (
            <button
              className="btn plain"
              onClick={(e) => {
                e.stopPropagation();
                toast.action?.run();
                dismiss(toast.id);
              }}
            >
              {toast.action.label}
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
