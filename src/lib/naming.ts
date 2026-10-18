import type { Translate } from "../i18n/core";
import { normalizeCycle, prNumber, series } from "./builds";
import type { Channel } from "./types";

interface Named {
  version: string;
  releaseCycle: string;
  channel: Channel;
  patch: string | null;
  id?: string;
}

/** Human name of a build: "Blender 5.1.2", "Blender 5.4 Alpha", "Blender 4.5 · PR #123". */
export function buildName(b: Named, tr: Translate, labels?: Record<string, string>): string {
  const custom = b.id ? labels?.[b.id]?.trim() : "";
  if (custom) return custom;
  if (!b.version) return "Blender";
  const pr = prNumber(b.patch);
  if (b.channel === "experimental" && pr) return `Blender ${series(b.version)} · PR #${pr}`;
  const cycle = normalizeCycle(b.releaseCycle);
  if (cycle === "stable") return `Blender ${b.version}`;
  return `Blender ${series(b.version)} ${tr(`cycle.${cycle}`)}`;
}

export function cycleLabel(cycle: string, tr: Translate, short = false): string {
  const c = normalizeCycle(cycle);
  if (c === "candidate" && short) return tr("cycle.candidate.short");
  return tr(`cycle.${c}`);
}
