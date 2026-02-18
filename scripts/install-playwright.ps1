#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$ROOT_DIR = Split-Path -Parent $PSScriptRoot
$TOOLS_DIR = Join-Path $ROOT_DIR ".local\tools"
New-Item -ItemType Directory -Force -Path $TOOLS_DIR | Out-Null

Write-Host "[borgclaw] Installing Playwright..."

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Host "[borgclaw] ERROR: Node.js is required. Install from https://nodejs.org" -ForegroundColor Red
    exit 1
}

$PLAYWRIGHT_DIR = Join-Path $TOOLS_DIR "playwright"
New-Item -ItemType Directory -Force -Path $PLAYWRIGHT_DIR | Out-Null

Set-Location $PLAYWRIGHT_DIR

if (-not (Test-Path "package.json")) {
    @"
{
  "name": "borgclaw-playwright",
  "version": "1.0.0",
  "private": true,
  "dependencies": {
    "playwright": "^1.40.0"
  }
}
"@ | Out-File -FilePath "package.json" -Encoding utf8
}

if (-not (Test-Path "node_modules")) {
    Write-Host "[borgclaw] Installing npm dependencies..."
    npm install
}

Write-Host "[borgclaw] Installing browser binaries..."
npx playwright install chromium

Copy-Item -Path (Join-Path $ROOT_DIR "scripts\playwright\playwright-bridge.js") -Destination $PLAYWRIGHT_DIR -Force

Write-Host "[borgclaw] Playwright installed to $PLAYWRIGHT_DIR"
Write-Host "[borgclaw] Bridge: $PLAYWRIGHT_DIR\playwright-bridge.js"
