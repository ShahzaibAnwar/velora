// Shared helpers for the Velora extension (Chromium and Firefox builds use the same file).

export const api = globalThis.browser ?? globalThis.chrome;
export const HOST = "com.velora.download_manager";

export const DEFAULT_HANDOFF = {
  enabled: false,
  extensions: ["zip", "rar", "7z", "gz", "xz", "iso", "exe", "msi", "dmg", "apk", "mp4", "mkv", "avi", "mov", "mp3", "flac"],
  minSizeMb: 2,
};

const FILE_EXTENSIONS = new Set([
  ...DEFAULT_HANDOFF.extensions,
  "tar", "tgz", "bz2", "zst", "img", "msix", "appx", "deb", "rpm", "webm", "m4v", "wmv", "flv", "m4a", "aac",
  "wav", "ogg", "opus", "pdf", "epub", "docx", "xlsx", "pptx", "csv", "torrent",
]);
const MANIFEST_EXTENSIONS = new Set(["m3u8", "mpd", "ism", "f4m"]);

export function isHttp(url) {
  return typeof url === "string" && /^https?:\/\//i.test(url);
}

export function extensionOf(url) {
  try {
    const path = new URL(url).pathname;
    const last = path.split("/").pop() ?? "";
    const dot = last.lastIndexOf(".");
    return dot > 0 ? last.slice(dot + 1).toLowerCase() : "";
  } catch {
    return "";
  }
}

export function fileNameOf(url) {
  try {
    const last = new URL(url).pathname.split("/").filter(Boolean).pop() ?? "";
    return decodeURIComponent(last) || new URL(url).hostname;
  } catch {
    return url;
  }
}

/**
 * Classify a URL found on a page. Direct files, magnet links and HLS/DASH playlists (which Velora
 * downloads with yt-dlp and ffmpeg) are offered; other manifests and MediaSource/blob streams are
 * labelled but not sent (send the page itself instead).
 */
export function classify(url, tag) {
  if (/^blob:/i.test(url)) return { kind: "stream", label: "Browser stream — send this page instead" };
  if (/^magnet:\?/i.test(url)) return { kind: "file", label: "Magnet link (torrent)" };
  if (!isHttp(url)) return null;
  const ext = extensionOf(url);
  if (ext === "m3u8") return { kind: "file", label: "HLS stream" };
  if (ext === "mpd") return { kind: "file", label: "DASH stream" };
  if (MANIFEST_EXTENSIONS.has(ext)) return { kind: "manifest", label: "Streaming playlist — not supported" };
  if (FILE_EXTENSIONS.has(ext)) return { kind: "file", label: ext.toUpperCase() + " file" };
  if (tag === "video" || tag === "audio") return { kind: "file", label: tag === "video" ? "Video file" : "Audio file" };
  return null;
}

/** An X (Twitter) post address; Velora resolves these to the post's public MP4 files. */
export function isXPost(url) {
  try {
    const u = new URL(url);
    const host = u.hostname.replace(/^(www|mobile)\./, "");
    return (host === "x.com" || host === "twitter.com") && /\/status(es)?\/\d+/.test(u.pathname);
  } catch {
    return false;
  }
}

const UNSUPPORTED_SITES = [
  [/(^|\.)netflix\.com$|(^|\.)primevideo\.com$|(^|\.)disneyplus\.com$|(^|\.)hulu\.com$|(^|\.)max\.com$|(^|\.)hotstar\.com$/, "this streaming service"],
];

/**
 * Streaming services whose videos are DRM-protected; Velora never bypasses DRM. Other video sites
 * (YouTube, Instagram…) are sent to Velora, which reads them with yt-dlp.
 */
export function unsupportedVideoSite(url) {
  try {
    const host = new URL(url).hostname;
    const hit = UNSUPPORTED_SITES.find(([re]) => re.test(host));
    return hit ? hit[1] : null;
  } catch {
    return null;
  }
}

/** Send a message to Velora through native messaging. Never throws. */
export async function sendToVelora(message) {
  try {
    const res = await api.runtime.sendNativeMessage(HOST, message);
    return res ?? { ok: false, error: "No response from Velora." };
  } catch (e) {
    const text = String(e?.message ?? e);
    if (/not found|not installed|native messaging host|access to the specified/i.test(text)) {
      return { ok: false, error: "not_installed" };
    }
    return { ok: false, error: text };
  }
}

export function describeError(error) {
  switch (error) {
    case "not_installed":
      return "Browser integration isn't set up. In Velora, open Browser Integration and choose Connect browsers.";
    case "not_running":
      return "Velora isn't running. Sending a link will start it.";
    default:
      return error || "Something went wrong.";
  }
}
