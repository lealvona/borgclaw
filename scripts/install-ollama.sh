#!/usr/bin/env bash
# Install Ollama and pull an embeddings model for BorgClaw memory hybrid search

set -euo pipefail

MODEL="${BORGCLAW_EMBED_MODEL:-nomic-embed-text}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${GREEN}[borgclaw]${NC} $*"; }
warn() { echo -e "${YELLOW}[borgclaw]${NC} $*"; }
error() { echo -e "${RED}[borgclaw]${NC} $*"; }
info() { echo -e "${BLUE}[borgclaw]${NC} $*"; }

if ! command -v curl >/dev/null 2>&1; then
    error "curl is required to install Ollama."
    exit 1
fi

if command -v ollama >/dev/null 2>&1; then
    log "Ollama already installed: $(ollama --version)"
else
    log "Installing Ollama..."
    curl -fsSL https://ollama.com/install.sh | sh
fi

log "Ensuring Ollama service is reachable..."
if ! ollama list >/dev/null 2>&1; then
    warn "Ollama is installed but not responding yet. Start the service if needed, then rerun."
fi

log "Pulling embeddings model ${MODEL}..."
ollama pull "${MODEL}"

log ""
log "✓ Ollama embeddings runtime is ready"
info "Recommended BorgClaw config:"
log "  [memory]"
log "  embedding_endpoint = \"http://127.0.0.1:11434/api/embeddings\""
log "  hybrid_search = true"
