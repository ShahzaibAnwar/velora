import { useMemo, useState } from "react";
import { AnimatePresence } from "motion/react";
import { CheckCircle2, CircleAlert, Clock3, Download, Inbox, Plus, Search, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/feedback";
import { Select } from "@/components/ui/inputs";
import { attempt } from "@/lib/actions";
import { api } from "@/lib/api";
import { categoryMeta } from "@/lib/fileTypes";
import { matchesSearch, matchesView, useStore, type View } from "@/lib/store";
import { toast } from "@/lib/toast";
import type { Category, DownloadRecord } from "@/types/engine";

import { DownloadCard } from "./DownloadCard";

type Sort = "newest" | "oldest" | "name" | "size";

const sorters: Record<Sort, (a: DownloadRecord, b: DownloadRecord) => number> = {
  newest: (a, b) => (b.completedAt ?? b.createdAt) - (a.completedAt ?? a.createdAt),
  oldest: (a, b) => a.createdAt - b.createdAt,
  name: (a, b) => a.fileName.localeCompare(b.fileName),
  size: (a, b) => (b.totalSize ?? 0) - (a.totalSize ?? 0),
};

/** Running first, then everything else by the chosen order. */
function statusRank(r: DownloadRecord) {
  return ["probing", "downloading", "verifying"].includes(r.status) ? 0 : r.status === "paused" ? 1 : 2;
}

export function useFiltered(view: View, category: string | null) {
  const downloads = useStore((s) => s.downloads);
  const search = useStore((s) => s.search);
  return useMemo(
    () => Object.values(downloads).filter((r) => matchesView(r, view) && matchesSearch(r, search) && (!category || r.category === category)),
    [downloads, view, search, category],
  );
}

const empty: Partial<Record<View, { icon: React.ReactNode; title: string; body: string }>> = {
  all: { icon: <Inbox size={26} />, title: "No downloads yet", body: "Paste a link or press Ctrl+N to add your first download." },
  downloading: { icon: <Download size={26} />, title: "Nothing downloading", body: "Active and paused downloads appear here." },
  completed: { icon: <CheckCircle2 size={26} />, title: "No finished downloads", body: "Files you download will be listed here once they're safely on disk." },
  scheduled: { icon: <Clock3 size={26} />, title: "Nothing scheduled", body: "Choose “Schedule” when adding a download to start it later." },
  failed: { icon: <CircleAlert size={26} />, title: "No failed downloads", body: "Downloads that hit a problem show up here with the reason." },
};

export function DownloadList({ view, category }: { view: View; category?: Category | null }) {
  const [sort, setSort] = useState<Sort>("newest");
  const openAdd = useStore((s) => s.openAdd);
  const search = useStore((s) => s.search);
  const items = useFiltered(view, category ?? null);
  const sorted = useMemo(() => [...items].sort((a, b) => statusRank(a) - statusRank(b) || sorters[sort](a, b)), [items, sort]);

  const clear = async (scope: "completed" | "failed") => {
    const ids = await attempt("Couldn't clear the list", () => api.clear(scope));
    if (ids) toast.success(`Removed ${ids.length} item${ids.length === 1 ? "" : "s"}`, "Files on disk were not touched.");
  };

  const e = search
    ? { icon: <Search size={26} />, title: "No matches", body: `Nothing matches “${search}”.` }
    : category
      ? { icon: <Inbox size={26} />, title: `No ${categoryMeta[category].label.toLowerCase()} yet`, body: "Downloads in this category will appear here." }
      : (empty[view] ?? empty.all!);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 px-7 pb-3">
        <p className="text-[12.5px] font-medium text-muted tabular">
          {sorted.length} item{sorted.length === 1 ? "" : "s"}
        </p>
        <div className="ml-auto flex items-center gap-2">
          {view === "completed" && sorted.length > 0 && (
            <Button size="sm" variant="ghost" icon={<Trash2 size={14} />} onClick={() => clear("completed")}>
              Clear list
            </Button>
          )}
          {view === "failed" && sorted.length > 0 && (
            <Button size="sm" variant="ghost" icon={<Trash2 size={14} />} onClick={() => clear("failed")}>
              Clear failed
            </Button>
          )}
          <Select<Sort>
            ariaLabel="Sort"
            value={sort}
            onValueChange={setSort}
            className="h-8 w-36"
            options={[
              { value: "newest", label: "Newest first" },
              { value: "oldest", label: "Oldest first" },
              { value: "name", label: "Name" },
              { value: "size", label: "Size" },
            ]}
          />
        </div>
      </div>
      <div className="scroll-area min-h-0 flex-1 px-7 pb-7">
        {sorted.length === 0 ? (
          <EmptyState
            icon={e.icon}
            title={e.title}
            body={e.body}
            action={
              !search && (view === "all" || view === "downloading") ? (
                <Button variant="primary" icon={<Plus size={16} />} onClick={() => openAdd()}>
                  Add download
                </Button>
              ) : undefined
            }
          />
        ) : (
          <div className="flex flex-col" style={{ gap: "var(--card-gap)" }}>
            <AnimatePresence initial={false} mode="popLayout">
              {sorted.map((r) => (
                <DownloadCard key={r.id} id={r.id} />
              ))}
            </AnimatePresence>
          </div>
        )}
      </div>
    </div>
  );
}
