export function formatBytes(bytes: number, lang: string): string {
  if (!bytes || bytes < 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = bytes;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const digits = v >= 100 || i === 0 ? 0 : 1;
  return `${v.toLocaleString(lang, { maximumFractionDigits: digits })} ${units[i]}`;
}

const DIVISIONS: { amount: number; unit: Intl.RelativeTimeFormatUnit }[] = [
  { amount: 60, unit: "second" },
  { amount: 60, unit: "minute" },
  { amount: 24, unit: "hour" },
  { amount: 7, unit: "day" },
  { amount: 4.34524, unit: "week" },
  { amount: 12, unit: "month" },
  { amount: Number.POSITIVE_INFINITY, unit: "year" },
];

/** "5 minutes ago", localized. `unixSeconds` may be 0 for unknown. */
export function relativeTime(unixSeconds: number, lang: string): string {
  if (!unixSeconds) return "";
  const rtf = new Intl.RelativeTimeFormat(lang, { numeric: "auto", style: "short" });
  let duration = unixSeconds - Date.now() / 1000;
  if (Math.abs(duration) < 45) return rtf.format(0, "second");
  for (const d of DIVISIONS) {
    if (Math.abs(duration) < d.amount) return rtf.format(Math.round(duration), d.unit);
    duration /= d.amount;
  }
  return "";
}

export function formatDate(unixSeconds: number, lang: string): string {
  if (!unixSeconds) return "";
  return new Date(unixSeconds * 1000).toLocaleDateString(lang, {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

export function formatDuration(seconds: number, lang: string): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "";
  if (seconds < 60) return new Intl.NumberFormat(lang, { style: "unit", unit: "second" }).format(Math.ceil(seconds));
  const minutes = Math.ceil(seconds / 60);
  return new Intl.NumberFormat(lang, { style: "unit", unit: "minute" }).format(minutes);
}
