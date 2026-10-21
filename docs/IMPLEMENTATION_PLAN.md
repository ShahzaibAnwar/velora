# Implementation Plan & Task Tracker

Status legend: `[x]` done and verified by automated tests or a recorded manual check ·
`[~]` implemented and compiling, still needs manual verification on Windows · `[ ]` not started.
This file is the project's task tracker; update it in the same change as the work.

## Phase 1 — Research and architecture
- [x] Product vision, feature comparison, architecture, security, release plan (`docs/`)
- [x] Stack decision: Tauri 2, React 19, TypeScript 5.8, Tailwind 4, Rust engine, SQLite
- [x] Licence: MIT (confirmed); repository https://github.com/ShahzaibAnwar/velora

## Phase 2 — Visual foundation
- [x] Design tokens for dark and light themes, accents, density (`src/styles/tokens.css`)
- [x] Primitives: Button, IconButton, Input, Textarea, Switch, Select, Segmented, Dialog, Menu, Popover, Tooltip, Toast, ProgressBar, SegmentBar, SpeedChart
- [x] App shell: integrated title bar with window controls, sidebar with counts and live indicator, header, page transitions
- [x] Original logo and app icons (`assets/brand/velora-icon.svg`, `src-tauri/icons/`)
- [x] Empty, loading and error states; reduced-motion support; animation paused while hidden
- [x] UI reviewed in a browser with labelled preview data (dark and light)

## Phase 3 — Core engine
- [x] HTTP/HTTPS streaming to disk, redirects (tests s01, s10)
- [x] Probe: range support, size, validators, Content-Disposition file names (unit + s04, s05)
- [x] SQLite storage with migrations; settings persistence
- [x] Progress events → UI store; cards, list, details panel (UI tests)
- [x] Pause / resume / cancel / remove; atomic finalisation (s16, s22, s24)

## Phase 4 — Acceleration and reliability
- [x] Range segmentation with `If-Range` and strict `Content-Range` validation (s04, s06)
- [x] Dynamic splitting / work stealing (unit tests in `segment.rs`, s02)
- [x] Checkpoints and resume after restart with revalidation (s08, s09, s19)
- [x] Retry policy: backoff, jitter, `Retry-After`, permanent vs transient (s11, s12, s13, s23)
- [x] Global + per-download bandwidth limits (s21); scheduled limit window (unit-tested window logic)
- [x] SHA-256 verification (s18)
- [x] Disk-full and invalid destination handling (s14, s20)
- [x] Engine integration suite: 25 scenarios against a local server
- [x] Release binary end-to-end (2026-10-21): 96 MiB download via the native-host bridge, SHA-256 identical; forced kill at 56/200 MiB → automatic resume on restart, SHA-256 identical
- [x] Idle CPU after fix: ~1.7% of one core with the window open, <1% hidden in the tray (all Velora processes, this PC)

## Phase 5 — Advanced management
- [x] Queue: max active, priorities, reorder, pause/resume all, persisted queue state (s17)
- [x] Scheduling: start at a time, queue time window
- [x] Categories and per-category folders
- [x] Duplicate URL and destination conflict handling (s15)
- [x] History, search, import/export
- [~] Settings screens — all wired to the engine/app; needs a manual pass on Windows

## Phase 6 — System integration
Verified on Windows 11 (125% scaling, 1920×1080) with the installed release build, 2026-10-21:
- [x] Native GUI-subsystem window with taskbar button, custom title bar, maximise/restore glyph, no browser launched
- [x] Minimise to tray (option) and close to tray (option); restore by relaunching (single instance); one-time "still running" notification
- [x] Downloads continue while hidden in the tray: 200 MiB started and finished while hidden, SHA-256 identical
- [x] Window size/position remembered; first launch fits small or scaled screens
- [x] NSIS per-user installer: install, upgrade over an existing install, Start-menu and desktop shortcuts, Add/Remove Programs entry, uninstall
- [~] Tray icon menu clicks, taskbar progress bar, launch on startup, clipboard monitoring — need a hands-on check
- [ ] Windows 10 installer test (no Windows 10 machine available)

## Phase 7 — Browser integration
- [x] Native-host mode in `velora.exe`, framing and invocation detection (unit tests)
- [~] Named-pipe bridge with token and origin approval
- [~] Native messaging registration (Chrome, Edge, Chromium, Firefox) from the app
- [~] Chromium MV3 extension: context menu, popup, media scan, optional handoff
- [~] Firefox extension (same code, MV3 manifest)

## Phase 8 — QA and release
- [x] README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, CHANGELOG, LICENSE
- [x] Issue/PR templates, CI and release workflows, cargo-deny policy
- [ ] Clean-VM installer test on Windows 10 and 11
- [ ] Code signing
- [ ] Measured performance report (`scripts/bench-server.mjs`, methodology in RELEASE_PLAN.md)
- [~] Public repository prepared locally (initial commit); push and first draft pre-release await maintainer approval

## Next priorities
1. Manual Windows pass over everything marked `[~]` (tray, notifications, installer, extension flow).
2. Code signing certificate and signed installer.
3. Virtualised download list for very large histories (>2,000 items).
4. FTP support and per-host connection limits.
5. Localisation framework.
