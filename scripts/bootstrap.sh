#!/usr/bin/env bash
#
# BorgClaw Bootstrap Script
# =========================
# 
# PURPOSE:
#   This script performs ONE-TIME system initialization for BorgClaw.
#   It builds ALL release targets and creates application-wide infrastructure
#   (keys, stores, directories) so subsequent operations don't need rebuilds.
#
# WHAT THIS DOES:
#   1. Validates prerequisites (Rust, cargo, git)
#   2. Builds ALL release targets (borgclaw CLI, borgclaw-gateway)
#   3. Creates .local/ directory structure (tools, data, cache, keys)
#   4. Generates global encryption keys for the secret store
#   5. Creates runtime directories and initial configuration
#
# WHAT THIS DOES NOT DO:
#   - Does NOT configure user-specific settings (use onboarding.sh)
#   - Does NOT install external services (use install-*.sh scripts)
#   - Does NOT collect API keys or tokens (use onboarding.sh or setup-provider-key.sh)
#
# RUN THIS FIRST:
#   ./scripts/bootstrap.sh
#
# THEN RUN:
#   ./scripts/onboarding.sh  # For user configuration
#
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

log() { echo -e "${GREEN}[bootstrap]${NC} $*"; }
warn() { echo -e "${YELLOW}[bootstrap]${NC} $*"; }
error() { echo -e "${RED}[bootstrap]${NC} $*"; }
info() { echo -e "${BLUE}[bootstrap]${NC} $*"; }

clear
echo ""
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║                                                               ║"
echo "║   ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄   ║"
echo "║   █                                                           █   ║"
echo "║   █                      ██████╗  ██████╗                      █   ║"
echo "║   █                      ██╔══██╗██╔════╝                      █   ║"
echo "║   █                      ██████╔╝██║                           █   ║"
echo "║   █                      ██╔══██╗██║                           █   ║"
echo "║   █                      ██████╔╝╚██████╗                      █   ║"
echo "║   █                      ╚═════╝  ╚═════╝                      █   ║"
echo "║   █                                                           █   ║"
echo "║   █              Personal AI Agent Framework               █   ║"
echo "║   █                                                           █   ║"
echo "║   ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀   ║"
echo "║                                                               ║"
echo "║              🔧 SYSTEM BOOTSTRAP (One-Time Setup)              ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

# ============================================================================
# PHASE 1: PREREQUISITE CHECKS
# ============================================================================

log "Phase 1: Checking prerequisites..."

MISSING=0
check_command() {
    if command -v "$1" &> /dev/null; then
        return 0
    else
        return 1
    fi
}

if ! check_command rustc; then
    error "✗ Rust compiler (rustc) not found"
    error "  Install from: https://rustup.rs"
    MISSING=1
else
    log "✓ Rust: $(rustc --version)"
fi

if ! check_command cargo; then
    error "✗ Cargo build tool not found"
    MISSING=1
else
    log "✓ Cargo: $(cargo --version)"
fi

if ! check_command git; then
    error "✗ Git not found"
    MISSING=1
else
    log "✓ Git: $(git --version | head -1)"
fi

if [ $MISSING -eq 1 ]; then
    echo ""
    error "ERROR: Missing required tools. Please install them first."
    exit 1
fi

# ============================================================================
# PHASE 2: BUILD ALL RELEASE TARGETS
# ============================================================================

echo ""
log "Phase 2: Building ALL release targets..."
info "  This builds both binaries in release mode so they don't need"
info "  rebuilding during normal operation. This may take a few minutes."
echo ""

# Build all binaries in release mode
# This ensures both borgclaw and borgclaw-gateway are built
if cargo build --release --bins 2>&1 | tee /tmp/bootstrap-build.log; then
    log "✓ All targets built successfully"
else
    error "✗ Build failed. See /tmp/bootstrap-build.log for details."
    exit 1
fi

