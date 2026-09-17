@echo off
rem vigil.bat - portable keep-awake (self-contained batch + embedded PowerShell).
rem Generated from vigil.ps1 by tools/generate-bat.sh - do not edit both.
rem Ctrl+C asks "Terminate batch job (Y/N)?" - answer Y; cleanup still runs.
set "VIGIL_ARGS=%*"
powershell -NoProfile -ExecutionPolicy Bypass -Command "Invoke-Expression ((Get-Content -Raw -LiteralPath '%~f0') -split '\r?\n#VIGIL-PS\r?\n',2)[1]"
exit /b %ERRORLEVEL%
#VIGIL-PS
# vigil.ps1 - keep your system awake. Portable PowerShell port of vigil.
# Windows: SetThreadExecutionState (same mechanism as the Rust binary);
# macOS: caffeinate; Linux: systemd-inhibit.
# Works with Windows PowerShell 5.1+ and PowerShell 7+ (pwsh, any OS).

$VigilVersion = '0.0.2'

function Write-VigilHelp {
    # Keep the flag list and descriptions in sync with README.md.
    [Console]::Out.WriteLine(@"
Usage: vigil.ps1 [-t <duration>] [-V] [-h]

Keep your system awake.

Flags:
  -t, --timeout <duration>  Stay awake for this long (e.g. 2h, 45m, 30s);
                            --timeout=<duration> and -t=<duration> are
                            also accepted. Infinite by default.
  -h, --help                Print this help.
  -V, --version             Print the version.
"@)
}

function Convert-ToSeconds {
    # Mirrors the Rust parse_duration: units h, m, s in that order, each once,
    # digits required before every unit, overflow rejected.
    # Deviation: huge values are accumulated as doubles (exact far beyond any
    # realistic timeout); the overflow boundary is approximate at ~1.8e19s.
    param([string]$Value)
    $maxSeconds = [double]18446744073709551615
    if ([string]::IsNullOrEmpty($Value)) { throw 'empty duration' }
    $stage = 0
    $total = [double]0
    $n = [double]0
    $hasDigits = $false
    foreach ($c in $Value.ToCharArray()) {
        if ($c -ge '0' -and $c -le '9') {
            $hasDigits = $true
            $n = $n * 10 + [double]([int]$c - [int][char]'0')
            if ($n -gt $maxSeconds) { throw 'overflow' }
        } else {
            if (-not $hasDigits) { throw "missing number before unit '$c'" }
            switch ($c) {
                'h' { $newStage = 1; $seconds = $n * 3600 }
                'm' { $newStage = 2; $seconds = $n * 60 }
                's' { $newStage = 3; $seconds = $n }
                default { throw "unknown unit '$c' in duration" }
            }
            if ($stage -ge $newStage) {
                throw "unit '$c' is out of order or repeated (expected order: h, m, s)"
            }
            $stage = $newStage
            if ($seconds -gt $maxSeconds -or $total -gt $maxSeconds - $seconds) { throw 'overflow' }
            $total += $seconds
            $n = [double]0
            $hasDigits = $false
        }
    }
    if ($hasDigits) { throw 'digits must be followed by a unit (h, m, or s)' }
    $total
}

function Invoke-Vigil {
    param([string[]]$ArgList)

    $showHelp = $false
    $showVersion = $false
    $timeoutSeconds = $null

    $i = 0
    while ($i -lt $ArgList.Count) {
        $arg = $ArgList[$i]
        if ($arg -eq '-h' -or $arg -eq '--help' -or $arg -eq 'help') {
            $showHelp = $true
        } elseif ($arg -eq '-V' -or $arg -eq '--version') {
            $showVersion = $true
        } elseif ($arg -eq '-t' -or $arg -eq '--timeout') {
            $i++
            if ($i -ge $ArgList.Count -or $ArgList[$i].StartsWith('-')) {
                [Console]::Error.WriteLine("error: $arg requires a value")
                return 1
            }
            try { $timeoutSeconds = Convert-ToSeconds $ArgList[$i] }
            catch {
                [Console]::Error.WriteLine("error: invalid timeout: $($_.Exception.Message)")
                return 1
            }
        } elseif ($arg.StartsWith('--timeout=')) {
            try { $timeoutSeconds = Convert-ToSeconds $arg.Substring('--timeout='.Length) }
            catch {
                [Console]::Error.WriteLine("error: invalid timeout: $($_.Exception.Message)")
                return 1
            }
        } elseif ($arg.StartsWith('-t=')) {
            try { $timeoutSeconds = Convert-ToSeconds $arg.Substring(3) }
            catch {
                [Console]::Error.WriteLine("error: invalid timeout: $($_.Exception.Message)")
                return 1
            }
        } else {
            [Console]::Error.WriteLine("error: unknown argument '$arg'")
            return 1
        }
        $i++
    }

    if ($showHelp) { Write-VigilHelp; return 0 }
    if ($showVersion) { [Console]::Out.WriteLine("vigil $VigilVersion"); return 0 }

    $isWindowsHost = $IsWindows -or ($env:OS -eq 'Windows_NT')

    if ($isWindowsHost) {
        if (-not ('Vigil.Power' -as [type])) {
            Add-Type -Namespace Vigil -Name Power -MemberDefinition @'
[DllImport("kernel32.dll", SetLastError = true)]
public static extern uint SetThreadExecutionState(uint esFlags);
'@
        }
        $ES_CONTINUOUS = [uint32]0x80000000
        $ES_SYSTEM_REQUIRED = [uint32]0x1
        $ES_DISPLAY_REQUIRED = [uint32]0x2
        $state = [Vigil.Power]::SetThreadExecutionState(
            $ES_CONTINUOUS -bor $ES_SYSTEM_REQUIRED -bor $ES_DISPLAY_REQUIRED)
        if ($state -eq 0) {
            [Console]::Error.WriteLine('error: could not inhibit sleep (SetThreadExecutionState failed)')
            return 1
        }
        $timedOut = $false
        try {
            if (-not [Console]::IsOutputRedirected) {
                [Console]::Out.WriteLine('Vigil started. Press Ctrl+C to stop.')
            }
            if ($null -ne $timeoutSeconds) {
                # Cap at int32.MaxValue seconds (~68 years) - effectively infinite.
                $seconds = [math]::Min([double]$timeoutSeconds, [double][int32]::MaxValue)
                $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
                while ($stopwatch.Elapsed.TotalSeconds -lt $seconds) { Start-Sleep -Seconds 1 }
                $timedOut = $true
                if (-not [Console]::IsOutputRedirected) { [Console]::Out.WriteLine('Timeout reached.') }
            } else {
                while ($true) { Start-Sleep -Seconds 1 }
            }
        } finally {
            # Restore so the system can sleep again, even on Ctrl+C.
            [void][Vigil.Power]::SetThreadExecutionState($ES_CONTINUOUS)
            if (-not $timedOut -and -not [Console]::IsOutputRedirected) {
                [Console]::Out.WriteLine('Stopped.')
            }
        }
        return 0
    }

    # Unix: run the platform inhibitor in the foreground. Ctrl+C reaches it
    # directly (same process group), and it exits on its own at the timeout.
    if ($IsMacOS) {
        $exe = 'caffeinate'
        $childArgs = @('-d', '-i')
        if ($null -ne $timeoutSeconds) {
            $childArgs += @('-t', [string][math]::Min([double]$timeoutSeconds, [double][int32]::MaxValue))
        }
    } else {
        $exe = 'systemd-inhibit'
        $sleepFor = 2147483647
        if ($null -ne $timeoutSeconds) { $sleepFor = $timeoutSeconds }
        $childArgs = @('--what=idle:sleep', '--who=vigil', '--why=vigil', 'sleep', [string]$sleepFor)
    }
    try {
        & $exe @childArgs
        return $LASTEXITCODE
    } catch {
        [Console]::Error.WriteLine("error: failed to start ${exe}: $($_.Exception.Message)")
        return 1
    }
}

$callArgs = @()
if ($env:VIGIL_ARGS) {
    $callArgs = @(($env:VIGIL_ARGS -split '\s+') | Where-Object { $_ } | ForEach-Object { $_.Trim('"') })
}
exit (Invoke-Vigil $callArgs)
