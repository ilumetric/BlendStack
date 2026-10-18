import { Download, Play, RotateCcw } from "lucide-react";
import { useStore } from "../store";
import { useT } from "../i18n";
import { installedFor } from "../lib/builds";
import type { RemoteBuild } from "../lib/types";
import { Ring } from "./bits";

/** GET → progress ring → OPEN, App Store style. */
export function BuildAction({ build, replaces }: { build: RemoteBuild; replaces?: string | null }) {
  const tr = useT();
  const task = useStore((s) => s.downloads[build.id]);
  const installed = useStore((s) => s.installed);
  const download = useStore((s) => s.download);
  const cancel = useStore((s) => s.cancelDownload);
  const launch = useStore((s) => s.launch);

  const active = task && ["queued", "downloading", "verifying", "extracting"].includes(task.state);
  if (active) {
    const value =
      task.state === "downloading" && task.total > 0 && task.received > 0
        ? task.received / task.total
        : task.state === "extracting"
          ? task.progress
          : null;
    const pct = value === null ? "" : ` ${Math.round(value * 100)}%`;
    return (
      <button
        className="ring-btn"
        title={`${tr(`downloads.state.${task.state}`)}${pct} — ${tr("common.cancel")}`}
        onClick={() => cancel(build.id)}
      >
        <Ring value={value} />
        <span className="stop" />
      </button>
    );
  }

  const inst = installedFor(build, installed);
  if (inst) {
    return (
      <button className="pill" onClick={() => launch(inst)}>
        <Play size={11} fill="currentColor" />
        {tr("common.open")}
      </button>
    );
  }
  if (task?.state === "failed") {
    return (
      <button className="pill" title={task.error ?? ""} onClick={() => download(build, replaces)}>
        <RotateCcw size={12} />
        {tr("common.retry")}
      </button>
    );
  }
  return (
    <button className={`pill${replaces ? " filled" : ""}`} onClick={() => download(build, replaces)}>
      {replaces ? tr("common.update") : (
        <>
          <Download size={12} strokeWidth={2.6} />
          {tr("common.get")}
        </>
      )}
    </button>
  );
}
