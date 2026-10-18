import { ChevronsUpDown } from "lucide-react";
import { openMenuAt } from "./Menu";

interface Props<T extends string | number> {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}

/** Pop-up button in the app's own menu style (instead of the native `<select>`). */
export function Select<T extends string | number>({ value, options, onChange }: Props<T>) {
  const current = options.find((o) => o.value === value);
  return (
    <button
      className="select"
      aria-haspopup="menu"
      onClick={(e) =>
        openMenuAt(
          e.currentTarget,
          options.map((o) => ({ label: o.label, checked: o.value === value, onSelect: () => onChange(o.value) })),
          true,
        )
      }
    >
      <span>{current?.label ?? ""}</span>
      <ChevronsUpDown size={12} strokeWidth={2.4} />
    </button>
  );
}
