// Typed wrappers around the Tauri commands in `src-tauri/src/commands`.
// Outside Tauri (plain browser), calls go to a clearly labelled preview backend in `mock.ts`.

import type {
  AddCheck,
  AddRequest,
  AppInfo,
  Category,
  ClearScope,
  DownloadDetails,
  DownloadRecord,
  EngineEvent,
  ErrorInfo,
  NativeMessagingStatus,
  Priority,
  ProbeInfo,
  ResolvedMedia,
  Settings,
  Stats,
} from "@/types/engine";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
let invokeImpl: Invoke | null = null;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!invokeImpl) {
    if (isTauri) {
      const core = await import("@tauri-apps/api/core");
      invokeImpl = core.invoke as Invoke;
    } else {
      const mock = await import("./mock");
      invokeImpl = mock.mockInvoke as Invoke;
    }
  }
  return invokeImpl<T>(cmd, args);
}

/** Errors from commands are `{ code, message }`; normalise anything else. */
export function toError(e: unknown): ErrorInfo {
  if (e && typeof e === "object" && "message" in e) {
    const o = e as { code?: string; message: string };
    return { code: o.code ?? "error", message: o.message };
  }
  return { code: "error", message: typeof e === "string" ? e : "Something went wrong." };
}

export const api = {
  listDownloads: () => invoke<DownloadRecord[]>("list_downloads"),
  getDetails: (id: string) => invoke<DownloadDetails | null>("get_details", { id }),
  getStats: () => invoke<Stats>("get_stats"),
  probe: (url: string, referrer?: string | null) => invoke<ProbeInfo>("probe_url", { url, referrer: referrer ?? null }),
  resolveMedia: (url: string) => invoke<ResolvedMedia | null>("resolve_media", { url }),
  checkAdd: (url: string, fileName?: string | null, destDir?: string | null, category?: Category | null) =>
    invoke<AddCheck>("check_add", { url, fileName: fileName ?? null, destDir: destDir ?? null, category: category ?? null }),
  add: (request: AddRequest) => invoke<DownloadRecord>("add_download", { request }),
  pause: (id: string) => invoke<void>("pause_download", { id }),
  resume: (id: string) => invoke<void>("resume_download", { id }),
  restart: (id: string) => invoke<void>("restart_download", { id }),
  cancel: (id: string) => invoke<void>("cancel_download", { id }),
  remove: (ids: string[]) => invoke<void>("remove_downloads", { ids }),
  deleteFile: (id: string) => invoke<void>("delete_download_file", { id }),
  clear: (scope: ClearScope) => invoke<string[]>("clear_downloads", { scope }),
  pauseAll: () => invoke<void>("pause_all"),
  resumeAll: () => invoke<void>("resume_all"),
  setPriority: (id: string, priority: Priority) => invoke<void>("set_priority", { id, priority }),
  reorder: (ids: string[]) => invoke<void>("reorder_queue", { ids }),
  setSpeedLimit: (id: string, limit: number | null) => invoke<void>("set_speed_limit", { id, limit }),
  setConnections: (id: string, connections: number) => invoke<void>("set_connections", { id, connections }),
  schedule: (id: string, at: number | null) => invoke<void>("schedule_download", { id, at }),
  rename: (id: string, name: string) => invoke<DownloadRecord>("rename_download", { id, name }),
  setCategory: (id: string, category: Category) => invoke<DownloadRecord>("set_category", { id, category }),
  verifyChecksum: (id: string, expected?: string | null) => invoke<string>("verify_checksum", { id, expected: expected ?? null }),
  exportDownloads: (path: string) => invoke<void>("export_downloads", { path }),
  importDownloads: (path: string) => invoke<number>("import_downloads", { path }),
  openFile: (id: string) => invoke<void>("open_file", { id }),
  showInFolder: (id: string) => invoke<void>("show_in_folder", { id }),

  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<Settings>("update_settings", { settings }),
  openPath: (which: "data" | "logs" | "downloads") => invoke<void>("open_path", { which }),
  openExternal: (url: string) => invoke<void>("open_external", { url }),
  readClipboard: () => invoke<string>("read_clipboard"),
  setClipboard: (text: string) => invoke<void>("set_clipboard", { text }),
  quit: () => invoke<void>("quit_app"),

  nativeMessagingStatus: () => invoke<NativeMessagingStatus>("native_messaging_status"),
  registerNativeMessaging: () => invoke<NativeMessagingStatus>("register_native_messaging"),
  unregisterNativeMessaging: () => invoke<NativeMessagingStatus>("unregister_native_messaging"),
  bridgeDecide: (origin: string, allow: boolean) => invoke<void>("bridge_decide", { origin, allow }),
  revokeOrigin: (origin: string) => invoke<void>("revoke_origin", { origin }),
  openExtensionFolder: (browser: "chromium" | "firefox") => invoke<void>("open_extension_folder", { browser }),
};

type Unlisten = () => void;

/** Subscribe to a backend event. Works in Tauri and in preview mode. */
export async function listen<T>(event: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (isTauri) {
    const ev = await import("@tauri-apps/api/event");
    return ev.listen<T>(event, (e) => handler(e.payload));
  }
  const mock = await import("./mock");
  return mock.mockListen(event, handler as (p: unknown) => void);
}

export type { EngineEvent };

/** Native folder picker. Returns null if cancelled or unavailable. */
export async function pickFolder(defaultPath?: string): Promise<string | null> {
  if (!isTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const res = await open({ directory: true, multiple: false, defaultPath });
  return typeof res === "string" ? res : null;
}

export async function pickSaveFile(defaultPath: string): Promise<string | null> {
  if (!isTauri) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({ defaultPath, filters: [{ name: "Velora export", extensions: ["json"] }] });
}

export async function pickOpenFile(): Promise<string | null> {
  if (!isTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const res = await open({ multiple: false, filters: [{ name: "Velora export", extensions: ["json"] }] });
  return typeof res === "string" ? res : null;
}

export async function windowControl(action: "minimize" | "toggleMaximize" | "close") {
  if (!isTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const w = getCurrentWindow();
  if (action === "minimize") await w.minimize();
  else if (action === "toggleMaximize") await w.toggleMaximize();
  else await w.close();
}
