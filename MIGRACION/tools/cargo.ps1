# All transient Cargo/compiler writes stay under MIGRACION for this child process.
$ErrorActionPreference = 'Stop'
$migrationRoot = Split-Path -Parent $PSScriptRoot
$compilerRoot = Join-Path $migrationRoot '.local/toolchain'
$env:CARGO_HOME = Join-Path $migrationRoot '.local/cargo'
$env:RUSTUP_HOME = Join-Path $migrationRoot '.local/rustup'
$env:CARGO_TARGET_DIR = Join-Path $migrationRoot 'target'
$env:TMP = Join-Path $migrationRoot '.local/tmp'
$env:TEMP = $env:TMP
$env:RUSTC = Join-Path $compilerRoot 'bin/rustc.exe'
$env:RUSTDOC = Join-Path $compilerRoot 'bin/rustdoc.exe'
$env:PATH = (Join-Path $compilerRoot 'bin') + ';' + (Join-Path $compilerRoot 'lib/rustlib/x86_64-pc-windows-gnu/bin') + ';' + $env:PATH
New-Item -ItemType Directory -Force -Path $env:CARGO_HOME, $env:TMP | Out-Null
Push-Location $migrationRoot
try {
    & (Join-Path $compilerRoot 'bin/cargo.exe') @args
    $cargoCode = $LASTEXITCODE
} finally { Pop-Location }
exit $cargoCode
