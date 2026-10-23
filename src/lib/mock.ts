// PREVIEW BACKEND — DEVELOPMENT FIXTURES ONLY.
//
// Used when the UI runs in a normal browser (`npm run dev` without Tauri), so the interface can be
// designed and tested without the Rust engine. Every number here is simulated sample data. The UI
// shows a "Preview data" badge whenever this module is active. The real app never loads this file.

import type {
  AddRequest,
  Category,
  DownloadDetails,
  DownloadRecord,
  EngineEvent,
  NativeMessagingStatus,
  ProgressUpdate,
  SegmentView,
  Settings,
  Stats,
} from "@/types/engine";

type Handler = (payload: unknown) => void;
const listeners = new Map<string, Set<Handler>>();

export function mockListen(event: string, handler: Handler) {
  if (!listeners.has(event)) listeners.set(event, new Set());
  listeners.get(event)!.add(handler);
  start();
  return () => listeners.get(event)?.delete(handler);
}

function emit(event: string, payload: unknown) {
  listeners.get(event)?.forEach((h) => h(payload));
}
const emitEngine = (e: EngineEvent) => emit("engine://event", e);

const MB = 1024 * 1024;
const now = Date.now();
let counter = 0;
const id = () => `preview-${++counter}`;

function rec(partial: Partial<DownloadRecord> & Pick<DownloadRecord, "fileName" | "url" | "category" | "status">): DownloadRecord {
  return {
    id: id(),
    kind: "http",
    mediaFormat: null,
    infoHash: null,
    finalUrl: partial.url,
    nameAuto: false,
    destDir: "C:\\Users\\you\\Downloads",
    destAuto: true,
    priority: "normal",
    queuePos: counter,
    totalSize: null,
    downloaded: 0,
    resumable: true,
    connections: 8,
    speedLimit: null,
    etag: null,
    lastModified: null,
    mime: null,
    referrer: null,
    error: null,
    createdAt: now - counter * 60_000,
    startedAt: null,
    completedAt: null,
    scheduledAt: null,
    checksum: null,
    checksumState: "none",
    overwrite: false,
    restartPending: false,
    ...partial,
  };
}

const records = new Map<string, DownloadRecord>();
const live = new Map<string, { speed: number; segments: SegmentView[]; history: number[] }>();
const history: number[] = [];

function seed() {
  const list: DownloadRecord[] = [
    rec({ fileName: "ubuntu-24.04.3-desktop-amd64.iso", url: "https://releases.example.org/ubuntu-24.04.3-desktop-amd64.iso", category: "archive", status: "downloading", totalSize: 6114 * MB, downloaded: 2310 * MB, startedAt: now - 300_000 }),
    rec({ fileName: "Project-Showreel-4K.mp4", url: "https://cdn.example.com/media/Project-Showreel-4K.mp4", category: "video", status: "downloading", totalSize: 1840 * MB, downloaded: 1610 * MB, connections: 6, startedAt: now - 200_000 }),
    rec({ fileName: "design-assets-2026.zip", url: "https://files.example.net/design-assets-2026.zip", category: "archive", status: "paused", totalSize: 512 * MB, downloaded: 140 * MB }),
    rec({ fileName: "podcast-episode-142.mp3", url: "https://audio.example.fm/podcast-episode-142.mp3", category: "audio", status: "queued", totalSize: null, queuePos: 1 }),
    rec({ fileName: "VS-Code-Setup-x64.exe", url: "https://downloads.example.dev/VS-Code-Setup-x64.exe", category: "application", status: "queued", queuePos: 2, priority: "high" }),
    rec({ fileName: "annual-report.pdf", url: "https://example.org/reports/annual-report.pdf", category: "document", status: "scheduled", scheduledAt: now + 3 * 3600_000 }),
    rec({ fileName: "dataset-2026-q3.csv.gz", url: "https://data.example.gov/dataset-2026-q3.csv.gz", category: "archive", status: "completed", totalSize: 742 * MB, downloaded: 742 * MB, completedAt: now - 25 * 60_000, checksumState: "verified", checksum: "ab".repeat(32) }),
    rec({ fileName: "wallpaper-aurora.png", url: "https://images.example.com/wallpaper-aurora.png", category: "image", status: "completed", totalSize: 18 * MB, downloaded: 18 * MB, completedAt: now - 3 * 3600_000 }),
    rec({ fileName: "driver-package-31.0.exe", url: "https://support.example.com/driver-package-31.0.exe", category: "application", status: "failed", totalSize: 420 * MB, downloaded: 0, error: { code: "forbidden", message: "The server refused access (HTTP 403). The link may have expired or need a browser session." } }),
    rec({ fileName: "lecture-notes-week-7.docx", url: "https://uni.example.edu/notes/lecture-notes-week-7.docx", category: "document", status: "completed", totalSize: 3 * MB, downloaded: 3 * MB, completedAt: now - 27 * 3600_000 }),
  ];
  for (const r of list) records.set(r.id, r);
}

