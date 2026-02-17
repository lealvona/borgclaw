$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root

Write-Host "[borgclaw] Checking Rust toolchain..."
cargo --version
rustc --version

Write-Host "[borgclaw] Building workspace..."
cargo build

Write-Host "[borgclaw] Initializing config if missing..."
cargo run --bin borgclaw -- init

Write-Host "[borgclaw] Done. Start REPL with: .\scripts\repl.ps1"
