#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root

cargo run --bin borgclaw -- repl
