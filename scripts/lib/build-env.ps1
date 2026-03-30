Set-StrictMode -Version Latest

if (Get-Variable -Name BorgClawBuildEnvPsLoaded -Scope Script -ErrorAction SilentlyContinue) {
    return
}
$script:BorgClawBuildEnvPsLoaded = $true

if (-not (Get-Variable -Name RootDir -Scope Script -ErrorAction SilentlyContinue)) {
    $script:RootDir = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}

$script:BorgClawCacheRootDefault = Join-Path $script:RootDir ".local\cache"
$script:BorgClawTmpRootDefault = Join-Path $script:BorgClawCacheRootDefault "tmp"
$script:BorgClawTargetDirDefault = Join-Path $script:RootDir "target"
$script:BorgClawTargetSoftLimitGbDefault = 12
$script:BorgClawTmpPruneDaysDefault = 7

function Initialize-BorgClawBuildEnv {
    if (-not $env:CARGO_TARGET_DIR) {
        $env:CARGO_TARGET_DIR = $script:BorgClawTargetDirDefault
    }
    if (-not $env:BORGCLAW_CACHE_ROOT) {
        $env:BORGCLAW_CACHE_ROOT = $script:BorgClawCacheRootDefault
    }
    if (-not $env:BORGCLAW_TMP_ROOT) {
        $env:BORGCLAW_TMP_ROOT = $script:BorgClawTmpRootDefault
    }
    if (-not $env:TEMP) {
        $env:TEMP = $env:BORGCLAW_TMP_ROOT
    }
    if (-not $env:TMP) {
        $env:TMP = $env:TEMP
    }
    if (-not $env:CARGO_INCREMENTAL) {
        $env:CARGO_INCREMENTAL = "0"
    }
    if (-not $env:BORGCLAW_TARGET_SOFT_LIMIT_GB) {
        $env:BORGCLAW_TARGET_SOFT_LIMIT_GB = "$script:BorgClawTargetSoftLimitGbDefault"
    }
    if (-not $env:BORGCLAW_TMP_PRUNE_DAYS) {
        $env:BORGCLAW_TMP_PRUNE_DAYS = "$script:BorgClawTmpPruneDaysDefault"
    }

    New-Item -ItemType Directory -Force -Path $env:BORGCLAW_CACHE_ROOT | Out-Null
    New-Item -ItemType Directory -Force -Path $env:BORGCLAW_TMP_ROOT | Out-Null
    New-Item -ItemType Directory -Force -Path $env:CARGO_TARGET_DIR | Out-Null

    Clear-BorgClawTempArtifacts (Join-Path $script:RootDir ".tmp")
    Clear-BorgClawTempArtifacts $env:BORGCLAW_TMP_ROOT
    Clear-BorgClawIncrementalCacheIfOversized
}

function Get-BorgClawTargetDir {
    if ($env:CARGO_TARGET_DIR) {
        return $env:CARGO_TARGET_DIR
    }
    return $script:BorgClawTargetDirDefault
}

function Clear-BorgClawTempArtifacts {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        return
    }

    $cutoff = (Get-Date).AddDays(-[int]$env:BORGCLAW_TMP_PRUNE_DAYS)
    Get-ChildItem -Path $Path -Force -ErrorAction SilentlyContinue | Where-Object {
        $_.LastWriteTime -lt $cutoff -and (
            $_.Name -like "borgclaw_*" -or
            $_.Name -like "cargo-*" -or
            $_.Name -like "rustc*"
        )
    } | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
}

function Get-BorgClawTargetSizeKb {
    $targetDir = Get-BorgClawTargetDir
    if (-not (Test-Path $targetDir)) {
        return 0
    }

    $bytes = (Get-ChildItem -Path $targetDir -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
    if (-not $bytes) {
        return 0
    }

    return [int][Math]::Ceiling($bytes / 1KB)
}

function Clear-BorgClawIncrementalCacheIfOversized {
    $softLimitKb = [int]$env:BORGCLAW_TARGET_SOFT_LIMIT_GB * 1024 * 1024
    $targetSizeKb = Get-BorgClawTargetSizeKb
    if ($targetSizeKb -le $softLimitKb) {
        return
    }

    $targetDir = Get-BorgClawTargetDir
    foreach ($incrementalDir in @(
        (Join-Path $targetDir "debug\incremental"),
        (Join-Path $targetDir "release\incremental")
    )) {
        if (Test-Path $incrementalDir) {
            Remove-Item -Recurse -Force $incrementalDir -ErrorAction SilentlyContinue
        }
    }
}

function Show-BorgClawBuildEnv {
    Write-Host "[borgclaw] Build hygiene:" -ForegroundColor Yellow
    Write-Host "  target: $(Get-BorgClawTargetDir)" -ForegroundColor Gray
    Write-Host "  temp:   $($env:TEMP)" -ForegroundColor Gray
    Write-Host "  incr:   $($env:CARGO_INCREMENTAL)" -ForegroundColor Gray
}
