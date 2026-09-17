<#
.SYNOPSIS
    Keeps the system awake.
.DESCRIPTION
    Windows uses SetThreadExecutionState (restored on exit, including Ctrl+C).
    macOS uses caffeinate; Linux uses systemd-inhibit - run in the foreground
    so Ctrl+C and the timeout reach the inhibitor directly.
.EXAMPLE
    vigil.ps1
    Stays awake until Ctrl+C.
.EXAMPLE
    vigil.ps1 -t 1h30m
    Stays awake for 1h30m, then exits.
#>
[CmdletBinding()]
param(
    # Duration to stay awake: 30s, 45m, 2h, 1h30m. Indefinite by default.
    [Alias('t')]
    [string]$Timeout,

    # Print the version.
    [Alias('V')]
    [switch]$Version,

    # Print usage.
    [Alias('h')]
    [switch]$Help
)

$VigilVersion = '0.0.2'

if ($Help) {
    Write-Output @'
Usage: vigil.ps1 [-Timeout <duration>] [-Version] [-Help]

Keep your system awake. -Timeout accepts 30s, 45m, 2h, 1h30m; indefinite by default.
Aliases: -t = -Timeout, -V = -Version, -h = -Help.
'@
    exit 0
}
if ($Version) { Write-Output "vigil $VigilVersion"; exit 0 }

$timeoutSpan = $null
if ($Timeout) {
    $m = [regex]::Match($Timeout, '^(?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s)?$')
    if (-not $m.Success -or -not $m.Groups[1].Value + $m.Groups[2].Value + $m.Groups[3].Value) {
        throw "Invalid duration '$Timeout' - use forms like 30s, 45m, 2h, 1h30m."
    }
    $seconds = [double]$m.Groups[1].Value * 3600 + [double]$m.Groups[2].Value * 60 + [double]$m.Groups[3].Value
    if ($seconds -gt [int32]::MaxValue) {
        throw "Invalid duration '$Timeout' - the maximum is 2147483647s (~68 years)."
    }
    $timeoutSpan = [timespan]::FromSeconds($seconds)
}

if ($IsWindows -or $env:OS -eq 'Windows_NT') {
    if (-not ('Vigil.Power' -as [type])) {
        Add-Type -Namespace Vigil -Name Power -MemberDefinition @'
[DllImport("kernel32.dll", SetLastError = true)]
public static extern uint SetThreadExecutionState(uint esFlags);
'@
    }
    if ([Vigil.Power]::SetThreadExecutionState(0x80000000 -bor 1 -bor 2) -eq 0) {
        throw 'SetThreadExecutionState failed - cannot inhibit sleep.'
    }
    try {
        if (-not [Console]::IsOutputRedirected) { Write-Output 'Vigil started. Press Ctrl+C to stop.' }
        $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
        while ($null -eq $timeoutSpan -or $stopwatch.Elapsed -lt $timeoutSpan) { Start-Sleep -Seconds 1 }
        if (-not [Console]::IsOutputRedirected) { Write-Output 'Timeout reached.' }
    } finally {
        [void][Vigil.Power]::SetThreadExecutionState(0x80000000) # restore, even on Ctrl+C
    }
    exit 0
}

# Unix: the inhibitor runs in the foreground; Ctrl+C and the timeout reach it directly.
if ($IsMacOS) {
    $child = @('caffeinate', '-d', '-i')
    if ($timeoutSpan) { $child += @('-t', [string][int]$timeoutSpan.TotalSeconds) }
} else {
    $sleepFor = 2147483647
    if ($timeoutSpan) { $sleepFor = [int]$timeoutSpan.TotalSeconds }
    $child = @('systemd-inhibit', '--what=idle:sleep', '--who=vigil', '--why=vigil', 'sleep', [string]$sleepFor)
}
& $child[0] @($child | Select-Object -Skip 1)
exit $LASTEXITCODE
