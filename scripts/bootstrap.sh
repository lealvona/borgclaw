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

# Parse arguments
FORCE_REBUILD=false
for arg in "$@"; do
    case "$arg" in
        --force)
            FORCE_REBUILD=true
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --force    Force a clean rebuild even if binaries exist"
            echo "  --help     Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0              # Normal bootstrap (builds only if needed)"
            echo "  $0 --force      # Force rebuild from scratch"
            exit 0
            ;;
    esac
done

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

# Check if source files are newer than binaries (force rebuild if so)
BORGCLAW_BIN="$CARGO_TARGET_DIR/release/borgclaw"
GATEWAY_BIN="$CARGO_TARGET_DIR/release/borgclaw-gateway"

source_is_newer() {
    local binary="$1"
    if [ ! -f "$binary" ]; then
        return 0  # Binary doesn't exist, source is effectively newer
    fi
    
    # Find any Rust source files newer than the binary
    local newer_files
    newer_files=$(find "$ROOT_DIR/borgclaw-core/src" "$ROOT_DIR/borgclaw-cli/src" "$ROOT_DIR/borgclaw-gateway/src" \
        -name "*.rs" -newer "$binary" 2>/dev/null | head -1)
    
    if [ -n "$newer_files" ]; then
        return 0  # Source is newer
    fi
    
    # Also check Cargo.toml files
    newer_files=$(find "$ROOT_DIR" -maxdepth 2 -name "Cargo.toml" -newer "$binary" 2>/dev/null | head -1)
    
    if [ -n "$newer_files" ]; then
        return 0  # Cargo.toml is newer
    fi
    
    return 1  # Source is not newer
}

# Force rebuild if --force flag set or source is newer than binaries
if [ "$FORCE_REBUILD" = true ]; then
    warn "--force flag set: performing clean rebuild"
    log "Cleaning release build artifacts..."
    cargo clean --release 2>/dev/null || true
elif [ -f "$BORGCLAW_BIN" ] && source_is_newer "$BORGCLAW_BIN"; then
    warn "Source files are newer than existing binary"
    log "Forcing rebuild to ensure binaries are up to date..."
    cargo clean --release 2>/dev/null || true
elif [ -f "$GATEWAY_BIN" ] && source_is_newer "$GATEWAY_BIN"; then
    warn "Source files are newer than existing binary"
    log "Forcing rebuild to ensure binaries are up to date..."
    cargo clean --release 2>/dev/null || true
fi

# Build all binaries in release mode
# This ensures both borgclaw and borgclaw-gateway are built
if cargo build --release --bins 2>&1 | tee /tmp/bootstrap-build.log; then
    log "✓ All targets built successfully"
else
    error "✗ Build failed. See /tmp/bootstrap-build.log for details."
    exit 1
fi

# Verify binaries exist (variables defined earlier)
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
# PHASE 4: CREATE WORKSPACE INFRASTRUCTURE
# ============================================================================

echo ""
log "Phase 4: Creating workspace infrastructure..."

# Default workspace paths (matches AppConfig defaults)
WORKSPACE_DIR=".borgclaw/workspace"
SKILLS_DIR=".borgclaw/skills"
IDENTITY_FILE=".borgclaw/soul.md"

# Create workspace directories
mkdir -p "$WORKSPACE_DIR"
mkdir -p "$WORKSPACE_DIR/skills"
mkdir -p "$SKILLS_DIR"

log "✓ Workspace directory: $WORKSPACE_DIR"
log "✓ Workspace skills: $WORKSPACE_DIR/skills"
log "✓ Skills directory: $SKILLS_DIR"

# Create default identity/soul.md if it doesn't exist
if [ ! -f "$IDENTITY_FILE" ]; then
    log "  Creating default identity document..."
    cat > "$IDENTITY_FILE" << 'EOF'
# BorgClaw Agent Identity

You are BorgClaw, a helpful AI assistant focused on software engineering tasks.
You are running in a personal AI agent framework that emphasizes:

