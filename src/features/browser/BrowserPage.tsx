import { useEffect, useState } from "react";
import { motion } from "motion/react";
import { CheckCircle2, Chrome, Circle, FolderOpen, Link2Off, PlugZap, ShieldCheck, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/feedback";
import { Switch } from "@/components/ui/inputs";
import { attempt } from "@/lib/actions";
import { api } from "@/lib/api";
import { useStore } from "@/lib/store";
import { toast } from "@/lib/toast";
import type { NativeMessagingStatus } from "@/types/engine";

import { useSaveSettings } from "../settings/useSaveSettings";

function Step({ n, title, children }: { n: number; title: string; children: React.ReactNode }) {
  return (
    <li className="flex gap-3.5">
      <span className="grid size-7 shrink-0 place-items-center rounded-full bg-[color-mix(in_oklab,var(--accent)_14%,transparent)] text-[12px] font-bold text-accent">{n}</span>
      <div className="min-w-0 pb-1">
        <p className="text-[13px] font-semibold">{title}</p>
        <div className="mt-1 text-[12.5px] text-muted">{children}</div>
      </div>
    </li>
  );
}

export function BrowserPage() {
  const [status, setStatus] = useState<NativeMessagingStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const settings = useStore((s) => s.settings);
  const save = useSaveSettings();

  useEffect(() => {
    api.nativeMessagingStatus().then(setStatus).catch(() => {});
  }, []);

  const anyRegistered = status?.browsers.some((b) => b.registered);

  const connect = async () => {
    setBusy(true);
    const s = await attempt("Couldn't register browser integration", () => api.registerNativeMessaging());
    setBusy(false);
    if (s) {
      setStatus(s);
      toast.success("Browsers connected", "Now load the Velora extension in your browser.");
    }
  };
  const disconnect = async () => {
    setBusy(true);
    const s = await attempt("Couldn't remove browser integration", () => api.unregisterNativeMessaging());
    setBusy(false);
    if (s) setStatus(s);
  };

  return (
    <div className="scroll-area h-full px-7 pb-8">
      <div className="grid gap-4 xl:grid-cols-[1.2fr_1fr]">
        <motion.section initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} className="rounded-[18px] border border-line bg-card p-5 shadow-sm">
          <div className="flex items-start gap-3">
            <div className="grid size-10 place-items-center rounded-xl bg-[color-mix(in_oklab,var(--accent)_14%,transparent)] text-accent">
              <PlugZap size={20} />
            </div>
            <div className="flex-1">
              <h2 className="text-[15px] font-semibold">Connect your browsers</h2>
              <p className="mt-1 text-[12.5px] text-muted">
                Velora talks to its browser extension through native messaging — no network port is opened, and each extension must be approved here before it can add
                downloads.
              </p>
            </div>
          </div>

          <ul className="mt-5 grid gap-2">
            {status?.browsers.map((b) => (
              <li key={b.id} className="flex items-center gap-3 rounded-xl border border-line px-3.5 py-2.5">
                <Chrome size={16} className="text-subtle" />
                <span className="flex-1 text-[13px] font-medium">{b.name}</span>
                {b.registered ? (
                  <span className="flex items-center gap-1.5 text-[12px] font-semibold text-mint">
                    <CheckCircle2 size={14} /> Connected
                  </span>
                ) : (
                  <span className="flex items-center gap-1.5 text-[12px] text-subtle">
                    <Circle size={12} /> Not connected
                  </span>
                )}
              </li>
            )) ?? <Spinner />}
          </ul>

          <div className="mt-4 flex flex-wrap gap-2">
            <Button variant="primary" onClick={connect} disabled={busy || !status?.supported} icon={busy ? <Spinner size={14} className="text-on-accent" /> : <PlugZap size={15} />}>
              {anyRegistered ? "Repair connection" : "Connect browsers"}
            </Button>
            {anyRegistered && (
              <Button variant="ghost" onClick={disconnect} disabled={busy} icon={<Link2Off size={15} />}>
                Disconnect
              </Button>
            )}
          </div>
          <p className="mt-3 text-[11.5px] text-subtle">
            This writes small registration entries for your Windows user only (no administrator rights). Disconnect removes them.
          </p>
        </motion.section>

        <motion.section initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.05 }} className="rounded-[18px] border border-line bg-card p-5 shadow-sm">
          <h2 className="text-[15px] font-semibold">Install the extension</h2>
          <ol className="mt-4 grid gap-4">
            <Step n={1} title="Chrome or Edge">
              Open <code className="rounded bg-hover px-1">chrome://extensions</code> (or <code className="rounded bg-hover px-1">edge://extensions</code>), turn on Developer mode, choose
              “Load unpacked” and select the Chromium folder.
              <div className="mt-2">
                <Button size="sm" icon={<FolderOpen size={14} />} onClick={() => attempt("Couldn't open the folder", () => api.openExtensionFolder("chromium"))}>
                  Open Chromium folder
                </Button>
              </div>
            </Step>
            <Step n={2} title="Firefox">
              Open <code className="rounded bg-hover px-1">about:debugging</code> → This Firefox → “Load Temporary Add-on…” and pick <code className="rounded bg-hover px-1">manifest.json</code> in the Firefox
              folder.
              <div className="mt-2">
                <Button size="sm" icon={<FolderOpen size={14} />} onClick={() => attempt("Couldn't open the folder", () => api.openExtensionFolder("firefox"))}>
                  Open Firefox folder
                </Button>
              </div>
            </Step>
            <Step n={3} title="Approve it once">
              The first time the extension sends a link, Velora asks you to allow it.
            </Step>
          </ol>
        </motion.section>
      </div>

      <section className="mt-4 rounded-[18px] border border-line bg-card p-5 shadow-sm">
        <h2 className="text-[14px] font-semibold">Behaviour</h2>
        <div className="mt-3 flex items-center justify-between gap-4 rounded-xl border border-line px-4 py-3">
          <div>
            <p className="text-[13px] font-medium">Review links from the browser before downloading</p>
            <p className="text-[12px] text-muted">Shows the Add Download window so you can check the name and folder. Turn off to start immediately.</p>
          </div>
          <Switch
            label="Review links from the browser"
            checked={settings?.showAddDialogForBrowser ?? true}
            onCheckedChange={(v) => settings && save({ ...settings, showAddDialogForBrowser: v })}
          />
        </div>

        <h3 className="mt-5 mb-2 flex items-center gap-2 text-[12px] font-semibold tracking-wider text-subtle uppercase">
          <ShieldCheck size={14} /> Approved extensions
        </h3>
        {settings?.approvedOrigins.length ? (
          <ul className="grid gap-2">
            {settings.approvedOrigins.map((o) => (
              <li key={o} className="flex items-center gap-3 rounded-xl border border-line px-3.5 py-2.5">
                <span className="flex-1 truncate font-mono text-[12px]">{o}</span>
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Trash2 size={14} />}
                  onClick={async () => {
                    await attempt("Couldn't revoke", () => api.revokeOrigin(o));
                    const s = await api.getSettings();
                    useStore.getState().setSettings(s);
                  }}
                >
                  Revoke
                </Button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-[12.5px] text-muted">No extensions approved yet.</p>
        )}

        <p className="mt-5 text-[11.5px] leading-relaxed text-subtle">
          Video detection only finds direct media files on the page. Streaming playlists (HLS/DASH), browser-only streams and DRM-protected media are labelled and not downloaded.
          The extension never reads your cookies, so downloads that need you to be signed in should be saved by the browser.
        </p>
      </section>
    </div>
  );
}
