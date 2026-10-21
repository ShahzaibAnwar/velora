import { useMemo, type ReactNode } from "react";
import { motion } from "motion/react";
import {
  CalendarClock,
  CheckCircle2,
  CircleAlert,
  Download,
  FolderKanban,
  History,
  LayoutDashboard,
  Layers,
  ListOrdered,
  Monitor,
  Moon,
  PlugZap,
  Settings,
  Sun,
} from "lucide-react";

import { Tooltip } from "@/components/ui/overlay";
import { cn } from "@/lib/cn";
import { formatSpeed } from "@/lib/format";
import { matchesView, useStore, type View } from "@/lib/store";

import { useSaveSettings } from "../features/settings/useSaveSettings";

export function Logo({ size = 30 }: { size?: number }) {
  return <img src="/velora.svg" width={size} height={size} alt="" className="rounded-[9px] shadow-[0_6px_18px_-6px_color-mix(in_oklab,var(--accent)_70%,transparent)]" />;
}

interface NavItem {
  view: View;
  label: string;
  icon: ReactNode;
  count?: boolean;
}

const primary: NavItem[] = [
  { view: "dashboard", label: "Dashboard", icon: <LayoutDashboard size={17} /> },
  { view: "all", label: "All downloads", icon: <Layers size={17} />, count: true },
  { view: "downloading", label: "Downloading", icon: <Download size={17} />, count: true },
  { view: "completed", label: "Completed", icon: <CheckCircle2 size={17} />, count: true },
  { view: "queued", label: "Queued", icon: <ListOrdered size={17} />, count: true },
  { view: "scheduled", label: "Scheduled", icon: <CalendarClock size={17} />, count: true },
  { view: "failed", label: "Failed", icon: <CircleAlert size={17} />, count: true },
];

const secondary: NavItem[] = [
  { view: "categories", label: "Categories", icon: <FolderKanban size={17} /> },
  { view: "browser", label: "Browser integration", icon: <PlugZap size={17} /> },
  { view: "history", label: "History", icon: <History size={17} /> },
  { view: "settings", label: "Settings", icon: <Settings size={17} /> },
];

function NavButton({ item, active, count, onClick }: { item: NavItem; active: boolean; count?: number; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "group relative flex h-9 w-full items-center gap-3 rounded-[11px] px-3 text-[13px] font-medium transition-colors duration-150",
        active ? "text-fg" : "text-muted hover:text-fg",
      )}
      aria-current={active ? "page" : undefined}
      aria-label={count ? `${item.label}, ${count}` : item.label}
    >
      {active && (
        <motion.span
          layoutId="nav-active"
          className="absolute inset-0 rounded-[11px] border border-line bg-[color-mix(in_oklab,var(--card)_85%,var(--accent)_6%)] shadow-sm"
          transition={{ type: "spring", stiffness: 520, damping: 40 }}
        />
      )}
      {!active && <span className="absolute inset-0 rounded-[11px] bg-hover opacity-0 transition-opacity group-hover:opacity-100" />}
      <span className={cn("relative transition-colors", active ? "text-accent" : "text-subtle group-hover:text-muted")}>{item.icon}</span>
      <span className="relative flex-1 truncate text-left">{item.label}</span>
      {count != null && count > 0 && (
        <span
          className={cn(
            "relative min-w-6 rounded-full px-1.5 text-center text-[11px] font-semibold tabular",
            item.view === "failed" ? "bg-[color-mix(in_oklab,var(--coral)_16%,transparent)] text-coral" : "bg-hover text-muted",
          )}
        >
          {count}
        </span>
      )}
    </button>
  );
}

export function Sidebar() {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const downloads = useStore((s) => s.downloads);
  const totalSpeed = useStore((s) => s.totalSpeed);
  const activeCount = useStore((s) => Object.keys(s.progress).length);
  const settings = useStore((s) => s.settings);
  const save = useSaveSettings();

  const counts = useMemo(() => {
    const list = Object.values(downloads);
    const out: Partial<Record<View, number>> = {};
    for (const item of primary) if (item.count) out[item.view] = list.filter((r) => matchesView(r, item.view)).length;
    return out;
  }, [downloads]);

  const cycleTheme = () => {
    if (!settings) return;
    const next = settings.theme === "system" ? "dark" : settings.theme === "dark" ? "light" : "system";
    save({ ...settings, theme: next });
  };
  const ThemeIcon = settings?.theme === "light" ? Sun : settings?.theme === "dark" ? Moon : Monitor;

  return (
    <aside className="flex h-full w-[236px] shrink-0 flex-col border-r border-line bg-sidebar">
      <div className="flex h-14 items-center gap-2.5 px-5" data-tauri-drag-region>
        <Logo />
        <div className="pointer-events-none leading-tight">
          <p className="text-[14.5px] font-bold tracking-[-0.02em]">Velora</p>
          <p className="text-[10.5px] font-medium text-subtle">Download Manager</p>
        </div>
      </div>

      <nav className="scroll-area flex-1 px-3 pt-2">
        <div className="grid gap-0.5">
          {primary.map((item) => (
            <NavButton key={item.view} item={item} active={view === item.view} count={item.count ? counts[item.view] : undefined} onClick={() => setView(item.view)} />
          ))}
        </div>
        <div className="mx-3 my-3 h-px bg-line" />
        <div className="grid gap-0.5">
          {secondary.map((item) => (
            <NavButton key={item.view} item={item} active={view === item.view} onClick={() => setView(item.view)} />
          ))}
        </div>
      </nav>

      <div className="m-3 rounded-2xl border border-line bg-card p-3">
        <div className="flex items-center gap-2.5">
          <span className={cn("size-2 rounded-full", activeCount > 0 ? "live-dot bg-mint" : "bg-subtle/60")} />
          <div className="min-w-0 flex-1">
            <p className="text-[12px] font-semibold">{activeCount > 0 ? `${activeCount} downloading` : "Idle"}</p>
            <p className="text-[11px] text-muted tabular">{activeCount > 0 ? formatSpeed(totalSpeed) : "Ready for new links"}</p>
          </div>
          <Tooltip content={`Theme: ${settings?.theme ?? "system"} (click to change)`}>
            <button onClick={cycleTheme} className="grid size-8 place-items-center rounded-lg text-muted transition hover:bg-hover hover:text-fg" aria-label="Change theme">
              <ThemeIcon size={16} />
            </button>
          </Tooltip>
        </div>
      </div>
    </aside>
  );
}
