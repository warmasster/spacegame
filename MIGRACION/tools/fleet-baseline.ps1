$ErrorActionPreference = 'Stop'
$migrationRoot = Split-Path -Parent $PSScriptRoot
$projectRoot = Split-Path -Parent $migrationRoot
$env:TSX_DISABLE_CACHE = '1'
$env:TMP = Join-Path $migrationRoot '.local/tmp'
$env:TEMP = $env:TMP
New-Item -ItemType Directory -Force -Path $env:TMP | Out-Null
Push-Location $projectRoot
try {
    & node --import tsx tools/perf/fleet.ts 100 2>&1 | Tee-Object -FilePath (Join-Path $migrationRoot 'evidence/h0-fleet.txt')
    $result = $LASTEXITCODE
} finally { Pop-Location }
exit $result
