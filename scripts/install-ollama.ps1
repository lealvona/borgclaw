#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$Model = if ($env:BORGCLAW_EMBED_MODEL) { $env:BORGCLAW_EMBED_MODEL } else { "nomic-embed-text" }

if (Get-Command ollama -ErrorAction SilentlyContinue) {
    Write-Host "[borgclaw] Ollama already installed: $(ollama --version)" -ForegroundColor Green
} elseif (Get-Command winget -ErrorAction SilentlyContinue) {
    Write-Host "[borgclaw] Installing Ollama with winget..." -ForegroundColor Green
    winget install --id Ollama.Ollama -e --accept-package-agreements --accept-source-agreements
} else {
    Write-Host "[borgclaw] winget is not available. Install Ollama from https://ollama.com/download/windows" -ForegroundColor Red
    exit 1
}

Write-Host "[borgclaw] Pulling embeddings model $Model..." -ForegroundColor Green
ollama pull $Model

Write-Host ""
Write-Host "[borgclaw] ✓ Ollama embeddings runtime is ready" -ForegroundColor Green
Write-Host "[borgclaw] Recommended BorgClaw config:" -ForegroundColor Cyan
Write-Host "  [memory]"
Write-Host "  embedding_endpoint = `"http://127.0.0.1:11434/api/embeddings`""
Write-Host "  hybrid_search = true"
