import { useEffect } from "react";

import { pasteLink } from "@/app/Header";
import { actions, useDialogs } from "@/lib/actions";
import { useStore } from "@/lib/store";
import { isRunning } from "@/types/engine";

function typing(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable || t.getAttribute("role") === "combobox");
}

/**
 * Global shortcuts:
 * Ctrl+N add · Ctrl+V paste link · Ctrl+F or / search · Ctrl+, settings · Space pause/resume the selected
 * download · Delete remove it · Ctrl+1…7 switch views.
 */
export function useHotkeys() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const s = useStore.getState();
      if (s.addOpen || document.querySelector("[role=dialog]")) return;
      const ctrl = e.ctrlKey || e.metaKey;
      const key = e.key.toLowerCase();
      if (ctrl && key === "n") {
        e.preventDefault();
        s.openAdd();
      } else if (ctrl && key === "v" && !typing(e)) {
        e.preventDefault();
        pasteLink();
      } else if ((ctrl && key === "f") || (key === "/" && !typing(e))) {
        e.preventDefault();
        window.dispatchEvent(new Event("velora:focus-search"));
      } else if (ctrl && key === ",") {
        e.preventDefault();
        s.setView("settings");
      } else if (ctrl && /^[1-7]$/.test(key)) {
        e.preventDefault();
        const views = ["dashboard", "all", "downloading", "completed", "queued", "scheduled", "failed"] as const;
        s.setView(views[Number(key) - 1]);
      } else if (!typing(e) && s.selectedId) {
        const r = s.downloads[s.selectedId];
        if (!r) return;
        if (key === " ") {
          e.preventDefault();
          if (isRunning(r.status)) actions.pause(r.id);
          else if (r.status !== "completed") actions.resume(r.id);
        } else if (key === "delete") {
          e.preventDefault();
          useDialogs.getState().open("remove", r);
        } else if (key === "enter" && r.status === "completed") {
          actions.openFile(r.id);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
