#!/usr/bin/env bash
# Setup provider API key for BorgClaw
#
# This script helps configure API keys for your chosen LLM provider.
# Run this after onboarding if you selected "Status" instead of configuring
# the provider, or if you're getting 401 Unauthorized errors.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/build-env.sh"
borgclaw_prepare_build_env

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${GREEN}[borgclaw]${NC} $*"; }
warn() { echo -e "${YELLOW}[borgclaw]${NC} $*"; }
error() { echo -e "${RED}[borgclaw]${NC} $*"; }
info() { echo -e "${BLUE}[borgclaw]${NC} $*"; }

# Check if borgclaw binary exists
BORGCLAW="$(borgclaw_target_dir)/release/borgclaw"
if [ ! -f "$BORGCLAW" ]; then
    BORGCLAW="$(borgclaw_target_dir)/debug/borgclaw"
    if [ ! -f "$BORGCLAW" ]; then
        error "BorgClaw binary not found. Please run ./scripts/bootstrap.sh first"
        exit 1
    fi
fi

# Read current config to get provider
CONFIG_FILE="${HOME}/.config/borgclaw/config.toml"
if [ ! -f "$CONFIG_FILE" ]; then
    warn "Config file not found at ${CONFIG_FILE}"
    warn "Please run ./scripts/onboarding.sh first"
    exit 1
fi

# Extract provider from config
PROVIDER=$(grep -E '^provider\s*=' "$CONFIG_FILE" | head -1 | sed 's/.*=\s*"\([^"]*\)".*/\1/')
if [ -z "$PROVIDER" ]; then
    PROVIDER="anthropic"  # default
fi

# Map provider to API key environment variable
case "$PROVIDER" in
    openai)
        API_KEY_NAME="OPENAI_API_KEY"
        API_KEY_URL="https://platform.openai.com/api-keys"
        ;;
    anthropic)
        API_KEY_NAME="ANTHROPIC_API_KEY"
        API_KEY_URL="https://console.anthropic.com/settings/keys"
        ;;
    google)
        API_KEY_NAME="GOOGLE_API_KEY"
        API_KEY_URL="https://makersuite.google.com/app/apikey"
        ;;
    kimi)
        API_KEY_NAME="KIMI_API_KEY"
        API_KEY_URL="https://platform.moonshot.ai/"
        ;;
    minimax)
        API_KEY_NAME="MINIMAX_API_KEY"
        API_KEY_URL="https://platform.minimax.io/"
        ;;
    z)
        API_KEY_NAME="Z_API_KEY"
        API_KEY_URL="https://z.ai/model-api"
        ;;
    *)
        error "Unknown provider: $PROVIDER"
        exit 1
        ;;
esac

log "Current provider: ${PROVIDER}"
log "Required API key: ${API_KEY_NAME}"

# Check if key is already stored
if $BORGCLAW secrets check "$API_KEY_NAME" &>/dev/null; then
    log "✓ API key ${API_KEY_NAME} is already stored"
    
    # Check if it's the right key
    read -p "Do you want to update it? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        log "Keeping existing API key"
        exit 0
    fi
fi

info ""
info "You'll need your ${API_KEY_NAME}"
info "Get it from: ${API_KEY_URL}"
info ""

# Prompt for API key
read -s -p "Enter ${API_KEY_NAME}: " API_KEY
echo

if [ -z "$API_KEY" ]; then
    error "API key cannot be empty"
    exit 1
fi

# Store the key
log "Storing API key in encrypted vault..."
if echo "$API_KEY" | $BORGCLAW secrets set "$API_KEY_NAME"; then
    log "✓ API key stored successfully"
    log ""
    log "You can now use the REPL:"
    log "  ./scripts/repl.sh"
else
    error "✗ Failed to store API key"
    exit 1
fi
