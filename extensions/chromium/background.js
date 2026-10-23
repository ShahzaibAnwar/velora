import { api, DEFAULT_HANDOFF, extensionOf, fileNameOf, isHttp, isXPost, sendToVelora, unsupportedVideoSite } from "./common.js";

const MENU_LINK = "velora-link";
const MENU_MEDIA = "velora-media";
const MENU_PAGE = "velora-page";

async function createMenus() {
  await api.contextMenus.removeAll();
  api.contextMenus.create({ id: MENU_LINK, title: "Download link with Velora", contexts: ["link"] });
  api.contextMenus.create({ id: MENU_MEDIA, title: "Download with Velora", contexts: ["image", "video", "audio"] });
  api.contextMenus.create({ id: MENU_PAGE, title: "Send page address to Velora", contexts: ["page"] });
}

api.runtime.onInstalled.addListener(createMenus);
api.runtime.onStartup?.addListener(createMenus);

async function flash(ok) {
  try {
    await api.action.setBadgeBackgroundColor({ color: ok ? "#34E6A3" : "#FF718B" });
    await api.action.setBadgeText({ text: ok ? "✓" : "!" });
    setTimeout(() => api.action.setBadgeText({ text: "" }), 2500);
  } catch {
    // Badges are cosmetic.
  }
}

api.contextMenus.onClicked.addListener(async (info, tab) => {
  let url = info.menuItemId === MENU_LINK ? info.linkUrl : info.menuItemId === MENU_MEDIA ? info.srcUrl : info.pageUrl;
  // Sites play videos as browser streams (blob:); send the page instead and Velora reads it (the
  // post's MP4 on X, yt-dlp elsewhere).
  if (info.menuItemId === MENU_MEDIA && !isHttp(url) && isHttp(info.pageUrl)) url = info.pageUrl;
  const magnet = /^magnet:\?/i.test(url ?? "");
  // DRM streaming services are not sent; Velora could only save the web page itself.
  if ((!isHttp(url) && !magnet) || unsupportedVideoSite(url)) {
    await flash(false);
    return;
  }
  const res = await sendToVelora({
    type: "add",
    url,
    referrer: isHttp(tab?.url) ? tab.url : undefined,
    pageTitle: tab?.title,
    source: "context-menu",
  });
  await flash(res.ok);
});

// --- Optional download handoff -------------------------------------------------------------------
// Off by default and only active once the user grants the optional "downloads" permission in the
// options page. Matching downloads are sent to Velora first; the browser's copy is cancelled only if
// Velora accepted the link, so nothing is lost if Velora is unavailable.

async function onDownloadCreated(item) {
  const { handoff } = await api.storage.local.get({ handoff: DEFAULT_HANDOFF });
  if (!handoff.enabled) return;
  const url = item.finalUrl || item.url;
  if (!isHttp(url) || item.byExtensionId === api.runtime.id) return;
  const ext = extensionOf(url) || extensionOf("https://x/" + (item.filename || "").split(/[\\/]/).pop());
  if (!handoff.extensions.includes(ext)) return;
  if (item.totalBytes > 0 && item.totalBytes < handoff.minSizeMb * 1024 * 1024) return;
  const res = await sendToVelora({
    type: "add",
    url,
    referrer: isHttp(item.referrer) ? item.referrer : undefined,
    filename: fileNameOf(url),
    source: "handoff",
  });
  if (res.ok) {
    try {
      await api.downloads.cancel(item.id);
      await api.downloads.erase({ id: item.id });
    } catch {
      // The browser may already have finished a tiny file.
    }
  }
  await flash(res.ok);
}

function registerHandoff() {
  if (api.downloads && !api.downloads.onCreated.hasListener(onDownloadCreated)) {
    api.downloads.onCreated.addListener(onDownloadCreated);
  }
}

registerHandoff();
api.permissions.onAdded.addListener(registerHandoff);
