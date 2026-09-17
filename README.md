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

**Flags:**

| Flag | Description |
|------|-------------|
| `-t, --timeout <DURATION>` | Duration to stay awake (e.g. `30s`, `45m`, `2h`, `1h30m`). `--timeout=DURATION` and `-t=DURATION` are also accepted. Omit for indefinite. |
| `-V, --version` | Print the installed version. |
| `-h, --help` | Print help. |

## Portable scripts (no Rust toolchain needed)

- `vigil.ps1` — PowerShell, runs on Windows PowerShell 5.1+ and PowerShell 7+ (Windows, macOS, Linux)
- `vigil.bat` — self-contained Windows batch file with the PowerShell engine embedded

```powershell
# PowerShell
./vigil.ps1 -t 2h
```

```bat
rem cmd
vigil.bat -t 2h
```

Notes:

- Execution policy may block `.ps1` files: run `powershell -ExecutionPolicy Bypass -File vigil.ps1` or unblock the downloaded file.
- Ctrl+C in the `.bat` prompts "Terminate batch job (Y/N)?" — answer Y; cleanup still runs.
- `vigil.bat` is generated from `vigil.ps1` (`tools/generate-bat.sh`); edit `vigil.ps1`, regenerate the `.bat`.
- Flags, duration parsing, and messages mirror the Rust binary.

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
