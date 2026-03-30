#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$ROOT_DIR = Split-Path -Parent $PSScriptRoot
Set-Location $ROOT_DIR

$BaseImageName = if ($args.Count -gt 0) { $args[0] } else { "borgclaw-sandbox:base" }
$RemoteImageName = if ($args.Count -gt 1) { $args[1] } else { "borgclaw-sandbox:remote" }

if (-not (Get-Command "docker" -ErrorAction SilentlyContinue)) {
    Write-Host "[borgclaw] docker is required to build the sandbox image" -ForegroundColor Red
    exit 1
}

Write-Host "[borgclaw] Building Docker sandbox base image: $BaseImageName" -ForegroundColor Yellow
docker build -t $BaseImageName -f docker/sandbox-base.Dockerfile .

Write-Host "[borgclaw] Building Docker sandbox remote image: $RemoteImageName" -ForegroundColor Yellow
docker build -t $RemoteImageName -f docker/sandbox-remote.Dockerfile .

Write-Host "[borgclaw] Docker sandbox images ready: $BaseImageName, $RemoteImageName" -ForegroundColor Green