let settings: Settings = {
  launchOnStartup: false,
  startMinimized: false,
  minimizeToTray: false,
  closeToTray: true,
  notifyOnComplete: true,
  notifyOnFailure: true,
  language: "en",
  defaultDir: "C:\\Users\\you\\Downloads",
  autoCategorize: false,
  categoryFolders: {},
  overwritePolicy: "ask",
  maxActive: 3,
  defaultConnections: 8,
  maxRetries: 5,
  retryMaxDelaySecs: 60,
  resumeOnStartup: true,
  proxy: { mode: "system", url: "" },
  globalSpeedLimit: 0,
  scheduledLimit: { window: { enabled: false, start: "09:00", end: "18:00", days: [1, 2, 3, 4, 5] }, limit: MB },
  connectTimeoutSecs: 20,
  readTimeoutSecs: 60,
  queueWindow: { enabled: false, start: "01:00", end: "07:00", days: [] },
  theme: "system",
  accent: "cyan",
  reducedMotion: "system",
  density: "comfortable",
  clipboardMonitor: false,
  diagnosticLogs: true,
  approvedOrigins: [],
  showAddDialogForBrowser: true,
};

let started = false;
function start() {
  if (started) return;
  started = true;
  seed();
  setInterval(tick, 500);
}

function segmentsFor(r: DownloadRecord): SegmentView[] {
  const total = r.totalSize ?? 0;
  const n = Math.max(1, Math.min(r.connections, 8));
  const len = Math.floor(total / n);
  return Array.from({ length: n }, (_, i) => {
    const start = i * len;
    const end = i === n - 1 ? total - 1 : start + len - 1;
    const frac = Math.min(1, (r.downloaded / Math.max(1, total)) * (0.75 + ((i * 37) % 50) / 100));
    return { start, end, downloaded: Math.floor((end - start + 1) * frac), active: frac < 1 };
  });
}

function tick() {
  const items: ProgressUpdate[] = [];
  for (const r of records.values()) {
    if (r.status === "probing") {
      r.status = "downloading";
      r.totalSize ??= 300 * MB;
      emitEngine({ type: "upserted", record: { ...r } });
    }
    if (r.status !== "downloading") continue;
    const state = live.get(r.id) ?? { speed: 0, segments: [], history: [] };
    const target = (6 + Math.sin(Date.now() / 3000 + r.queuePos) * 3) * MB;
    state.speed = Math.max(0, state.speed * 0.6 + target * 0.4);
    r.downloaded = Math.min(r.totalSize ?? 0, r.downloaded + state.speed / 2);
    state.segments = segmentsFor(r);
    state.history = [...state.history.slice(-59), Math.round(state.speed)];
    live.set(r.id, state);
    const remaining = (r.totalSize ?? 0) - r.downloaded;
    items.push({ id: r.id, downloaded: r.downloaded, totalSize: r.totalSize, speed: Math.round(state.speed), etaSecs: Math.ceil(remaining / Math.max(1, state.speed)), connections: r.connections });
    if (r.totalSize && r.downloaded >= r.totalSize) {
      r.status = "completed";
      r.completedAt = Date.now();
      live.delete(r.id);
      emitEngine({ type: "upserted", record: { ...r } });
      emitEngine({ type: "completed", record: { ...r } });
    }
  }
  const total = items.reduce((a, b) => a + b.speed, 0);
  history.push(total);
  if (history.length > 120) history.shift();
  emitEngine({ type: "progress", items, totalSpeed: total });
}

