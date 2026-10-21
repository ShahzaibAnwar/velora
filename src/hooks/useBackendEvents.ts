import { useEffect } from "react";

import { api, listen, toError } from "@/lib/api";
import { useStore } from "@/lib/store";
import { toast } from "@/lib/toast";
import type { EngineEvent } from "@/types/engine";

export interface ApprovalRequest {
  origin: string;
  browser: string;
}

/** Load initial state and subscribe to engine, clipboard and browser-bridge events. */
export function useBackendEvents(onApproval: (req: ApprovalRequest) => void) {
  useEffect(() => {
    const s = useStore.getState();
    let disposed = false;
    const unlisten: (() => void)[] = [];
    const sub = <T,>(event: string, handler: (p: T) => void) =>
      listen<T>(event, handler).then((u) => (disposed ? u() : unlisten.push(u)));

    sub<EngineEvent>("engine://event", (e) => useStore.getState().applyEvent(e));
    sub<{ url: string }>("clipboard://link", ({ url }) => {
      const name = decodeURIComponent(url.split("?")[0].split("/").pop() || url);
      toast.info("Download link copied", name, { label: "Download with Velora", run: () => useStore.getState().openAdd({ url }) });
    });
    sub<{ url: string; referrer?: string | null; filename?: string | null }>("bridge://add", (p) =>
      useStore.getState().openAdd({ url: p.url, referrer: p.referrer ?? null, fileName: p.filename ?? null }),
    );
    sub<ApprovalRequest>("bridge://approval", onApproval);
    sub<null>("app://add", () => useStore.getState().openAdd());
    sub<{ urls: string[] }>("app://urls", ({ urls }) => useStore.getState().openAdd(urls.length === 1 ? { url: urls[0] } : { urls }));

    Promise.all([api.listDownloads(), api.getSettings(), api.getStats()])
      .then(([list, settings, stats]) => {
        s.setAll(list);
        s.setSettings(settings);
        s.setStats(stats);
      })
      .catch((e) => toast.error("Couldn't load your downloads", toError(e).message));

    return () => {
      disposed = true;
      unlisten.forEach((u) => u());
    };
  }, [onApproval]);
}
