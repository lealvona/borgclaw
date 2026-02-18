#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$ROOT_DIR = Split-Path -Parent $PSScriptRoot
$TOOLS_DIR = Join-Path $ROOT_DIR ".local\tools"
New-Item -ItemType Directory -Force -Path $TOOLS_DIR | Out-Null

Write-Host "[borgclaw] Installing whisper.cpp..."

$WHISPER_DIR = Join-Path $TOOLS_DIR "whisper.cpp"
$WHISPER_REPO = "https://github.com/ggerganov/whisper.cpp"

if (Test-Path $WHISPER_DIR) {
    Write-Host "[borgclaw] whisper.cpp already exists at $WHISPER_DIR"
    Write-Host "[borgclaw] Pulling latest changes..."
    Set-Location $WHISPER_DIR
    git pull
} else {
    Write-Host "[borgclaw] Cloning whisper.cpp..."
    git clone $WHISPER_REPO $WHISPER_DIR
    Set-Location $WHISPER_DIR
}

Write-Host "[borgclaw] Building whisper.cpp..."

if (Get-Command cmake -ErrorAction SilentlyContinue) {
    $BUILD_DIR = Join-Path $WHISPER_DIR "build"
    New-Item -ItemType Directory -Force -Path $BUILD_DIR | Out-Null
    Set-Location $BUILD_DIR
    cmake ..
    cmake --build . --config Release -j
} else {
    Write-Host "[borgclaw] WARNING: cmake not found. Please build manually." -ForegroundColor Yellow
    Write-Host "[borgclaw] See: https://github.com/ggerganov/whisper.cpp#build"
}

Write-Host "[borgclaw] Downloading base model (base.en)..."
$MODELS_DIR = Join-Path $WHISPER_DIR "models"
New-Item -ItemType Directory -Force -Path $MODELS_DIR | Out-Null

$MODEL_URL = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
$MODEL_PATH = Join-Path $MODELS_DIR "ggml-base.en.bin"

if (-not (Test-Path $MODEL_PATH)) {
    Invoke-WebRequest -Uri $MODEL_URL -OutFile $MODEL_PATH
}

Write-Host "[borgclaw] whisper.cpp installed to $WHISPER_DIR"
Write-Host "[borgclaw] For better quality, download larger models from:"
Write-Host "  https://huggingface.co/ggerganov/whisper.cpp"
