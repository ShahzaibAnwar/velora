import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Bell, CheckCircle2, ClipboardPaste, FlaskConical, Minus, Pause, Play, Plus, Search, Settings, Square, X, XCircle } from "lucide-react";

import { Button, IconButton, Kbd } from "@/components/ui/button";
import { Popover, Tooltip } from "@/components/ui/overlay";
import { actions } from "@/lib/actions";
import { api, isTauri, windowControl } from "@/lib/api";
import { cn } from "@/lib/cn";
import { relativeTime } from "@/lib/format";
import { useStore, viewTitles } from "@/lib/store";
import { toast } from "@/lib/toast";

import { extractUrls } from "../features/add/AddDownloadDialog";

export async function pasteLink() {
  const text = await api.readClipboard().catch(() => "");
  const urls = extractUrls(text ?? "");
  if (urls.length === 0) {
    toast.info("No link on the clipboard", "Copy a download link first, then paste it here.");
    return;
  }
  useStore.getState().openAdd(urls.length === 1 ? { url: urls[0] } : { urls });
}

function Notifications() {
  const notices = useStore((s) => s.notices);
  const unread = useStore((s) => s.unreadNotices);
  const markRead = useStore((s) => s.markNoticesRead);
  const clear = useStore((s) => s.clearNotices);
  const select = useStore((s) => s.select);
  return (
    <Popover
      trigger={
        <button onClick={markRead} className="relative grid size-8.5 place-items-center rounded-[10px] text-muted transition hover:bg-hover hover:text-fg" aria-label="Notifications">
          <Bell size={17} />
          <AnimatePresence>
            {unread > 0 && (
              <motion.span
                initial={{ scale: 0 }}
                animate={{ scale: 1 }}
                exit={{ scale: 0 }}
                className="absolute top-1 right-1 grid min-w-4 place-items-center rounded-full bg-coral px-1 text-[9.5px] font-bold text-white"
              >
                {unread}
              </motion.span>
            )}
          </AnimatePresence>
        </button>
      }
    >
      <div className="flex items-center justify-between px-2 pt-1 pb-2">
        <p className="text-[13px] font-semibold">Activity</p>
        {notices.length > 0 && (
          <button onClick={clear} className="text-[11.5px] font-semibold text-muted hover:text-fg">
            Clear
          </button>
        )}
      </div>
      {notices.length === 0 ? (
        <p className="px-2 pb-3 text-[12.5px] text-muted">Finished and failed downloads from this session appear here.</p>
      ) : (
        <ul className="scroll-area grid max-h-80 gap-0.5">
          {notices.map((n) => (
            <li key={n.id}>
              <button onClick={() => select(n.downloadId)} className="flex w-full items-center gap-2.5 rounded-xl px-2 py-2 text-left hover:bg-hover">
                {n.kind === "completed" ? <CheckCircle2 size={16} className="shrink-0 text-mint" /> : <XCircle size={16} className="shrink-0 text-coral" />}
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[12.5px] font-semibold">{n.title}</p>
                  <p className="text-[11px] text-muted">
                    {n.kind === "completed" ? "Finished" : "Failed"} · {relativeTime(n.at)}
                  </p>
                </div>
              </button>
            </li>
          ))}
        </ul>
      )}
    </Popover>
  );
}

/** The Windows "restore down" glyph: two overlapping squares. */
function RestoreIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 13 13" fill="none" stroke="currentColor" strokeWidth="1.3" aria-hidden>
      <rect x="1" y="3.5" width="8.5" height="8.5" rx="1.5" />
      <path d="M3.5 3.5V2.5A1.5 1.5 0 0 1 5 1h5.5A1.5 1.5 0 0 1 12 2.5V8a1.5 1.5 0 0 1-1.5 1.5h-1" />
    </svg>
  );
}

