import { type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertTriangle, CheckCircle2, Info, Loader2, X } from "lucide-react";

import { cn } from "@/lib/cn";
import { useToasts } from "@/lib/toast";
import type { DownloadRecord } from "@/types/engine";

export function Spinner({ size = 16, className }: { size?: number; className?: string }) {
  return <Loader2 size={size} className={cn("spin text-accent", className)} aria-label="Loading" />;
}

export function Toaster() {
  const { toasts, dismiss } = useToasts();
  return (
    <div className="pointer-events-none fixed right-5 bottom-5 z-[100] flex w-[360px] flex-col gap-2.5">
      <AnimatePresence initial={false}>
        {toasts.map((t) => {
          const Icon = t.tone === "success" ? CheckCircle2 : t.tone === "error" ? AlertTriangle : Info;
          const color = t.tone === "success" ? "var(--mint)" : t.tone === "error" ? "var(--coral)" : "var(--accent)";
          return (
            <motion.div
              key={t.id}
              layout
              initial={{ opacity: 0, y: 16, scale: 0.97 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, x: 40, transition: { duration: 0.18 } }}
              transition={{ type: "spring", stiffness: 420, damping: 32 }}
              className="pointer-events-auto flex gap-3 rounded-2xl border border-line-strong bg-raised p-3.5 shadow-lg"
              role="status"
            >
              <Icon size={18} style={{ color }} className="mt-px shrink-0" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-[13px] font-semibold">{t.title}</p>
                {t.description && <p className="mt-0.5 line-clamp-2 break-all text-[12px] text-muted">{t.description}</p>}
                {t.action && (
                  <button
                    onClick={() => {
                      t.action!.run();
                      dismiss(t.id);
                    }}
                    className="mt-2 rounded-lg bg-[color-mix(in_oklab,var(--accent)_16%,transparent)] px-2.5 py-1 text-[12px] font-semibold text-accent transition hover:bg-[color-mix(in_oklab,var(--accent)_24%,transparent)]"
                  >
                    {t.action.label}
                  </button>
                )}
              </div>
              <button onClick={() => dismiss(t.id)} className="grid size-6 place-items-center rounded-md text-subtle hover:text-fg" aria-label="Dismiss">
                <X size={14} />
              </button>
            </motion.div>
          );
        })}
      </AnimatePresence>
    </div>
  );
}

export function EmptyState({ icon, title, body, action }: { icon: ReactNode; title: string; body: ReactNode; action?: ReactNode }) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.25 }}
      className="mx-auto flex max-w-sm flex-col items-center px-6 py-14 text-center"
    >
      <div className="relative mb-5 grid size-16 place-items-center rounded-[20px] border border-line bg-card text-accent shadow-md">
        <div className="absolute inset-0 rounded-[20px] bg-[radial-gradient(circle_at_30%_20%,color-mix(in_oklab,var(--accent)_22%,transparent),transparent_70%)]" />
        <span className="relative">{icon}</span>
      </div>
      <h3 className="text-[15px] font-semibold">{title}</h3>
      <p className="mt-1.5 text-[13px] text-muted">{body}</p>
      {action && <div className="mt-5">{action}</div>}
    </motion.div>
  );
}

const statusStyle: Record<DownloadRecord["status"], { label: string; color: string }> = {
  queued: { label: "Queued", color: "var(--muted)" },
  scheduled: { label: "Scheduled", color: "var(--violet)" },
  paused: { label: "Paused", color: "var(--amber)" },
  probing: { label: "Connecting", color: "var(--accent)" },
  downloading: { label: "Downloading", color: "var(--accent)" },
  verifying: { label: "Verifying", color: "var(--violet)" },
  completed: { label: "Completed", color: "var(--mint)" },
  failed: { label: "Failed", color: "var(--coral)" },
  cancelled: { label: "Cancelled", color: "var(--subtle)" },
};

export function StatusBadge({ status }: { status: DownloadRecord["status"] }) {
  const s = statusStyle[status];
  const live = status === "downloading" || status === "probing" || status === "verifying";
  return (
    <span
      className="inline-flex h-5.5 items-center gap-1.5 rounded-full px-2 text-[11px] font-semibold"
      style={{ color: s.color, background: `color-mix(in oklab, ${s.color} 13%, transparent)` }}
    >
      <span className={cn("size-1.5 rounded-full", live && "live-dot")} style={{ background: s.color }} />
      {s.label}
    </span>
  );
}

export function statusLabel(status: DownloadRecord["status"]) {
  return statusStyle[status].label;
}
