#!/usr/bin/env bash
# Provision a local PostgreSQL + pgvector runtime for BorgClaw memory

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

IMAGE="${PGVECTOR_IMAGE:-pgvector/pgvector:pg16}"
CONTAINER_NAME="${BORGCLAW_PGVECTOR_CONTAINER:-borgclaw-pgvector}"
PORT="${BORGCLAW_PGVECTOR_PORT:-5432}"
POSTGRES_USER="${BORGCLAW_PGVECTOR_USER:-postgres}"
POSTGRES_PASSWORD="${BORGCLAW_PGVECTOR_PASSWORD:-postgres}"
POSTGRES_DB="${BORGCLAW_PGVECTOR_DB:-borgclaw}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${GREEN}[borgclaw]${NC} $*"; }
warn() { echo -e "${YELLOW}[borgclaw]${NC} $*"; }
error() { echo -e "${RED}[borgclaw]${NC} $*"; }
info() { echo -e "${BLUE}[borgclaw]${NC} $*"; }

if ! command -v docker >/dev/null 2>&1; then
    error "Docker is required to provision the local pgvector runtime."
    error "Install Docker Desktop / Docker Engine and rerun this script."
    exit 1
fi

if ! docker info >/dev/null 2>&1; then
    error "Docker is installed but not running."
    exit 1
fi

log "Pulling ${IMAGE}..."
docker pull "${IMAGE}"

if docker ps -a --format '{{.Names}}' | grep -qx "${CONTAINER_NAME}"; then
    log "Container ${CONTAINER_NAME} already exists. Starting it if needed..."
    docker start "${CONTAINER_NAME}" >/dev/null
else
    log "Creating container ${CONTAINER_NAME}..."
    docker run -d \
        --name "${CONTAINER_NAME}" \
        -e POSTGRES_USER="${POSTGRES_USER}" \
        -e POSTGRES_PASSWORD="${POSTGRES_PASSWORD}" \
        -e POSTGRES_DB="${POSTGRES_DB}" \
        -p "${PORT}:5432" \
        "${IMAGE}" >/dev/null
fi

log "Waiting for PostgreSQL to accept connections..."
for _ in $(seq 1 30); do
    if docker exec "${CONTAINER_NAME}" pg_isready -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

if ! docker exec "${CONTAINER_NAME}" pg_isready -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" >/dev/null 2>&1; then
    error "Timed out waiting for PostgreSQL in ${CONTAINER_NAME}."
    exit 1
fi

log "Enabling pgvector extension..."
docker exec "${CONTAINER_NAME}" \
    psql -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" \
    -c "CREATE EXTENSION IF NOT EXISTS vector;" >/dev/null

log ""
log "✓ Local pgvector runtime is ready"
info "Connection string:"
log "  postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:${PORT}/${POSTGRES_DB}"
info "Next steps:"
log "  1. Set [memory].backend = \"postgres\""
log "  2. Set [memory].connection_string to the value above"
log "  3. Install embeddings runtime: ./scripts/install-ollama.sh"
log "  4. Pull an embeddings model: ollama pull nomic-embed-text"
