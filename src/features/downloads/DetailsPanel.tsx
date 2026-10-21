import { useEffect, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertTriangle, Copy, ExternalLink, FolderOpen, Pause, Play, RefreshCw, ShieldAlert, ShieldCheck, X } from "lucide-react";

import { Button, IconButton } from "@/components/ui/button";
import { StatusBadge } from "@/components/ui/feedback";
import { Select } from "@/components/ui/inputs";
import { ProgressBar, SegmentBar, toneFor } from "@/components/ui/progress";
import { SpeedChart } from "@/components/ui/SpeedChart";
import { actions, attempt, useDialogs } from "@/lib/actions";
import { api } from "@/lib/api";
import { FileIcon, categoryMeta } from "@/lib/fileTypes";
import { dateTime, formatBytes, formatEta, formatSpeed, percent } from "@/lib/format";
import { useStore } from "@/lib/store";
import { toast } from "@/lib/toast";
import { isRunning, type DownloadDetails } from "@/types/engine";

export function DetailsPanel() {
  const selectedId = useStore((s) => s.selectedId);
  return (
    <AnimatePresence>
      {selectedId && <Panel key="details" id={selectedId} />}
    </AnimatePresence>
  );
}

function Row({ label, children, mono }: { label: string; children: ReactNode; mono?: boolean }) {
  return (
    <div className="grid grid-cols-[112px_1fr] gap-3 py-2 text-[12.5px]">
      <span className="text-muted">{label}</span>
      <span className={mono ? "break-all whitespace-pre-line font-mono text-[11.5px]" : "break-words"} data-selectable>
        {children}
      </span>
    </div>
  );
}

