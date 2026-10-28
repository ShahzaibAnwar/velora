# Architecture

## Overview

```
┌──────────────────────────── velora.exe (one binary) ─────────────────────────────┐
│                                                                                    │
│  WebView2 UI (React + TypeScript)                                                  │
│    features/*  ──►  lib/api.ts (typed invoke wrappers)  ◄── lib/events.ts          │
│          ▲ Tauri commands                         ▲ Tauri events (throttled 2/s)   │
│  ────────┼────────────────────────────────────────┼─────────────────────────────   │
│  src-tauri (app crate)                            │                                │
│    commands/   system/ (tray, notifications, taskbar, clipboard, autostart)        │
│    bridge/ (named-pipe server for the browser)    │                                │
│          │                                        │                                │
│          ▼                                        │                                │
│  crates/velora-engine  (no Tauri dependency)      │                                │
│    Engine ── scheduler loop ── queue ── DownloadTask(s) ── segment workers         │
│      │            │                                   │                            │
│      ▼            ▼                                   ▼                            │
│    storage/ (SQLite: records, settings)      checkpoint/ (*.velora.json)           │
│    net/ (reqwest client, probe, range validation, retry policy, rate limiter)      │
│    security/ (URL, file name & path rules, log redaction)                          │
└────────────────────────────────────────────────────────────────────────────────────┘
         ▲  stdio (native messaging)
  velora.exe --native-host  (same binary, no window) ◄── Browser extension (MV3)
```

### Why these choices

