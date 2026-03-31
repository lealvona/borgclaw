#!/usr/bin/env bash
#
# BorgClaw User Installation Script
# =================================
#
# PURPOSE:
#   Installs BorgClaw binaries to a user-level directory and optionally
#   configures PATH. This is a non-invasive installation that doesn't
#   require system-wide permissions.
#
# INSTALLATION METHODS:
#   1. Copy to user bin directory (~/.local/bin/ on Linux/macOS)
#   2. Add target directory to PATH (if copy is not desired)
#
# USAGE:
#   ./scripts/install.sh              # Interactive installation
#   ./scripts/install.sh --path       # Only add to PATH, don't copy
#   ./scripts/install.sh --force      # Overwrite existing installation
#

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# Source build environment
source "$ROOT_DIR/scripts/lib/build-env.sh"
borgclaw_prepare_build_env

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "${GREEN}[install]${NC} $*"; }
warn() { echo -e "${YELLOW}[install]${NC} $*"; }
error() { echo -e "${RED}[install]${NC} $*"; }
info() { echo -e "${BLUE}[install]${NC} $*"; }
section() { echo -e "${CYAN}$*${NC}"; }

# Parse arguments
PATH_ONLY=false
FORCE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --path)
            PATH_ONLY=true
            shift
            ;;
        --force)
            FORCE=true
            shift
            ;;
        -h|--help)
            cat << 'EOF'
Usage: ./scripts/install.sh [OPTIONS]

Install BorgClaw to user directory and/or configure PATH.

OPTIONS:
    --path      Only add to PATH, don't copy binaries
    --force     Overwrite existing installation
    -h, --help  Show this help message

EXAMPLES:
    # Standard install (copy to ~/.local/bin)
    ./scripts/install.sh

    # Only add target dir to PATH (no copy)
    ./scripts/install.sh --path

    # Reinstall (overwrite existing)
    ./scripts/install.sh --force

EOF
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

echo ""
section "═══════════════════════════════════════════════════════════════"
section "           BorgClaw User Installation"
section "═══════════════════════════════════════════════════════════════"
echo ""

# ============================================================================
# LOCATE BINARIES
# ============================================================================

BORGCLAW_BIN="$(borgclaw_locate_binary)"
GATEWAY_BIN="$(borgclaw_locate_binary "$(borgclaw_target_dir)" borgclaw-gateway)"

if [ -z "$BORGCLAW_BIN" ]; then
    error "BorgClaw binary not found!"
    error "Please run ./scripts/bootstrap.sh first to build the binaries."
    exit 1
fi

log "Found binaries:"
info "  borgclaw: $BORGCLAW_BIN"
if [ -n "$GATEWAY_BIN" ]; then
    info "  borgclaw-gateway: $GATEWAY_BIN"
fi

# ============================================================================
# DETERMINE INSTALLATION METHOD
# ============================================================================

INSTALL_DIR=""
SHELL_PROFILE=""

if [ "$PATH_ONLY" = true ]; then
    log "PATH-only mode: Will add target directory to PATH"
    INSTALL_DIR="$(borgclaw_target_dir)"
else
    # Try to find/create user bin directory
    INSTALL_DIR="$(borgclaw_user_bin_dir)" || {
        warn "Could not create user bin directory"
        warn "Falling back to PATH-only installation"
        PATH_ONLY=true
        INSTALL_DIR="$(borgclaw_target_dir)"
    }
fi

# Detect shell profile
 detect_shell_profile() {
    case "$(basename "$SHELL")" in
        bash)
            if [ -f "$HOME/.bashrc" ]; then
                printf '%s\n' "$HOME/.bashrc"
            elif [ -f "$HOME/.bash_profile" ]; then
                printf '%s\n' "$HOME/.bash_profile"
            fi
            ;;
        zsh)
            if [ -f "$HOME/.zshrc" ]; then
                printf '%s\n' "$HOME/.zshrc"
            fi
            ;;
        fish)
            if [ -d "$HOME/.config/fish" ]; then
                printf '%s\n' "$HOME/.config/fish/config.fish"
            fi
            ;;
    esac
}

SHELL_PROFILE="$(detect_shell_profile)"

# ============================================================================
# SHOW INSTALLATION PLAN
# ============================================================================

echo ""
section "Installation Plan:"
echo ""

if [ "$PATH_ONLY" = true ]; then
    info "  Method: Add to PATH only (no copy)"
    info "  Directory to add: $INSTALL_DIR/release"
else
    info "  Method: Copy to user bin directory"
    info "  Target directory: $INSTALL_DIR"
    info "  Binaries to install:"
    info "    - borgclaw"
    [ -n "$GATEWAY_BIN" ] && info "    - borgclaw-gateway"
