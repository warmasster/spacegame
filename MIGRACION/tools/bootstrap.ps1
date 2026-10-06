# Portable compiler: extract official components; no installer, registry or global PATH changes.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$migrationRoot = Split-Path -Parent $PSScriptRoot
$localRoot = Join-Path $migrationRoot '.local'
$downloads = Join-Path $localRoot 'downloads'
$compilerRoot = Join-Path $localRoot 'toolchain'
New-Item -ItemType Directory -Force -Path $downloads, $compilerRoot | Out-Null
$manifestPath = Join-Path $migrationRoot 'channel-rust-stable.toml'
if (!(Test-Path -LiteralPath $manifestPath)) {
    Invoke-WebRequest 'https://static.rust-lang.org/dist/2026-09-03/channel-rust-1.98.1.toml' -OutFile $manifestPath
}
$manifest = Get-Content -LiteralPath $manifestPath -Raw
foreach ($component in @('rustc', 'cargo', 'rust-std', 'rust-mingw', 'rustfmt-preview', 'clippy-preview')) {
    $section = [regex]::Match($manifest, "(?ms)^\[pkg\.$component\.target\.x86_64-pc-windows-gnu\]\r?\n(.*?)(?=^\[)").Groups[1].Value
    $url = [regex]::Match($section, '(?m)^xz_url = "([^"]+)"').Groups[1].Value
    $hash = [regex]::Match($section, '(?m)^xz_hash = "([^"]+)"').Groups[1].Value
    if (!$url -or !$hash) { throw "Missing official component: $component" }
    $archive = Join-Path $downloads ([IO.Path]::GetFileName($url))
    if (!(Test-Path -LiteralPath $archive)) { Invoke-WebRequest $url -OutFile $archive }
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hash) { throw "SHA256 mismatch: $component" }
    $unpack = Join-Path $downloads $component
    New-Item -ItemType Directory -Force -Path $unpack | Out-Null
    & tar -xf $archive -C $unpack
    if ($LASTEXITCODE -ne 0) { throw "Cannot extract $component" }
    $package = Get-ChildItem -LiteralPath $unpack -Directory | Select-Object -First 1
    foreach ($part in Get-Content -LiteralPath (Join-Path $package.FullName 'components')) {
        Get-ChildItem -LiteralPath (Join-Path $package.FullName $part) | Copy-Item -Destination $compilerRoot -Recurse -Force
    }
    Write-Output "Ready: $component"
}
& (Join-Path $compilerRoot 'bin/rustc.exe') --version
if ($LASTEXITCODE -ne 0) { throw 'rustc failed' }