function update(id: string, f: (r: DownloadRecord) => void) {
  const r = records.get(id);
  if (!r) throw { code: "not_found", message: "That download no longer exists." };
  f(r);
  emitEngine({ type: "upserted", record: { ...r } });
}

const nm: NativeMessagingStatus = {
  supported: true,
  browsers: [
    { id: "chrome", name: "Google Chrome", registered: false },
    { id: "edge", name: "Microsoft Edge", registered: false },
    { id: "chromium", name: "Chromium", registered: false },
    { id: "firefox", name: "Mozilla Firefox", registered: false },
  ],
  hostPath: "C:\\Users\\you\\AppData\\Local\\Velora\\velora.exe",
  manifestDir: "C:\\Users\\you\\AppData\\Roaming\\com.velora.downloadmanager\\native-messaging",
  chromiumExtensionIds: ["ekimhgmgmpdokgffllechdpepeeakmhk"],
  firefoxExtensionId: "velora-extension@velora-download-manager",
};

function categoryOf(name: string): Category {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (["mp4", "mkv", "webm", "mov"].includes(ext)) return "video";
  if (["mp3", "flac", "m4a", "wav"].includes(ext)) return "audio";
  if (["png", "jpg", "jpeg", "gif", "webp"].includes(ext)) return "image";
  if (["pdf", "docx", "xlsx", "txt"].includes(ext)) return "document";
  if (["zip", "rar", "7z", "iso", "gz"].includes(ext)) return "archive";
  if (["exe", "msi", "apk"].includes(ext)) return "application";
  return "other";
}