| Decision | Reason | Alternatives considered |
| --- | --- | --- |
| **Tauri 2** | Uses the WebView2 runtime already on Windows 10/11, small installer, Rust backend, capability-based permission system. | Electron (ships Chromium + Node, ~10× bigger), WinUI 3 (Windows-only, C#). |
| **Engine as its own crate** | The engine has no UI or Tauri dependency, so it can be tested headless against a local server and reused by a CLI later. | Engine inside the app crate. |
| **reqwest 0.12 with the platform TLS stack** | Mature async HTTP; on Windows it uses SChannel, so corporate root certificates in the Windows store work. Certificate validation is never disabled. | hyper directly (more code), rustls (needs separate root store handling). |
| **SQLite (rusqlite, bundled)** | One file, transactional, no server. WAL mode. | sled/redb (less tooling), JSON files (no transactions). |
| **Per-download JSON checkpoint** | Segment progress is written atomically (write temp → flush → rename) next to the partial file, only *after* the data it describes has been flushed. A crash can lose progress, never correctness. | Storing segments in SQLite (works too, but couples hot-path writes to the DB). |
| **Single binary for the native host** | When a browser starts Velora with an extension origin argument, `main` switches to native-host mode before any window is created. No sidecar to sign, ship and keep in sync. | A second `velora-host.exe`. |
| **Named pipe + token for browser → app** | Local-only (remote clients rejected), per-user token file, explicit extension approval in the app. | Localhost HTTP (vulnerable to CSRF/DNS rebinding unless carefully hardened). |
| **Hand-written TS mirror types** | The command surface is small; `src/types/engine.ts` mirrors `velora_engine::model` with `serde(rename_all = "camelCase")`. | ts-rs / specta code generation (more build machinery). |

## Engine

### Lifecycle of a download

```
 add ──► Queued ──(slot free & queue running)──► Probing ──► Downloading ──► Verifying ──► Completed
   │        ▲                                      │             │  ▲
   │        └──────────── resume ◄─── Paused ◄─────┴── pause ────┘  │ retry (backoff)
   └──► Scheduled ──(time reached)──► Queued                      Failed ──► Retry/Restart
```

Statuses: `queued`, `scheduled`, `paused`, `probing`, `downloading`, `verifying`, `completed`,
`failed`, `cancelled`.

### Probe

`GET <url>` with `Range: bytes=0-0`, `Accept-Encoding: identity`, redirects followed (max 10).

| Response | Conclusion |
| --- | --- |
| `206` with `Content-Range: bytes 0-0/N` | Ranges supported, size `N`. Resume and segmentation allowed. |
| `200` with `Content-Length: N` | No range support. Single stream, size known, **no resume**. |
| `200` without length (chunked) | Single stream, size unknown, no resume. |
| Any `Content-Encoding` other than `identity` | Single stream, bytes saved exactly as sent, no resume. |
| `4xx`/`5xx` | Classified by the retry policy (see below). |

The probe records the final URL, `ETag`, `Last-Modified`, `Content-Type`, and the file name from
`Content-Disposition` (RFC 6266 / RFC 8187 `filename*`) or the URL path.

### Segmentation and dynamic splitting

- Initial segment count = `min(connections, size / MIN_SEGMENT)` where `MIN_SEGMENT` is 1 MiB.
  The default connection count is 8 (configurable 1–16). Small files use one connection.
- Every ranged request carries `Range: bytes=a-b` and `If-Range: <ETag | Last-Modified>`.
- Every `206` response is validated: the `Content-Range` start must equal `a`, the end must be ≤ `b`,
  the total must equal the known size, and the `ETag` (if present) must match. A `200` to a ranged
  request means the resource changed or the server dropped range support → the task stops instead of
  stitching mismatched bytes.
- When a worker finishes its segment it **steals work**: it finds the segment with the most remaining
  bytes, and if that remainder is at least `2 × MIN_SPLIT` (512 KiB) it shrinks that segment's end to
  the midpoint and starts a new connection on the second half. Ends only ever move down, and the owner
  re-reads its end before every write, so two workers never claim the same bytes. Total connections
  never exceed the configured count.
- Workers buffer up to 512 KiB and write with positional I/O (`seek_write`) on a blocking thread.
  The file is created sparse on NTFS so writing near the end does not zero-fill gigabytes first.

### Progress, checkpoints and finalisation

- Each segment tracks `start`, `end` (inclusive) and `written` (bytes flushed to the OS).
- Every second (and on pause/stop) the task calls `sync_data` on the part file and then atomically
  replaces `<name>.velora.json` with `{url, final_url, size, etag, last_modified, segments}`.
- On completion: all segments must be fully written, the file length must equal the size, the optional
  SHA-256 is checked, the file is flushed, and `<name>.velora-part` is renamed to the final name. Only
  then is the status set to `completed`.

### Resume after restart

1. Load the record from SQLite and the checkpoint file. Missing/invalid checkpoint or part file → start
   over (the partial data is discarded, never trusted).
2. Re-probe the URL. If size, strong ETag or Last-Modified changed → status `failed` with
   *"The file on the server changed"* and a **Restart** action. Nothing is appended to stale data.
3. Otherwise continue the unfinished segments with `If-Range` protection.

### Retry policy

| Condition | Action |
| --- | --- |
| Connect/read timeouts, connection reset, unexpected EOF, `408`, `425`, `429`, `500`, `502`, `503`, `504` | Retry the segment. Delay = `Retry-After` if present (capped at 5 min) else full-jitter exponential backoff (1 s base, 60 s cap). |
| `400`, `401`, `403`, `404`, `405`, `410`, `451`, TLS/certificate errors, invalid URL | Fail immediately with a readable message. |
| Disk full / access denied / path too long | Fail immediately; nothing retried. |
| Retry budget (default 5 attempts without progress) exhausted | Fail; user can Retry. |

Progress resets the attempt counter, so a slow-but-alive download is never failed by the budget.

### Concurrency and limits

- `max_active` downloads (default 3) are started automatically from the queue, ordered by
  priority (high → low) then queue position. A download the user starts manually always starts.
- Per download: at most `connections` workers. Globally: bounded by `max_active × 16`.
- Bandwidth: a shared token bucket (global) plus an optional per-download bucket. Workers acquire
  tokens before reading the next chunk, so TCP back-pressure slows the real transfer.
- Scheduled speed limit: a different global limit inside a daily time window.

### Scheduler loop (1 s tick)

Promotes `scheduled` items whose time has arrived, applies the queue start/stop window and the
scheduled speed limit, fills free slots from the queue, and samples speed for the dashboard chart.

## Download kinds

Every record has a `kind` that picks the task the scheduler runs. Queueing, pause/resume, limits,
history and the UI are shared; only the transfer differs.

| Kind | Used for | Engine |
| --- | --- | --- |
| `http` | Direct file links | Velora's own segmented engine (above). |
| `media` | Video sites (YouTube, Vimeo, Instagram, TikTok…), HLS `.m3u8` and DASH `.mpd` streams, other pages yt-dlp recognises | yt-dlp, with ffmpeg to merge separate video/audio or remux streams. |
| `torrent` | `magnet:` links and `.torrent` URLs | librqbit, in-process. |

### Media (yt-dlp + ffmpeg)

- **Resolving.** X posts use the built-in resolver. Known video sites and stream manifests go to
  `yt-dlp -J`; other HTML pages are tried only once yt-dlp is installed, so plain file links never
  wait for it. The format list becomes a few choices (one per resolution plus audio only), each
  carrying a yt-dlp format selector. DRM streaming services stay refused.
- **Helpers on first use.** `tools.rs` downloads `yt-dlp.exe` and the LGPL ffmpeg build from the
  projects' official GitHub releases into `%APPDATA%\com.velora.downloadmanager\tools\`. Each file
  is checked against the SHA-256 list from the same release before it is moved into place. yt-dlp is
  fetched in the background 20 s after start-up if missing, and refreshed once it is a week old;
  ffmpeg is fetched only when a download needs a merge or an HLS/DASH remux. Helper downloads give
  up on a server that doesn't answer within 30 s, replace connections that stall (60 s) or crawl
  (under 64 KB/s over 15 s) and resume with a `Range` request; the Add dialog polls
  `get_tool_setup` to show progress instead of "Checking server".
- **Running.** yt-dlp runs without a console window inside a Windows Job Object, so pausing ends it
  and every process it started (its Python child, ffmpeg). Progress comes from a
  `--progress-template` line; separate video and audio streams are added up so the bar only moves
  forward. yt-dlp's `.part` files make pause/resume work; cancelling deletes them.

### Torrents (librqbit)

- One session for the whole app, started on first use. DHT state and each torrent's metadata
  (`<info hash>.torrent`) are kept in `%APPDATA%\com.velora.downloadmanager\torrents\`, so a magnet
  link is resolved through the network only once.
- Outgoing connections only (no listening port, so no firewall prompt). Velora downloads; it
  doesn't seed. A torrent leaves the session when it finishes, pauses or is cancelled. Resuming adds
  it back and librqbit re-checks the pieces on disk.
- Layout: multi-file torrents get their own folder, a single file goes straight into the
  destination. If that name is taken, Velora uses a fresh folder, because the file name is fixed by
  the torrent. The record's `infoHash` marks that Velora chose the path, and only such paths are
  deleted on cancel.

### Keeping the PC awake

While anything is downloading, the app asks Windows not to sleep (`SetThreadExecutionState` with
`ES_SYSTEM_REQUIRED`); the display can still turn off.

## Storage

- `%APPDATA%\com.velora.downloadmanager\velora.db` — SQLite, WAL. Tables: `downloads`, `settings`.
  Migrations in `src-tauri/migrations` are embedded and applied by `PRAGMA user_version`.
- Partial data lives next to the destination: `<dest>\<name>.velora-part` and `<name>.velora.json`.
  Keeping it on the destination volume makes the final rename atomic.
- Logs: `%LOCALAPPDATA%\com.velora.downloadmanager\logs\velora.log` (rotated daily, 7 files).
  URLs are logged with query strings and credentials removed.
- Browser bridge token: `%APPDATA%\com.velora.downloadmanager\bridge.token` (32 random bytes, hex).

## App crate (`src-tauri`)

| Module | Responsibility |
| --- | --- |
| `commands/` | Thin `#[tauri::command]` wrappers. Validate input, call the engine, map errors to `{code, message}`. |
| `events.rs` | Forwards engine events to the webview, throttled to two progress batches per second. |
| `system/tray.rs` | Tray icon & menu (show, pause all, resume all, quit), close/minimise-to-tray. |
| `system/notify.rs` | Completion / failure notifications (setting-controlled). |
| `system/taskbar.rs` | Aggregated taskbar progress. |
| `system/clipboard.rs` | Opt-in watcher: polls `GetClipboardSequenceNumber` (cheap), reads text only when it changes. |
| `system/native_messaging.rs` | Writes/removes the native-host manifests and `HKCU` registry keys on user request. |
| `bridge/` | Named-pipe server (`first_pipe_instance`, remote clients rejected) and the native-host client. |

## Frontend (`src`)

```
src/
  app/            App shell, router state, providers, title bar
  components/     Design-system primitives (Button, Dialog, Menu, Switch, Progress, Tooltip…)
  features/       dashboard, downloads, queue, categories, history, settings, browser, add-download
  hooks/          useEngineEvents, useHotkeys, useReducedMotion…
  lib/            api.ts (invoke wrappers), format.ts, store.ts (zustand), motion.ts
  styles/         tokens.css (dark/light design tokens), globals.css
  types/          engine.ts (mirrors Rust model)
```

- State: a single zustand store keyed by download id. Progress events patch only the changed fields;
  components subscribe with selectors so a progress tick re-renders only the affected card.
- Motion: `motion/react` springs for surfaces, 150–300 ms tweens for micro-interactions, `MotionConfig`
  honours *reduced motion* (OS setting or app setting). The speed chart and progress shimmer pause when
  `document.hidden`.

## Browser integration

```
Extension ──connectNative──► velora.exe --native-host (stdio, 4-byte length + JSON)
                                   │  reads bridge.token
                                   ▼
                     \\.\pipe\velora-bridge-<user>  ──►  app bridge server
                                                          │ unknown origin? → "Allow this extension?" prompt
                                                          ▼
                                                        Engine.add(...)
```

- The native-host manifest lists exact extension IDs (`allowed_origins` / `allowed_extensions`).
- The bridge accepts: `ping`, `add` (`url`, optional `referrer`, `filename`, `pageTitle`), `status`.
- Extension permissions: `contextMenus`, `nativeMessaging`, `activeTab`, `scripting`, `storage`;
  `downloads` is an *optional* permission requested only if the user enables download handoff.

## Security boundaries

See [SECURITY.md](SECURITY.md). Summary: http/https only, TLS always verified, file names sanitised,
destination confined to the chosen folder, no automatic execution, opt-in clipboard and interception,
redacted logs, local-only authenticated browser bridge.
