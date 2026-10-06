param([switch]$Bench)
$ErrorActionPreference = 'Stop'
$migrationRoot = Split-Path -Parent $PSScriptRoot
New-Item -ItemType Directory -Force -Path (Join-Path $migrationRoot 'out'), (Join-Path $migrationRoot 'evidence') | Out-Null
function Check-Cargo {
    param([string[]]$CargoArgs, [string]$Log)
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'cargo.ps1') @CargoArgs 2>&1 |
        Tee-Object -FilePath (Join-Path $migrationRoot "evidence/$Log")
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed: $CargoArgs" }
}
& node (Join-Path $PSScriptRoot 'verify-reference.mjs')
if ($LASTEXITCODE -ne 0) { throw 'Reference changed' }
Check-Cargo -CargoArgs @('fmt','--all','--','--check') -Log 'h0-fmt.txt'
Check-Cargo -CargoArgs @('test','--workspace','--locked','--','--nocapture') -Log 'h0-tests.txt'
Check-Cargo -CargoArgs @('clippy','--workspace','--locked','--all-targets','--','-D','warnings') -Log 'h0-clippy.txt'
Check-Cargo -CargoArgs @('run','--release','--locked','-p','lunar-app','--','demo') -Log 'h0-demo.txt'
& node (Join-Path $PSScriptRoot 'atlas-png.mjs')
if ($LASTEXITCODE -ne 0) { throw 'Atlas export failed' }
if ($Bench) {
    # Sequential: no competing benchmark/compiler process.
    & node (Join-Path $PSScriptRoot 'bench-ts.mjs')
    if ($LASTEXITCODE -ne 0) { throw 'TS microbenchmark failed (generate oracle with reference.mjs first)' }
    Check-Cargo -CargoArgs @('run','--release','--locked','-p','lunar-app','--','bench','--seconds','2','--out','evidence/h0-rust-surface.json') -Log 'h0-bench.txt'
}
& node (Join-Path $PSScriptRoot 'guard.mjs') check
if ($LASTEXITCODE -ne 0) { throw 'Project files changed outside MIGRACION' }
Write-Output 'H0 PASS. GPU, window, CDLOD and fleet gate remain pending.'
