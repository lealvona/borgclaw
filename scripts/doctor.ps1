#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$ROOT_DIR = Split-Path -Parent $PSScriptRoot
Set-Location $ROOT_DIR

Write-Host "[borgclaw] System Doctor" -ForegroundColor Cyan
Write-Host "========================"
Write-Host ""

function Test-Tool {
    param(
        [string]$Command,
        [string]$Description,
        [bool]$Required = $false
    )
    
    if (Get-Command $Command -ErrorAction SilentlyContinue) {
        $version = switch ($Command) {
            "rustc" { rustc --version 2>$null }
            "cargo" { cargo --version 2>$null }
            "node" { node --version 2>$null }
            "docker" { docker --version 2>$null }
            "git" { git --version 2>$null }
            "psql" { psql --version 2>$null }
            "ollama" { ollama --version 2>$null }
            "bw" { (bw --version 2>$null | Select-Object -First 1) }
            "op" { op --version 2>$null }
            "signal-cli" { signal-cli --version 2>$null }
            default { "installed" }
        }
        Write-Host "✓ $Description`: $version" -ForegroundColor Green
        return $true
    } else {
        if ($Required) {
            Write-Host "✗ $Description`: NOT FOUND (required)" -ForegroundColor Red
            return $false
        } else {
            Write-Host "○ $Description`: NOT FOUND (optional)" -ForegroundColor Yellow
            return $true
        }
    }
}

function Test-File {
    param(
        [string]$Path,
        [string]$Description
    )
    
    if (Test-Path $Path) {
        Write-Host "✓ $Description`: exists" -ForegroundColor Green
    } else {
        Write-Host "○ $Description`: missing" -ForegroundColor Yellow
    }
}

$errors = 0

Write-Host "=== Required Tools ===" -ForegroundColor White
if (-not (Test-Tool "rustc" "Rust compiler" $true)) { $errors++ }
if (-not (Test-Tool "cargo" "Cargo build tool" $true)) { $errors++ }
if (-not (Test-Tool "git" "Git version control" $true)) { $errors++ }

Write-Host ""
Write-Host "=== Optional Tools ===" -ForegroundColor White
Test-Tool "node" "Node.js (for Playwright)" | Out-Null
Test-Tool "docker" "Docker (for pgvector runtime and command sandbox)" | Out-Null
Test-Tool "psql" "PostgreSQL client" | Out-Null
Test-Tool "ollama" "Ollama (embeddings runtime)" | Out-Null
Test-Tool "signal-cli" "Signal CLI (for Signal channel)" | Out-Null
Test-Tool "bw" "Bitwarden CLI (vault)" | Out-Null
Test-Tool "op" "1Password CLI (vault)" | Out-Null

Write-Host ""
Write-Host "=== Project Files ===" -ForegroundColor White
Test-File "Cargo.toml" "Workspace manifest"
Test-File "borgclaw-core\Cargo.toml" "Core crate manifest"
Test-File "borgclaw-cli\Cargo.toml" "CLI crate manifest"
Test-File "borgclaw-gateway\Cargo.toml" "Gateway crate manifest"

# Runtime configuration
$CONFIG_DIR = "$env:USERPROFILE\.config\borgclaw"
if (Test-Path "$CONFIG_DIR\config.toml") {
    Write-Host "✓ Runtime config: $CONFIG_DIR\config.toml" -ForegroundColor Green
} else {
    Write-Host "○ Runtime config: not configured (run .\scripts\onboarding.ps1)" -ForegroundColor Yellow
}

# Secrets encryption key (PR #196)
if (Test-Path "$CONFIG_DIR\.secrets_key") {
    Write-Host "✓ Secrets encryption key: $CONFIG_DIR\.secrets_key" -ForegroundColor Green
} else {
    Write-Host "○ Secrets encryption key: not initialized (will be created on first use)" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== Build Status ===" -ForegroundColor White
cargo check --quiet 2>$null
if ($LASTEXITCODE -eq 0) {
    Write-Host "✓ Code compiles successfully" -ForegroundColor Green
} else {
    Write-Host "✗ Code has compilation errors" -ForegroundColor Red
    $errors++
}

Write-Host ""
Write-Host "=== Optional Components ===" -ForegroundColor White
if (Test-Path ".local\tools\playwright") {
    Write-Host "✓ Playwright: installed" -ForegroundColor Green
} else {
    Write-Host "○ Playwright: not installed (run .\scripts\install-playwright.ps1)" -ForegroundColor Yellow
}

if (Test-Path ".local\tools\whisper.cpp") {
    Write-Host "✓ whisper.cpp: installed" -ForegroundColor Green
} else {
    Write-Host "○ whisper.cpp: not installed (run .\scripts\install-whisper.ps1)" -ForegroundColor Yellow
}

if (Get-Command "docker" -ErrorAction SilentlyContinue) {
    Write-Host "✓ pgvector runtime installer available (run .\scripts\install-pgvector.ps1)" -ForegroundColor Green
    docker image inspect borgclaw-sandbox:base *> $null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "✓ Docker sandbox image: borgclaw-sandbox:base" -ForegroundColor Green
    } else {
        Write-Host "○ Docker sandbox image: not built (run .\scripts\install-docker-sandbox.ps1)" -ForegroundColor Yellow
    }
} else {
    Write-Host "○ pgvector runtime not installable via helper script until Docker is available" -ForegroundColor Yellow
    Write-Host "○ Docker command sandbox not installable via helper script until Docker is available" -ForegroundColor Yellow
}

if (Get-Command "ollama" -ErrorAction SilentlyContinue) {
    Write-Host "✓ Ollama embeddings runtime: installed" -ForegroundColor Green
} else {
    Write-Host "○ Ollama embeddings runtime: not installed (run .\scripts\install-ollama.ps1)" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== Available Commands ===" -ForegroundColor White
Write-Host "  borgclaw status          - Show system status"
Write-Host "  borgclaw doctor          - Run diagnostics"
Write-Host "  borgclaw self-test       - Run self-test (exits 1 on failure)"
Write-Host "  borgclaw schedules list  - List scheduled tasks"
Write-Host "  borgclaw heartbeat list  - List heartbeat tasks"
Write-Host "  borgclaw secrets list    - List stored secrets"
Write-Host "  borgclaw backup export   - Export runtime state"

Write-Host ""
Write-Host "========================"
if ($errors -eq 0) {
    Write-Host "✅ All checks passed!" -ForegroundColor Green
} else {
    Write-Host "❌ $errors error(s) found" -ForegroundColor Red
    exit 1
}
