#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$ROOT_DIR = Split-Path -Parent $PSScriptRoot
Set-Location $ROOT_DIR

$ImageName = if ($args.Count -gt 0) { $args[0] } else { "borgclaw-sandbox:base" }
$DockerfilePath = if ($env:DOCKERFILE_PATH) { $env:DOCKERFILE_PATH } else { "docker/sandbox-base.Dockerfile" }

if (-not (Get-Command "docker" -ErrorAction SilentlyContinue)) {
    Write-Host "[borgclaw] docker is required to build the sandbox image" -ForegroundColor Red
    exit 1
}

Write-Host "[borgclaw] Building Docker sandbox image: $ImageName" -ForegroundColor Yellow
docker build -t $ImageName -f $DockerfilePath .

Write-Host "[borgclaw] Docker sandbox image ready: $ImageName" -ForegroundColor Green
