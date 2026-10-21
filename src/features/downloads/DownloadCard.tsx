import { memo, useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Check, ExternalLink, FolderOpen, Pause, Play, RefreshCw, ShieldAlert, ShieldCheck } from "lucide-react";

import { IconButton } from "@/components/ui/button";
import { StatusBadge } from "@/components/ui/feedback";
import { ProgressBar, toneFor } from "@/components/ui/progress";
import { Tooltip } from "@/components/ui/overlay";
import { actions } from "@/lib/actions";
import { cn } from "@/lib/cn";
import { FileIcon } from "@/lib/fileTypes";
import { formatBytes, formatEta, formatSpeed, hostOf, percent, relativeTime } from "@/lib/format";
import { useStore } from "@/lib/store";
import { isRunning } from "@/types/engine";

import { DownloadMenu } from "./DownloadMenu";

const CELEBRATE_MS = 2600;

export const DownloadCard = memo(function DownloadCard({ id, queueIndex }: { id: string; queueIndex?: number }) {
  const record = useStore((s) => s.downloads[id]);
  const progress = useStore((s) => s.progress[id]);
  const selected = useStore((s) => s.selectedId === id);
  const completedAt = useStore((s) => s.justCompleted[id]);
  const select = useStore((s) => s.select);
  const [celebrate, setCelebrate] = useState(false);

  useEffect(() => {
    if (!completedAt || Date.now() - completedAt > CELEBRATE_MS) return;
    setCelebrate(true);
    const t = window.setTimeout(() => setCelebrate(false), CELEBRATE_MS);
    return () => window.clearTimeout(t);
  }, [completedAt]);

  if (!record) return null;
  const running = isRunning(record.status);
  const downloaded = progress?.downloaded ?? record.downloaded;
  const total = progress?.totalSize ?? record.totalSize;
  const pct = record.status === "completed" ? 100 : percent(downloaded, total);
  const tone = toneFor(record.status);

  let meta: string;
  switch (record.status) {
    case "completed":
      meta = `${formatBytes(record.totalSize ?? record.downloaded)} · ${relativeTime(record.completedAt)}`;
      break;
    case "scheduled":
      meta = `Starts ${relativeTime(record.scheduledAt)}`;
      break;
    case "queued":
      meta = queueIndex != null ? `#${queueIndex + 1} in queue` : "Waiting in queue";
      break;
    case "failed":
    case "cancelled":
      meta = record.error?.message ?? (record.status === "cancelled" ? "Cancelled" : "Failed");
      break;
    case "probing":
      meta = "Contacting server…";
      break;
    default:
      meta = total ? `${formatBytes(downloaded)} of ${formatBytes(total)}` : downloaded ? formatBytes(downloaded) : "Size unknown";
  }

  return (
    <motion.div
      layout="position"
      initial={{ opacity: 0, y: 10, scale: 0.99 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, scale: 0.98, transition: { duration: 0.16 } }}
      transition={{ type: "spring", stiffness: 380, damping: 34 }}
      onClick={() => select(selected ? null : id)}
      onDoubleClick={() => record.status === "completed" && actions.openFile(id)}
      className={cn(
        "group relative overflow-hidden rounded-[var(--radius-card)] border bg-card transition-[border-color,box-shadow,background] duration-200",
        selected ? "border-[color-mix(in_oklab,var(--accent)_55%,transparent)] shadow-[var(--glow)]" : "border-line hover:border-line-strong hover:shadow-md",
      )}
      style={{ padding: "var(--row-pad) calc(var(--row-pad) + 2px)" }}
      data-testid="download-card"
    >
      <AnimatePresence>
        {celebrate && (
          <motion.div
            className="pointer-events-none absolute inset-0"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0, transition: { duration: 0.8 } }}
            style={{ background: "radial-gradient(120% 140% at 0% 50%, color-mix(in oklab, var(--mint) 16%, transparent), transparent 60%)" }}
          />
        )}
      </AnimatePresence>

      <div className="relative flex items-center gap-3.5">
        <div className="relative">
          <FileIcon category={record.category} />
          <AnimatePresence>
            {celebrate && (
              <motion.span
                className="absolute -right-1.5 -bottom-1.5 grid size-5 place-items-center rounded-full bg-mint text-[var(--bg)] shadow-md"
                initial={{ scale: 0, rotate: -40 }}
                animate={{ scale: 1, rotate: 0 }}
                exit={{ scale: 0 }}
                transition={{ type: "spring", stiffness: 500, damping: 18 }}
              >
                <Check size={12} strokeWidth={3.2} />
              </motion.span>
            )}
          </AnimatePresence>
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <p className="truncate text-[13.5px] font-semibold tracking-[-0.005em]" title={record.fileName}>
              {record.fileName}
            </p>
            {record.priority === "high" && !["completed", "failed", "cancelled"].includes(record.status) && (
              <span className="rounded-md bg-[color-mix(in_oklab,var(--coral)_14%,transparent)] px-1.5 text-[10.5px] font-bold text-coral">HIGH</span>
            )}
            {record.checksumState === "verified" && (
              <Tooltip content="SHA-256 matches">
                <ShieldCheck size={14} className="shrink-0 text-mint" />
              </Tooltip>
            )}
            {record.checksumState === "mismatch" && (
              <Tooltip content="SHA-256 does not match the expected value">
                <ShieldAlert size={14} className="shrink-0 text-coral" />
              </Tooltip>
            )}
          </div>
          <div className="mt-0.5 flex min-w-0 items-center gap-2 text-[12px] text-muted">
            <span className="shrink-0">{hostOf(record.finalUrl ?? record.url)}</span>
            <span className="text-subtle">·</span>
            <span className={cn("truncate tabular", (record.status === "failed" || record.status === "cancelled") && "text-coral")}>{meta}</span>
          </div>
        </div>

        <div className="hidden shrink-0 flex-col items-end gap-0.5 text-right sm:flex">
          {running && record.status !== "probing" ? (
            <>
              <span className="text-[13px] font-semibold tabular">{formatSpeed(progress?.speed)}</span>
              <span className="text-[11.5px] text-muted tabular">
                {pct != null ? `${pct.toFixed(pct < 10 ? 1 : 0)}%` : "—"} · {formatEta(progress?.etaSecs)}
              </span>
            </>
          ) : (
            <StatusBadge status={record.status} />
          )}
        </div>

        <div className="flex shrink-0 items-center gap-0.5" onClick={(e) => e.stopPropagation()}>
          {running && (
            <IconButton label="Pause" onClick={() => actions.pause(id)}>
              <Pause size={17} />
            </IconButton>
          )}
          {(record.status === "paused" || record.status === "queued" || record.status === "scheduled") && (
            <IconButton label={record.status === "paused" ? "Resume" : "Start now"} tone="accent" onClick={() => actions.resume(id)}>
              <Play size={17} />
            </IconButton>
          )}
          {(record.status === "failed" || record.status === "cancelled") && (
            <IconButton label="Retry" tone="accent" onClick={() => actions.resume(id)}>
              <RefreshCw size={16} />
            </IconButton>
          )}
          {record.status === "completed" && (
            <>
              <IconButton label="Open file" onClick={() => actions.openFile(id)}>
                <ExternalLink size={16} />
              </IconButton>
              <IconButton label="Show in folder" onClick={() => actions.showInFolder(id)}>
                <FolderOpen size={16} />
              </IconButton>
            </>
          )}
          <DownloadMenu record={record} />
        </div>
      </div>

      {record.status !== "completed" ? (
        <div className="relative mt-3 flex items-center gap-3">
          <ProgressBar value={pct} tone={tone} />
          {running && progress && progress.connections > 0 && (
            <Tooltip content={`${progress.connections} active connection${progress.connections === 1 ? "" : "s"}`}>
              <span className="shrink-0 text-[11px] font-semibold text-subtle tabular">{progress.connections}×</span>
            </Tooltip>
          )}
        </div>
      ) : (
        celebrate && (
          <motion.div className="relative mt-3" initial={{ opacity: 1 }} animate={{ opacity: 0 }} transition={{ delay: 1.6, duration: 0.8 }}>
            <ProgressBar value={100} tone="done" />
          </motion.div>
        )
      )}
    </motion.div>
  );
});
