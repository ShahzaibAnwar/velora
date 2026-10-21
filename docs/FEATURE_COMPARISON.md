# Feature Comparison

This comparison uses only public sources: vendor websites, feature lists, help pages, changelogs and
open-source code/documentation. IDM was **not** disassembled, decompiled or otherwise
reverse-engineered, and none of its assets are used. Capabilities listed for other products are
summarised from their public documentation and may differ by version.

Legend: ✅ supported · ◐ partial / limited · ❌ not supported · — not applicable.
The **Velora v0.1** column describes what this repository actually implements (see
[IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) for status).

## Core engine

| Capability | IDM (public docs) | Free Download Manager | JDownloader 2 | aria2 | Motrix | Velora v0.1 |
| --- | --- | --- | --- | --- | --- | --- |
| HTTP/HTTPS | ✅ | ✅ | ✅ | ✅ | ✅ (aria2) | ✅ |
| FTP | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ (planned) |
| BitTorrent / Metalink | ❌ | ✅ | ◐ | ✅ | ✅ | ❌ |
| Multi-connection (range) downloads | ✅ dynamic segmentation | ✅ | ✅ chunks | ✅ `-x/-s/-k` | ✅ | ✅ dynamic splitting |
| Dynamic re-splitting of slow/finished segments | ✅ (advertised) | ◐ | ◐ | ◐ (`min-split-size`) | ◐ | ✅ |
| Resume after interruption | ✅ | ✅ | ✅ | ✅ (control file) | ✅ | ✅ (checkpoint file) |
| Resource-change detection on resume | ◐ | ◐ | ◐ | ◐ | ◐ | ✅ (`If-Range`, ETag, Last-Modified, size) |
| Retry with backoff / `Retry-After` | ◐ | ◐ | ✅ | ◐ (`retry-wait`) | ◐ | ✅ exponential + jitter + `Retry-After` |
| Global speed limit | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Per-download speed limit | ◐ | ✅ | ◐ | ✅ | ✅ | ✅ |
| Scheduled speed limit | ◐ | ✅ | ◐ | ❌ | ❌ | ✅ (time window) |
| Checksum verification | ❌ | ◐ | ✅ (CRC/MD5 from host) | ✅ (`--checksum`) | ◐ | ✅ SHA-256 |
| Proxy (HTTP/SOCKS) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ (system / none / manual) |
| Disk preallocation | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ (sparse on NTFS) |

## Management

| Capability | IDM | FDM | JDownloader | aria2 | Motrix | Velora v0.1 |
| --- | --- | --- | --- | --- | --- | --- |
| Queues & max simultaneous downloads | ✅ multiple queues | ✅ | ✅ | ✅ | ✅ | ✅ single queue with priorities |
| Manual reordering | ✅ | ◐ | ✅ | — | ◐ | ✅ |
| Scheduler (start/stop times) | ✅ | ✅ | ◐ (extension) | ❌ | ❌ | ✅ |
| Categories / per-category folder | ✅ | ✅ | ◐ (packages) | ❌ | ❌ | ✅ |
| Duplicate URL / filename conflict handling | ✅ | ✅ | ✅ | ◐ (`auto-file-renaming`) | ◐ | ✅ |
| Batch / list import | ✅ | ✅ | ✅ (link grabber) | ✅ (`-i`) | ✅ | ✅ (multi-URL paste) |
| Import / export of the list | ✅ | ◐ | ✅ | ◐ | ❌ | ✅ (JSON export/import) |
| History / search | ◐ | ✅ | ✅ | ❌ | ◐ | ✅ |
| Clipboard monitoring | ✅ | ✅ | ✅ (default on) | ❌ | ❌ | ✅ (opt-in) |

## Integration

| Capability | IDM | FDM | JDownloader | aria2 | Motrix | Velora v0.1 |
| --- | --- | --- | --- | --- | --- | --- |
| Browser extension | ✅ | ✅ | ✅ (via MyJDownloader / Click'n'Load) | ❌ (third-party) | ◐ | ✅ Chromium + Firefox |
| Browser ↔ app transport | Proprietary | Native messaging | Local HTTP (Click'n'Load) | JSON-RPC (third-party) | JSON-RPC | Native messaging + paired named pipe |
| Intercept all browser downloads | ✅ (default on) | ✅ | ◐ | — | ◐ | ◐ opt-in, filtered by extension/size |
| Video "grabber" on streaming sites | ✅ | ✅ | ✅ (plugins) | ❌ | ❌ | ◐ direct media files only; HLS/DASH labelled, not downloaded |
| Tray, notifications | ✅ | ✅ | ✅ | — | ✅ | ✅ |
| Taskbar progress | ✅ | ✅ | ❌ | — | ✅ | ✅ |
| Remote / headless control | ❌ | ◐ (remote) | ✅ (MyJD) | ✅ (RPC) | ◐ | ❌ |

## What the comparison taught us

1. **Dynamic segmentation is the real acceleration feature.** Splitting a file once at start is not
   enough: when one connection finishes early it should take over half of the largest remaining
   segment. Velora implements this with a minimum split size so it never thrashes.
2. **Most corruption bugs come from resuming against a changed file.** Velora sends `If-Range` on every
   ranged request, validates every `Content-Range`, and refuses to stitch bytes from a different
   resource version.
3. **Silent interception is the most-complained-about behaviour** of existing managers (broken
   authenticated downloads, unwanted pop-ups). Velora makes it opt-in and explains its limits.
4. **Local HTTP control endpoints are a recurring security problem** (CSRF from web pages, other local
   users). Velora uses native messaging and a local-only named pipe with a per-user token and explicit
   extension approval instead.
5. **Traditional managers look dated and dense.** There is room for a modern, calm, legible UI that
   still shows the information power users need (segments, connections, validators, errors).

## Sources consulted

- IDM public website feature pages, FAQ and version history (internetdownloadmanager.com).
- Free Download Manager public website and feature descriptions (freedownloadmanager.org).
- JDownloader public wiki and support knowledge base (jdownloader.org, support.jdownloader.org).
- aria2 manual (aria2.github.io/manual) — options `split`, `max-connection-per-server`,
  `min-split-size`, `continue`, `checksum`, `max-overall-download-limit`.
- Motrix public README and documentation (github.com/agalwood/Motrix).
- RFC 9110 (HTTP Semantics: `Range`, `If-Range`, `Content-Range`, `ETag`, `Retry-After`),
  RFC 6266 / RFC 8187 (`Content-Disposition` and extended filename encoding).
- Chrome and Firefox native messaging documentation; Chrome Manifest V3 permission guidance.
- Microsoft documentation on file naming rules, sparse files, and Windows notifications.
