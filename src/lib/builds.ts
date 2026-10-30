import type { Channel, InstalledBuild, RemoteBuild, Settings } from "./types";

type AnyBuild = Pick<RemoteBuild, "version" | "releaseCycle" | "channel" | "patch">;

export function parseVersion(v: string): [number, number, number, number] {
  const parts = v.split(".");
  const num = (s: string | undefined) => parseInt(s ?? "0", 10) || 0;
  const suffix = /[a-z]$/.exec(v)?.[0].charCodeAt(0) ?? 0;
  return [num(parts[0]), num(parts[1]), num(parts[2]), suffix];
}

export function compareVersions(a: string, b: string): number {
  const pa = parseVersion(a);
  const pb = parseVersion(b);
  for (let i = 0; i < 4; i++) if (pa[i] !== pb[i]) return pa[i] - pb[i];
  return 0;
}

export function series(v: string): string {
  const [major, minor] = parseVersion(v);
  return `${major}.${minor}`;
}

export const CYCLE_RANK: Record<string, number> = { stable: 4, candidate: 3, rc: 3, beta: 2, alpha: 1 };

export function normalizeCycle(cycle: string): "stable" | "candidate" | "beta" | "alpha" {
  if (cycle === "rc") return "candidate";
  if (cycle === "candidate" || cycle === "beta" || cycle === "alpha") return cycle;
  return "stable";
}

/** CSS accent variable for a build's maturity. */
export function tone(b: AnyBuild): string {
  if (b.channel === "experimental") return "pink";
  switch (normalizeCycle(b.releaseCycle)) {
    case "alpha":
      return "purple";
    case "beta":
      return "blue";
    case "candidate":
      return "red";
    default:
      return b.channel === "custom" ? "gray" : "orange";
  }
}

export function prNumber(patch: string | null): string | null {
  const m = /PR(\d+)/i.exec(patch ?? "");
  return m ? m[1] : null;
}

export function prUrl(patch: string | null): string | null {
  const n = prNumber(patch);
  return n ? `https://projects.blender.org/blender/blender/pulls/${n}` : null;
}

export function releaseNotesUrl(version: string): string {
  return `https://developer.blender.org/docs/release_notes/${series(version)}/`;
}

/** Remote build that is already on disk. */
export function installedFor(remote: RemoteBuild, installed: InstalledBuild[]): InstalledBuild | undefined {
  return installed.find(
    (i) =>
      i.sourceId === remote.id ||
      (!i.sourceId &&
        i.channel === remote.channel &&
        i.version === remote.version &&
        (remote.hash ? i.hash === remote.hash : true)),
  );
}

/** Newer build of the same line (series for stable/daily, pull request for experimental). */
export function findUpdate(
  b: InstalledBuild,
  remote: Partial<Record<Channel, RemoteBuild[]>>,
  installed: InstalledBuild[],
): RemoteBuild | null {
  if (b.external || b.channel === "custom") return null;
  const list = remote[b.channel] ?? [];
  let best: RemoteBuild | null = null;
  for (const r of list) {
    if (r.id === b.sourceId) continue;
    let newer: boolean;
    if (b.channel === "stable") {
      newer = series(r.version) === series(b.version) && compareVersions(r.version, b.version) > 0;
    } else if (b.channel === "experimental") {
      newer = !!b.patch && r.patch === b.patch && (b.date ?? 0) < r.date;
    } else {
      newer =
        series(r.version) === series(b.version) &&
        normalizeCycle(r.releaseCycle) === normalizeCycle(b.releaseCycle) &&
        (b.date ?? 0) < r.date;
    }
    if (!newer || installedFor(r, installed)) continue;
    if (!best || r.date > best.date || compareVersions(r.version, best.version) > 0) best = r;
  }
  return best;
}

export function sortInstalled(list: InstalledBuild[]): InstalledBuild[] {
  return [...list].sort(
    (a, b) =>
      compareVersions(b.version, a.version) ||
      (CYCLE_RANK[b.releaseCycle] ?? 0) - (CYCLE_RANK[a.releaseCycle] ?? 0) ||
      (b.date ?? 0) - (a.date ?? 0),
  );
}

/** The build the big Launch button starts. */
export function resolveDefault(installed: InstalledBuild[], settings: Settings | null): InstalledBuild | null {
  if (!installed.length) return null;
  if (settings?.defaultBuild) {
    const d = installed.find((b) => b.id === settings.defaultBuild);
    if (d) return d;
  }
  const launched = settings?.lastLaunched ?? {};
  const byLaunch = [...installed].sort((a, b) => (launched[b.id] ?? 0) - (launched[a.id] ?? 0));
  if (launched[byLaunch[0].id]) return byLaunch[0];
  return sortInstalled(installed.filter((b) => b.channel === "stable"))[0] ?? sortInstalled(installed)[0];
}

/**
 * Best build to open a file saved by Blender `fileSeries` with: the default
 * build when it is of that series, then stable builds, then the newest.
 * Matches `best_in_series` in launch.rs.
 */
export function bestForSeries(
  installed: InstalledBuild[],
  fileSeries: string,
  fallback: InstalledBuild | null,
  defaultBuild: string | null = null,
): InstalledBuild | null {
  const same = sortInstalled(installed.filter((b) => series(b.version) === fileSeries));
  return same.find((b) => b.id === defaultBuild) ?? same.find((b) => b.channel === "stable") ?? same[0] ?? fallback;
}

/** The closest installed version newer than `fileSeries`, which can read the file. */
export function closestNewer(installed: InstalledBuild[], fileSeries: string): InstalledBuild | null {
  const newer = sortInstalled(installed.filter((b) => compareVersions(series(b.version), fileSeries) > 0));
  const oldest = newer[newer.length - 1];
  return oldest ? bestForSeries(newer, series(oldest.version), null) : null;
}
