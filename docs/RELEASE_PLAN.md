# Release Plan

## Versioning

[Semantic Versioning 2.0](https://semver.org/) with pre-release tags.

| Version | Meaning | Gate |
| --- | --- | --- |
| `0.1.0-alpha.N` | Feature-complete for the v0.1 scope, tested by developers | Engine test suite green, app builds, manual smoke test on Windows 11 |
| `0.1.0-beta.N` | Public testing | Clean-VM installer test on Windows 10 and 11, 7-day soak with no corruption reports, signed binaries |
| `0.1.0` | First stable | All [quality gates](#quality-gates) met, no open P0/P1 bugs |

The version lives in three places that CI checks stay equal: `package.json`, `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json`.

## Licence

**MIT** (confirmed by the maintainer). Permissive, compatible with every runtime dependency (MIT, Apache-2.0, ISC, BSD,
Zlib, Unicode-3.0, OFL-1.1 for the bundled Inter font), and the most common licence in the Tauri/Rust
ecosystem, which lowers the barrier for contributors.

Alternatives: *Apache-2.0* (explicit patent grant; heavier notice requirements) or *GPL-3.0*
(forces forks to stay open, but blocks some redistribution such as app stores with extra terms).
MIT was chosen before the first public push; the licence is hard to change once outside
contributions arrive.

Repository: <https://github.com/ShahzaibAnwar/velora>.

Dependency licences are checked with `cargo deny check licenses` (config in `deny.toml`).

## Release process (all steps local until the maintainer approves publishing)

1. Update `CHANGELOG.md`, bump the version in the three files.
2. `npm run check` (type-check + lint + unit tests) and `cargo test --workspace`.
3. `npm run tauri build` → `target/release/bundle/nsis/Velora_<version>_x64-setup.exe`.
4. Manual smoke test from [the checklist](#manual-smoke-test) on a clean Windows VM.
5. Tag `v<version>` locally. **Do not push** until the maintainer approves.
6. After approval: push the tag; the `release.yml` workflow builds the installer and creates a
   **draft** GitHub release. The maintainer reviews and publishes it by hand.

## Quality gates

- [ ] Engine integration suite passes on Windows (the 20 scenarios in `crates/velora-engine/tests`).
- [ ] No download is marked completed before its bytes are flushed and renamed.
- [ ] Forced kill during a 1 GB segmented download → restart → resumes and the SHA-256 matches.
- [ ] Settings persist across restarts; every visible setting has an effect.
- [ ] Installer: install, launch, uninstall on clean Windows 10 22H2 and Windows 11 VMs without admin.
- [ ] Native messaging registration works for Chrome, Edge and Firefox; removal cleans the registry.
- [ ] `cargo audit` / `npm audit --omit=dev` have no unresolved high-severity advisories.
- [ ] Binaries and installer are code-signed (beta onwards).

## Manual smoke test

1. Install with the NSIS installer (per user). The app starts, tray icon appears.
2. Paste a large file URL (for example a Linux ISO from a mirror that supports ranges). The Add dialog
   shows size, type and "Resume supported".
3. Start it. Check segments in the details panel; pause, resume; kill the process from Task Manager;
   restart Velora; it resumes.
4. Set a 1 MB/s global limit; the measured speed drops to ~1 MB/s.
5. Add three more downloads with max active = 2; the third waits in the queue and starts automatically.
6. Enter a wrong SHA-256; the mismatch state is shown.
7. Register browser integration, load the extension, right-click a link → *Download with Velora*;
   approve the extension in the app; the download appears.
8. Toggle light/dark/system theme and reduced motion.
9. Uninstall. Downloaded files remain; app data removal is offered.

## Performance methodology

Numbers are never published without this procedure.

- Same machine, same wired network, same time window; record CPU, RAM, OS build, link speed.
- Test files: a local server (`scripts/bench-server.mjs`, throttled per connection to emulate a
  per-connection-limited CDN) and two public mirrors that support range requests.
- For each tool (browser, Velora at 1/4/8 connections, other managers if installed): 5 runs, report the
  median and min/max wall time, discard nothing silently.
- Idle metrics: CPU and private bytes after 5 minutes idle with the window hidden, from Windows
  Performance Monitor.
- Results go to `docs/benchmarks/<date>.md` with the raw data.

## Distribution

- GitHub Releases: NSIS installer (`*-setup.exe`) and portable zip. SHA-256 sums published alongside.
- Microsoft Store (future): MSIX packaging, requires a Partner Center account.
- winget manifest (future, after stable).
