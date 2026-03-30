#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

param(
    [switch]$All
)

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root
. (Join-Path $Root "scripts\lib\build-env.ps1")
Initialize-BorgClawBuildEnv

$targetDir = Get-BorgClawTargetDir

Write-Host "[borgclaw] Cleaning build scratch..." -ForegroundColor Yellow
foreach ($incrementalDir in @(
    (Join-Path $targetDir "debug\incremental"),
    (Join-Path $targetDir "release\incremental")
)) {
    if (Test-Path $incrementalDir) {
        Remove-Item -Recurse -Force $incrementalDir -ErrorAction SilentlyContinue
    }
}
Clear-BorgClawTempArtifacts (Join-Path $Root ".tmp")
Clear-BorgClawTempArtifacts $env:BORGCLAW_TMP_ROOT

if ($All) {
    Write-Host "[borgclaw] Running cargo clean..." -ForegroundColor Yellow
    cargo clean
}

Write-Host "[borgclaw] Remaining build directories:" -ForegroundColor Yellow
if (Test-Path $targetDir) {
    Get-Item $targetDir | ForEach-Object {
        $targetSizeMb = [Math]::Round(((Get-ChildItem -Path $_.FullName -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum / 1MB), 2)
        Write-Host "  $($_.FullName): ${targetSizeMb} MB" -ForegroundColor Gray
    }
}
if (Test-Path $env:BORGCLAW_TMP_ROOT) {
    $tmpSizeMb = [Math]::Round(((Get-ChildItem -Path $env:BORGCLAW_TMP_ROOT -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum / 1MB), 2)
    Write-Host "  $($env:BORGCLAW_TMP_ROOT): ${tmpSizeMb} MB" -ForegroundColor Gray
}
