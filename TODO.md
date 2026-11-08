# TODO

Open maintenance items. IDs are stable while an item is open; the analysis for
each lives with the audit run that found it.

## Verification gaps

- VER-1 — low — Real sleep prevention is only verified by hand. The wiring
  (`systemd-inhibit`/`caffeinate` argv, guard lifecycle, Windows API call) is
  covered by tests, but whether the OS actually stays awake needs a manual
  `vigil -t 5m` on each platform.