# Verify binaries exist
BORGCLAW_BIN="$CARGO_TARGET_DIR/release/borgclaw"
GATEWAY_BIN="$CARGO_TARGET_DIR/release/borgclaw-gateway"

if [ ! -f "$BORGCLAW_BIN" ]; then
    error "✗ borgclaw binary not found at expected location"
    error "  Expected: $BORGCLAW_BIN"
    exit 1
fi

if [ ! -f "$GATEWAY_BIN" ]; then
    error "✗ borgclaw-gateway binary not found at expected location"
    error "  Expected: $GATEWAY_BIN"
    exit 1
fi

log "✓ BorgClaw CLI: $BORGCLAW_BIN"
log "✓ BorgClaw Gateway: $GATEWAY_BIN"

# ============================================================================
# PHASE 3: CREATE DIRECTORY STRUCTURE
# ============================================================================

echo ""
log "Phase 3: Creating directory structure..."

# Create .local subdirectories
mkdir -p .local/tools
mkdir -p .local/data
mkdir -p .local/cache
mkdir -p .local/keys
mkdir -p .local/logs

log "✓ .local/tools   - External tools (Playwright, whisper, etc.)"
log "✓ .local/data    - Runtime data (databases, tokens, state)"
log "✓ .local/cache   - Build cache and temporary files"
log "✓ .local/keys    - Encryption keys and certificates"
log "✓ .local/logs    - Application logs"

# Add to .gitignore if not present
if [ ! -f ".gitignore" ] || ! grep -q "^\.local" .gitignore 2>/dev/null; then
    echo ".local/" >> .gitignore
    log "✓ Added .local/ to .gitignore"
fi

# ============================================================================
# PHASE 4: CREATE GLOBAL ENCRYPTION KEYS
# ============================================================================

echo ""
log "Phase 4: Initializing global encryption keys..."

BORGCLAW_WORKSPACE="${HOME}/.borgclaw"
SECRETS_FILE="${BORGCLAW_WORKSPACE}/secrets.enc"
KEY_FILE="${BORGCLAW_WORKSPACE}/secrets.enc.key"

mkdir -p "$BORGCLAW_WORKSPACE"

if [ -f "$KEY_FILE" ]; then
    log "✓ Encryption key already exists: $KEY_FILE"
else
    # Initialize secret store by storing a test value (creates key automatically)
    log "  Generating encryption key..."
    if echo "bootstrap_init" | "$BORGCLAW_BIN" secrets set "_bootstrap_test" 2>/dev/null; then
        "$BORGCLAW_BIN" secrets delete "_bootstrap_test" 2>/dev/null || true
        log "✓ Encryption key generated: $KEY_FILE"
        
        warn ""
        warn "╔══════════════════════════════════════════════════════════════╗"
        warn "║  ⚠️  IMPORTANT: BACK UP YOUR ENCRYPTION KEY!                 ║"
        warn "╠══════════════════════════════════════════════════════════════╣"
        warn "║                                                              ║"
        warn "║  Your encryption key is stored at:                           ║"
        warn "║    $KEY_FILE"
        warn "║                                                              ║"
        warn "║  If you lose this key, your secrets CANNOT be recovered!     ║"
        warn "║                                                              ║"
        warn "║  Recommended: Create a backup now:                           ║"
        warn "║    cp $KEY_FILE $KEY_FILE.backup"
        warn "║                                                              ║"
        warn "║  Store the backup in a password manager or secure location.  ║"
        warn "║                                                              ║"
        warn "╚══════════════════════════════════════════════════════════════╝"
        warn ""
    else
        error "✗ Failed to initialize encryption key"
        error "  This may indicate a problem with the BorgClaw binary."
        exit 1
    fi
fi

# ============================================================================
# PHASE 5: CREATE RUNTIME CONFIGURATION
# ============================================================================

echo ""
log "Phase 5: Setting up runtime configuration..."

CONFIG_DIR="${HOME}/.config/borgclaw"
mkdir -p "$CONFIG_DIR"

