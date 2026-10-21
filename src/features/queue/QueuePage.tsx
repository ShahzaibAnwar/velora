import { useEffect, useMemo, useRef, useState } from "react";
import { Reorder, useDragControls } from "motion/react";
import { ChevronDown, ChevronUp, Clock3, GripVertical, ListOrdered, Pause, Play } from "lucide-react";

import { Button, IconButton } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/feedback";
import { Select } from "@/components/ui/inputs";
import { actions, attempt, useDialogs } from "@/lib/actions";
import { api } from "@/lib/api";
import { FileIcon } from "@/lib/fileTypes";
import { formatBytes, hostOf, relativeTime } from "@/lib/format";
import { matchesSearch, useStore } from "@/lib/store";
import type { DownloadRecord, Priority } from "@/types/engine";

import { DownloadList } from "../downloads/DownloadList";

function QueueRow({
  record,
  index,
  count,
  onMove,
  onDragEnd,
}: {
  record: DownloadRecord;
  index: number;
  count: number;
  onMove: (from: number, to: number) => void;
  onDragEnd: () => void;
}) {
  const controls = useDragControls();
  const select = useStore((s) => s.select);
  const openDialog = useDialogs((s) => s.open);
  return (
    <Reorder.Item
      value={record.id}
      dragListener={false}
      dragControls={controls}
      onDragEnd={onDragEnd}
      className="flex items-center gap-3 rounded-[var(--radius-card)] border border-line bg-card px-3 py-2.5 shadow-sm"
      whileDrag={{ scale: 1.015, boxShadow: "var(--shadow-lg)", zIndex: 5 }}
      transition={{ type: "spring", stiffness: 500, damping: 40 }}
    >
      <button
        className="grid h-9 w-6 cursor-grab touch-none place-items-center rounded-md text-subtle hover:bg-hover hover:text-fg active:cursor-grabbing"
        onPointerDown={(e) => controls.start(e)}
        aria-label="Drag to reorder"
      >
        <GripVertical size={16} />
      </button>
      <span className="w-6 text-center text-[12px] font-semibold text-subtle tabular">{index + 1}</span>
      <FileIcon category={record.category} size={34} />
      <button className="min-w-0 flex-1 text-left" onClick={() => select(record.id)}>
        <p className="truncate text-[13px] font-semibold">{record.fileName}</p>
        <p className="truncate text-[11.5px] text-muted">
          {hostOf(record.url)}
          {record.totalSize ? ` · ${formatBytes(record.totalSize)}` : ""}
          {record.downloaded > 0 ? ` · ${formatBytes(record.downloaded)} done` : ""}
        </p>
      </button>
      <Select<Priority>
        ariaLabel="Priority"
        value={record.priority}
        onValueChange={(p) => actions.setPriority(record.id, p)}
        className="h-8 w-28"
        options={[
          { value: "high", label: "High" },
          { value: "normal", label: "Normal" },
          { value: "low", label: "Low" },
        ]}
      />
      <div className="flex items-center">
        <IconButton label="Move up" size="sm" disabled={index === 0} onClick={() => onMove(index, index - 1)}>
          <ChevronUp size={16} />
        </IconButton>
        <IconButton label="Move down" size="sm" disabled={index === count - 1} onClick={() => onMove(index, index + 1)}>
          <ChevronDown size={16} />
        </IconButton>
        <IconButton label="Schedule" size="sm" onClick={() => openDialog("schedule", record)}>
          <Clock3 size={15} />
        </IconButton>
        <IconButton label="Start now" tone="accent" onClick={() => actions.resume(record.id)}>
          <Play size={16} />
        </IconButton>
      </div>
    </Reorder.Item>
  );
}