- **Security-first**: All operations go through approval gates and security scanning
- **Workspace-aware**: You operate within a defined workspace directory
- **Tool-enabled**: You have access to file operations, command execution, and integrations
- **Memory-backed**: Conversations are stored for context across sessions

## Core Directives

1. Be concise and actionable in responses
2. Prefer reading files before making changes
3. Use tools rather than describing what you would do
4. When uncertain, ask clarifying questions
5. Follow the user's git workflow and coding conventions

## Communication Style

- Use clear, professional language
- Format code and terminal output appropriately
- Provide context for recommendations
- Surface security or safety concerns proactively

## Safety Rules

- Never execute destructive operations without explicit confirmation
- Respect workspace boundaries for file operations
- Flag potentially sensitive data exposure
- Prefer read-only inspection when possible

---
*This identity document can be edited via: borgclaw config set agent.soul_path <path>*
*Or through the web UI: Press Ctrl+, in the gateway dashboard*
EOF
    log "✓ Default identity created: $IDENTITY_FILE"
else
    log "✓ Identity document already exists: $IDENTITY_FILE"
fi

# Copy bundled skills to workspace if they exist
if [ -d "skills" ]; then
    log "  Copying bundled skills to workspace..."
    for skill_file in skills/*.md; do
        if [ -f "$skill_file" ]; then
            skill_name=$(basename "$skill_file" .md)
            target_dir="$SKILLS_DIR/$skill_name"
            if [ ! -d "$target_dir" ]; then
                mkdir -p "$target_dir"
                cp "$skill_file" "$target_dir/SKILL.md"
                log "    ✓ Installed skill: $skill_name"
            fi
        fi
    done
fi

# ============================================================================
# PHASE 5: CREATE GLOBAL ENCRYPTION KEYS
# ============================================================================

echo ""
log "Phase 5: Initializing global encryption keys..."

BORGCLAW_WORKSPACE="${HOME}/.borgclaw"
SECRETS_FILE="${BORGCLAW_WORKSPACE}/secrets.enc"
KEY_FILE="${BORGCLAW_WORKSPACE}/secrets.enc.key"

mkdir -p "$BORGCLAW_WORKSPACE"

if [ -f "$KEY_FILE" ]; then
    log "✓ Encryption key already exists: $KEY_FILE"
else
    log "  Generating 256-bit encryption key..."
    
    # Generate a proper 32-byte (256-bit) random key using OpenSSL or /dev/urandom
    if command -v openssl >/dev/null 2>&1; then
        openssl rand -out "$KEY_FILE" 32
    else
        # Fallback to /dev/urandom (32 bytes = 256 bits)
        head -c 32 /dev/urandom > "$KEY_FILE"
    fi
    
    # Verify key was created with correct size
    key_size=$(stat -c%s "$KEY_FILE" 2>/dev/null || stat -f%z "$KEY_FILE" 2>/dev/null || echo "0")
    if [ -f "$KEY_FILE" ] && [ "$key_size" = "32" ]; then
        chmod 600 "$KEY_FILE"
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
        error "✗ Failed to generate encryption key"
        error "  Please ensure OpenSSL is installed or /dev/urandom is available."
        exit 1
    fi
fi

# ============================================================================
# PHASE 7: CREATE RUNTIME CONFIGURATION
# ============================================================================

echo ""
log "Phase 7: Setting up runtime configuration..."

CONFIG_DIR="${HOME}/.config/borgclaw"
mkdir -p "$CONFIG_DIR"

log "✓ Config directory: $CONFIG_DIR"

# ============================================================================
# PHASE 8: CHECK OPTIONAL COMPONENTS
# ============================================================================

echo ""
log "Phase 8: Checking optional components..."

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
# PHASE 9: MIGRATE LEGACY ENVIRONMENT
# ============================================================================

echo ""
log "Phase 9: Checking for legacy configuration..."

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
