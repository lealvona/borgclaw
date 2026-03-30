#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"

$Image = if ($env:PGVECTOR_IMAGE) { $env:PGVECTOR_IMAGE } else { "pgvector/pgvector:pg16" }
$ContainerName = if ($env:BORGCLAW_PGVECTOR_CONTAINER) { $env:BORGCLAW_PGVECTOR_CONTAINER } else { "borgclaw-pgvector" }
$Port = if ($env:BORGCLAW_PGVECTOR_PORT) { $env:BORGCLAW_PGVECTOR_PORT } else { "5432" }
$PostgresUser = if ($env:BORGCLAW_PGVECTOR_USER) { $env:BORGCLAW_PGVECTOR_USER } else { "postgres" }
$PostgresPassword = if ($env:BORGCLAW_PGVECTOR_PASSWORD) { $env:BORGCLAW_PGVECTOR_PASSWORD } else { "postgres" }
$PostgresDb = if ($env:BORGCLAW_PGVECTOR_DB) { $env:BORGCLAW_PGVECTOR_DB } else { "borgclaw" }

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    Write-Host "[borgclaw] Docker is required to provision the local pgvector runtime." -ForegroundColor Red
    exit 1
}

docker info *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Host "[borgclaw] Docker is installed but not running." -ForegroundColor Red
    exit 1
}

Write-Host "[borgclaw] Pulling $Image..." -ForegroundColor Green
docker pull $Image

$existing = docker ps -a --format '{{.Names}}' | Where-Object { $_ -eq $ContainerName }
if ($existing) {
    Write-Host "[borgclaw] Container $ContainerName already exists. Starting it if needed..." -ForegroundColor Green
    docker start $ContainerName *> $null
} else {
    Write-Host "[borgclaw] Creating container $ContainerName..." -ForegroundColor Green
    docker run -d `
        --name $ContainerName `
        -e "POSTGRES_USER=$PostgresUser" `
        -e "POSTGRES_PASSWORD=$PostgresPassword" `
        -e "POSTGRES_DB=$PostgresDb" `
        -p "${Port}:5432" `
        $Image *> $null
}

Write-Host "[borgclaw] Waiting for PostgreSQL to accept connections..." -ForegroundColor Green
for ($i = 0; $i -lt 30; $i++) {
    docker exec $ContainerName pg_isready -U $PostgresUser -d $PostgresDb *> $null
    if ($LASTEXITCODE -eq 0) {
        break
    }
    Start-Sleep -Seconds 1
}

docker exec $ContainerName pg_isready -U $PostgresUser -d $PostgresDb *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Host "[borgclaw] Timed out waiting for PostgreSQL in $ContainerName." -ForegroundColor Red
    exit 1
}

Write-Host "[borgclaw] Enabling pgvector extension..." -ForegroundColor Green
docker exec $ContainerName psql -U $PostgresUser -d $PostgresDb -c "CREATE EXTENSION IF NOT EXISTS vector;" *> $null

Write-Host ""
Write-Host "[borgclaw] ✓ Local pgvector runtime is ready" -ForegroundColor Green
Write-Host "[borgclaw] Connection string:" -ForegroundColor Cyan
Write-Host "  postgres://${PostgresUser}:${PostgresPassword}@localhost:${Port}/${PostgresDb}"
Write-Host "[borgclaw] Next steps:" -ForegroundColor Cyan
Write-Host "  1. Set [memory].backend = `"postgres`""
Write-Host "  2. Set [memory].connection_string to the value above"
Write-Host "  3. Install embeddings runtime: .\scripts\install-ollama.ps1"
Write-Host "  4. Pull an embeddings model: ollama pull nomic-embed-text"
