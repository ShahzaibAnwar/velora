import { api, DEFAULT_HANDOFF } from "./common.js";

const enabled = document.getElementById("enabled");
const extensions = document.getElementById("extensions");
const minSize = document.getElementById("min-size");
const saved = document.getElementById("saved");

async function load() {
  const { handoff } = await api.storage.local.get({ handoff: DEFAULT_HANDOFF });
  const granted = await api.permissions.contains({ permissions: ["downloads"] });
  enabled.checked = handoff.enabled && granted;
  extensions.value = handoff.extensions.join(", ");
  minSize.value = handoff.minSizeMb;
}

document.getElementById("save").addEventListener("click", async () => {
  let on = enabled.checked;
  if (on) {
    // Must be requested from a user gesture; shows the browser's own permission prompt.
    on = await api.permissions.request({ permissions: ["downloads"] });
  } else {
    await api.permissions.remove({ permissions: ["downloads"] }).catch(() => {});
  }
  const handoff = {
    enabled: on,
    extensions: extensions.value
      .split(/[\s,]+/)
      .map((s) => s.replace(/^\./, "").toLowerCase())
      .filter(Boolean),
    minSizeMb: Math.max(0, Number(minSize.value) || 0),
  };
  await api.storage.local.set({ handoff });
  enabled.checked = on;
  saved.hidden = false;
  setTimeout(() => (saved.hidden = true), 1500);
});

load();
