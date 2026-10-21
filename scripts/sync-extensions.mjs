// Copy the shared extension sources from extensions/chromium to extensions/firefox (everything except
// manifest.json, which differs per browser) and refresh the extension icons from the app icons.
//
//   node scripts/sync-extensions.mjs

import { copyFileSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const chromium = join(root, "extensions", "chromium");
const firefox = join(root, "extensions", "firefox");
const appIcons = join(root, "src-tauri", "icons");

for (const dir of [chromium, firefox]) {
  mkdirSync(join(dir, "icons"), { recursive: true });
  for (const [size, source] of [["16", "32x32.png"], ["32", "32x32.png"], ["48", "64x64.png"], ["128", "128x128.png"]]) {
    copyFileSync(join(appIcons, source), join(dir, "icons", `icon-${size}.png`));
  }
}

for (const name of readdirSync(chromium)) {
  if (name === "manifest.json" || name === "icons") continue;
  copyFileSync(join(chromium, name), join(firefox, name));
}
console.log("Extensions synced.");
