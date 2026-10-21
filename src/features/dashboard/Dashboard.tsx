import { useEffect, useMemo, type ReactNode } from "react";
import { motion } from "motion/react";
import { Activity, ArrowRight, CheckCircle2, Gauge, HardDriveDownload, ListOrdered, Plus, Sparkles } from "lucide-react";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/feedback";
import { SpeedChart } from "@/components/ui/SpeedChart";
import { api } from "@/lib/api";
import { formatBytes, formatSpeed } from "@/lib/format";
import { useStore, type View } from "@/lib/store";
import { isRunning } from "@/types/engine";

import { DownloadCard } from "../downloads/DownloadCard";

function StatCard({ icon, label, value, sub, color, index }: { icon: ReactNode; label: string; value: string; sub?: string; color: string; index: number }) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: index * 0.04, type: "spring", stiffness: 300, damping: 30 }}
      className="relative overflow-hidden rounded-[18px] border border-line bg-card p-4 shadow-sm"
    >
      {/* A gradient rather than a CSS blur filter: same look, no extra compositing layer. */}
      <div
        className="pointer-events-none absolute -top-12 -right-12 size-36 rounded-full"
        style={{ background: `radial-gradient(circle, color-mix(in oklab, ${color} 16%, transparent), transparent 70%)` }}
      />
      <div className="relative flex items-center gap-2.5 text-[12px] font-semibold text-muted">
        <span className="grid size-7 place-items-center rounded-lg" style={{ color, background: `color-mix(in oklab, ${color} 14%, transparent)` }}>
          {icon}
        </span>
        {label}
      </div>
      <p className="relative mt-3 text-[24px] leading-none font-semibold tracking-tight tabular">{value}</p>
      {sub && <p className="relative mt-1.5 text-[11.5px] text-subtle tabular">{sub}</p>}
    </motion.div>
  );
}

