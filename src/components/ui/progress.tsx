import { motion } from "motion/react";

import { cn } from "@/lib/cn";
import type { DownloadStatus, SegmentView } from "@/types/engine";

export type ProgressTone = "active" | "done" | "paused" | "error" | "idle";

export function toneFor(status: DownloadStatus): ProgressTone {
  switch (status) {
    case "downloading":
    case "probing":
    case "verifying":
      return "active";
    case "completed":
      return "done";
    case "paused":
    case "queued":
    case "scheduled":
      return "paused";
    case "failed":
    case "cancelled":
      return "error";
  }
}

const fills: Record<ProgressTone, string> = {
  active: "linear-gradient(90deg, var(--accent), var(--accent-2))",
  done: "linear-gradient(90deg, var(--mint), color-mix(in oklab, var(--mint) 70%, var(--cyan)))",
  paused: "color-mix(in oklab, var(--amber) 75%, transparent)",
  error: "var(--coral)",
  idle: "color-mix(in oklab, var(--muted) 45%, transparent)",
};

/**
 * Progress bar. `value` null means unknown size: an indeterminate sweep is shown while active.
 * Width changes are tweened so 2 Hz progress updates read as continuous motion.
 */
export function ProgressBar({ value, tone, className, height = 6 }: { value: number | null; tone: ProgressTone; className?: string; height?: number }) {
  const indeterminate = value == null && tone === "active";
  return (
    <div
      className={cn("relative w-full overflow-hidden rounded-full bg-track", className)}
      style={{ height }}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value == null ? undefined : Math.round(value)}
    >
      {indeterminate ? (
        <div className="progress-shimmer absolute inset-0 rounded-full" style={{ background: "color-mix(in oklab, var(--accent) 25%, transparent)" }} />
      ) : (
        <motion.div
          className={cn("relative h-full overflow-hidden rounded-full", tone === "active" && "progress-shimmer")}
          initial={false}
          animate={{ width: `${value ?? (tone === "done" ? 100 : 0)}%`, background: fills[tone] }}
          transition={{ width: { type: "tween", ease: "linear", duration: 0.5 }, background: { duration: 0.6 } }}
        />
      )}
    </div>
  );
}

/** Visual map of the parallel byte ranges, drawn from real engine segment data. */
export function SegmentBar({ segments, total, height = 14 }: { segments: SegmentView[]; total: number; height?: number }) {
  if (!segments.length || total <= 0) return null;
  return (
    <div className="relative flex w-full gap-[2px] overflow-hidden rounded-md" style={{ height }}>
      {segments.map((s) => {
        const len = s.end - s.start + 1;
        const frac = Math.min(1, s.downloaded / len);
        return (
          <div key={s.start} className="relative h-full overflow-hidden rounded-[3px] bg-track" style={{ flexGrow: len, flexBasis: 0 }}>
            <motion.div
              className="h-full"
              initial={false}
              animate={{ width: `${frac * 100}%` }}
              transition={{ type: "tween", ease: "linear", duration: 0.5 }}
              style={{
                background: frac >= 1 ? "var(--mint)" : "linear-gradient(90deg, var(--accent), var(--accent-2))",
                boxShadow: s.active && frac < 1 ? "0 0 10px color-mix(in oklab, var(--accent) 60%, transparent)" : undefined,
              }}
            />
          </div>
        );
      })}
    </div>
  );
}
