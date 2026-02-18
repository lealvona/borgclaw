#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$ROOT_DIR = Split-Path -Parent $PSScriptRoot
Set-Location $ROOT_DIR

Write-Host ""
Write-Host "╔════════════════════════════════════════════════════════════════╗" -ForegroundColor Magenta
Write-Host "║              BorgClaw Onboarding Wizard                        ║" -ForegroundColor Cyan
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
cargo run --bin borgclaw -- init

Write-Host ""
Write-Host "╔════════════════════════════════════════════════════════════════╗" -ForegroundColor Magenta
Write-Host "║                    Setup Complete!                             ║" -ForegroundColor Green
Write-Host "╚════════════════════════════════════════════════════════════════╝" -ForegroundColor Magenta
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Yellow
Write-Host "  • Start REPL:     .\scripts\repl.ps1"
Write-Host "  • Start Gateway:  .\scripts\gateway.ps1"
Write-Host "  • Check system:   .\scripts\doctor.ps1"
Write-Host ""
