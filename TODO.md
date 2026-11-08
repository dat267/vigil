# TODO

Open maintenance items, newest audit first. IDs are stable while an item is
open; the analysis for each lives with the audit run that found it.

## Medium

- CI-1 — medium — Linux CI runners have no seat session, so every
  inhibition-dependent CLI test self-skips. A change that breaks real
  inhibition would still pass CI.

## Low

- SEC-1 — low — The release and test workflows pin third-party actions to
  mutable major tags (`@v4`, `@v2`) rather than commit SHAs.
- DOC-1 — low — The README install command writes to `~/.local/bin` without
  creating it first, so it fails on machines where that directory does not
  exist.

## Verification gaps

- VER-1 — Windows and macOS runtime paths (console handler,
  `SetThreadExecutionState`, `caffeinate -w`) are only type- and
  clippy-checked on this host. Treat them as unverified until exercised on
  those platforms.
