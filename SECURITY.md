# Security Policy

## Supported versions

Velora is in alpha. Only the latest pre-release receives security fixes.

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Instead, use GitHub's
**“Report a vulnerability”** button (private security advisories) on the repository's Security tab.

Include:
- what an attacker can do and under which conditions,
- steps to reproduce (a URL pattern, server response, or file name that triggers it),
- the Velora version and Windows version.

We aim to acknowledge reports within 72 hours and to publish a fix or mitigation within 30 days for
confirmed issues. Credit is given in the release notes unless you prefer otherwise.

## Scope

In scope: the download engine, the desktop app, the browser bridge (native messaging host and named
pipe), the browser extensions, and the installer.

Especially interesting: path traversal or file-name tricks, writing outside the chosen folder,
corrupted output that is reported as complete, TLS validation problems, ways for a web page or
another user to drive Velora, and leaks of URLs or tokens into logs.

The design and threat model are documented in [docs/SECURITY.md](docs/SECURITY.md).
