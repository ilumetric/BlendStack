import { useEffect, useRef, useState, type ReactNode } from "react";
import { create } from "zustand";
import { AlertTriangle } from "lucide-react";
import { t } from "../i18n";

interface ConfirmOptions {
  title: string;
  body?: string;
  confirmLabel: string;
  cancelLabel?: string;
  danger?: boolean;
}

interface PromptOptions {
  title: string;
  hint?: string;
  placeholder?: string;
  value?: string;
  mono?: boolean;
}

type Dialog =
  | ({ kind: "confirm"; resolve: (ok: boolean) => void } & ConfirmOptions)
  | ({ kind: "prompt"; resolve: (value: string | null) => void } & PromptOptions);

const useDialogs = create<{ dialog: Dialog | null }>(() => ({ dialog: null }));

export function confirmDialog(options: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => useDialogs.setState({ dialog: { kind: "confirm", resolve, ...options } }));
}

export function promptDialog(options: PromptOptions): Promise<string | null> {
  return new Promise((resolve) => useDialogs.setState({ dialog: { kind: "prompt", resolve, ...options } }));
}

function close() {
  useDialogs.setState({ dialog: null });
}

export function Backdrop({ onClose, children }: { onClose: () => void; children: ReactNode }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      {children}
    </div>
  );
}

export function DialogHost() {
  const dialog = useDialogs((s) => s.dialog);
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (dialog?.kind === "prompt") {
      setValue(dialog.value ?? "");
      requestAnimationFrame(() => inputRef.current?.select());
    }
  }, [dialog]);

  if (!dialog) return null;

  if (dialog.kind === "confirm") {
    const answer = (ok: boolean) => {
      dialog.resolve(ok);
      close();
    };
    return (
      <Backdrop onClose={() => answer(false)}>
        <div className="sheet small alert" role="alertdialog">
          {dialog.danger && <AlertTriangle size={34} color="var(--orange)" />}
          <h2>{dialog.title}</h2>
          {dialog.body && <p>{dialog.body}</p>}
          <div className="buttons">
            <button className="btn lg" onClick={() => answer(false)}>
              {dialog.cancelLabel ?? t("common.cancel")}
            </button>
            <button className={`btn lg ${dialog.danger ? "danger" : "primary"}`} autoFocus onClick={() => answer(true)}>
              {dialog.confirmLabel}
            </button>
          </div>
        </div>
      </Backdrop>
    );
  }

  const answer = (v: string | null) => {
    dialog.resolve(v);
    close();
  };
  return (
    <Backdrop onClose={() => answer(null)}>
      <form
        className="sheet small"
        onSubmit={(e) => {
          e.preventDefault();
          answer(value);
        }}
      >
        <div className="sheet-head">
          <h2>{dialog.title}</h2>
        </div>
        <div className="sheet-body">
          <input
            ref={inputRef}
            className={`input${dialog.mono ? " mono" : ""}`}
            style={{ height: 32 }}
            value={value}
            placeholder={dialog.placeholder}
            onChange={(e) => setValue(e.target.value)}
            spellCheck={false}
          />
          {dialog.hint && (
            <p className="muted" style={{ margin: "8px 2px 0", fontSize: 12 }}>
              {dialog.hint}
            </p>
          )}
        </div>
        <div className="sheet-foot">
          <button type="button" className="btn" onClick={() => answer(null)}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="btn primary">
            {t("common.save")}
          </button>
        </div>
      </form>
    </Backdrop>
  );
}
