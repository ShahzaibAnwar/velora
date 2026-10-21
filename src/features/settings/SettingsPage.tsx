import { useEffect, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import {
  Download,
  FolderOpen,
  Github,
  Globe,
  Info,
  Lock,
  Monitor,
  Moon,
  Palette,
  PlugZap,
  Settings2,
  Sun,
  Timer,
  Trash2,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input, Segmented, Select, Switch } from "@/components/ui/inputs";
import { Dialog } from "@/components/ui/overlay";
import { attempt } from "@/lib/actions";
import { api, isTauri, pickFolder } from "@/lib/api";
import { cn } from "@/lib/cn";
import { CATEGORIES, categoryMeta } from "@/lib/fileTypes";
import { formatSpeed, parseRate } from "@/lib/format";
import { useStore } from "@/lib/store";
import { toast } from "@/lib/toast";
import type { AppInfo, Settings, TimeWindow } from "@/types/engine";

import { useSaveSettings } from "./useSaveSettings";

export const REPO_URL = "https://github.com/ShahzaibAnwar/velora";

type SectionId = "general" | "downloads" | "network" | "scheduling" | "appearance" | "browser" | "privacy" | "about";

const sections: { id: SectionId; label: string; icon: ReactNode }[] = [
  { id: "general", label: "General", icon: <Settings2 size={16} /> },
  { id: "downloads", label: "Downloads", icon: <Download size={16} /> },
  { id: "network", label: "Network", icon: <Globe size={16} /> },
  { id: "scheduling", label: "Scheduling", icon: <Timer size={16} /> },
  { id: "appearance", label: "Appearance", icon: <Palette size={16} /> },
  { id: "browser", label: "Browser integration", icon: <PlugZap size={16} /> },
  { id: "privacy", label: "Privacy & security", icon: <Lock size={16} /> },
  { id: "about", label: "About", icon: <Info size={16} /> },
];

function Card({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="mb-4 rounded-[18px] border border-line bg-card shadow-sm">
      {title && <h3 className="px-5 pt-4 pb-1 text-[12px] font-semibold tracking-wider text-subtle uppercase">{title}</h3>}
      <div className="divide-y divide-line">{children}</div>
    </section>
  );
}

function Row({ title, description, children, stacked }: { title: string; description?: ReactNode; children: ReactNode; stacked?: boolean }) {
  return (
    <div className={cn("flex gap-4 px-5 py-3.5", stacked ? "flex-col" : "items-center justify-between")}>
      <div className="min-w-0">
        <p className="text-[13px] font-medium">{title}</p>
        {description && <p className="mt-0.5 text-[12px] text-muted">{description}</p>}
      </div>
      <div className={cn(stacked ? "w-full" : "shrink-0")}>{children}</div>
    </div>
  );
}

/** Text input that saves on blur or Enter, so typing doesn't hit the engine on every keystroke. */
function CommitInput({ value, onCommit, placeholder, className, type = "text" }: { value: string; onCommit: (v: string) => void; placeholder?: string; className?: string; type?: string }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <Input
      type={type}
      value={draft}
      placeholder={placeholder}
      className={className}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => draft !== value && onCommit(draft)}
      onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
    />
  );
}

function RateInput({ value, onCommit }: { value: number; onCommit: (v: number) => void }) {
  const display = value ? formatSpeed(value).replace("/s", "") : "";
  return (
    <CommitInput
      value={display}
      placeholder="Unlimited"
      className="w-36"
      onCommit={(v) => {
        const parsed = parseRate(v);
        if (parsed == null) toast.error("Speed not understood", "Use values like 500 KB or 2 MB, or leave empty for unlimited.");
        else onCommit(parsed);
      }}
    />
  );
}

