# Contributing to Velora

Thanks for helping! Velora aims to be reliable first and beautiful second, so changes are judged on
both.

## Getting started

1. Install the prerequisites listed in the [README](README.md#build-from-source).
2. `npm install`, then `npm run app:dev` to run the desktop app with hot reload.
3. `npm run dev` runs only the UI in a browser with sample data — handy for design work.

## Before you open a pull request

```bash
cargo fmt --all
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace
```

```bash
npm run check
```

All four must pass; CI runs the same commands.

## Guidelines

- **Engine changes** (`crates/velora-engine`) need tests. Network behaviour is tested against the
  local server in `tests/common/mod.rs`; add a scenario there rather than calling real websites.
- **Never mark a download complete early.** Completion happens only after the size check, optional
  checksum, flush and atomic rename.
- **No fake numbers.** UI values must come from the engine. Sample data belongs only in
  `src/lib/mock.ts`, which is used solely when the UI runs outside Tauri.
- **Every visible setting must work.** Don't add toggles without behaviour.
- **Security-sensitive code** (URL handling, paths, file names, the browser bridge, logging) needs a
  note in the PR describing the threat it addresses. See [docs/SECURITY.md](docs/SECURITY.md).
- Keep dependencies few and licence-compatible (MIT, Apache-2.0, BSD, ISC, Zlib, OFL for fonts).
  `cargo deny check` enforces this.
- Respect reduced motion: animations must be skippable and short.
- Match the surrounding code style. Rust is formatted with `rustfmt.toml`; TypeScript is linted by ESLint.

## Commit messages

Use the imperative mood and explain *why* in the body when it isn't obvious, e.g.
`engine: validate Content-Range end against the requested range`.

## Updating the task tracker

[docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md) is the project's task list. Tick items
off in the same pull request that completes them.

## Code of conduct

Participation is governed by our [Code of Conduct](CODE_OF_CONDUCT.md).
