// Mirrors `crates/velora-engine/src/model.rs` and `settings.rs`. Keep in sync.

export type DownloadStatus =
  | "queued"
  | "scheduled"
  | "paused"
  | "probing"
  | "downloading"
  | "verifying"
  | "completed"
  | "failed"
  | "cancelled";

export type Priority = "low" | "normal" | "high";
export type Category = "video" | "audio" | "image" | "document" | "archive" | "application" | "other";
export type ChecksumState = "none" | "pending" | "verified" | "mismatch";
/** `http`: Velora's segmented engine. `media`: yt-dlp (+ ffmpeg). `torrent`: BitTorrent. */
export type DownloadKind = "http" | "media" | "torrent";

export interface ErrorInfo {
  code: string;
  message: string;
}

export interface DownloadRecord {
  id: string;
  kind: DownloadKind;
  /** A magnet link for torrents. */
  url: string;
  mediaFormat: string | null;
  infoHash: string | null;
  finalUrl: string | null;
  fileName: string;
  nameAuto: boolean;
  destDir: string;
  destAuto: boolean;
  category: Category;
  status: DownloadStatus;
  priority: Priority;
  queuePos: number;
  totalSize: number | null;
  downloaded: number;
  resumable: boolean;
  connections: number;
  speedLimit: number | null;
  etag: string | null;
  lastModified: string | null;
  mime: string | null;
  referrer: string | null;
  error: ErrorInfo | null;
  createdAt: number;
  startedAt: number | null;
  completedAt: number | null;
  scheduledAt: number | null;
  checksum: string | null;
  checksumState: ChecksumState;
  overwrite: boolean;
  restartPending: boolean;
}

export type StartMode = { mode: "now" } | { mode: "queue" } | { mode: "paused" } | { mode: "schedule"; at: number };

export interface AddRequest {
  url: string;
  fileName?: string | null;
  destDir?: string | null;
  category?: Category | null;
  priority?: Priority;
  start?: StartMode;
  connections?: number | null;
  speedLimit?: number | null;
  checksum?: string | null;
  referrer?: string | null;
  overwrite?: boolean;
  /** yt-dlp format selector from a resolved media variant. */
  mediaFormat?: string | null;
}

export interface ProbeInfo {
  url: string;
  finalUrl: string;
  fileName: string;
  host: string;
  size: number | null;
  mime: string | null;
  resumable: boolean;
  etag: string | null;
  lastModified: string | null;
  contentEncoding: string | null;
  category: Category;
}

export interface MediaVariant {
  url: string;
  label: string;
  width: number | null;
  height: number | null;
  bitrate: number | null;
  mime: string;
  fileName: string;
  /** yt-dlp format selector; when set, `url` is the page and yt-dlp downloads it. */
  format: string | null;
  size: number | null;
}

/** A page (an X post, a video site, an HLS stream…) resolved to what it offers. Best variant first. */
export interface ResolvedMedia {
  source: string;
  pageUrl: string;
  title: string;
  author: string | null;
  variants: MediaVariant[];
}

/** A helper program (yt-dlp, ffmpeg) being downloaded on first use. */
export interface ToolSetup {
  tool: string;
  downloaded: number;
  total: number | null;
}

export interface AddCheck {
  duplicateOf: DownloadRecord | null;
  fileExists: boolean;
  conflictsWith: string | null;
  destDir: string;
  fileName: string;
  suggestedName: string;
  category: Category;
}

export interface ProgressUpdate {
  id: string;
  downloaded: number;
  totalSize: number | null;
  speed: number;
  etaSecs: number | null;
  connections: number;
}

export interface SegmentView {
  start: number;
  end: number;
  downloaded: number;
  active: boolean;
}

export interface DownloadDetails {
  record: DownloadRecord;
  progress: ProgressUpdate | null;
  segments: SegmentView[];
  speedHistory: number[];
  partPath: string;
}

export interface Stats {
  active: number;
  queued: number;
  currentSpeed: number;
  averageSpeed: number;
  completedToday: number;
  downloadedToday: number;
  totalDownloaded: number;
  totalCompleted: number;
  queuePaused: boolean;
  effectiveSpeedLimit: number;
  speedHistory: number[];
}

export type EngineEvent =
  | { type: "upserted"; record: DownloadRecord }
  | { type: "removed"; ids: string[] }
  | { type: "progress"; items: ProgressUpdate[]; totalSpeed: number }
  | { type: "completed"; record: DownloadRecord }
  | { type: "failed"; record: DownloadRecord }
  | { type: "queueState"; paused: boolean };

export type OverwritePolicy = "ask" | "rename" | "overwrite";
export type ProxyMode = "system" | "none" | "manual";

export interface TimeWindow {
  enabled: boolean;
  start: string;
  end: string;
  days: number[];
}

export interface Settings {
  launchOnStartup: boolean;
  startMinimized: boolean;
  minimizeToTray: boolean;
  closeToTray: boolean;
  notifyOnComplete: boolean;
  notifyOnFailure: boolean;
  language: string;
  defaultDir: string;
  autoCategorize: boolean;
  categoryFolders: Record<string, string>;
  overwritePolicy: OverwritePolicy;
  maxActive: number;
  defaultConnections: number;
  maxRetries: number;
  retryMaxDelaySecs: number;
  resumeOnStartup: boolean;
  proxy: { mode: ProxyMode; url: string };
  globalSpeedLimit: number;
  scheduledLimit: { window: TimeWindow; limit: number };
  connectTimeoutSecs: number;
  readTimeoutSecs: number;
  queueWindow: TimeWindow;
  theme: "system" | "dark" | "light";
  accent: "cyan" | "violet" | "mint" | "coral";
  reducedMotion: "system" | "on" | "off";
  density: "comfortable" | "compact";
  clipboardMonitor: boolean;
  diagnosticLogs: boolean;
  approvedOrigins: string[];
  showAddDialogForBrowser: boolean;
}

export type ClearScope = "completed" | "failed" | "finished";

export interface AppInfo {
  version: string;
  dataDir: string;
  logDir: string;
  dbPath: string;
  defaultDownloadDir: string;
  platform: string;
}

export interface NativeMessagingStatus {
  supported: boolean;
  browsers: { id: string; name: string; registered: boolean }[];
  hostPath: string;
  manifestDir: string;
  chromiumExtensionIds: string[];
  firefoxExtensionId: string;
}

export const RUNNING: DownloadStatus[] = ["probing", "downloading", "verifying"];
export const isRunning = (s: DownloadStatus) => RUNNING.includes(s);
export const isFinished = (s: DownloadStatus) => s === "completed" || s === "failed" || s === "cancelled";
