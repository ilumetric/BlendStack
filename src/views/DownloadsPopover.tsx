import { Download, Play, X } from "lucide-react";
import { useStore } from "../store";
import { useT } from "../i18n";
import { api } from "../lib/api";
import { formatBytes, formatDuration } from "../lib/format";
import { buildName } from "../lib/naming";
import { Popover } from "../components/Popover";
import { Glyph } from "../components/bits";
import type { DownloadTask } from "../lib/types";

const ACTIVE = ["queued", "downloading", "verifying", "extracting"];

export function DownloadsPopover({ anchor, onClose }: { anchor: HTMLElement | null; onClose: () => void }) {
  const tr = useT();
  const lang = useStore((s) => s.lang);
  const downloads = useStore((s) => s.downloads);
  const installed = useStore((s) => s.installed);
  const labels = useStore((s) => s.settings?.labels);
  const cancel = useStore((s) => s.cancelDownload);
  const launch = useStore((s) => s.launch);

  const tasks = Object.values(downloads).sort((a, b) => b.startedAt - a.startedAt);
  const finished = tasks.some((t) => !ACTIVE.includes(t.state));

  const clear = async () => {
    const left = await api.clearDownloads();
    useStore.setState({ downloads: Object.fromEntries(left.map((d) => [d.id, d])) });
  };

  const status = (task: DownloadTask) => {
    if (task.state === "downloading") {
      const total = task.total || task.build.fileSize;
      const eta = task.speed > 0 ? formatDuration((total - task.received) / task.speed, lang) : "";
      return [
        `${formatBytes(task.received, lang)} / ${formatBytes(total, lang)}`,
        task.speed > 0 ? `${formatBytes(task.speed, lang)}/s` : "",
        eta ? tr("downloads.left", { time: eta }) : "",
      ]
        .filter(Boolean)
        .join(" · ");
    }
    if (task.state === "failed") return task.error ?? tr("downloads.state.failed");
    if (task.state === "extracting") return `${tr("downloads.state.extracting")} ${Math.round(task.progress * 100)}%`;
    return tr(`downloads.state.${task.state}`);
  };

  return (
    <Popover anchor={anchor} onClose={onClose}>
      <div className="popover-head">
        <h3>{tr("downloads.title")}</h3>
        {finished && (
          <button className="btn plain" onClick={clear}>
            {tr("downloads.clear")}
          </button>
        )}
      </div>
      <div className="popover-body">
        {tasks.length === 0 && (
          <div className="popover-empty">
            <Download size={28} strokeWidth={1.5} className="faint" />
            <div style={{ marginTop: 8 }}>{tr("downloads.empty")}</div>
          </div>
        )}
        {tasks.map((task) => {
          const active = ACTIVE.includes(task.state);
          const value =
            task.state === "downloading" && task.total
              ? task.received / task.total
              : task.state === "extracting"
                ? task.progress
                : null;
          const inst = task.installedId ? installed.find((b) => b.id === task.installedId) : undefined;
          return (
            <div key={task.id} className="row" style={{ alignItems: "flex-start" }}>
              <Glyph build={task.build} size="sm" />
              <div className="row-main">
                <div className="row-title">
                  <span className="name">{buildName(task.build, tr, labels)}</span>
                </div>
                <div
                  className="row-sub"
                  style={{ color: task.state === "failed" ? "var(--danger)" : undefined, whiteSpace: "normal" }}
                >
                  {status(task)}
                </div>
                {active && (
                  <div className={`progress${value === null ? " indeterminate" : ""}`}>
                    <div style={{ width: `${(value ?? 0) * 100}%` }} />
                  </div>
                )}
              </div>
              <div className="row-actions">
                {active && (
                  <button className="icon-btn" title={tr("common.cancel")} onClick={() => cancel(task.id)}>
                    <X size={15} />
                  </button>
                )}
                {inst && (
                  <button className="pill" onClick={() => launch(inst)}>
                    <Play size={11} fill="currentColor" />
                    {tr("common.open")}
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </Popover>
  );
}
