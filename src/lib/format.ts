const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(n: number | null | undefined, digits = 1): string {
  if (n == null || !Number.isFinite(n)) return "—";
  if (n < 1024) return `${Math.round(n)} B`;
  let v = n;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 ? 0 : digits)} ${UNITS[i]}`;
}

export function formatSpeed(bps: number | null | undefined): string {
  if (!bps) return "0 B/s";
  return `${formatBytes(bps)}/s`;
}

export function formatEta(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(secs)) return "—";
  if (secs < 1) return "<1s";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 99) return "a long time";
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s.toString().padStart(2, "0")}s`;
  return `${s}s`;
}

export function percent(done: number, total: number | null | undefined): number | null {
  if (!total) return null;
  return Math.max(0, Math.min(100, (done / total) * 100));
}

export function hostOf(url: string): string {
  if (/^magnet:/i.test(url)) return "BitTorrent";
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

const rtf = typeof Intl !== "undefined" ? new Intl.RelativeTimeFormat(undefined, { numeric: "auto" }) : null;

export function relativeTime(ms: number | null | undefined, now = Date.now()): string {
  if (!ms) return "—";
  const diff = (ms - now) / 1000;
  const abs = Math.abs(diff);
  if (!rtf) return new Date(ms).toLocaleString();
  if (abs < 45) return diff < 0 ? "just now" : "in a moment";
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour");
  if (abs < 86400 * 7) return rtf.format(Math.round(diff / 86400), "day");
  return new Date(ms).toLocaleDateString();
}

export function dateTime(ms: number | null | undefined): string {
  if (!ms) return "—";
  return new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export function dayLabel(ms: number, now = new Date()): string {
  const d = new Date(ms);
  const start = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round((start(now) - start(d)) / 86400000);
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  return d.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" });
}

/** Parse "2.5 MB/s", "500k", "1m" style input into bytes per second; empty → 0 (unlimited). */
export function parseRate(input: string): number | null {
  const s = input.trim().toLowerCase().replace(/\/s$/, "").replace(/ps$/, "");
  if (!s) return 0;
  const m = s.match(/^(\d+(?:\.\d+)?)\s*([kmg]?)b?$/);
  if (!m) return null;
  const mult = { "": 1, k: 1024, m: 1024 ** 2, g: 1024 ** 3 }[m[2] as "" | "k" | "m" | "g"];
  return Math.round(parseFloat(m[1]) * mult);
}
