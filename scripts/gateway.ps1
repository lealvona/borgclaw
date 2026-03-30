#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root
. (Join-Path $Root "scripts\lib\build-env.ps1")
Initialize-BorgClawBuildEnv

cargo run --bin borgclaw-gateway
