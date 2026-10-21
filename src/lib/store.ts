import { create } from "zustand";

import type { DownloadRecord, EngineEvent, ProgressUpdate, Settings, Stats } from "@/types/engine";

export type View =
  | "dashboard"
  | "all"
  | "downloading"
  | "completed"
  | "queued"
  | "scheduled"
  | "failed"
  | "categories"
  | "browser"
  | "history"
  | "settings";

export interface AddPrefill {
  url?: string;
  urls?: string[];
  referrer?: string | null;
  fileName?: string | null;
}

export interface Notice {
  id: string;
  kind: "completed" | "failed";
  title: string;
  at: number;
  downloadId: string;
}

interface State {
  ready: boolean;
  downloads: Record<string, DownloadRecord>;
  progress: Record<string, ProgressUpdate>;
  totalSpeed: number;
  stats: Stats | null;
  settings: Settings | null;
  queuePaused: boolean;
  view: View;
  categoryFilter: string | null;
  search: string;
  selectedId: string | null;
  addOpen: boolean;
  addPrefill: AddPrefill | null;
  notices: Notice[];
  unreadNotices: number;
  /** Ids that just completed, for the one-off success animation. */
  justCompleted: Record<string, number>;

  setAll: (list: DownloadRecord[]) => void;
  applyEvent: (e: EngineEvent) => void;
  setStats: (s: Stats) => void;
  setSettings: (s: Settings) => void;
  setView: (v: View, categoryFilter?: string | null) => void;
  setSearch: (s: string) => void;
  select: (id: string | null) => void;
  openAdd: (prefill?: AddPrefill | null) => void;
  closeAdd: () => void;
  markNoticesRead: () => void;
  clearNotices: () => void;
}

export const useStore = create<State>((set) => ({
  ready: false,
  downloads: {},
  progress: {},
  totalSpeed: 0,
  stats: null,
  settings: null,
  queuePaused: false,
  view: "dashboard",
  categoryFilter: null,
  search: "",
  selectedId: null,
  addOpen: false,
  addPrefill: null,
  notices: [],
  unreadNotices: 0,
  justCompleted: {},

  setAll: (list) => set({ downloads: Object.fromEntries(list.map((r) => [r.id, r])), ready: true }),

  applyEvent: (e) =>
    set((s) => {
      switch (e.type) {
        case "upserted": {
          const prev = s.downloads[e.record.id];
          const downloads = { ...s.downloads, [e.record.id]: e.record };
          let progress = s.progress;
          if (prev && prev.status !== e.record.status && !["probing", "downloading", "verifying"].includes(e.record.status)) {
            progress = { ...s.progress };
            delete progress[e.record.id];
          }
          return { downloads, progress };
        }
        case "removed": {
          const downloads = { ...s.downloads };
          const progress = { ...s.progress };
          for (const id of e.ids) {
            delete downloads[id];
            delete progress[id];
          }
          return { downloads, progress, selectedId: s.selectedId && e.ids.includes(s.selectedId) ? null : s.selectedId };
        }
        case "progress": {
          const progress: Record<string, ProgressUpdate> = {};
          for (const p of e.items) progress[p.id] = p;
          return { progress, totalSpeed: e.totalSpeed };
        }
        case "completed": {
          const notice: Notice = { id: `${e.record.id}-c`, kind: "completed", title: e.record.fileName, at: Date.now(), downloadId: e.record.id };
          return {
            notices: [notice, ...s.notices].slice(0, 30),
            unreadNotices: s.unreadNotices + 1,
            justCompleted: { ...s.justCompleted, [e.record.id]: Date.now() },
          };
        }
        case "failed": {
          const notice: Notice = { id: `${e.record.id}-f-${Date.now()}`, kind: "failed", title: e.record.fileName, at: Date.now(), downloadId: e.record.id };
          return { notices: [notice, ...s.notices].slice(0, 30), unreadNotices: s.unreadNotices + 1 };
        }
        case "queueState":
          return { queuePaused: e.paused };
      }
    }),

  setStats: (stats) => set({ stats, queuePaused: stats.queuePaused }),
  setSettings: (settings) => set({ settings }),
  setView: (view, categoryFilter = null) => set({ view, categoryFilter, selectedId: null }),
  setSearch: (search) => set({ search }),
  select: (selectedId) => set({ selectedId }),
  openAdd: (prefill = null) => set({ addOpen: true, addPrefill: prefill }),
  closeAdd: () => set({ addOpen: false, addPrefill: null }),
  markNoticesRead: () => set({ unreadNotices: 0 }),
  clearNotices: () => set({ notices: [], unreadNotices: 0 }),
}));

export const viewTitles: Record<View, string> = {
  dashboard: "Dashboard",
  all: "All downloads",
  downloading: "Downloading",
  completed: "Completed",
  queued: "Queue",
  scheduled: "Scheduled",
  failed: "Failed",
  categories: "Categories",
  browser: "Browser integration",
  history: "History",
  settings: "Settings",
};

export function matchesView(r: DownloadRecord, view: View): boolean {
  switch (view) {
    case "downloading":
      return ["probing", "downloading", "verifying", "paused"].includes(r.status);
    case "completed":
      return r.status === "completed";
    case "queued":
      return r.status === "queued";
    case "scheduled":
      return r.status === "scheduled";
    case "failed":
      return r.status === "failed" || r.status === "cancelled";
    default:
      return true;
  }
}

export function matchesSearch(r: DownloadRecord, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  return r.fileName.toLowerCase().includes(needle) || r.url.toLowerCase().includes(needle);
}
