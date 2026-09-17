#!/usr/bin/env bash
# Regenerates vigil.bat from vigil.ps1 (single source of truth for the engine).
# The batch wrapper re-reads its own file, splits on the #VIGIL-PS marker, and
# executes the embedded engine with args from the VIGIL_ARGS environment
# variable. Requires: python3, sed.
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'EOF'
ps1 = open('vigil.ps1', 'rb').read().decode('ascii')
assert ps1.endswith('exit (Invoke-Vigil @($args))\n'), 'unexpected vigil.ps1 bootstrap'
engine = ps1[: -len('exit (Invoke-Vigil @($args))\n')]

bat_bootstrap = '''$callArgs = @()
if ($env:VIGIL_ARGS) {
    $callArgs = @(($env:VIGIL_ARGS -split '\\s+') | Where-Object { $_ } | ForEach-Object { $_.Trim('"') })
}
exit (Invoke-Vigil $callArgs)
'''

header = '''@echo off\nrem vigil.bat - portable keep-awake (self-contained batch + embedded PowerShell).\nrem Generated from vigil.ps1 by tools/generate-bat.sh - do not edit both.\nrem Ctrl+C asks "Terminate batch job (Y/N)?" - answer Y; cleanup still runs.\nset "VIGIL_ARGS=%*"\npowershell -NoProfile -ExecutionPolicy Bypass -Command "Invoke-Expression ((Get-Content -Raw -LiteralPath '%~f0') -split '\\r?\\n#VIGIL-PS\\r?\\n',2)[1]"\nexit /b %ERRORLEVEL%\n#VIGIL-PS\n'''

open('vigil.bat', 'wb').write((header + engine + bat_bootstrap).encode('ascii'))
EOF

# CRLF for cmd's sake (LF-only batch files are fragile)
sed -i 's/\r$//' vigil.bat
sed -i 's/$/\r/' vigil.bat
echo "generated vigil.bat from vigil.ps1"
