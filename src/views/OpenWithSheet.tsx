import { useEffect, useMemo, useState } from "react";
import { FileBox, Play } from "lucide-react";
import { useStore } from "../store";
import { useT } from "../i18n";
import { bestForSeries, resolveDefault, sortInstalled } from "../lib/builds";
import { buildName, cycleLabel } from "../lib/naming";
import { Backdrop } from "../components/Dialogs";
import { Glyph } from "../components/bits";

/** Picks a build for files dropped on the window, opened via "Open File…" or passed by the OS. */
export function OpenWithSheet() {
  const tr = useT();
  const files = useStore((s) => s.openFiles);
  const installed = useStore((s) => s.installed);
  const settings = useStore((s) => s.settings);
  const lists = useStore((s) => s.files);
  const launch = useStore((s) => s.launch);
  const file = files[0];

  const builds = useMemo(() => sortInstalled(installed), [installed]);
  const fileSeries =
    [...(lists.recent ?? []), ...(lists.recovery ?? []), ...(lists.backups ?? [])].find((r) => r.path === file)?.series ?? "";
  const suggested = bestForSeries(installed, fileSeries, resolveDefault(installed, settings));
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => setSelected(suggested?.id ?? null), [file, suggested?.id]);

  if (!file) return null;
  const name = file.split(/[\\/]/).pop() ?? file;
  const next = () => useStore.setState((s) => ({ openFiles: s.openFiles.slice(1) }));
  const go = async () => {
    const b = installed.find((i) => i.id === selected);
    if (b) await launch(b, file);
    next();
  };

  return (
    <Backdrop onClose={next}>
      <div className="sheet small" role="dialog">
        <div className="sheet-head" style={{ alignItems: "flex-start" }}>
          <FileBox size={30} strokeWidth={1.5} style={{ color: "var(--accent)", flex: "none" }} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <h2 style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {tr("openWith.title", { name })}
            </h2>
            <div className="muted" style={{ fontSize: 12 }}>
              {fileSeries ? tr("openWith.saved", { series: fileSeries }) : tr("openWith.subtitle")}
            </div>
          </div>
        </div>
        <div className="sheet-body" style={{ maxHeight: 340, paddingLeft: 12, paddingRight: 12 }}>
          {builds.map((b) => (
            <div
              key={b.id}
              className={`row${b.id === selected ? " selected" : ""}`}
              onClick={() => setSelected(b.id)}
              onDoubleClick={() => {
                setSelected(b.id);
                void launch(b, file).then(next);
              }}
            >
              <Glyph build={b} size="sm" />
              <div className="row-main">
                <div className="row-title">
                  <span className="name">{buildName(b, tr, settings?.labels)}</span>
                </div>
                <div className="row-sub">
                  {b.channel === "stable" ? tr("channel.stable") : `${cycleLabel(b.releaseCycle, tr)} · ${b.version}`}
                </div>
              </div>
            </div>
          ))}
        </div>
        <div className="sheet-foot">
          <button className="btn" onClick={next}>
            {tr("common.cancel")}
          </button>
          <button className="btn primary" disabled={!selected} onClick={go} autoFocus>
            <Play size={12} fill="currentColor" />
            {tr("common.open")}
          </button>
        </div>
      </div>
    </Backdrop>
  );
}
