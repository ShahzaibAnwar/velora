import { create } from "zustand";

import { api, toError } from "@/lib/api";
import { toast } from "@/lib/toast";
import type { Category, DownloadRecord, Priority } from "@/types/engine";

/** Run an engine action and turn failures into a readable toast. */
export async function attempt<T>(failTitle: string, fn: () => Promise<T>): Promise<T | undefined> {
  try {
    return await fn();
  } catch (e) {
    toast.error(failTitle, toError(e).message);
    return undefined;
  }
}

export const actions = {
  pause: (id: string) => attempt("Couldn't pause", () => api.pause(id)),
  resume: (id: string) => attempt("Couldn't start the download", () => api.resume(id)),
  restart: (id: string) => attempt("Couldn't restart", () => api.restart(id)),
  cancel: (id: string) => attempt("Couldn't cancel", () => api.cancel(id)),
  remove: (ids: string[]) => attempt("Couldn't remove", () => api.remove(ids)),
  openFile: (id: string) => attempt("Couldn't open the file", () => api.openFile(id)),
  showInFolder: (id: string) => attempt("Couldn't open the folder", () => api.showInFolder(id)),
  setPriority: (id: string, p: Priority) => attempt("Couldn't change priority", () => api.setPriority(id, p)),
  setCategory: async (id: string, c: Category) => {
    const r = await attempt("Couldn't move the download", () => api.setCategory(id, c));
    if (r) toast.success("Moved", r.status === "completed" ? `Now in ${r.destDir}` : undefined);
  },
  copyUrl: async (r: DownloadRecord) => {
    const ok = await attempt("Couldn't copy", () => api.setClipboard(r.url));
    if (ok !== undefined) toast.success("Link copied");
  },
  pauseAll: () => attempt("Couldn't pause all", () => api.pauseAll()),
  resumeAll: () => attempt("Couldn't resume", () => api.resumeAll()),
};

export type DialogKind = "rename" | "delete" | "limit" | "schedule" | "checksum" | "remove";

interface DialogState {
  kind: DialogKind | null;
  record: DownloadRecord | null;
  open: (kind: DialogKind, record: DownloadRecord) => void;
  close: () => void;
}

export const useDialogs = create<DialogState>((set) => ({
  kind: null,
  record: null,
  open: (kind, record) => set({ kind, record }),
  close: () => set({ kind: null }),
}));