function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    let alive = true;
    import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
      const w = getCurrentWindow();
      const sync = () => w.isMaximized().then((m) => alive && setMaximized(m));
      sync();
      const u = await w.onResized(sync);
      if (alive) unlisten = u;
      else u();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  if (!isTauri) return null;
  return (
    <div className="-mr-3 ml-2 flex h-full items-stretch">
      <button className="grid w-11 place-items-center text-muted transition hover:bg-hover hover:text-fg" aria-label="Minimise" title="Minimise" onClick={() => windowControl("minimize")}>
        <Minus size={16} />
      </button>
      <button
        className="grid w-11 place-items-center text-muted transition hover:bg-hover hover:text-fg"
        aria-label={maximized ? "Restore down" : "Maximise"}
        title={maximized ? "Restore down" : "Maximise"}
        onClick={() => windowControl("toggleMaximize")}
      >
        {maximized ? <RestoreIcon /> : <Square size={12.5} />}
      </button>
      <button className="grid w-11 place-items-center text-muted transition hover:bg-[#e81123] hover:text-white" aria-label="Close" title="Close" onClick={() => windowControl("close")}>
        <X size={17} />
      </button>
    </div>
  );
}

export function Header() {
  const view = useStore((s) => s.view);
  const search = useStore((s) => s.search);
  const setSearch = useStore((s) => s.setSearch);
  const setView = useStore((s) => s.setView);
  const openAdd = useStore((s) => s.openAdd);
  const queuePaused = useStore((s) => s.queuePaused);
  const anyActive = useStore((s) => Object.keys(s.progress).length > 0);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const focus = () => searchRef.current?.focus();
    window.addEventListener("velora:focus-search", focus);
    return () => window.removeEventListener("velora:focus-search", focus);
  }, []);

  const paused = queuePaused && !anyActive;

  return (
    <header className="flex h-14 shrink-0 items-center gap-3 border-b border-line pr-3 pl-7" data-tauri-drag-region>
      <AnimatePresence mode="wait" initial={false}>
        <motion.h1
          key={view}
          initial={{ opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -4 }}
          transition={{ duration: 0.15 }}
          className="pointer-events-none min-w-0 truncate text-[17px] font-semibold tracking-[-0.015em]"
        >
          {viewTitles[view]}
        </motion.h1>
      </AnimatePresence>

      {!isTauri && (
        <Tooltip content="Running in a normal browser: downloads and numbers are simulated sample data. Start the desktop app with npm run app:dev for the real engine.">
          <span className="flex shrink-0 items-center gap-1.5 rounded-full border border-[color-mix(in_oklab,var(--amber)_40%,transparent)] bg-[color-mix(in_oklab,var(--amber)_12%,var(--bg))] px-2.5 py-1 text-[11px] font-semibold text-amber">
            <FlaskConical size={12} /> Preview data
          </span>
        </Tooltip>
      )}

      <div className="flex-1 self-stretch" data-tauri-drag-region />

      <div className="relative w-64 max-w-[30vw]">
        <Search size={15} className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-subtle" />
        <input
          ref={searchRef}
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
            if (e.target.value && (view === "dashboard" || view === "settings" || view === "browser" || view === "categories")) setView("all");
          }}
          onKeyDown={(e) => e.key === "Escape" && (setSearch(""), (e.target as HTMLInputElement).blur())}
          placeholder="Search downloads"
          className="h-8.5 w-full rounded-[10px] border border-line bg-[color-mix(in_oklab,var(--bg)_50%,var(--card))] pr-14 pl-9 text-[12.5px] outline-none transition placeholder:text-subtle focus:border-[color-mix(in_oklab,var(--accent)_55%,transparent)] focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--accent)_14%,transparent)]"
          aria-label="Search downloads"
        />
        <span className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2">{search ? null : <Kbd>Ctrl F</Kbd>}</span>
      </div>

      <Tooltip content="Paste link from clipboard (Ctrl+V)">
        <Button size="sm" variant="secondary" icon={<ClipboardPaste size={15} />} onClick={pasteLink} className="h-8.5">
          Paste
        </Button>
      </Tooltip>
      <Tooltip content="Add download (Ctrl+N)">
        <Button size="sm" variant="primary" icon={<Plus size={16} strokeWidth={2.4} />} onClick={() => openAdd()} className="h-8.5" data-testid="add-button">
          Add
        </Button>
      </Tooltip>

      <div className="mx-1 h-5 w-px bg-line" />

      <IconButton label={paused ? "Resume all" : "Pause all"} onClick={() => (paused ? actions.resumeAll() : actions.pauseAll())} className={cn(paused && "text-amber")}>
        {paused ? <Play size={17} /> : <Pause size={17} />}
      </IconButton>
      <Notifications />
      <IconButton label="Settings (Ctrl+,)" onClick={() => setView("settings")}>
        <Settings size={17} />
      </IconButton>
      <WindowControls />
    </header>
  );
}
