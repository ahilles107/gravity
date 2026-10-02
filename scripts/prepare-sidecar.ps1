# Build and stage the native Windows daemon for Tauri.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    $hostLine = rustc -vV | Where-Object { $_.StartsWith('host: ') }
    if ($LASTEXITCODE -ne 0 -or !$hostLine) { throw 'Cannot determine Rust host triple' }
    $triple = $hostLine.Substring(6)
    if (!$triple.EndsWith('windows-msvc')) { throw 'Use a native Windows MSVC Rust toolchain' }
    cargo build --release --locked -p gravityd
    if ($LASTEXITCODE -ne 0) { throw 'Daemon build failed' }
    $destination = Join-Path $repoRoot 'apps/desktop/src-tauri/binaries'
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'target/release/gravityd.exe') -Destination (Join-Path $destination "gravityd-$triple.exe")
    Write-Output "Staged gravityd-$triple.exe"
} finally {
    Pop-Location
}
