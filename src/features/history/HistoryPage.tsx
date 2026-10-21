import { useMemo, useState } from "react";
import { motion } from "motion/react";
import { CheckCircle2, CircleSlash, Download, History as HistoryIcon, Trash2, Upload, XCircle } from "lucide-react";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/feedback";
import { Dialog } from "@/components/ui/overlay";
import { Segmented } from "@/components/ui/inputs";
import { attempt } from "@/lib/actions";
import { api, isTauri, pickOpenFile, pickSaveFile } from "@/lib/api";
import { FileIcon } from "@/lib/fileTypes";
import { dayLabel, formatBytes, hostOf } from "@/lib/format";
import { matchesSearch, useStore } from "@/lib/store";
import { toast } from "@/lib/toast";
import type { DownloadRecord } from "@/types/engine";

type Filter = "all" | "completed" | "failed";

export function HistoryPage() {
  const downloads = useStore((s) => s.downloads);
  const search = useStore((s) => s.search);
  const select = useStore((s) => s.select);
  const [filter, setFilter] = useState<Filter>("all");
  const [confirm, setConfirm] = useState(false);

  const groups = useMemo(() => {
    const items = Object.values(downloads)
      .filter((r) => ["completed", "failed", "cancelled"].includes(r.status))
      .filter((r) => filter === "all" || (filter === "completed" ? r.status === "completed" : r.status !== "completed"))
      .filter((r) => matchesSearch(r, search))
      .sort((a, b) => (b.completedAt ?? b.createdAt) - (a.completedAt ?? a.createdAt));
    const map = new Map<string, DownloadRecord[]>();
    for (const r of items) {
      const label = dayLabel(r.completedAt ?? r.createdAt);
      map.set(label, [...(map.get(label) ?? []), r]);
    }
    return [...map.entries()];
  }, [downloads, filter, search]);

  const exportList = async () => {
    const path = await pickSaveFile(`velora-downloads-${new Date().toISOString().slice(0, 10)}.json`);
    if (!path) return;
    const ok = await attempt("Export failed", () => api.exportDownloads(path));
    if (ok !== undefined) toast.success("Download list exported", path);
  };

  const importList = async () => {
    const path = await pickOpenFile();
    if (!path) return;
    const n = await attempt("Import failed", () => api.importDownloads(path));
    if (n !== undefined) toast.success(`Imported ${n} item${n === 1 ? "" : "s"}`, "Unfinished downloads were added paused.");
  };

  const clearHistory = async () => {
    const ids = await attempt("Couldn't clear history", () => api.clear("finished"));
    if (ids) toast.success(`Cleared ${ids.length} entr${ids.length === 1 ? "y" : "ies"}`, "Downloaded files were not touched.");
    setConfirm(false);
  };

  return (
    <div className="scroll-area h-full px-7 pb-8">
      <div className="mb-5 flex flex-wrap items-center gap-2">
        <Segmented<Filter>
          value={filter}
          onValueChange={setFilter}
          options={[
            { value: "all", label: "All" },
            { value: "completed", label: "Completed" },
            { value: "failed", label: "Failed & cancelled" },
          ]}
        />
        <div className="ml-auto flex gap-2">
          {isTauri && (
            <>
              <Button size="sm" variant="ghost" icon={<Upload size={14} />} onClick={importList}>
                Import
              </Button>
              <Button size="sm" variant="ghost" icon={<Download size={14} />} onClick={exportList}>
                Export
              </Button>
            </>
          )}
          <Button size="sm" variant="ghost" icon={<Trash2 size={14} />} onClick={() => setConfirm(true)} disabled={groups.length === 0}>
            Clear history
          </Button>
        </div>
      </div>

      {groups.length === 0 ? (
        <EmptyState icon={<HistoryIcon size={26} />} title="No history yet" body="Finished, failed and cancelled downloads are kept here so you can find them later." />
      ) : (
        groups.map(([day, items], gi) => (
          <motion.section key={day} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: Math.min(gi * 0.04, 0.2) }} className="mb-6">
            <h2 className="sticky top-0 z-10 mb-2 bg-bg py-1 text-[12px] font-semibold tracking-wider text-subtle uppercase">{day}</h2>
            <div className="relative ml-2 border-l border-line pl-5">
              {items.map((r) => {
                const ok = r.status === "completed";
                const Icon = ok ? CheckCircle2 : r.status === "failed" ? XCircle : CircleSlash;
                return (
                  <button key={r.id} onClick={() => select(r.id)} className="group relative flex w-full items-center gap-3 rounded-xl px-2 py-2 text-left transition hover:bg-hover">
                    <span className="absolute top-1/2 -left-[27px] grid size-3.5 -translate-y-1/2 place-items-center rounded-full bg-bg">
                      <Icon size={14} className={ok ? "text-mint" : r.status === "failed" ? "text-coral" : "text-subtle"} />
                    </span>
                    <FileIcon category={r.category} size={32} />
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-[13px] font-semibold">{r.fileName}</p>
                      <p className="truncate text-[11.5px] text-muted">
                        {hostOf(r.url)} · {ok ? formatBytes(r.totalSize ?? r.downloaded) : (r.error?.message ?? "Cancelled")}
                      </p>
                    </div>
                    <span className="shrink-0 text-[11.5px] text-subtle tabular">
                      {new Date(r.completedAt ?? r.createdAt).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}
                    </span>
                  </button>
                );
              })}
            </div>
          </motion.section>
        ))
      )}

      <Dialog
        open={confirm}
        onOpenChange={setConfirm}
        title="Clear download history?"
        description="Finished, failed and cancelled entries are removed from Velora. Files you downloaded stay on your disk."
        width={460}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirm(false)}>
              Cancel
            </Button>
            <Button variant="danger" onClick={clearHistory}>
              Clear history
            </Button>
          </>
        }
      >
        <span />
      </Dialog>
    </div>
  );
}
