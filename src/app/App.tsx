import { useCallback, useState } from "react";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { ShieldQuestion } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Toaster } from "@/components/ui/feedback";
import { Dialog, TooltipProvider } from "@/components/ui/overlay";
import { attempt } from "@/lib/actions";
import { api } from "@/lib/api";
import { useStore, type View } from "@/lib/store";

import { AddDownloadDialog } from "../features/add/AddDownloadDialog";
import { BrowserPage } from "../features/browser/BrowserPage";
import { CategoriesPage } from "../features/categories/CategoriesPage";
import { Dashboard } from "../features/dashboard/Dashboard";
import { DetailsPanel } from "../features/downloads/DetailsPanel";
import { DialogHost } from "../features/downloads/DialogHost";
import { DownloadList } from "../features/downloads/DownloadList";
import { HistoryPage } from "../features/history/HistoryPage";
import { QueuePage } from "../features/queue/QueuePage";
import { SettingsPage } from "../features/settings/SettingsPage";
import { useAppearance } from "../hooks/useAppearance";
import { useBackendEvents, type ApprovalRequest } from "../hooks/useBackendEvents";
import { useHotkeys } from "../hooks/useHotkeys";
import { Header } from "./Header";
import { Sidebar } from "./Sidebar";

function Page({ view }: { view: View }) {
  switch (view) {
    case "dashboard":
      return <Dashboard />;
    case "queued":
      return <QueuePage />;
    case "categories":
      return <CategoriesPage />;
    case "history":
      return <HistoryPage />;
    case "browser":
      return <BrowserPage />;
    case "settings":
      return <SettingsPage />;
    default:
      return <DownloadList view={view} />;
  }
}

export function App() {
  const settings = useStore((s) => s.settings);
  const view = useStore((s) => s.view);
  const categoryFilter = useStore((s) => s.categoryFilter);
  const ready = useStore((s) => s.ready);
  const { reduced } = useAppearance(settings);
  const [approvals, setApprovals] = useState<ApprovalRequest[]>([]);

  const onApproval = useCallback((req: ApprovalRequest) => setApprovals((a) => (a.some((x) => x.origin === req.origin) ? a : [...a, req])), []);
  useBackendEvents(onApproval);
  useHotkeys();

  const pending = approvals[0];
  const decide = async (allow: boolean) => {
    if (!pending) return;
    await attempt("Couldn't save your choice", () => api.bridgeDecide(pending.origin, allow));
    if (allow) useStore.getState().setSettings(await api.getSettings());
    setApprovals((a) => a.slice(1));
  };

  return (
    <MotionConfig reducedMotion={reduced ? "always" : "never"} transition={{ type: "spring", stiffness: 400, damping: 34 }}>
      <TooltipProvider>
        <div className="flex h-full overflow-hidden bg-bg text-fg">
          <Sidebar />
          <main className="relative flex min-w-0 flex-1 flex-col">
            <Header />
            <div className="relative min-h-0 flex-1 overflow-hidden">
              <AnimatePresence mode="wait" initial={false}>
                <motion.div
                  key={`${view}-${categoryFilter ?? ""}`}
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -6, transition: { duration: 0.12 } }}
                  transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
                  className="absolute inset-0 pt-5"
                >
                  {ready || view === "settings" ? <Page view={view} /> : null}
                </motion.div>
              </AnimatePresence>
              <DetailsPanel />
            </div>
          </main>
        </div>
        <AddDownloadDialog />
        <DialogHost />
        <Toaster />
        <Dialog
          open={Boolean(pending)}
          onOpenChange={(o) => !o && decide(false)}
          title="Allow this browser extension?"
          description={pending ? `${pending.browser} wants to send downloads to Velora.` : undefined}
          icon={<ShieldQuestion size={22} className="mt-0.5 text-accent" />}
          width={480}
          footer={
            <>
              <Button variant="ghost" onClick={() => decide(false)}>
                Don't allow
              </Button>
              <Button variant="primary" onClick={() => decide(true)}>
                Allow
              </Button>
            </>
          }
        >
          <p className="text-[12.5px] text-muted">Only allow this if you just installed the Velora extension yourself. You can revoke it later in Browser integration.</p>
          <p className="mt-3 rounded-xl bg-hover px-3 py-2 font-mono text-[11.5px] break-all" data-selectable>
            {pending?.origin}
          </p>
        </Dialog>
      </TooltipProvider>
    </MotionConfig>
  );
}
