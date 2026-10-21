# Velora Download Manager — Product Vision

> **Download beautifully. Accelerate everything.**

## What Velora is

Velora is a free, open-source download manager for Windows 10 and 11 (64-bit). macOS and Linux come later.
It downloads ordinary HTTP/HTTPS files more reliably and, where the server allows it, faster than a
browser, and wraps that engine in an interface people actually want to keep open.

Velora is built from scratch. It uses no code, binaries, graphics or branding from Internet Download
Manager (IDM) or any other proprietary product. Other download managers were studied only through
their public documentation and publicly observable features (see [FEATURE_COMPARISON.md](FEATURE_COMPARISON.md)).

## Who it is for

| Person | What they need |
| --- | --- |
| People on slow or unreliable connections | Downloads that survive dropped Wi-Fi, sleep and restarts without starting over |
| People who download large files (ISOs, games, datasets, video) | Real resume, multi-connection downloads, integrity checks |
| Power users | Queues, schedules, bandwidth limits, categories, keyboard-first workflow |
| Everyone else | A fast, calm, good-looking app that gets out of the way |

## Product principles

1. **Reliability over decoration.** A download is never marked complete until every byte is on disk,
   verified against the expected size, flushed and atomically renamed into place.
2. **Honest numbers.** Speeds, ETAs, segment maps and "resume supported" badges come from the engine,
   never from guesses. If the server has not proved it supports byte ranges, Velora does not claim resume.
3. **Local-first and private.** No accounts, no telemetry, no ads, no cloud dependency. Clipboard
   monitoring is opt-in and never leaves the machine.
4. **Safe by default.** TLS verification is always on, file names are sanitised, nothing downloaded is
   ever executed automatically, and browser integration requires explicit installation and pairing.
5. **Fast to use.** Adding a download takes one paste and one keypress. Every common action has a shortcut.
6. **Motion with purpose.** Animation explains state changes (a download starting, finishing, moving).
   It is short (150–300 ms), respects *reduced motion*, and stops when the window is hidden.
7. **No fake acceleration.** More connections only help when the bottleneck is per-connection.
   Velora uses bounded, adaptive concurrency and never advertises a fixed speed-up factor.

## Scope for v0.1.0-alpha

- HTTP/HTTPS engine: redirects, segmented range downloads with dynamic splitting, resume across restarts,
  retries with backoff and `Retry-After`, global and per-download bandwidth limits, SHA-256 verification.
- Download management: persistent queue with priorities and reordering, max simultaneous downloads,
  scheduling, categories with per-category folders, duplicate and conflict detection, history and search.
- Windows integration: tray, close/minimise to tray, native notifications, taskbar progress, launch on
  startup, single instance, folder pickers, open file / show in Explorer, NSIS per-user installer.
- Browser integration: Chromium (Chrome/Edge) and Firefox extensions using native messaging with
  explicit pairing; context menu, "send this page", on-demand detection of direct media files.
- Opt-in clipboard monitoring.

## Explicit non-goals

- No DRM circumvention, no bypassing paywalls, logins or access controls.
- No promise to download "any video from any site". Streaming manifests (HLS/DASH), `blob:` and
  MediaSource streams are detected and labelled, not downloaded, in this release.
- No browser cookie export. Downloads that need a logged-in session should stay in the browser.
- No background service: schedules run only while Velora is running (it can live in the tray).
- No FTP, BitTorrent or Metalink in v0.1.

## Success measures (to be measured, not assumed)

- Zero corrupted files across the automated fault-injection suite (interruptions, wrong ranges,
  changed ETags, disk-full).
- Downloads resume after a forced kill of the process.
- Idle CPU close to 0% with the window hidden; UI stays at 60 fps while 5 downloads run.
- Throughput measured against a single browser download on the same URL, network and time of day,
  using the method in [RELEASE_PLAN.md](RELEASE_PLAN.md#performance-methodology).
