# Black-box UX tests for vigil.ps1 - mirrors the scope of tests/cli.rs:
# exit codes, stream separation, message text. Run: pwsh -NoProfile -File tests/portable.tests.ps1

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$scriptPath = Join-Path $repoRoot 'vigil.ps1'
$version = '0.0.2'

$script:failures = 0

function Invoke-Script {
    param([string[]]$CliArgs)
    $tmpOut = New-TemporaryFile
    $tmpErr = New-TemporaryFile
    & pwsh -NoProfile -File $scriptPath @CliArgs 1>$tmpOut 2>$tmpErr
    $code = $LASTEXITCODE
    $out = [IO.File]::ReadAllText($tmpOut)
    $err = [IO.File]::ReadAllText($tmpErr)
    Remove-Item $tmpOut, $tmpErr -ErrorAction SilentlyContinue
    [pscustomobject]@{ Code = $code; Out = $out; Err = $err }
}

function Assert {
    param([string]$Name, [bool]$Condition)
    if ($Condition) { Write-Host "ok   $Name" }
    else { Write-Host "FAIL $Name"; $script:failures++ }
}

# ---------------------------------------------------------------- version
$r = Invoke-Script @('--version')
Assert 'version prints vigil <version> to stdout' ($r.Code -eq 0 -and $r.Out -eq "vigil $version`n" -and $r.Err -eq '')
$r = Invoke-Script @('-V')
Assert 'short version flag works' ($r.Code -eq 0 -and $r.Out.StartsWith('vigil '))

# ---------------------------------------------------------------- help
$r = Invoke-Script @('--help')
Assert 'help prints usage to stdout' ($r.Code -eq 0 -and $r.Out.Contains('Usage:') -and $r.Err -eq '')
Assert 'help documents all flags' (($r.Out.Contains('-t, --timeout')) -and ($r.Out.Contains('-h, --help')) -and ($r.Out.Contains('-V, --version')))
Assert 'help does not document -q (removed)' (-not $r.Out.Contains('-q'))
$r = Invoke-Script @('-h')
Assert 'short help flag works' ($r.Code -eq 0 -and $r.Out.Contains('Usage:'))
$r = Invoke-Script @('help')
Assert 'bare help word works' ($r.Code -eq 0 -and $r.Out.Contains('Usage:'))
$r = Invoke-Script @('-h', '-V')
Assert 'help wins over version' ($r.Code -eq 0 -and $r.Out.Contains('Usage:') -and -not $r.Out.Contains('vigil '))

# ---------------------------------------------------------------- errors
$r = Invoke-Script @('--bogus')
Assert 'unknown argument reported on stderr, exit 1' ($r.Code -eq 1 -and $r.Err.Contains("error: unknown argument '--bogus'") -and $r.Out -eq '')
$r = Invoke-Script @('-q')
Assert 'removed -q rejected as unknown' ($r.Code -eq 1 -and $r.Err.Contains("error: unknown argument '-q'"))
$r = Invoke-Script @('-t')
Assert 'timeout without value reports error' ($r.Code -eq 1 -and $r.Err.Contains('requires a value'))
$r = Invoke-Script @('-t', '-2h')
Assert 'dash value rejected' ($r.Code -eq 1 -and $r.Err.Contains('requires a value'))
$r = Invoke-Script @('--timeout')
Assert 'timeout at end without value reports error' ($r.Code -eq 1 -and $r.Err.Contains('requires a value'))

# ------------------------------------------------- invalid durations
foreach ($case in @(
    @{ args = @('-t', '5x'); name = 'unknown unit rejected' },
    @{ args = @('-t', '5h30'); name = 'trailing digits rejected' },
    @{ args = @('-t', '1h1h'); name = 'repeated units rejected' },
    @{ args = @('-t', '1m1h'); name = 'out-of-order units rejected' },
    @{ args = @('-t', '99999999999999999999h'); name = 'overflow rejected' },
    @{ args = @('-t', ''); name = 'empty value rejected' },
    @{ args = @('--timeout='); name = 'empty = value rejected' }
)) {
    $r = Invoke-Script $case.args
    Assert $case.name ($r.Code -eq 1 -and $r.Err.Contains('error: invalid timeout') -and $r.Out -eq '')
}

# ------------------------------------------------- run behavior (host-dependent)
$r = Invoke-Script @('-t', '0s')
if ($r.Code -eq 0) {
    Assert 'successful run is silent on piped streams' ($r.Out -eq '' -and $r.Err -eq '')
} else {
    Assert 'failed inhibition reported on stderr, not stdout' ($r.Err -ne '' -and $r.Out -eq '')
}

Write-Host ''
if ($script:failures -gt 0) {
    Write-Host "$($script:failures) test(s) failed"
    exit 1
}
Write-Host 'all portable tests passed'
