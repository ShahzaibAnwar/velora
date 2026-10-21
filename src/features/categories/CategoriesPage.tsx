import { useMemo } from "react";
import { motion } from "motion/react";
import { ArrowLeft, FolderCog } from "lucide-react";

import { Button } from "@/components/ui/button";
import { CATEGORIES, FileIcon, categoryMeta } from "@/lib/fileTypes";
import { formatBytes } from "@/lib/format";
import { useStore } from "@/lib/store";
import type { Category } from "@/types/engine";

import { DownloadList } from "../downloads/DownloadList";

export function CategoriesPage() {
  const downloads = useStore((s) => s.downloads);
  const filter = useStore((s) => s.categoryFilter) as Category | null;
  const setView = useStore((s) => s.setView);
  const settings = useStore((s) => s.settings);

  const summary = useMemo(() => {
    const m = new Map<Category, { count: number; bytes: number; active: number }>();
    for (const c of CATEGORIES) m.set(c, { count: 0, bytes: 0, active: 0 });
    for (const r of Object.values(downloads)) {
      const e = m.get(r.category)!;
      e.count++;
      if (r.status === "completed") e.bytes += r.totalSize ?? r.downloaded;
      if (["downloading", "probing", "verifying"].includes(r.status)) e.active++;
    }
    return m;
  }, [downloads]);

  if (filter) {
    return (
      <div className="flex h-full flex-col">
        <div className="flex items-center gap-3 px-7 pb-3">
          <Button size="sm" variant="ghost" icon={<ArrowLeft size={14} />} onClick={() => setView("categories")}>
            All categories
          </Button>
          <FileIcon category={filter} size={28} />
          <h2 className="text-[14px] font-semibold">{categoryMeta[filter].label}</h2>
        </div>
        <div className="min-h-0 flex-1">
          <DownloadList view="all" category={filter} />
        </div>
      </div>
    );
  }

  const folderFor = (c: Category) =>
    settings?.categoryFolders?.[c] || (settings?.autoCategorize ? `${settings.defaultDir}\\${categoryMeta[c].label}` : settings?.defaultDir);

  return (
    <div className="scroll-area h-full px-7 pb-8">
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-3 xl:grid-cols-4">
        {CATEGORIES.map((c, i) => {
          const s = summary.get(c)!;
          return (
            <motion.button
              key={c}
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: i * 0.03, type: "spring", stiffness: 320, damping: 30 }}
              whileHover={{ y: -2 }}
              onClick={() => setView("categories", c)}
              className="group rounded-[18px] border border-line bg-card p-4 text-left shadow-sm transition-[border-color,box-shadow] hover:border-line-strong hover:shadow-md"
            >
              <div className="flex items-start justify-between">
                <FileIcon category={c} size={42} />
                {s.active > 0 && (
                  <span className="flex items-center gap-1.5 rounded-full bg-[color-mix(in_oklab,var(--accent)_14%,transparent)] px-2 py-0.5 text-[11px] font-semibold text-accent">
                    <span className="live-dot size-1.5 rounded-full bg-accent" /> {s.active} active
                  </span>
                )}
              </div>
              <p className="mt-4 text-[14px] font-semibold">{categoryMeta[c].label}</p>
              <p className="mt-0.5 text-[12px] text-muted tabular">
                {s.count} item{s.count === 1 ? "" : "s"} · {formatBytes(s.bytes)}
              </p>
              <p className="mt-3 truncate font-mono text-[10.5px] text-subtle" title={folderFor(c)}>
                {folderFor(c)}
              </p>
            </motion.button>
          );
        })}
      </div>
      <div className="mt-6 flex items-center gap-3 rounded-2xl border border-dashed border-line px-5 py-4 text-[12.5px] text-muted">
        <FolderCog size={18} className="shrink-0 text-subtle" />
        <p className="flex-1">Files are sorted into categories by extension and type. Choose a folder per category in Settings → Downloads.</p>
        <Button size="sm" onClick={() => setView("settings")}>
          Category folders
        </Button>
      </div>
    </div>
  );
}
