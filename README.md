# vigil

Keep your system awake. Works on Linux, macOS, and Windows.

## Install

### Linux (amd64)

```bash
mkdir -p ~/.local/bin
(
  set -eo pipefail
  cd ~/.local/bin
  curl -fsSLO https://github.com/dat267/vigil/releases/latest/download/vigil-x86_64-unknown-linux-gnu
  curl -fsSLO https://github.com/dat267/vigil/releases/latest/download/SHA256SUMS
  grep 'vigil-x86_64-unknown-linux-gnu$' SHA256SUMS | sha256sum -c -
  mv vigil-x86_64-unknown-linux-gnu vigil
  chmod +x vigil
  rm SHA256SUMS
)
```

### macOS (Apple Silicon)

```bash
mkdir -p ~/.local/bin
(
  set -eo pipefail
  cd ~/.local/bin
  curl -fsSLO https://github.com/dat267/vigil/releases/latest/download/vigil-aarch64-apple-darwin
  curl -fsSLO https://github.com/dat267/vigil/releases/latest/download/SHA256SUMS
  grep 'vigil-aarch64-apple-darwin$' SHA256SUMS | shasum -a 256 -c -
  mv vigil-aarch64-apple-darwin vigil
  chmod +x vigil
  rm SHA256SUMS
)
```

### Windows (PowerShell)

Windows (x64), PowerShell 5.1+.

```powershell
& {
    $asset = 'vigil-x86_64-pc-windows-msvc.exe'
    $base  = 'https://github.com/dat267/vigil/releases/latest/download'
    $dest  = Join-Path $HOME 'bin'
    $tmp   = Join-Path ([IO.Path]::GetTempPath()) ("vigil-" + [guid]::NewGuid())

    [Net.ServicePointManager]::SecurityProtocol = `
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    New-Item -ItemType Directory -Force -Path $dest, $tmp | Out-Null

    try {
        Invoke-WebRequest "$base/$asset" -OutFile (Join-Path $tmp $asset) -UseBasicParsing
        Invoke-WebRequest "$base/SHA256SUMS" -OutFile (Join-Path $tmp 'SHA256SUMS') -UseBasicParsing
        $entry = Select-String -Path (Join-Path $tmp 'SHA256SUMS') `
            -Pattern " $([regex]::Escape($asset))$" | Select-Object -First 1
        if (-not $entry) { throw "No checksum for $asset in SHA256SUMS" }
        $expected = $entry.Line.Split()[0].ToUpper()
        $actual = (Get-FileHash (Join-Path $tmp $asset) -Algorithm SHA256).Hash.ToUpper()
        if ($actual -ne $expected) { throw "Checksum mismatch for $asset" }
        Move-Item -Force (Join-Path $tmp $asset) (Join-Path $dest 'vigil.exe')
    } finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not $userPath) { $userPath = '' }
    $parts = @($userPath -split ';' | Where-Object { $_ })
    $added = $parts -notcontains $dest
    if ($added) {
        [Environment]::SetEnvironmentVariable('Path', (($parts + $dest) -join ';'), 'User')
        $env:Path = "$env:Path;$dest"
    }
    Write-Host "Installed $dest\vigil.exe"
    if ($added) { Write-Host "Added $dest to your user PATH; open a new terminal to use vigil." }
}
```

### Build from source

Rust stable:

```bash
cargo install --git https://github.com/dat267/vigil
```

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