log "✓ Config directory: $CONFIG_DIR"

# ============================================================================
# PHASE 6: CHECK OPTIONAL COMPONENTS
# ============================================================================

echo ""
log "Phase 6: Checking optional components..."

# Check for Node.js (needed for Playwright)
if check_command node; then
    log "✓ Node.js: $(node --version)"
    if [ ! -d ".local/tools/playwright" ]; then
        info "  Install Playwright: ./scripts/install-playwright.sh"
    fi
else
    warn "○ Node.js not found (optional, needed for browser automation)"
    info "  Install from: https://nodejs.org"
fi

# Check for Docker
if check_command docker; then
    log "✓ Docker: $(docker --version | head -1)"
    info "  Available install scripts:"
    info "    ./scripts/install-pgvector.sh         - PostgreSQL with pgvector"
    info "    ./scripts/install-docker-sandbox.sh   - Command sandbox"
else
    warn "○ Docker not found (optional)"
fi

# Check for Ollama
if check_command ollama; then
    log "✓ Ollama: $(ollama --version 2>/dev/null || echo 'installed')"
else
    info "  Install Ollama: ./scripts/install-ollama.sh"
fi

# Check for GitHub CLI
if check_command gh; then
    log "✓ GitHub CLI: $(gh --version | head -1)"
else
    warn "○ GitHub CLI not found (optional, recommended for GitHub integration)"
    info "  Install: https://cli.github.com"
fi

# ============================================================================
# PHASE 7: MIGRATE LEGACY ENVIRONMENT
# ============================================================================

echo ""
log "Phase 7: Checking for legacy configuration..."

# Check if .env exists with secrets that should be migrated
if [ -f ".env" ]; then
    ENV_SECRETS=$(grep -E '(_API_KEY|_TOKEN|_SECRET|_PASSWORD|CLIENT_ID|CLIENT_SECRET)=' .env 2>/dev/null | wc -l)
    if [ "$ENV_SECRETS" -gt 0 ]; then
        echo ""
        warn "⚠ Found $ENV_SECRETS secret(s) in .env file"
        warn "  Secrets should be stored in the encrypted vault for security."
        echo ""
        read -p "Migrate .env secrets to encrypted store now? [y/N] " -n 1 -r
        echo ""
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            ./scripts/migrate-env-to-secrets.sh
        else
            info "  You can migrate later with: ./scripts/migrate-env-to-secrets.sh"
        fi
    fi
fi

# ============================================================================
# COMPLETION
# ============================================================================

echo ""
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║                    ✅ BOOTSTRAP COMPLETE!                      ║"
echo "╠═══════════════════════════════════════════════════════════════╣"
echo "║                                                               ║"
echo "║  All release targets have been built and will NOT need        ║"
echo "║  rebuilding for normal operation.                            ║"
echo "║                                                               ║"
echo "║  NEXT STEPS:                                                  ║"
echo "║                                                               ║"
echo "║  1. Run onboarding to configure your personal settings:       ║"
echo "║     ./scripts/onboarding.sh                                   ║"
echo "║                                                               ║"
echo "║  2. Check system health:                                      ║"
echo "║     ./scripts/doctor.sh                                       ║"
echo "║                                                               ║"
echo "║  3. Start using BorgClaw:                                     ║"
echo "║     ./scripts/repl.sh        (Interactive chat)               ║"
echo "║     ./scripts/gateway.sh     (Web dashboard)                  ║"
echo "║                                                               ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

# Show build info
borgclaw_print_build_env
if [ -d "$CARGO_TARGET_DIR" ]; then
    echo "  Target size: $(du -sh "$CARGO_TARGET_DIR" 2>/dev/null | awk '{print $1}')"
fi
echo ""
info "Build cache management:"
info "  ./scripts/clean-build-cache.sh         # Trim incremental cache"
info "  ./scripts/clean-build-cache.sh --all   # Full clean rebuild"
echo ""
