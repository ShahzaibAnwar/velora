# Changelog

All notable changes to Velora are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- X (Twitter) post videos: a post link resolves to the post's public MP4 files (quality picker in the
  Add dialog; best quality when the browser sends a post directly). Public posts only.
- Extension 0.1.1: "Video in this post" on X, right-clicking an X video sends the post, and a clear
  explanation on sites Velora deliberately doesn't download from (YouTube, Instagram, Facebook…).
- `scripts/inspect-pe-icons.mjs`: validates a .ico or the icon resources embedded in an .exe and can
  extract them for comparison; `scripts/make-ico.ps1` builds the Windows-native icon.
- Installer branding images; product name "Velora Download Manager"; window size/position memory;
  one-time "still running in the tray" notification; maximise/restore glyph.

### Fixed
- Extension popup buttons did nothing ("Send this page's address" never sent): the clicked button was
  read after an `await`, when the browser has already cleared `event.currentTarget`.
- App icon showed as generic in Start/Search and the taskbar: the .ico now uses bitmap frames for small
  sizes (PNG only at 256 px), and the window sets proper large and small icons for the current DPI.
- The NSIS uninstaller (`uninstall.exe`) had a corrupt icon resource: with no uninstaller icon set,
  NSIS wrote its built-in 7-image layout over our 8-image icon. `bundle.windows.nsis.uninstallerIcon`
  now uses the same `icon.ico`.
- The window opened maximised on small or highly scaled screens; it now fits and centres on first launch.
- Idle CPU use with the dashboard open (≈14% → ≈2% of one core).

## [0.1.0-alpha.1] — not yet released

First development build.

### Added
- Rust download engine: HTTP/HTTPS, redirects, segmented range downloads with dynamic splitting,
  `If-Range` and `Content-Range` validation, crash-safe checkpoints, resume after restart with remote
  revalidation, retries with backoff/jitter/`Retry-After`, global, per-download and scheduled speed
  limits, SHA-256 verification, sparse preallocation on NTFS, free-space check.
- Persistent queue with priorities and reordering, scheduling, queue time window, categories and
  per-category folders, duplicate and file-name conflict handling, history, import/export.
- Desktop app (Tauri 2): dashboard with live speed chart, download cards, details panel with segment
  map, Add Download dialog with server inspection, queue manager, categories, history, settings.
- Windows integration: tray, close/minimise to tray, notifications, taskbar progress, autostart,
  single instance, opt-in clipboard monitoring, per-user NSIS installer.
- Browser integration: Chromium and Firefox MV3 extensions, native messaging host mode, named-pipe
  bridge with token and per-extension approval, optional download handoff.
- 53 engine tests (28 unit + 25 local-server integration scenarios), 5 app tests, 14 UI tests.

### Known limitations
- Unsigned binaries; SmartScreen/Smart App Control may warn or block.
- No FTP, BitTorrent, HLS/DASH or authenticated (cookie) downloads.
- English only.
