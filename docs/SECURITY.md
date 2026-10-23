# Security Design

This document describes Velora's threat model and the controls implemented in code. For reporting
vulnerabilities see the repository-level [SECURITY.md](../SECURITY.md).

## Assets

- The user's files and file system (Velora writes files to disk).
- The user's browsing context (URLs, referrers, page titles sent from the browser extension).
- Clipboard contents (if monitoring is enabled).
- The integrity of downloaded files.

## Threats and controls

| # | Threat | Control | Where |
| --- | --- | --- | --- |
| T1 | Malicious server sends a file name like `..\..\Startup\evil.exe` or `CON` | File names are sanitised: path separators, `<>:"\|?*`, control characters, trailing dots/spaces and Windows reserved device names are removed or replaced; length capped at 200 characters; the joined path must stay inside the chosen folder. | `velora-engine/src/security/filename.rs`, `path.rs` |
| T2 | Non-HTTP schemes (`file:`, `javascript:`, UNC paths) submitted via UI, clipboard or extension | Only `http` and `https` URLs, plus `magnet:` links with a valid BitTorrent v1 info hash, are accepted; URL length capped at 8 KiB. | `security/url.rs`, `torrent.rs` |
| T3 | MITM / invalid certificates | TLS certificate validation is always on (platform verifier). There is no "ignore certificate errors" option. Certificate failures are permanent errors with a readable message. | `net/client.rs` |
| T4 | Corrupted output from a changed server file or a misbehaving server | `If-Range` on every ranged request, strict `Content-Range` validation, ETag/Last-Modified/size revalidation on resume, refusal to stitch, final size check, optional SHA-256. | `download/` |
| T5 | Overwriting an existing user file | Explicit overwrite policy (*ask* by default, *rename*, *overwrite*). Partial data uses a separate `.velora-part` file; the final rename never replaces a file unless the policy says so. | `download/finalize.rs` |
| T6 | Running a downloaded file | Velora never executes downloads. *Open file* is an explicit user action delegated to the shell, so Windows SmartScreen / Mark-of-the-Web prompts still apply. | `commands/files.rs` |
| T7 | A web page or local process drives Velora through a local HTTP endpoint | There is no HTTP endpoint. The browser bridge is a named pipe that rejects remote clients, is created with `first_pipe_instance` (prevents squatting), and requires a per-user random token stored in the user's profile. New extension origins must be approved in the app. | `bridge/` |
| T8 | Over-privileged extension | MV3, minimal permissions, no host permissions; page scanning uses `activeTab` + `scripting` only when the user opens the popup. Download interception is off by default and uses an optional permission. | `extensions/` |
| T9 | Secrets leaking into logs | Logged URLs have user info and query strings stripped; request headers are never logged. Diagnostic logging can be turned off. | `security/redact.rs` |
| T10 | Clipboard privacy | Monitoring is opt-in, local only, inspects text only when the clipboard sequence number changes, and keeps no history beyond a small in-memory de-duplication set. | `system/clipboard.rs` |
| T11 | Cookie theft | Velora never reads or exports browser cookies. | — |
| T12 | Privilege escalation | Per-user installer (no admin), no services, no UAC prompts, writes only to the user profile and user-chosen folders. | `tauri.conf.json` (NSIS `currentUser`) |
| T13 | Resource exhaustion (huge headers, endless redirects, thousands of connections) | Max 10 redirects, bounded connections per download (≤16) and globally, bounded write buffers (512 KiB per connection), request timeouts. | `net/`, `download/` |
| T14 | Webview compromise via injected content | Strict CSP (`default-src 'self'`), no remote content loaded in the webview, Tauri capabilities restrict the commands and plugins the UI can call. | `tauri.conf.json`, `capabilities/` |
| T15 | Vulnerable dependencies | `cargo audit` and `npm audit` run in CI; dependencies kept minimal. | `.github/workflows/ci.yml` |
| T16 | Tampered helper programs (yt-dlp, ffmpeg) fetched at run time | Fetched only over HTTPS from the projects' official GitHub releases, streamed to a temporary file, compared with the SHA-256 published in the same release, size-capped, and moved into place only on a match. Nothing runs before it is verified. Helpers run without a shell (arguments passed as a list, URL after `--`), inside a Job Object that ends the whole process tree on pause. | `tools.rs`, `process.rs` |
| T17 | Torrent paths escaping the download folder or overwriting user files | Torrent and file names are sanitised; Velora picks a folder or file name that didn't exist yet and records it (`infoHash`), and only such paths are deleted on cancel. No listening port; Velora doesn't seed. | `torrent.rs` |

## Things Velora deliberately does not do

- Disable TLS verification, even per download.
- Circumvent DRM, paywalls, authentication, or rate limits (it honours `Retry-After`).
- Silently intercept browser downloads or silently elevate privileges.
- Send any data to a server other than the one the user asked to download from, except: GitHub
  release downloads of yt-dlp and ffmpeg on first use, and, for torrents, BitTorrent trackers, DHT
  nodes and peers (which see the torrent's info hash and the user's IP address, as with any torrent
  client).

## Residual risks / open items

- Downloads that require cookies fail (by design). The error message points users back to the browser.
- yt-dlp's checksum list comes from the same GitHub release as the binary, so it guards against
  corruption and mirror tampering, not against a compromised upstream release.
- The bridge token protects against other users, not against malware already running as the same user
  (which could equally edit Velora's files). This matches the OS security boundary.
- The installer and binaries are unsigned in the alpha. SmartScreen will warn until a code-signing
  certificate is obtained (see [RELEASE_PLAN.md](RELEASE_PLAN.md)).