function FolderInput({ value, onChange, placeholder }: { value: string; onChange: (v: string) => void; placeholder?: string }) {
  return (
    <div className="flex w-full gap-2">
      <CommitInput value={value} onCommit={onChange} placeholder={placeholder} className="font-mono text-[12px]" />
      <Button
        icon={<FolderOpen size={15} />}
        onClick={async () => {
          const dir = await pickFolder(value || undefined);
          if (dir) onChange(dir);
        }}
        disabled={!isTauri}
      >
        Browse
      </Button>
    </div>
  );
}

const DAYS = ["S", "M", "T", "W", "T", "F", "S"];

function WindowEditor({ value, onChange, children }: { value: TimeWindow; onChange: (w: TimeWindow) => void; children?: ReactNode }) {
  return (
    <div className="grid gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-[12.5px] text-muted">
          From
          <CommitInput type="time" value={value.start} onCommit={(v) => onChange({ ...value, start: v })} className="w-30" />
        </label>
        <label className="flex items-center gap-2 text-[12.5px] text-muted">
          to
          <CommitInput type="time" value={value.end} onCommit={(v) => onChange({ ...value, end: v })} className="w-30" />
        </label>
        {children}
      </div>
      <div className="flex items-center gap-1.5">
        {DAYS.map((d, i) => {
          const on = value.days.length === 0 || value.days.includes(i);
          return (
            <button
              key={i}
              onClick={() => {
                const current = value.days.length === 0 ? [0, 1, 2, 3, 4, 5, 6] : value.days;
                const next = current.includes(i) ? current.filter((x) => x !== i) : [...current, i].sort();
                onChange({ ...value, days: next.length === 7 ? [] : next });
              }}
              className={cn(
                "grid size-8 place-items-center rounded-lg text-[12px] font-semibold transition",
                on ? "bg-[color-mix(in_oklab,var(--accent)_18%,transparent)] text-accent" : "bg-hover text-subtle",
              )}
              aria-pressed={on}
            >
              {d}
            </button>
          );
        })}
        <span className="ml-2 text-[11.5px] text-subtle">{value.days.length === 0 ? "Every day" : `${value.days.length} day${value.days.length === 1 ? "" : "s"}`}</span>
      </div>
    </div>
  );
}