function Panel({ id }: { id: string }) {
  const record = useStore((s) => s.downloads[id]);
  const progress = useStore((s) => s.progress[id]);
  const select = useStore((s) => s.select);
  const openDialog = useDialogs((s) => s.open);
  const [details, setDetails] = useState<DownloadDetails | null>(null);
  const running = record ? isRunning(record.status) : false;

  useEffect(() => {
    let alive = true;
    const load = () => api.getDetails(id).then((d) => alive && setDetails(d)).catch(() => {});
    load();
    if (!running) return () => void (alive = false);
    const t = window.setInterval(() => !document.hidden && load(), 1000);
    return () => {
      alive = false;
      window.clearInterval(t);
    };
  }, [id, running, record?.status]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && select(null);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [select]);

  if (!record) return null;
  const downloaded = progress?.downloaded ?? record.downloaded;
  const total = progress?.totalSize ?? record.totalSize;
  const pct = record.status === "completed" ? 100 : percent(downloaded, total);
  const segments = details?.segments ?? [];

  return (
    <motion.aside
      initial={{ x: 40, opacity: 0 }}
      animate={{ x: 0, opacity: 1 }}
      exit={{ x: 40, opacity: 0, transition: { duration: 0.18 } }}
      transition={{ type: "spring", stiffness: 380, damping: 36 }}
      className="absolute top-0 right-0 bottom-0 z-30 flex w-[420px] max-w-[92%] flex-col border-l border-line bg-[color-mix(in_oklab,var(--card)_92%,var(--bg))] shadow-lg"
      aria-label="Download details"
    >
      <div className="flex items-start gap-3 border-b border-line px-5 py-4">
        <FileIcon category={record.category} size={44} />
        <div className="min-w-0 flex-1">
          <h2 className="line-clamp-2 text-[14.5px] font-semibold break-all" data-selectable>
            {record.fileName}
          </h2>
          <div className="mt-1.5 flex items-center gap-2">
            <StatusBadge status={record.status} />
            <span className="text-[11.5px] text-muted">{categoryMeta[record.category].label}</span>
          </div>
        </div>
        <IconButton label="Close details" onClick={() => select(null)}>
          <X size={17} />
        </IconButton>
      </div>

      <div className="scroll-area flex-1 px-5 py-4">
        <section className="rounded-2xl border border-line bg-card p-4">
          <div className="flex items-end justify-between">
            <span className="text-[28px] leading-none font-semibold tracking-tight tabular">{pct != null ? `${pct.toFixed(pct < 10 && pct > 0 ? 1 : 0)}%` : "—"}</span>
            <span className="text-[12px] text-muted tabular">
              {formatBytes(downloaded)} / {formatBytes(total)}
            </span>
          </div>
          <ProgressBar value={pct} tone={toneFor(record.status)} className="mt-3" height={8} />
          {running && (
            <div className="mt-3 grid grid-cols-3 gap-2 text-center">
              <Metric label="Speed" value={formatSpeed(progress?.speed)} />
              <Metric label="Time left" value={formatEta(progress?.etaSecs)} />
              <Metric label="Connections" value={String(progress?.connections ?? 0)} />
            </div>
          )}
        </section>

        {record.error && (
          <section className="mt-3 flex gap-2.5 rounded-2xl border border-[color-mix(in_oklab,var(--coral)_35%,transparent)] bg-[color-mix(in_oklab,var(--coral)_9%,transparent)] p-3.5 text-[12.5px]">
            <AlertTriangle size={16} className="mt-px shrink-0 text-coral" />
            <div>
              <p className="font-semibold text-coral">Download stopped</p>
              <p className="mt-0.5 text-fg/85" data-selectable>
                {record.error.message}
              </p>
            </div>
          </section>
        )}

        {details && details.speedHistory.length > 1 && (
          <section className="mt-4">
            <h3 className="mb-2 text-[11.5px] font-semibold tracking-wider text-subtle uppercase">Speed history</h3>
            <div className="rounded-2xl border border-line bg-card px-2 pt-2">
              <SpeedChart samples={details.speedHistory} height={80} slots={60} />
            </div>
          </section>
        )}

        {segments.length > 0 && total ? (
          <section className="mt-4">
            <h3 className="mb-2 flex items-center justify-between text-[11.5px] font-semibold tracking-wider text-subtle uppercase">
              <span>Segments</span>
              <span className="normal-case tracking-normal text-muted">
                {segments.length} part{segments.length === 1 ? "" : "s"} · {segments.filter((s) => s.active).length} active
              </span>
            </h3>
            <SegmentBar segments={segments} total={total} />
          </section>
        ) : null}

        <section className="mt-4 divide-y divide-line">
          <Row label="Source" mono>
            {record.url}
          </Row>
          {record.finalUrl && record.finalUrl !== record.url && (
            <Row label="Resolved to" mono>
              {record.finalUrl}
            </Row>
          )}
          <Row label="Saved to" mono>
            {record.destDir}
          </Row>
          <Row label="Size">{record.totalSize != null ? `${formatBytes(record.totalSize)} (${record.totalSize.toLocaleString()} bytes)` : "Unknown"}</Row>
          {record.mime && <Row label="Type">{record.mime}</Row>}
          <Row label="Resume">
            {record.resumable ? (
              <span className="text-mint">Supported — verified with a range request</span>
            ) : record.totalSize != null || record.status !== "queued" ? (
              <span className="text-amber">Not supported by this server — pausing restarts the download</span>
            ) : (
              "Checked when the download starts"
            )}
          </Row>
          <Row label="Connections">
            <Select
              ariaLabel="Connections"
              value={String(record.connections)}
              onValueChange={(v) => attempt("Couldn't change connections", () => api.setConnections(id, Number(v)))}
              className="h-8 w-28"
              options={[1, 2, 4, 6, 8, 12, 16].map((n) => ({ value: String(n), label: String(n) }))}
            />
            <span className="mt-1 block text-[11px] text-subtle">Applies the next time the download starts.</span>
          </Row>
          <Row label="Speed limit">
            <button className="font-semibold text-accent hover:underline" onClick={() => openDialog("limit", record)}>
              {record.speedLimit ? `${formatSpeed(record.speedLimit)}` : "Unlimited"}
            </button>
          </Row>
          <Row label="Priority">{record.priority[0].toUpperCase() + record.priority.slice(1)}</Row>
          <Row label="Added">{dateTime(record.createdAt)}</Row>
          {record.startedAt && <Row label="Started">{dateTime(record.startedAt)}</Row>}
          {record.completedAt && <Row label="Completed">{dateTime(record.completedAt)}</Row>}
          {record.scheduledAt && <Row label="Scheduled for">{dateTime(record.scheduledAt)}</Row>}
          {(record.etag || record.lastModified) && (
            <Row label="Validators" mono>
              {[record.etag && `ETag ${record.etag}`, record.lastModified && `Last-Modified ${record.lastModified}`].filter(Boolean).join("\n")}
            </Row>
          )}
          <Row label="SHA-256">
            {record.checksumState === "verified" && (
              <span className="inline-flex items-center gap-1.5 text-mint">
                <ShieldCheck size={14} /> Verified
              </span>
            )}
            {record.checksumState === "mismatch" && (
              <span className="inline-flex items-center gap-1.5 text-coral">
                <ShieldAlert size={14} /> Doesn't match — the file may be corrupted or different
              </span>
            )}
            {record.checksumState === "pending" && "Verifying…"}
            {record.checksumState === "none" && (record.status === "completed" ? (
              <button className="font-semibold text-accent hover:underline" onClick={() => openDialog("checksum", record)}>
                Compute or verify…
              </button>
            ) : (
              "Not provided"
            ))}
            {record.checksum && <span className="mt-1 block font-mono text-[11px] break-all text-subtle">{record.checksum}</span>}
          </Row>
        </section>
      </div>

      <div className="flex flex-wrap items-center gap-2 border-t border-line px-5 py-3.5">
        {running && (
          <Button size="sm" icon={<Pause size={14} />} onClick={() => actions.pause(id)}>
            Pause
          </Button>
        )}
        {(record.status === "paused" || record.status === "queued" || record.status === "scheduled") && (
          <Button size="sm" variant="primary" icon={<Play size={14} />} onClick={() => actions.resume(id)}>
            {record.status === "paused" ? "Resume" : "Start now"}
          </Button>
        )}
        {(record.status === "failed" || record.status === "cancelled") && (
          <Button size="sm" variant="primary" icon={<RefreshCw size={14} />} onClick={() => actions.resume(id)}>
            Retry
          </Button>
        )}
        {record.error?.code === "remote_changed" && (
          <Button size="sm" icon={<RefreshCw size={14} />} onClick={() => actions.restart(id)}>
            Restart download
          </Button>
        )}
        {record.status === "completed" && (
          <Button size="sm" variant="primary" icon={<ExternalLink size={14} />} onClick={() => actions.openFile(id)}>
            Open
          </Button>
        )}
        <Button size="sm" icon={<FolderOpen size={14} />} onClick={() => actions.showInFolder(id)}>
          Folder
        </Button>
        <Button
          size="sm"
          variant="ghost"
          icon={<Copy size={14} />}
          onClick={async () => {
            await actions.copyUrl(record);
          }}
        >
          Copy link
        </Button>
        {details?.partPath && record.status !== "completed" && (
          <button
            className="ml-auto text-[11px] text-subtle hover:text-muted"
            title={details.partPath}
            onClick={() => attempt("Couldn't copy", () => api.setClipboard(details.partPath)).then(() => toast.success("Path copied"))}
          >
            Partial file path
          </button>
        )}
      </div>
    </motion.aside>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-xl bg-hover px-2 py-2">
      <p className="text-[13px] font-semibold tabular">{value}</p>
      <p className="text-[10.5px] text-muted">{label}</p>
    </div>
  );
}
