#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Command
)

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root
. (Join-Path $Root "scripts\lib\build-env.ps1")
Initialize-BorgClawBuildEnv

if (-not $Command -or $Command.Count -eq 0) {
    Write-Host "Usage: .\scripts\with-build-env.ps1 <command> [args...]" -ForegroundColor Yellow
    exit 1
}

if ($Command.Count -eq 1) {
    & $Command[0]
} else {
    & $Command[0] $Command[1..($Command.Count - 1)]
}
exit $LASTEXITCODE
