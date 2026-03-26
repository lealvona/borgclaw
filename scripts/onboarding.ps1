#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$ROOT_DIR = Split-Path -Parent $PSScriptRoot
Set-Location $ROOT_DIR

function Show-Help {
    Write-Host @"
Usage: .\scripts\onboarding.ps1 [OPTIONS]

BorgClaw Onboarding Wizard

OPTIONS:
    --quick              Run minimal onboarding (skip integrations)
    --update             Reconfigure existing setup
    --features          Show available feature flags
    -h, --help          Show this help message

FEATURES:
    github              GitHub integration
    google              Google Workspace integration
    browser             Browser automation (Playwright)
    stt                 Speech-to-text (whisper.cpp)
    tts                 Text-to-speech (ElevenLabs)
    image               Image generation (Stable Diffusion)
    url                 URL shortener
    telegram            Telegram channel
    signal              Signal channel
    webhook             Webhook channel
    websocket           WebSocket channel (default: enabled)

EXAMPLES:
    # Minimal onboarding (skip integrations)
    .\scripts\onboarding.ps1 --quick

    # Reconfigure existing setup
    .\scripts\onboarding.ps1 --update

    # Show available features
    .\scripts\onboarding.ps1 --features
"@
}

function Show-Features {
    Write-Host @"
Available Features:
  github              GitHub integration
  google              Google Workspace integration
  browser             Browser automation (Playwright)
  stt                 Speech-to-text (whisper.cpp)
  tts                 Text-to-speech (ElevenLabs)
  image               Image generation (Stable Diffusion)
  url                 URL shortener
  telegram            Telegram channel
  signal              Signal channel
  webhook             Webhook channel
  websocket           WebSocket channel (default: enabled)
"@
}

# Parse arguments
$QUICK_MODE = $false
$UPDATE_MODE = $false

for ($i = 0; $i -lt $args.Count; $i++) {
    switch ($args[$i]) {
        "--quick" { $QUICK_MODE = $true }
        "--update" { $UPDATE_MODE = $true }
        "--features" { Show-Features; exit 0 }
        "-h" { Show-Help; exit 0 }
        "--help" { Show-Help; exit 0 }
        default { Write-Host "Unknown option: $($args[$i])"; Show-Help; exit 1 }
    }
}

Write-Host ""
Write-Host "╔════════════════════════════════════════════════════════════════╗" -ForegroundColor Magenta
Write-Host "║              🤖 BorgClaw Onboarding Wizard 🤖                   ║" -ForegroundColor Cyan
Write-Host "╚════════════════════════════════════════════════════════════════╝" -ForegroundColor Magenta
Write-Host ""

$CONFIG_FILE = "config.toml"

function Test-Config {
    if (Test-Path $CONFIG_FILE) {
        $content = Get-Content $CONFIG_FILE -Raw
        return $content -match "api_key"
    }
    return $false
}

if (Test-Config) {
    Write-Host "✓ Configuration found at $CONFIG_FILE" -ForegroundColor Green
    Write-Host ""
    Write-Host "Your BorgClaw is already configured!"
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  [r] Reconfigure  - Run setup again"
    Write-Host "  [s] Status       - Show current config"
    Write-Host "  [q] Quit         - Exit without changes"
    Write-Host ""
    $choice = Read-Host "Choice [r/s/q]"
    
    switch ($choice.ToLower()) {
        "r" {
            Write-Host ""
            Write-Host "[borgclaw] Starting reconfiguration..." -ForegroundColor Yellow
        }
        "s" {
            Write-Host ""
            Write-Host "[borgclaw] Current configuration:" -ForegroundColor Yellow
            Get-Content $CONFIG_FILE
            exit 0
        }
        default {
            Write-Host "[borgclaw] Exiting..." -ForegroundColor Yellow
            exit 0
        }
    }
} else {
    Write-Host "○ No configuration found. Starting setup..." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "[borgclaw] Running configuration wizard..." -ForegroundColor Yellow

if ($QUICK_MODE) {
    Write-Host "[borgclaw] Running in QUICK mode (minimal configuration)" -ForegroundColor Cyan
    cargo run --bin borgclaw -- init --quick
} else {
    cargo run --bin borgclaw -- init
}

Write-Host ""
Write-Host "╔════════════════════════════════════════════════════════════════╗" -ForegroundColor Magenta
Write-Host "║                    ✅ Setup Complete!                          ║" -ForegroundColor Green
Write-Host "╚════════════════════════════════════════════════════════════════╝" -ForegroundColor Magenta
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Yellow
Write-Host "  • Start REPL:     .\scripts\repl.ps1"
Write-Host "  • Start Gateway:  .\scripts\gateway.ps1"
Write-Host "  • Check system:   .\scripts\doctor.ps1"
Write-Host "  • Manage secrets: borgclaw secrets list"
Write-Host ""
