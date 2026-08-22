# vigil

Keep your system awake. Works on Linux, macOS, and Windows.

## Install

```bash
# Linux (amd64)
curl -fsSL https://github.com/dat267/vigil/releases/latest/download/vigil-x86_64-unknown-linux-gnu -o ~/.local/bin/vigil && chmod +x ~/.local/bin/vigil

# macOS (Apple Silicon)
curl -fsSL https://github.com/dat267/vigil/releases/latest/download/vigil-aarch64-apple-darwin -o ~/.local/bin/vigil && chmod +x ~/.local/bin/vigil

# Build from source (Rust stable)
cargo install --git https://github.com/dat267/vigil
```

> **Windows:** Download `vigil-x86_64-pc-windows-msvc.exe` from [releases](https://github.com/dat267/vigil/releases) and add it to your PATH.
>
> **Other architectures:** `aarch64-unknown-linux-gnu`, `armv7-unknown-linux-gnueabihf`, `x86_64-apple-darwin`, `aarch64-pc-windows-msvc`
>
> **Linux via SSH:** `systemd-inhibit` requires an active local seat session. Run with `sudo` if denied.

## Usage

```
vigil                        # Stay awake indefinitely (Ctrl+C to stop)
vigil -t 2h                  # Stay awake for 2 hours, then exit
vigil --timeout=2h           # Equivalent long-option form
vigil -t 45m -s              # Stay awake for 45 minutes, then shut down
```

**Flags:**

| Flag | Description |
|------|-------------|
| `-t, --timeout <DURATION>` | Duration to stay awake (e.g. `30s`, `45m`, `2h`, `1h30m`). `--timeout=DURATION` and `-t=DURATION` are also accepted. Omit for indefinite. |
| `-s, --shutdown` | Shut down the system when the timeout expires. Requires `-t`. On Linux/macOS this usually requires elevated privileges (`sudo`). |
| `-q, --quiet` | Suppress normal output (fatal errors are still reported) and hide the console window on Windows. |
| `-V, --version` | Print the installed version. |
| `-h, --help` | Print help. |

## How it works

| Platform | Mechanism |
|----------|-----------|
| Linux    | `systemd-inhibit --what=idle:sleep` via logind |
| macOS    | `caffeinate -d -i -w <pid>` (exits automatically when vigil exits) |
| Windows  | `SetThreadExecutionState(ES_CONTINUOUS \| ES_SYSTEM_REQUIRED \| ES_DISPLAY_REQUIRED)` |

## Limitations

- On Linux the inhibition is held by a child `systemd-inhibit sleep` helper. It is
  cleaned up on graceful exit (Ctrl+C, SIGTERM, timeout, shutdown). If vigil is
  killed with `SIGKILL`, a harmless `sleep` helper process may remain until logout.
- `-s` shuts the machine down; on Linux/macOS this usually requires elevated
  privileges (run vigil with `sudo`).

## Build

```bash
cargo build --release
```

Binary is at `target/release/vigil`.

## License

MIT
