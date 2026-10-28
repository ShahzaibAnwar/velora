# Changelog

All notable changes to Velora are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0-alpha.3] — 2026-10-28 (development pre-release)

### Fixed
- A first YouTube (or other video) link could stay on "Checking server" indefinitely while yt-dlp was
  being fetched: GitHub requests had no response timeout, and a stalled or very slow connection was
  waited on forever. Helper downloads now time out (30 s without a response, 60 s without data),
  replace connections slower than 64 KB/s and resume from the current byte, and the Add dialog shows
  "Setting up video support · N%" while this happens.

### Changed
- yt-dlp is fetched in the background 20 s after start-up when it isn't installed yet, so the first
  video link doesn't wait for it.

## [0.1.0-alpha.2] — 2026-10-23 (development pre-release)

### Added
- Video sites and streams through yt-dlp: YouTube, Vimeo, Instagram, TikTok, Facebook, Reddit,
  SoundCloud and the other sites yt-dlp supports, plus HLS (`.m3u8`) and DASH (`.mpd`) links. The Add
  dialog lists one choice per resolution plus audio only. yt-dlp and ffmpeg are downloaded from their
  official GitHub releases on first use and checked against the published SHA-256 lists. Pause and
  resume continue partial files. DRM streaming services are still refused.
- BitTorrent: magnet links and `.torrent` URLs (librqbit). Metadata is cached, pieces already on disk
  are re-checked on resume, and multi-file torrents get their own folder. Download only; no seeding
  and no listening port.
- The PC no longer goes to sleep while downloads are running.
- Extension 0.1.3: sends video pages (right-click a playing video) and magnet links, and offers HLS/DASH
  links found on a page. Only DRM streaming services are still refused.

### Security
- yt-dlp runs with `--ignore-config`, so a downloaded file named `yt-dlp.conf` can't inject options.
- Helper downloads give up after 60 s without data instead of waiting forever.
- Dependency updates from the audit: tokio 1.54.1 (1.54.0 was yanked for an `Interval` race), zip 9
  (extraction advisories; Velora never used the affected `extract`), clipboard-win 5.4.3 (yanked
  version), and torrent piece hashing moved from the unmaintained crypto-hash crate to ring.
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

## [0.1.0-alpha.1] — 2026-10-21 (development pre-release)

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