export function SettingsPage() {
  const settings = useStore((s) => s.settings);
  const save = useSaveSettings();
  const setView = useStore((s) => s.setView);
  const [section, setSection] = useState<SectionId>("general");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  useEffect(() => {
    api.appInfo().then(setInfo).catch(() => {});
  }, []);

  if (!settings) return null;
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => save({ ...settings, [key]: value });

  const content: Record<SectionId, ReactNode> = {
    general: (
      <>
        <Card title="Startup & window">
          <Row title="Launch Velora when Windows starts" description="Starts minimised to the tray so scheduled downloads can run.">
            <Switch label="Launch on startup" checked={settings.launchOnStartup} onCheckedChange={(v) => set("launchOnStartup", v)} />
          </Row>
          <Row title="Start minimised" description="Open in the tray instead of showing the window.">
            <Switch label="Start minimised" checked={settings.startMinimized} onCheckedChange={(v) => set("startMinimized", v)} />
          </Row>
          <Row title="Minimise to tray" description="Hide the window from the taskbar when minimised.">
            <Switch label="Minimise to tray" checked={settings.minimizeToTray} onCheckedChange={(v) => set("minimizeToTray", v)} />
          </Row>
          <Row title="Keep running when the window is closed" description="Downloads continue in the tray. Turn off to quit Velora when you close the window.">
            <Switch label="Close to tray" checked={settings.closeToTray} onCheckedChange={(v) => set("closeToTray", v)} />
          </Row>
        </Card>
        <Card title="Notifications">
          <Row title="When a download finishes">
            <Switch label="Notify on completion" checked={settings.notifyOnComplete} onCheckedChange={(v) => set("notifyOnComplete", v)} />
          </Row>
          <Row title="When a download fails">
            <Switch label="Notify on failure" checked={settings.notifyOnFailure} onCheckedChange={(v) => set("notifyOnFailure", v)} />
          </Row>
        </Card>
        <Card title="Language">
          <Row title="Display language" description="More languages are welcome as contributions on GitHub.">
            <Select value={settings.language} onValueChange={(v) => set("language", v)} className="w-40" options={[{ value: "en", label: "English" }]} />
          </Row>
        </Card>
      </>
    ),
    downloads: (
      <>
        <Card title="Location">
          <Row title="Default download folder" stacked>
            <FolderInput value={settings.defaultDir} onChange={(v) => set("defaultDir", v)} />
          </Row>
          <Row title="Sort into category folders" description="Save videos to …\Video, archives to …\Compressed, and so on, inside the default folder.">
            <Switch label="Automatic categorisation" checked={settings.autoCategorize} onCheckedChange={(v) => set("autoCategorize", v)} />
          </Row>
          <Row title="Custom folder per category" description="Leave empty to use the default behaviour above." stacked>
            <div className="grid gap-2">
              {CATEGORIES.map((c) => (
                <div key={c} className="grid grid-cols-[120px_1fr] items-center gap-3">
                  <span className="text-[12.5px] text-muted">{categoryMeta[c].label}</span>
                  <FolderInput
                    value={settings.categoryFolders[c] ?? ""}
                    placeholder="Default"
                    onChange={(v) => {
                      const next = { ...settings.categoryFolders };
                      if (v.trim()) next[c] = v.trim();
                      else delete next[c];
                      set("categoryFolders", next);
                    }}
                  />
                </div>
              ))}
            </div>
          </Row>
          <Row title="When a file with the same name exists">
            <Select
              value={settings.overwritePolicy}
              onValueChange={(v) => set("overwritePolicy", v)}
              className="w-48"
              options={[
                { value: "ask", label: "Ask me" },
                { value: "rename", label: "Keep both (rename)" },
                { value: "overwrite", label: "Replace the old file" },
              ]}
            />
          </Row>
        </Card>
        <Card title="Speed & reliability">
          <Row title="Simultaneous downloads" description="How many downloads the queue runs at once.">
            <Select value={String(settings.maxActive)} onValueChange={(v) => set("maxActive", Number(v))} className="w-24" options={[1, 2, 3, 4, 5, 6, 8, 10].map((n) => ({ value: String(n), label: String(n) }))} />
          </Row>
          <Row title="Connections per download" description="Used when the server supports byte ranges. More connections help only when each connection is individually limited.">
            <Select value={String(settings.defaultConnections)} onValueChange={(v) => set("defaultConnections", Number(v))} className="w-24" options={[1, 2, 4, 6, 8, 12, 16].map((n) => ({ value: String(n), label: String(n) }))} />
          </Row>
          <Row title="Retries before giving up" description="Per connection, for temporary problems such as timeouts or HTTP 503. Progress resets the count.">
            <Select value={String(settings.maxRetries)} onValueChange={(v) => set("maxRetries", Number(v))} className="w-24" options={[0, 1, 3, 5, 8, 10, 20].map((n) => ({ value: String(n), label: String(n) }))} />
          </Row>
          <Row title="Longest wait between retries">
            <Select
              value={String(settings.retryMaxDelaySecs)}
              onValueChange={(v) => set("retryMaxDelaySecs", Number(v))}
              className="w-32"
              options={[10, 30, 60, 120, 300].map((n) => ({ value: String(n), label: n < 60 ? `${n} seconds` : `${n / 60} minute${n === 60 ? "" : "s"}` }))}
            />
          </Row>
          <Row title="Resume unfinished downloads on startup" description="Downloads that were running when Velora closed continue automatically.">
            <Switch label="Resume on startup" checked={settings.resumeOnStartup} onCheckedChange={(v) => set("resumeOnStartup", v)} />
          </Row>
        </Card>
      </>
    ),
    network: (
      <>
        <Card title="Bandwidth">
          <Row title="Global speed limit" description="Applies to all downloads together. Empty means unlimited.">
            <RateInput value={settings.globalSpeedLimit} onCommit={(v) => set("globalSpeedLimit", v)} />
          </Row>
          <Row
            title="Different limit during certain hours"
            description="For example, slow down during work hours."
          >
            <Switch
              label="Scheduled speed limit"
              checked={settings.scheduledLimit.window.enabled}
              onCheckedChange={(v) => set("scheduledLimit", { ...settings.scheduledLimit, window: { ...settings.scheduledLimit.window, enabled: v } })}
            />
          </Row>
          <AnimatePresence initial={false}>
            {settings.scheduledLimit.window.enabled && (
              <motion.div initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0 }} className="overflow-hidden">
                <div className="px-5 py-3.5">
                  <WindowEditor value={settings.scheduledLimit.window} onChange={(w) => set("scheduledLimit", { ...settings.scheduledLimit, window: w })}>
                    <label className="flex items-center gap-2 text-[12.5px] text-muted">
                      limit to
                      <RateInput value={settings.scheduledLimit.limit} onCommit={(v) => set("scheduledLimit", { ...settings.scheduledLimit, limit: v })} />
                    </label>
                  </WindowEditor>
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </Card>
        <Card title="Proxy">
          <Row title="Proxy server">
            <Segmented
              value={settings.proxy.mode}
              onValueChange={(v) => set("proxy", { ...settings.proxy, mode: v })}
              options={[
                { value: "system", label: "System" },
                { value: "none", label: "None" },
                { value: "manual", label: "Manual" },
              ]}
            />
          </Row>
          {settings.proxy.mode === "manual" && (
            <Row title="Proxy address" description="http://host:port, https://host:port or socks5://host:port" stacked>
              <CommitInput value={settings.proxy.url} placeholder="http://proxy.example.com:8080" onCommit={(v) => set("proxy", { ...settings.proxy, url: v.trim() })} className="font-mono text-[12px]" />
            </Row>
          )}
        </Card>
        <Card title="Timeouts">
          <Row title="Connection timeout">
            <Select value={String(settings.connectTimeoutSecs)} onValueChange={(v) => set("connectTimeoutSecs", Number(v))} className="w-32" options={[5, 10, 20, 30, 60].map((n) => ({ value: String(n), label: `${n} seconds` }))} />
          </Row>
          <Row title="Read timeout" description="How long to wait for data before retrying the connection.">
            <Select value={String(settings.readTimeoutSecs)} onValueChange={(v) => set("readTimeoutSecs", Number(v))} className="w-32" options={[15, 30, 60, 120, 300].map((n) => ({ value: String(n), label: `${n} seconds` }))} />
          </Row>
        </Card>
      </>
    ),
    scheduling: (
      <Card title="Queue schedule">
        <Row title="Only run the queue during set hours" description="Queued downloads start inside this window and are paused (and re-queued) when it ends. Downloads you start yourself aren't affected.">
          <Switch label="Queue time window" checked={settings.queueWindow.enabled} onCheckedChange={(v) => set("queueWindow", { ...settings.queueWindow, enabled: v })} />
        </Row>
        {settings.queueWindow.enabled && (
          <div className="px-5 py-3.5">
            <WindowEditor value={settings.queueWindow} onChange={(w) => set("queueWindow", w)} />
          </div>
        )}
        <div className="px-5 py-3.5 text-[12px] text-muted">
          Schedules run only while Velora is open or minimised to the tray. Turn on “Launch Velora when Windows starts” in General to keep them running after a restart.
        </div>
      </Card>
    ),
    appearance: (
      <>
        <Card title="Theme">
          <div className="grid grid-cols-3 gap-3 p-5">
            {(
              [
                { v: "system", label: "Follow Windows", icon: <Monitor size={18} /> },
                { v: "dark", label: "Dark", icon: <Moon size={18} /> },
                { v: "light", label: "Light", icon: <Sun size={18} /> },
              ] as const
            ).map((t) => (
              <button
                key={t.v}
                onClick={() => set("theme", t.v)}
                className={cn(
                  "flex flex-col items-center gap-2 rounded-2xl border p-4 text-[12.5px] font-semibold transition",
                  settings.theme === t.v ? "border-[color-mix(in_oklab,var(--accent)_60%,transparent)] bg-[color-mix(in_oklab,var(--accent)_8%,transparent)] text-fg" : "border-line text-muted hover:text-fg",
                )}
              >
                {t.icon}
                {t.label}
              </button>
            ))}
          </div>
          <Row title="Accent colour">
            <div className="flex gap-2">
              {(["cyan", "violet", "mint", "coral"] as const).map((a) => (
                <button
                  key={a}
                  onClick={() => set("accent", a)}
                  aria-label={a}
                  className={cn("size-7 rounded-full ring-offset-2 ring-offset-[var(--card)] transition", settings.accent === a && "ring-2 ring-[var(--fg)]")}
                  style={{ background: `var(--${a})` }}
                />
              ))}
            </div>
          </Row>
        </Card>
        <Card title="Motion & layout">
          <Row title="Reduce motion" description="“System” follows the Windows animation setting.">
            <Segmented
              value={settings.reducedMotion}
              onValueChange={(v) => set("reducedMotion", v)}
              options={[
                { value: "system", label: "System" },
                { value: "on", label: "On" },
                { value: "off", label: "Off" },
              ]}
            />
          </Row>
          <Row title="Density">
            <Segmented
              value={settings.density}
              onValueChange={(v) => set("density", v)}
              options={[
                { value: "comfortable", label: "Comfortable" },
                { value: "compact", label: "Compact" },
              ]}
            />
          </Row>
        </Card>
      </>
    ),
    browser: (
      <Card>
        <Row title="Browser integration" description="Connect Chrome, Edge and Firefox, install the extension and manage approved extensions.">
          <Button onClick={() => setView("browser")} icon={<PlugZap size={15} />}>
            Open
          </Button>
        </Row>
        <Row title="Review links from the browser before downloading">
          <Switch label="Review links from the browser" checked={settings.showAddDialogForBrowser} onCheckedChange={(v) => set("showAddDialogForBrowser", v)} />
        </Row>
      </Card>
    ),
    privacy: (
      <>
        <Card title="Clipboard">
          <Row title="Watch the clipboard for download links" description="When you copy a link to a file, Velora offers to download it. Nothing leaves your computer.">
            <Switch label="Clipboard monitoring" checked={settings.clipboardMonitor} onCheckedChange={(v) => set("clipboardMonitor", v)} />
          </Row>
        </Card>
        <Card title="Diagnostics">
          <Row title="Diagnostic logs" description="Detailed local logs that help fix problems. Web addresses are logged without query strings or passwords. When off, only errors are recorded.">
            <Switch label="Diagnostic logs" checked={settings.diagnosticLogs} onCheckedChange={(v) => set("diagnosticLogs", v)} />
          </Row>
          <Row title="Log folder" description={info?.logDir}>
            <Button icon={<FolderOpen size={15} />} onClick={() => attempt("Couldn't open", () => api.openPath("logs"))} disabled={!isTauri}>
              Open
            </Button>
          </Row>
        </Card>
        <Card title="Data">
          <Row title="Velora's data folder" description={info?.dataDir}>
            <Button icon={<FolderOpen size={15} />} onClick={() => attempt("Couldn't open", () => api.openPath("data"))} disabled={!isTauri}>
              Open
            </Button>
          </Row>
          <Row title="Clear download history" description="Removes finished, failed and cancelled entries. Your files are not deleted.">
            <Button variant="danger" icon={<Trash2 size={15} />} onClick={() => setConfirmClear(true)}>
              Clear history
            </Button>
          </Row>
        </Card>
        <Card title="Privacy">
          <div className="px-5 py-4 text-[12.5px] leading-relaxed text-muted">
            Velora has no accounts, no telemetry and no ads. It connects only to the servers you download from. Browser integration uses native messaging with an approval step,
            and Velora never reads browser cookies. TLS certificates are always verified.
          </div>
        </Card>
      </>
    ),
    about: (
      <Card>
        <div className="flex items-center gap-4 px-5 py-5">
          <img src="/velora.svg" alt="" className="size-14 rounded-2xl shadow-md" />
          <div>
            <p className="text-[17px] font-semibold">Velora Download Manager</p>
            <p className="text-[12.5px] text-muted">Download beautifully. Accelerate everything.</p>
            <p className="mt-1 text-[12px] text-subtle tabular">Version {info?.version ?? "—"} · Alpha</p>
          </div>
        </div>
        <Row title="Source code" description="Velora is free and open source.">
          <Button icon={<Github size={15} />} onClick={() => attempt("Couldn't open the link", () => api.openExternal(REPO_URL))} disabled={!isTauri}>
            GitHub
          </Button>
        </Row>
        <Row title="Licence" description="MIT License. Third-party licences are listed in the repository.">
          <span className="text-[12.5px] font-semibold">MIT</span>
        </Row>
        <Row title="Updates" description="Velora doesn't contact any server to check for updates. New versions are published on GitHub Releases.">
          <Button onClick={() => attempt("Couldn't open the link", () => api.openExternal(`${REPO_URL}/releases`))} disabled={!isTauri}>
            Releases
          </Button>
        </Row>
        <Row title="Contributors" description="Built by the Velora contributors. Bug reports and pull requests are welcome.">
          <Button onClick={() => attempt("Couldn't open the link", () => api.openExternal(`${REPO_URL}/blob/main/CONTRIBUTING.md`))} disabled={!isTauri}>
            How to contribute
          </Button>
        </Row>
      </Card>
    ),
  };

  return (
    <div className="flex h-full min-h-0 gap-6 px-7 pb-6">
      <nav className="w-52 shrink-0">
        <ul className="grid gap-0.5">
          {sections.map((s) => (
            <li key={s.id}>
              <button
                onClick={() => setSection(s.id)}
                className={cn(
                  "relative flex h-9 w-full items-center gap-2.5 rounded-[10px] px-3 text-[13px] font-medium transition-colors",
                  section === s.id ? "text-fg" : "text-muted hover:bg-hover hover:text-fg",
                )}
              >
                {section === s.id && (
                  <motion.span layoutId="settings-nav" className="absolute inset-0 rounded-[10px] border border-line bg-card shadow-sm" transition={{ type: "spring", stiffness: 500, damping: 38 }} />
                )}
                <span className="relative text-subtle">{s.icon}</span>
                <span className="relative">{s.label}</span>
              </button>
            </li>
          ))}
        </ul>
      </nav>
      <div className="scroll-area min-w-0 flex-1 pr-1">
        <AnimatePresence mode="wait" initial={false}>
          <motion.div key={section} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={{ duration: 0.16 }} className="max-w-3xl">
            {content[section]}
          </motion.div>
        </AnimatePresence>
      </div>

      <Dialog
        open={confirmClear}
        onOpenChange={setConfirmClear}
        title="Clear download history?"
        description="Finished, failed and cancelled entries are removed. Files on disk are kept."
        width={440}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmClear(false)} icon={<X size={14} />}>
              Cancel
            </Button>
            <Button
              variant="danger"
              onClick={async () => {
                const ids = await attempt("Couldn't clear history", () => api.clear("finished"));
                if (ids) toast.success(`Cleared ${ids.length} entr${ids.length === 1 ? "y" : "ies"}`);
                setConfirmClear(false);
              }}
            >
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