export function QueuePage() {
  const downloads = useStore((s) => s.downloads);
  const search = useStore((s) => s.search);
  const queuePaused = useStore((s) => s.queuePaused);
  const settings = useStore((s) => s.settings);
  const setSettings = useStore((s) => s.setSettings);
  const queued = useMemo(
    () => Object.values(downloads).filter((r) => r.status === "queued").sort((a, b) => a.queuePos - b.queuePos || a.createdAt - b.createdAt),
    [downloads],
  );
  const [order, setOrder] = useState<string[]>(queued.map((r) => r.id));
  const orderRef = useRef(order);
  orderRef.current = order;
  useEffect(() => setOrder(queued.map((r) => r.id)), [queued]);

  const commit = (ids: string[]) => attempt("Couldn't reorder the queue", () => api.reorder(ids));
  const move = (from: number, to: number) => {
    const next = [...order];
    const [id] = next.splice(from, 1);
    next.splice(to, 0, id);
    setOrder(next);
    commit(next);
  };
  // Reordering by drag is disabled while a search filter hides part of the queue.
  const visible = order.map((id) => downloads[id]).filter((r): r is DownloadRecord => Boolean(r) && matchesSearch(r, search));
  const scheduled = Object.values(downloads).some((r) => r.status === "scheduled");
  const qw = settings?.queueWindow;

  const setMaxActive = async (v: string) => {
    if (!settings) return;
    const saved = await attempt("Couldn't save", () => api.updateSettings({ ...settings, maxActive: Number(v) }));
    if (saved) setSettings(saved);
  };

  return (
    <div className="scroll-area h-full px-7 pb-8">
      <div className="mb-4 flex flex-wrap items-center gap-3 rounded-[18px] border border-line bg-card px-4 py-3 shadow-sm">
        <span className={queuePaused ? "size-2 rounded-full bg-amber" : "size-2 rounded-full bg-mint"} />
        <div className="min-w-0 flex-1">
          <p className="text-[13px] font-semibold">{queuePaused ? "Queue paused" : "Queue running"}</p>
          <p className="text-[11.5px] text-muted">
            {qw?.enabled
              ? `Starts downloads between ${qw.start} and ${qw.end}${qw.days.length ? " on selected days" : ""}.`
              : "Starts the next download whenever a slot is free. Drag to reorder."}
          </p>
        </div>
        <div className="flex items-center gap-2 text-[12px] text-muted">
          At most
          <Select
            ariaLabel="Maximum simultaneous downloads"
            value={String(settings?.maxActive ?? 3)}
            onValueChange={setMaxActive}
            className="h-8 w-16"
            options={[1, 2, 3, 4, 5, 6, 8, 10].map((n) => ({ value: String(n), label: String(n) }))}
          />
          at once
        </div>
        <Button size="sm" variant={queuePaused ? "primary" : "secondary"} icon={queuePaused ? <Play size={14} /> : <Pause size={14} />} onClick={() => (queuePaused ? actions.resumeAll() : actions.pauseAll())}>
          {queuePaused ? "Resume queue" : "Pause queue"}
        </Button>
      </div>

      {visible.length === 0 ? (
        <EmptyState icon={<ListOrdered size={26} />} title="The queue is empty" body="Choose “Queue” when adding downloads to line them up. They start automatically as slots free up." />
      ) : (
        <Reorder.Group
          axis="y"
          values={visible.map((r) => r.id)}
          onReorder={(ids) => !search && setOrder(ids)}
          className="flex flex-col"
          style={{ gap: "var(--card-gap)" }}
        >
          {visible.map((r, i) => (
            <QueueRow key={r.id} record={r} index={i} count={visible.length} onMove={move} onDragEnd={() => commit(orderRef.current)} />
          ))}
        </Reorder.Group>
      )}

      {scheduled && (
        <div className="mt-8">
          <h2 className="mb-1 text-[13px] font-semibold">Scheduled</h2>
          <p className="mb-3 text-[11.5px] text-muted">Next start {relativeTime(Math.min(...Object.values(downloads).filter((r) => r.status === "scheduled").map((r) => r.scheduledAt ?? Infinity)))}</p>
          <div className="-mx-7 h-auto">
            <DownloadList view="scheduled" />
          </div>
        </div>
      )}
    </div>
  );
}
