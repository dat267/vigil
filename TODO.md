# TODO

Open maintenance items. IDs are stable while an item is open; the analysis for
each lives with the audit run that found it.

## Verification gaps

- VER-1 — low — Real sleep prevention on Windows and macOS is still a manual
  check on those platforms. Linux was verified on a live seat: `vigil -t 8s`
  registered a `sleep:idle` block and released it on timeout, SIGINT, and
  SIGTERM.

## Blocked on decision

- INT-1 — low — The README install downloads a release binary without a
  checksum or signature check. Adding a `SHA256SUMS` asset and a verification
  step changes the install instructions, so it needs a decision before patching.
