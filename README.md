# vigil

**vigil** keeps your system awake — preventing sleep, suspend, and idle screen blanking — for as long as you need.

## Features

- Works on **Linux**, **macOS**, and **Windows**
- Optional **timed duration** — exit automatically after N hours/minutes/seconds
- Optional **system shutdown** when the timer expires
- Single static binary, no dependencies

## Installation

### One-liner (download & run directly)

Pick the command for your platform. These fetch the latest release binary and run it entirely in memory — nothing is written to disk.

#### Linux (amd64)

```bash
curl -fsSL https://github.com/dat267/vigil/releases/latest/download/vigil-linux-amd64 | bash -s -- start
```

> For **ARM64** (e.g. Raspberry Pi 64-bit), replace `linux-amd64` with `linux-arm64`.  
> For **ARM** (e.g. Raspberry Pi 32-bit), use `linux-arm`.

#### macOS (Apple Silicon)

```bash
curl -fsSL https://github.com/dat267/vigil/releases/latest/download/vigil-darwin-arm64 -o /tmp/vigil && chmod +x /tmp/vigil && /tmp/vigil start
```

> For **Intel Macs**, replace `darwin-arm64` with `darwin-amd64`.

> **Note:** macOS Gatekeeper will block unsigned binaries downloaded via curl on first run. Use the install method below, or right-click → Open in Finder on first launch.

#### Windows (PowerShell)

```powershell
$url = "https://github.com/dat267/vigil/releases/latest/download/vigil-windows-amd64.exe"
$tmp = "$env:TEMP\vigil.exe"
Invoke-WebRequest -Uri $url -OutFile $tmp
& $tmp start
```

> For **ARM64 Windows**, replace `windows-amd64.exe` with `windows-arm64.exe`.

---

### Install to PATH (recommended)

#### Linux / macOS

```bash
# Linux amd64
curl -fsSL https://github.com/dat267/vigil/releases/latest/download/vigil-linux-amd64 \
  -o ~/.local/bin/vigil && chmod +x ~/.local/bin/vigil

# macOS Apple Silicon
curl -fsSL https://github.com/dat267/vigil/releases/latest/download/vigil-darwin-arm64 \
  -o ~/.local/bin/vigil && chmod +x ~/.local/bin/vigil
```

Make sure `~/.local/bin` is on your `$PATH`:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc && source ~/.bashrc
```

#### Windows (PowerShell)

```powershell
$url = "https://github.com/dat267/vigil/releases/latest/download/vigil-windows-amd64.exe"
$dest = "$env:USERPROFILE\.local\bin\vigil.exe"
New-Item -ItemType Directory -Force "$env:USERPROFILE\.local\bin" | Out-Null
Invoke-WebRequest -Uri $url -OutFile $dest
# Add to PATH (current user, permanent)
$path = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($path -notlike "*\.local\bin*") {
  [Environment]::SetEnvironmentVariable("PATH", "$path;$env:USERPROFILE\.local\bin", "User")
}
```

Restart your terminal, then run `vigil`.

---

### Build from source

Requires [Go 1.21+](https://go.dev/dl/).

```bash
git clone https://github.com/dat267/vigil.git
cd vigil
go build -o vigil .
```

---

## Usage

```
vigil <command> [flags]

COMMANDS
  start   Start the sleep inhibitor
  help    Show this help message
```

### `vigil start`

```
vigil start [flags]

FLAGS
  -t <duration>   Stay awake for the given duration, then exit.
                  Accepts Go duration strings: e.g. 30s, 45m, 2h, 1h30m.
                  Omit to run indefinitely.

  -s              Shut down the system after the timeout (-t) expires.
                  Requires -t; cannot be used alone.

  -h              Show this help.
```

### Examples

```bash
# Keep awake indefinitely (Ctrl+C to stop)
vigil start

# Keep awake for 2 hours, then exit
vigil start -t 2h

# Keep awake for 1 hour 30 minutes
vigil start -t 1h30m

# Keep awake for 45 minutes, then shut down the system
vigil start -t 45m -s
```

---

## How it works

| Platform | Mechanism |
|----------|-----------|
| **Linux** | Calls `systemd-inhibit --what=idle:sleep` to block both idle and sleep inhibitors via logind |
| **macOS** | Runs `caffeinate -d -i` to prevent display sleep and system idle sleep |
| **Windows** | Calls `SetThreadExecutionState` with `ES_CONTINUOUS \| ES_SYSTEM_REQUIRED \| ES_DISPLAY_REQUIRED` |

---

## Releases

Binaries are built and published automatically on every push to `main` via GitHub Actions.

| Platform | Architecture | Binary |
|----------|-------------|--------|
| Linux | amd64 | `vigil-linux-amd64` |
| Linux | arm64 | `vigil-linux-arm64` |
| Linux | arm | `vigil-linux-arm` |
| macOS | amd64 (Intel) | `vigil-darwin-amd64` |
| macOS | arm64 (Apple Silicon) | `vigil-darwin-arm64` |
| Windows | amd64 | `vigil-windows-amd64.exe` |
| Windows | arm64 | `vigil-windows-arm64.exe` |

Browse all releases: [github.com/dat267/vigil/releases](https://github.com/dat267/vigil/releases)

---

## License

MIT
