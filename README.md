# vigil

Keep your system awake. Works on Linux, macOS, and Windows.

## Install

```bash
# Linux (amd64)
curl -fsSL https://github.com/dat267/vigil/releases/latest/download/vigil-linux-amd64 -o ~/.local/bin/vigil && chmod +x ~/.local/bin/vigil

# macOS (Apple Silicon)
curl -fsSL https://github.com/dat267/vigil/releases/latest/download/vigil-darwin-arm64 -o ~/.local/bin/vigil && chmod +x ~/.local/bin/vigil

# Build from source (Go 1.25+)
go install github.com/dat267/vigil@latest
```

> **Windows:** Download `vigil-windows-amd64.exe` from [releases](https://github.com/dat267/vigil/releases) and add it to your PATH.
>
> **Other architectures:** `linux-arm64`, `linux-arm`, `darwin-amd64`, `windows-arm64.exe`
>
> **Linux via SSH:** `systemd-inhibit` requires an active local seat session. Run with `sudo` if denied.

## Usage

```
vigil start                  # Stay awake indefinitely (Ctrl+C to stop)
vigil start -t 2h            # Stay awake for 2 hours, then exit
vigil start -t 45m -s        # Stay awake for 45 minutes, then shut down
vigil version
```

**`vigil start` flags:**

| Flag | Description |
|------|-------------|
| `-t, --timeout=DURATION` | Duration to stay awake (e.g. `30s`, `45m`, `2h`, `1h30m`). Omit for indefinite. |
| `-s, --shutdown` | Shut down the system when the timeout expires. Requires `-t`. |

## How it works

| Platform | Mechanism |
|----------|-----------|
| Linux    | `systemd-inhibit --what=idle:sleep` via logind |
| macOS    | `caffeinate -d -i -w <pid>` (exits automatically when vigil exits) |
| Windows  | `SetThreadExecutionState(ES_CONTINUOUS \| ES_SYSTEM_REQUIRED \| ES_DISPLAY_REQUIRED)` |

## License

MIT