export async function mockInvoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  start();
  const a = args as Record<string, never>;
  await new Promise((r) => setTimeout(r, cmd === "probe_url" ? 450 : 20));
  const result: unknown = (() => {
    switch (cmd) {
      case "list_downloads":
        return [...records.values()].map((r) => ({ ...r }));
      case "get_stats": {
        const all = [...records.values()];
        const done = all.filter((r) => r.status === "completed");
        const s: Stats = {
          active: all.filter((r) => r.status === "downloading").length,
          queued: all.filter((r) => r.status === "queued").length,
          currentSpeed: history.at(-1) ?? 0,
          averageSpeed: history.length ? Math.round(history.reduce((x, y) => x + y, 0) / history.length) : 0,
          completedToday: done.filter((r) => (r.completedAt ?? 0) > now - 86400_000).length,
          downloadedToday: done.filter((r) => (r.completedAt ?? 0) > now - 86400_000).reduce((x, r) => x + (r.totalSize ?? 0), 0),
          totalDownloaded: done.reduce((x, r) => x + (r.totalSize ?? 0), 0),
          totalCompleted: done.length,
          queuePaused: false,
          effectiveSpeedLimit: settings.globalSpeedLimit,
          speedHistory: history.filter((_, i) => i % 2 === 0),
        };
        return s;
      }
      case "get_details": {
        const r = records.get(a.id);
        if (!r) return null;
        const l = live.get(r.id);
        const d: DownloadDetails = {
          record: { ...r },
          progress: l ? { id: r.id, downloaded: r.downloaded, totalSize: r.totalSize, speed: l.speed, etaSecs: null, connections: r.connections } : null,
          segments: l?.segments ?? (r.totalSize && r.status !== "completed" ? segmentsFor(r) : []),
          speedHistory: l?.history.filter((_, i) => i % 2 === 0) ?? [],
          partPath: `${r.destDir}\\${r.fileName}.velora-part`,
        };
        return d;
      }
      case "probe_url": {
        const url = String(a.url);
        if (/^magnet:/i.test(url)) {
          const name = new URLSearchParams(url.slice(url.indexOf("?") + 1)).get("dn") || "Sample torrent";
          return { url, finalUrl: url, fileName: name, host: "BitTorrent", size: 2310 * MB, mime: "3 files", resumable: true, etag: null, lastModified: null, contentEncoding: null, category: "video" };
        }
        const name = decodeURIComponent(new URL(url).pathname.split("/").pop() || "download");
        return { url, finalUrl: url, fileName: name, host: new URL(url).hostname, size: 734 * MB, mime: "application/octet-stream", resumable: true, etag: '"preview"', lastModified: null, contentEncoding: null, category: categoryOf(name) };
      }
      case "check_add": {
        const url = String(a.url);
        const name = (a.fileName as string) || decodeURIComponent(new URL(url).pathname.split("/").pop() || "download");
        const dup = [...records.values()].find((r) => r.url === url && !["completed", "failed", "cancelled"].includes(r.status)) ?? null;
        return { duplicateOf: dup, fileExists: false, conflictsWith: null, destDir: settings.defaultDir, fileName: name, suggestedName: name, category: categoryOf(name) };
      }
      case "add_download": {
        const req = a.request as unknown as AddRequest;
        const name = req.fileName || decodeURIComponent(new URL(req.url).pathname.split("/").pop() || "download");
        const mode = req.start?.mode ?? "now";
        const r = rec({
          kind: /^magnet:/i.test(req.url) ? "torrent" : req.mediaFormat ? "media" : "http",
          mediaFormat: req.mediaFormat ?? null,
          url: req.url,
          fileName: name,
          category: req.category ?? categoryOf(name),
          status: mode === "now" ? "probing" : mode === "paused" ? "paused" : mode === "schedule" ? "scheduled" : "queued",
          createdAt: Date.now(),
          priority: req.priority ?? "normal",
          connections: req.connections ?? settings.defaultConnections,
          scheduledAt: req.start?.mode === "schedule" ? req.start.at : null,
        });
        records.set(r.id, r);
        emitEngine({ type: "upserted", record: { ...r } });
        return r;
      }
      case "pause_download":
        update(a.id, (r) => (r.status = "paused"));
        live.delete(a.id);
        return null;
      case "resume_download":
      case "restart_download":
        update(a.id, (r) => {
          r.status = "downloading";
          r.error = null;
          r.totalSize ??= 200 * MB;
          if (cmd === "restart_download") r.downloaded = 0;
        });
        return null;
      case "cancel_download":
        update(a.id, (r) => {
          r.status = "cancelled";
          r.downloaded = 0;
        });
        return null;
      case "remove_downloads":
      case "delete_download_file": {
        const ids: string[] = cmd === "remove_downloads" ? (a.ids as unknown as string[]) : [a.id];
        ids.forEach((i) => records.delete(i));
        emitEngine({ type: "removed", ids });
        return null;
      }
      case "clear_downloads": {
        const scope = String(a.scope);
        const ids = [...records.values()]
          .filter((r) => (scope === "completed" ? r.status === "completed" : scope === "failed" ? r.status === "failed" || r.status === "cancelled" : ["completed", "failed", "cancelled"].includes(r.status)))
          .map((r) => r.id);
        ids.forEach((i) => records.delete(i));
        emitEngine({ type: "removed", ids });
        return ids;
      }
      case "pause_all":
        for (const r of records.values()) if (r.status === "downloading") update(r.id, (x) => (x.status = "paused"));
        emitEngine({ type: "queueState", paused: true });
        return null;
      case "resume_all":
        for (const r of records.values()) if (r.status === "paused") update(r.id, (x) => (x.status = "downloading"));
        emitEngine({ type: "queueState", paused: false });
        return null;
      case "set_priority":
        update(a.id, (r) => (r.priority = a.priority));
        return null;
      case "reorder_queue":
        (a.ids as unknown as string[]).forEach((i, idx) => update(i, (r) => (r.queuePos = idx)));
        return null;
      case "set_speed_limit":
        update(a.id, (r) => (r.speedLimit = a.limit));
        return null;
      case "set_connections":
        update(a.id, (r) => (r.connections = a.connections));
        return null;
      case "schedule_download":
        update(a.id, (r) => {
          r.scheduledAt = a.at;
          r.status = a.at ? "scheduled" : "queued";
        });
        return null;
      case "rename_download":
        update(a.id, (r) => (r.fileName = a.name));
        return { ...records.get(a.id)! };
      case "set_category":
        update(a.id, (r) => (r.category = a.category));
        return { ...records.get(a.id)! };
      case "verify_checksum":
        return "ab".repeat(32);
      case "get_settings":
        return { ...settings };
      case "update_settings":
        settings = { ...(a.settings as unknown as Settings) };
        return { ...settings };
      case "app_info":
        return { version: "0.1.0-alpha.2 (preview)", dataDir: "C:\\Users\\you\\AppData\\Roaming\\com.velora.downloadmanager", logDir: "C:\\Users\\you\\AppData\\Local\\com.velora.downloadmanager\\logs", dbPath: "velora.db", defaultDownloadDir: "C:\\Users\\you\\Downloads", platform: "preview" };
      case "native_messaging_status":
        return { ...nm };
      case "register_native_messaging":
        nm.browsers.forEach((b) => (b.registered = true));
        return { ...nm };
      case "unregister_native_messaging":
        nm.browsers.forEach((b) => (b.registered = false));
        return { ...nm };
      case "read_clipboard":
        return "";
      case "resolve_media": {
        const url = String(a.url);
        if (/^https:\/\/(www\.)?(youtube\.com\/watch|youtu\.be\/)/.test(url)) {
          // What yt-dlp reports for a video page (preview data).
          return {
            source: "YouTube",
            pageUrl: url,
            title: "Sample video title (preview data)",
            author: "Example Channel",
            variants: [
              { url, label: "1080p", width: 1920, height: 1080, bitrate: 4_400_000, mime: "video/mp4", fileName: "Sample video title (1080p).mp4", format: "bv*[height=1080][ext=mp4]+ba[ext=m4a]/bv*[height=1080]+ba/b[height=1080]", size: 212 * MB },
              { url, label: "720p", width: 1280, height: 720, bitrate: 2_300_000, mime: "video/mp4", fileName: "Sample video title (720p).mp4", format: "bv*[height=720][ext=mp4]+ba[ext=m4a]/bv*[height=720]+ba/b[height=720]", size: 118 * MB },
              { url, label: "Audio only · m4a", width: null, height: null, bitrate: 129_000, mime: "audio/m4a", fileName: "Sample video title.m4a", format: "ba[ext=m4a]/ba", size: 9 * MB },
            ],
          };
        }
        if (!/^https:\/\/(www\.)?(x|twitter)\.com\/[^/]+\/status\/\d+/.test(url)) return null;
        return {
          source: "X",
          pageUrl: url,
          title: "Sample post text (preview data)",
          author: "example",
          variants: [
            { url: "https://video.twimg.com/sample/vid/avc1/1280x720/a.mp4", label: "1280×720", width: 1280, height: 720, bitrate: 2176000, mime: "video/mp4", fileName: "example_1_720p.mp4", format: null, size: null },
            { url: "https://video.twimg.com/sample/vid/avc1/640x360/b.mp4", label: "640×360", width: 640, height: 360, bitrate: 832000, mime: "video/mp4", fileName: "example_1_360p.mp4", format: null, size: null },
          ],
        };
      }
      default:
        return null;
    }
  })();
  return result as T;
}
