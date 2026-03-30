#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$ROOT_DIR = Split-Path -Parent $PSScriptRoot
Set-Location $ROOT_DIR
. (Join-Path $ROOT_DIR "scripts\lib\build-env.ps1")
Initialize-BorgClawBuildEnv

Write-Host ""
Write-Host "╔═══════════════════════════════════════════════════════════════╗" -ForegroundColor Magenta
Write-Host "║                                                               ║" -ForegroundColor Magenta
Write-Host "║                      ██████╗  ██████╗                      ║" -ForegroundColor Cyan
Write-Host "║                      ██╔══██╗██╔════╝                      ║" -ForegroundColor Cyan
Write-Host "║                      ██████╔╝██║                           ║" -ForegroundColor Cyan
Write-Host "║                      ██╔══██╗██║                           ║" -ForegroundColor Cyan
Write-Host "║                      ██████╔╝╚██████╗                      ║" -ForegroundColor Cyan
Write-Host "║                      ╚═════╝  ╚═════╝                      ║" -ForegroundColor Cyan
Write-Host "║                                                               ║" -ForegroundColor Magenta
Write-Host "║                Personal AI Agent Framework                 ║" -ForegroundColor Magenta
Write-Host "║                                                               ║" -ForegroundColor Magenta
Write-Host "╚═══════════════════════════════════════════════════════════════╝" -ForegroundColor Magenta
Write-Host ""

function Test-Command {
    param($Command)
    if (Get-Command $Command -ErrorAction SilentlyContinue) {
        $path = (Get-Command $Command).Source
        Write-Host "✓ $Command`: $path" -ForegroundColor Green
        return $true
    } else {
        Write-Host "✗ $Command`: NOT FOUND" -ForegroundColor Red
        return $false
    }
}

Write-Host "[borgclaw] Checking prerequisites..." -ForegroundColor Yellow
$missing = $false

if (-not (Test-Command "rustc")) { $missing = $true }
if (-not (Test-Command "cargo")) { $missing = $true }
if (-not (Test-Command "git")) { $missing = $true }

if ($missing) {
    Write-Host ""
    Write-Host "[borgclaw] ERROR: Missing required tools." -ForegroundColor Red
    Write-Host "[borgclaw] Install Rust from: https://rustup.rs" -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "[borgclaw] Rust versions:" -ForegroundColor Yellow
rustc --version
cargo --version

Write-Host ""
Write-Host "[borgclaw] Optional tools:" -ForegroundColor Yellow
Test-Command "node" | Out-Null
Test-Command "docker" | Out-Null
Test-Command "psql" | Out-Null
Test-Command "ollama" | Out-Null
Test-Command "signal-cli" | Out-Null
Test-Command "bw" | Out-Null
Test-Command "op" | Out-Null

Write-Host ""
Show-BorgClawBuildEnv
if (Test-Path $env:CARGO_TARGET_DIR) {
    $targetSizeMb = [Math]::Round(((Get-ChildItem -Path $env:CARGO_TARGET_DIR -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum / 1MB), 2)
    Write-Host "  current target size: ${targetSizeMb} MB" -ForegroundColor Gray
}
Write-Host "  clean incremental cache: .\scripts\clean-build-cache.ps1" -ForegroundColor Gray
Write-Host "  full cargo clean:        .\scripts\clean-build-cache.ps1 -All" -ForegroundColor Gray

Write-Host ""
Write-Host "[borgclaw] Building workspace..." -ForegroundColor Yellow
cargo build --release

Write-Host ""
Write-Host "[borgclaw] Creating .local directory structure..." -ForegroundColor Yellow
New-Item -ItemType Directory -Force -Path ".local\tools" | Out-Null
New-Item -ItemType Directory -Force -Path ".local\data" | Out-Null
New-Item -ItemType Directory -Force -Path ".local\cache" | Out-Null

if (-not (Test-Path ".gitignore")) {
    ".local/" | Out-File -FilePath ".gitignore" -Encoding utf8
} elseif (-not (Select-String -Path ".gitignore" -Pattern "^\.local" -Quiet)) {
    Add-Content -Path ".gitignore" -Value ".local/"
}

Write-Host ""
Write-Host "[borgclaw] Memory runtimes:" -ForegroundColor Yellow
if (Get-Command "docker" -ErrorAction SilentlyContinue) {
    Write-Host "  PostgreSQL + pgvector runtime: .\scripts\install-pgvector.ps1" -ForegroundColor Gray
    Write-Host "  Docker command sandbox image: .\scripts\install-docker-sandbox.ps1" -ForegroundColor Gray
} else {
    Write-Host "  Docker not found; pgvector and Docker sandbox helper installers require Docker" -ForegroundColor Yellow
}
if (Get-Command "ollama" -ErrorAction SilentlyContinue) {
    Write-Host "  ✓ Ollama: Installed" -ForegroundColor Green
} else {
    Write-Host "  Embeddings runtime (recommended for hybrid search): .\scripts\install-ollama.ps1" -ForegroundColor Gray
}

Write-Host ""
Write-Host "[borgclaw] ✅ Bootstrap complete!" -ForegroundColor Green
Write-Host ""
Write-Host "[borgclaw] Next steps:" -ForegroundColor Yellow
Write-Host "  1. Run onboarding:    .\scripts\onboarding.ps1"
Write-Host "  2. Check system:      .\scripts\doctor.ps1"
Write-Host "  3. Start REPL:        .\scripts\repl.ps1"
Write-Host "  4. Start Gateway:     .\scripts\gateway.ps1"
Write-Host ""
