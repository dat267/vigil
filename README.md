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
```

In a terminal, vigil shows what it is doing and how much time is left:

```
$ vigil -t 2h
Vigil: preventing sleep for 2h (Ctrl+C to stop).
Awake 1m5s | 1h58m55s left
```

On exit it prints a one-line summary (`Awake 2h. Timeout reached.`). When
stdout is not a terminal (pipes, logs, CI) vigil stays silent, so
`vigil > /dev/null` behaves as a quiet mode.

If sleep inhibition cannot start or is lost, the error includes the platform's
own explanation plus a `help:` hint when one is known:

```
error: sleep inhibition failed: inhibitor exited (exit status: 1): Failed to connect to system scope bus via local transport: Connection refused
help: logind is not reachable; check that systemd-logind is running
```

**Flags:**

| Flag | Description |
|------|-------------|
| `-t, --timeout <DURATION>` | Duration to stay awake (e.g. `30s`, `45m`, `2h`, `1h30m`). `--timeout=DURATION` and `-t=DURATION` are also accepted. Omit for indefinite. |
| `-V, --version` | Print the installed version. |
| `-h, --help` | Print help, including examples. |

## How it works

| Platform | Mechanism |
|----------|-----------|
| Linux    | `systemd-inhibit --what=idle:sleep` via logind |
| macOS    | `caffeinate -d -i -w <pid>` (exits automatically when vigil exits) |
| Windows  | `SetThreadExecutionState(ES_CONTINUOUS \| ES_SYSTEM_REQUIRED \| ES_DISPLAY_REQUIRED)` |

## Limitations

- On Linux the inhibition is held by a child `systemd-inhibit sleep` helper. It is
  cleaned up on graceful exit (Ctrl+C, SIGTERM, timeout). If vigil is
  killed with `SIGKILL`, a harmless `sleep` helper process may remain until logout.

## Build

```bash
cargo build --release
```

Binary is at `target/release/vigil`.

## License

MIT
