import { api, classify, describeError, fileNameOf, isHttp, isXPost, sendToVelora, unsupportedVideoSite } from "./common.js";

const statusEl = document.getElementById("status");
const listEl = document.getElementById("list");
const resultsEl = document.getElementById("results");
const resultsTitle = document.getElementById("results-title");

async function activeTab() {
  const [tab] = await api.tabs.query({ active: true, currentWindow: true });
  return tab;
}

function setStatus(text, tone = "muted") {
  statusEl.textContent = text;
  statusEl.className = tone;
}

async function refreshStatus() {
  const res = await sendToVelora({ type: "ping" });
  if (res.ok) {
    setStatus(
      res.approved ? `Connected to Velora ${res.version ?? ""}`.trim() : "Connected — Velora will ask you to allow this extension once.",
      "ok",
    );
  } else {
    setStatus(describeError(res.error), res.error === "not_running" ? "muted" : "warn");
  }
}

async function send(url, button) {
  const tab = await activeTab();
  button.disabled = true;
  const res = await sendToVelora({
    type: "add",
    url,
    referrer: isHttp(tab?.url) ? tab.url : undefined,
    pageTitle: tab?.title,
    source: "popup",
  });
  button.textContent = res.ok ? "Sent ✓" : "Failed";
  if (!res.ok) setStatus(describeError(res.error), "warn");
}

// Note: `event.currentTarget` is cleared once the event finishes dispatching, so the clicked button
// must be captured before the first `await`.
document.getElementById("send-page").addEventListener("click", async (e) => {
  const button = e.currentTarget;
  const tab = await activeTab();
  if (!isHttp(tab?.url)) {
    setStatus("This page's address can't be downloaded.", "warn");
    return;
  }
  const blocked = unsupportedVideoSite(tab.url);
  if (blocked) {
    setStatus(`Velora doesn't download videos from ${blocked}. They're protected or only available when signed in.`, "warn");
    return;
  }
  await send(tab.url, button);
});

// Runs inside the page (activeTab + scripting). Collects candidate URLs only; it changes nothing.
function scanPage() {
  const found = new Map();
  const add = (raw, tag) => {
    try {
      const url = new URL(raw, location.href).href;
      if (!found.has(url)) found.set(url, { url, tag });
    } catch {
      /* ignore malformed URLs */
    }
  };
  document.querySelectorAll("video, audio").forEach((m) => {
    if (m.currentSrc) add(m.currentSrc, m.tagName.toLowerCase());
    if (m.src) add(m.src, m.tagName.toLowerCase());
    m.querySelectorAll("source[src]").forEach((s) => add(s.src, m.tagName.toLowerCase()));
  });
  document.querySelectorAll("a[href]").forEach((a) => add(a.href, "a"));
  return [...found.values()].slice(0, 2000);
}

document.getElementById("scan").addEventListener("click", async (e) => {
  const scanButton = e.currentTarget;
  scanButton.disabled = true;
  const tab = await activeTab();
  let items = [];
  try {
    const [result] = await api.scripting.executeScript({ target: { tabId: tab.id }, func: scanPage });
    items = result?.result ?? [];
  } catch {
    setStatus("This page can't be scanned (browser pages and stores are off limits).", "warn");
    scanButton.disabled = false;
    return;
  }
  let classified = items
    .map((i) => ({ ...i, info: classify(i.url, i.tag) }))
    .filter((i) => i.info)
    .sort((a, b) => (a.info.kind === "file" ? -1 : 1) - (b.info.kind === "file" ? -1 : 1))
    .slice(0, 60);

  if (isXPost(tab?.url)) {
    // X streams its player, but Velora can fetch the post's public MP4 directly.
    classified = [
      { url: tab.url, info: { kind: "file", label: "Video in this post — Velora picks the best MP4" }, title: "X post video" },
      ...classified.filter((i) => i.info.kind === "file"),
    ];
  }
  const blocked = unsupportedVideoSite(tab?.url);

  listEl.replaceChildren();
  resultsEl.hidden = false;
  resultsTitle.textContent = classified.length ? `Found ${classified.length} item${classified.length === 1 ? "" : "s"}` : "Nothing downloadable found";
  if (blocked && !classified.some((i) => i.info.kind === "file")) {
    const li = document.createElement("li");
    li.className = "notice";
    li.textContent = `Velora doesn't download videos from ${blocked}. They're protected or only available when signed in, and Velora never bypasses either.`;
    listEl.append(li);
  }
  for (const item of classified) {
    const li = document.createElement("li");
    const text = document.createElement("div");
    const name = document.createElement("strong");
    name.textContent = item.title ?? fileNameOf(item.url);
    name.title = item.url;
    const label = document.createElement("span");
    label.className = item.info.kind === "file" ? "muted" : "warn";
    label.textContent = item.info.label;
    text.append(name, label);
    li.append(text);
    if (item.info.kind === "file") {
      const btn = document.createElement("button");
      btn.textContent = "Download";
      btn.addEventListener("click", () => send(item.url, btn));
      li.append(btn);
    }
    listEl.append(li);
  }
  scanButton.disabled = false;
});

refreshStatus();