fi

if [ -n "$SHELL_PROFILE" ]; then
    info "  Shell profile: $SHELL_PROFILE"
else
    warn "  Shell profile: Not detected (manual PATH configuration required)"
fi

echo ""

# Check if already installed
if [ "$PATH_ONLY" = false ] && [ -f "$INSTALL_DIR/borgclaw" ] && [ "$FORCE" = false ]; then
    warn "BorgClaw is already installed at: $INSTALL_DIR/borgclaw"
    warn "Use --force to overwrite, or --path to only configure PATH"
    echo ""
    exit 0
fi

prompt "Continue with installation? [Y/n]: "
read -r confirm
if [[ "$confirm" =~ ^[Nn]$ ]]; then
    log "Installation cancelled"
    exit 0
fi

# ============================================================================
# PERFORM INSTALLATION
# ============================================================================

echo ""
log "Installing..."

if [ "$PATH_ONLY" = true ]; then
    # PATH-only installation
    log "Adding target directory to PATH..."
    
    if borgclaw_in_path "$INSTALL_DIR/release"; then
        log "Target directory is already in PATH"
    elif [ -n "$SHELL_PROFILE" ]; then
        echo "" >> "$SHELL_PROFILE"
        echo "# BorgClaw (added by install.sh on $(date +%Y-%m-%d))" >> "$SHELL_PROFILE"
        echo "export PATH=\"$INSTALL_DIR/release:\$PATH\"" >> "$SHELL_PROFILE"
        log "✓ Added to PATH in $SHELL_PROFILE"
        warn "Please run: source $SHELL_PROFILE"
        warn "Or restart your terminal for changes to take effect"
    else
        error "Could not detect shell profile"
        info "Please manually add this to your shell profile:"
        info "  export PATH=\"$INSTALL_DIR/release:\$PATH\""
    fi
else
    # Copy binaries
    log "Copying binaries to $INSTALL_DIR..."
    
    mkdir -p "$INSTALL_DIR"
    
    # Copy borgclaw
    cp "$BORGCLAW_BIN" "$INSTALL_DIR/borgclaw"
    chmod +x "$INSTALL_DIR/borgclaw"
    log "✓ Installed: $INSTALL_DIR/borgclaw"
    
    # Copy gateway if available
    if [ -n "$GATEWAY_BIN" ]; then
        cp "$GATEWAY_BIN" "$INSTALL_DIR/borgclaw-gateway"
        chmod +x "$INSTALL_DIR/borgclaw-gateway"
        log "✓ Installed: $INSTALL_DIR/borgclaw-gateway"
    fi
    
    # Check if install dir is in PATH
    if ! borgclaw_in_path "$INSTALL_DIR"; then
        warn "Installation directory is not in your PATH"
        
        if [ -n "$SHELL_PROFILE" ]; then
            echo "" >> "$SHELL_PROFILE"
            echo "# BorgClaw (added by install.sh on $(date +%Y-%m-%d))" >> "$SHELL_PROFILE"
            echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$SHELL_PROFILE"
            log "✓ Added $INSTALL_DIR to PATH in $SHELL_PROFILE"
            warn "Please run: source $SHELL_PROFILE"
            warn "Or restart your terminal for changes to take effect"
        else
            info "Please manually add this to your shell profile:"
            info "  export PATH=\"$INSTALL_DIR:\$PATH\""
        fi
    fi
fi

# ============================================================================
# COMPLETION
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "           ✅ Installation Complete"
section "═══════════════════════════════════════════════════════════════"
echo ""

if [ "$PATH_ONLY" = true ]; then
    info "BorgClaw will be accessible via:"
    info "  $INSTALL_DIR/release/borgclaw"
else
    info "BorgClaw is now installed at:"
    info "  $INSTALL_DIR/borgclaw"
    
    if borgclaw_in_path "$INSTALL_DIR"; then
        log "✓ Already in PATH - you can use 'borgclaw' directly"
    fi
fi

echo ""
info "Next steps:"
info "  1. Run onboarding (if not done yet): ./scripts/onboarding.sh"
info "  2. Start the gateway: borgclaw-gateway (or ./scripts/gateway.sh)"
info "  3. Open dashboard: http://localhost:3000"
echo ""

# Test installation
if [ "$PATH_ONLY" = false ] && borgclaw_in_path "$INSTALL_DIR"; then
    log "Testing installation..."
    if command -v borgclaw &>/dev/null; then
        borgclaw --version 2>/dev/null || true
        log "✓ Installation verified!"
    else
        warn "borgclaw not found in PATH after installation"
        warn "Please restart your terminal or run: source $SHELL_PROFILE"
    fi
fi

echo ""