function Section({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return (
    <section className="mt-7">
      <div className="mb-3 flex items-center justify-between">
        <h2 className="text-[13px] font-semibold tracking-[-0.005em]">{title}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}

function SeeAll({ view }: { view: View }) {
  const setView = useStore((s) => s.setView);
  return (
    <button onClick={() => setView(view)} className="flex items-center gap-1 text-[12px] font-semibold text-muted transition hover:text-accent">
      See all <ArrowRight size={13} />
    </button>
  );
}

export function Dashboard() {
  const stats = useStore((s) => s.stats);
  const setStats = useStore((s) => s.setStats);
  const downloads = useStore((s) => s.downloads);
  const totalSpeed = useStore((s) => s.totalSpeed);
  const openAdd = useStore((s) => s.openAdd);

  // Stats (including the per-second speed history) come from the engine. Poll every second while
  // something is moving, slowly when idle, never while hidden, and only re-render on real changes.
  const busy = totalSpeed > 0 || Object.values(downloads).some((r) => isRunning(r.status));
  useEffect(() => {
    let alive = true;
    let last = "";
    const load = () =>
      !document.hidden &&
      api
        .getStats()
        .then((s) => {
          const key = JSON.stringify(s);
          if (alive && key !== last) {
            last = key;
            setStats(s);
          }
        })
        .catch(() => {});
    load();
    const t = window.setInterval(load, busy ? 1000 : 5000);
    return () => {
      alive = false;
      window.clearInterval(t);
    };
  }, [setStats, busy]);

  const { active, upcoming, recent } = useMemo(() => {
    const all = Object.values(downloads);
    return {
      active: all.filter((r) => isRunning(r.status)).sort((a, b) => a.createdAt - b.createdAt),
      upcoming: all
        .filter((r) => r.status === "queued" || r.status === "scheduled")
        .sort((a, b) => (a.status === b.status ? a.queuePos - b.queuePos : a.status === "queued" ? -1 : 1))
        .slice(0, 4),
      recent: all
        .filter((r) => r.status === "completed")
        .sort((a, b) => (b.completedAt ?? 0) - (a.completedAt ?? 0))
        .slice(0, 4),
    };
  }, [downloads]);

  const empty = Object.keys(downloads).length === 0;

  return (
    <div className="scroll-area h-full px-7 pb-8">
      <div className="grid grid-cols-2 gap-3 xl:grid-cols-4">
        <StatCard index={0} icon={<Activity size={15} />} label="Active downloads" value={String(stats?.active ?? active.length)} sub={`${stats?.queued ?? 0} waiting in queue`} color="var(--accent)" />
        <StatCard
          index={1}
          icon={<Gauge size={15} />}
          label="Current speed"
          value={formatSpeed(totalSpeed)}
          sub={stats?.averageSpeed ? `${formatSpeed(stats.averageSpeed)} average (last minute)` : stats?.effectiveSpeedLimit ? `Limited to ${formatSpeed(stats.effectiveSpeedLimit)}` : "No active transfer"}
          color="var(--violet)"
        />
        <StatCard index={2} icon={<CheckCircle2 size={15} />} label="Finished today" value={String(stats?.completedToday ?? 0)} sub={formatBytes(stats?.downloadedToday ?? 0)} color="var(--mint)" />
        <StatCard index={3} icon={<HardDriveDownload size={15} />} label="Total downloaded" value={formatBytes(stats?.totalDownloaded ?? 0)} sub={`${stats?.totalCompleted ?? 0} files`} color="var(--coral)" />
      </div>

      <motion.section
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.12 }}
        className="mt-4 overflow-hidden rounded-[18px] border border-line bg-card shadow-sm"
      >
        <div className="flex items-center justify-between px-5 pt-4">
          <div>
            <h2 className="text-[13px] font-semibold">Download speed</h2>
            <p className="text-[11.5px] text-muted">Last 90 seconds · measured by the engine</p>
          </div>
          <p className="text-[20px] font-semibold tracking-tight text-gradient tabular">{formatSpeed(totalSpeed)}</p>
        </div>
        <div className="px-2 pt-2">
          <SpeedChart samples={stats?.speedHistory ?? []} height={150} />
        </div>
      </motion.section>

      {empty ? (
        <EmptyState
          icon={<Sparkles size={26} />}
          title="Ready when you are"
          body="Copy a download link and press Ctrl+V, or add one manually. Velora splits large files across several connections when the server allows it."
          action={
            <Button variant="primary" icon={<Plus size={16} />} onClick={() => openAdd()}>
              Add your first download
            </Button>
          }
        />
      ) : (
        <>
          <Section title={`Active downloads${active.length ? ` · ${active.length}` : ""}`} action={active.length > 0 ? <SeeAll view="downloading" /> : undefined}>
            {active.length ? (
              <div className="flex flex-col" style={{ gap: "var(--card-gap)" }}>
                {active.map((r) => (
                  <DownloadCard key={r.id} id={r.id} />
                ))}
              </div>
            ) : (
              <p className="rounded-2xl border border-dashed border-line px-5 py-6 text-center text-[12.5px] text-muted">Nothing is downloading right now.</p>
            )}
          </Section>

          <div className={upcoming.length > 0 && recent.length > 0 ? "grid gap-x-6 xl:grid-cols-2" : "grid"}>
            {upcoming.length > 0 && (
              <Section title="Up next" action={<SeeAll view="queued" />}>
                <div className="flex flex-col" style={{ gap: "var(--card-gap)" }}>
                  {upcoming.map((r, i) => (
                    <DownloadCard key={r.id} id={r.id} queueIndex={r.status === "queued" ? i : undefined} />
                  ))}
                </div>
              </Section>
            )}
            {recent.length > 0 && (
              <Section title="Recently completed" action={<SeeAll view="completed" />}>
                <div className="flex flex-col" style={{ gap: "var(--card-gap)" }}>
                  {recent.map((r) => (
                    <DownloadCard key={r.id} id={r.id} />
                  ))}
                </div>
              </Section>
            )}
          </div>
          {upcoming.length === 0 && recent.length === 0 && active.length === 0 && (
            <p className="mt-6 flex items-center justify-center gap-2 text-[12.5px] text-muted">
              <ListOrdered size={15} /> Your queue is empty.
            </p>
          )}
        </>
      )}
    </div>
  );
}
