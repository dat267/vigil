# Black-box UX tests for vigil.ps1. Run: pwsh -NoProfile -File tests/portable.tests.ps1

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
$r = Invoke-Script @('-Version')
Assert 'version prints vigil <version> to stdout' ($r.Code -eq 0 -and $r.Out -eq "vigil $version`n" -and $r.Err -eq '')
$r = Invoke-Script @('-V')
Assert 'short version alias works' ($r.Code -eq 0 -and $r.Out.StartsWith('vigil '))

# ---------------------------------------------------------------- help
$r = Invoke-Script @('-Help')
Assert 'help prints usage to stdout' ($r.Code -eq 0 -and $r.Out.Contains('Usage:') -and $r.Err -eq '')
Assert 'usage documents -Timeout and aliases' ($r.Out.Contains('-t') -and $r.Out.Contains('-Timeout'))
$r = Invoke-Script @('-h')
Assert 'short help alias works' ($r.Code -eq 0 -and $r.Out.Contains('Usage:'))

# ---------------------------------------------------------------- binding errors
foreach ($case in @('--bogus', '-q', '--timeout=2h', '-t=2h')) {
    $r = Invoke-Script @($case)
    Assert "unknown/removed form '$case' fails loudly" ($r.Code -ne 0 -and $r.Err -ne '' -and $r.Out -eq '')
}
$r = Invoke-Script @('-t')
Assert 'missing -t value fails' ($r.Code -ne 0 -and $r.Err -ne '')

# ---------------------------------------------------------------- duration validation
foreach ($bad in @('5x', '5h30', '1h1h', '1m1h', '1m2m', '99999999999999999h')) {
    $r = Invoke-Script @('-t', $bad)
    Assert "invalid duration '$bad' rejected" ($r.Code -eq 1 -and $r.Err.Contains('Invalid duration') -and $r.Out -eq '')
}

# ---------------------------------------------------------------- run behavior (host-dependent)
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
