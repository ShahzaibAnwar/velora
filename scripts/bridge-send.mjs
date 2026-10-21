// Talk to a running Velora through its native-messaging host, exactly as the browser extension does.
// Useful for testing the bridge without a browser.
//
//   node scripts/bridge-send.mjs <path-to-velora.exe> '{"type":"ping"}'
//   node scripts/bridge-send.mjs <path-to-velora.exe> '{"type":"add","url":"http://127.0.0.1:8787/bench.bin"}'
//
// The host is started with `--native-host`, so its origin is reported as "unknown"; Velora will ask
// for approval in the app before accepting downloads from it.

import { spawn } from "node:child_process";

const [exe, json] = process.argv.slice(2);
if (!exe || !json) {
  console.error("usage: node scripts/bridge-send.mjs <velora.exe> '<json message>'");
  process.exit(2);
}

const child = spawn(exe, ["--native-host"], { stdio: ["pipe", "pipe", "inherit"] });
const payload = Buffer.from(json, "utf8");
const header = Buffer.alloc(4);
header.writeUInt32LE(payload.length, 0);
child.stdin.write(Buffer.concat([header, payload]));

let buf = Buffer.alloc(0);
child.stdout.on("data", (d) => {
  buf = Buffer.concat([buf, d]);
  if (buf.length >= 4) {
    const len = buf.readUInt32LE(0);
    if (buf.length >= 4 + len) {
      console.log(buf.subarray(4, 4 + len).toString("utf8"));
      child.stdin.end();
    }
  }
});
child.on("exit", (code) => process.exit(code ?? 0));
setTimeout(() => {
  console.error("timed out waiting for a reply");
  child.kill();
  process.exit(1);
}, 30000);
