// Local benchmark server for repeatable throughput tests (see docs/RELEASE_PLAN.md).
//
// Serves a deterministic file with Range support and a per-connection speed cap, which models CDNs
// that limit each connection. With a cap, segmented downloads should scale with connections until the
// link is saturated; without one, extra connections should make little difference.
//
//   node scripts/bench-server.mjs --size 512 --per-conn 4 --port 8787
//   -> http://127.0.0.1:8787/bench.bin   (512 MiB, 4 MiB/s per connection)

import { createServer } from "node:http";

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? Number(process.argv[i + 1]) : fallback;
};
const SIZE = arg("size", 256) * 1024 * 1024;
const PER_CONN = arg("per-conn", 4) * 1024 * 1024; // bytes/s, 0 = unlimited
const PORT = arg("port", 8787);
const CHUNK = 64 * 1024;
const ETAG = `"bench-${SIZE}"`;

// One reusable block of pseudo-random bytes; the byte at offset i is block[i % block.length].
const block = Buffer.alloc(1024 * 1024);
for (let i = 0; i < block.length; i++) block[i] = (i * 2654435761) >>> 24;

function slice(start, len) {
  const out = Buffer.allocUnsafe(len);
  for (let i = 0; i < len; ) {
    const off = (start + i) % block.length;
    const n = Math.min(len - i, block.length - off);
    block.copy(out, i, off, off + n);
    i += n;
  }
  return out;
}

createServer((req, res) => {
  if (req.url !== "/bench.bin") {
    res.writeHead(404).end();
    return;
  }
  let start = 0;
  let end = SIZE - 1;
  const range = /^bytes=(\d+)-(\d*)$/.exec(req.headers.range ?? "");
  const ifRange = req.headers["if-range"];
  if (range && (!ifRange || ifRange === ETAG)) {
    start = Number(range[1]);
    end = range[2] ? Math.min(Number(range[2]), SIZE - 1) : SIZE - 1;
    if (start > end) {
      res.writeHead(416, { "Content-Range": `bytes */${SIZE}` }).end();
      return;
    }
    res.writeHead(206, { "Content-Range": `bytes ${start}-${end}/${SIZE}`, "Content-Length": end - start + 1, ETag: ETAG, "Accept-Ranges": "bytes" });
  } else {
    res.writeHead(200, { "Content-Length": SIZE, ETag: ETAG, "Accept-Ranges": "bytes" });
  }
  if (req.method === "HEAD") return res.end();

  let pos = start;
  const began = Date.now();
  const pump = () => {
    while (pos <= end) {
      if (PER_CONN > 0) {
        const allowed = ((Date.now() - began) / 1000) * PER_CONN + CHUNK;
        if (pos - start > allowed) return setTimeout(pump, 10);
      }
      const n = Math.min(CHUNK, end - pos + 1);
      const ok = res.write(slice(pos, n));
      pos += n;
      if (!ok) return res.once("drain", pump);
    }
    res.end();
  };
  req.on("close", () => (pos = end + 1));
  pump();
}).listen(PORT, "127.0.0.1", () => {
  console.log(`Serving ${SIZE / 1024 / 1024} MiB at http://127.0.0.1:${PORT}/bench.bin (${PER_CONN ? `${PER_CONN / 1024 / 1024} MiB/s per connection` : "unlimited"})`);
});
