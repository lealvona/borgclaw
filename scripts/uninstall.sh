#!/usr/bin/env bash
#
# BorgClaw User Uninstall Script
# ================================
#
# PURPOSE:
#   Removes BorgClaw binaries from the user-level install directory and
#   removes the PATH entry added by install.sh from shell profiles.
#   Does NOT remove configuration (~/.config/borgclaw/) or runtime state
#   (.borgclaw/) — those are user data.
#
# USAGE:
#   ./scripts/uninstall.sh              # Interactive uninstall
#   ./scripts/uninstall.sh --force      # Skip confirmation prompt
#   ./scripts/uninstall.sh --purge      # Also remove config and state dirs
#

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log()     { echo -e "${GREEN}[uninstall]${NC} $*"; }
warn()    { echo -e "${YELLOW}[uninstall]${NC} $*"; }
error()   { echo -e "${RED}[uninstall]${NC} $*"; }
info()    { echo -e "${BLUE}[uninstall]${NC} $*"; }
section() { echo -e "${CYAN}$*${NC}"; }

# Parse arguments
FORCE=false
PURGE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --force)
            FORCE=true
            shift
            ;;
        --purge)
            PURGE=true
            shift
            ;;
        -h|--help)
            cat << 'EOF'
Usage: ./scripts/uninstall.sh [OPTIONS]

Remove BorgClaw binaries and PATH configuration.

OPTIONS:
    --force     Skip confirmation prompt
    --purge     Also remove ~/.config/borgclaw/ and .borgclaw/ state
    -h, --help  Show this help message

NOTE:
    Configuration (~/.config/borgclaw/) and runtime state (.borgclaw/) are
    preserved by default. Use --purge to remove them as well.

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
section "           BorgClaw Uninstall"
section "═══════════════════════════════════════════════════════════════"
echo ""

# ============================================================================
# LOCATE INSTALLED BINARIES
# ============================================================================

INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/borgclaw"
STATE_DIR="$ROOT_DIR/.borgclaw"

FOUND_BINS=()
for bin in borgclaw borgclaw-gateway; do
    if [ -f "$INSTALL_DIR/$bin" ]; then
        FOUND_BINS+=("$INSTALL_DIR/$bin")
    fi
done

# Detect shell profiles to clean PATH entries from
detect_shell_profiles() {
    local profiles=()
    [ -f "$HOME/.bashrc" ]             && profiles+=("$HOME/.bashrc")
    [ -f "$HOME/.bash_profile" ]       && profiles+=("$HOME/.bash_profile")
    [ -f "$HOME/.zshrc" ]              && profiles+=("$HOME/.zshrc")
    [ -f "$HOME/.config/fish/config.fish" ] && profiles+=("$HOME/.config/fish/config.fish")
    printf '%s\n' "${profiles[@]}"
}

mapfile -t SHELL_PROFILES < <(detect_shell_profiles)

# ============================================================================
# SHOW UNINSTALL PLAN
# ============================================================================

echo ""
section "Uninstall Plan:"
echo ""

if [ ${#FOUND_BINS[@]} -eq 0 ]; then
    info "  No BorgClaw binaries found in $INSTALL_DIR"
else
    info "  Binaries to remove:"
    for bin in "${FOUND_BINS[@]}"; do
        info "    - $bin"
    done
fi

if [ ${#SHELL_PROFILES[@]} -gt 0 ]; then
    info "  Shell profiles to clean (remove borgclaw PATH entries):"
    for profile in "${SHELL_PROFILES[@]}"; do
        if grep -q "borgclaw" "$profile" 2>/dev/null; then
            info "    - $profile"
        fi
    done
fi

if [ "$PURGE" = true ]; then
    warn "  --purge: also removing configuration and state:"
    [ -d "$CONFIG_DIR" ] && warn "    - $CONFIG_DIR"
    [ -d "$STATE_DIR" ]  && warn "    - $STATE_DIR"
fi

echo ""

if [ ${#FOUND_BINS[@]} -eq 0 ] && [ "$PURGE" = false ]; then
    log "Nothing to uninstall."
    exit 0
fi

# ============================================================================
# CONFIRM
# ============================================================================

if [ "$FORCE" = false ]; then
    read -rp "Proceed with uninstall? [y/N] " confirm
    if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
        log "Uninstall cancelled."
        exit 0
    fi
fi

echo ""
log "Uninstalling..."

# ============================================================================
# REMOVE BINARIES
# ============================================================================

for bin in "${FOUND_BINS[@]}"; do
    rm -f "$bin"
    log "✓ Removed: $bin"
done

# ============================================================================
# CLEAN SHELL PROFILES
# ============================================================================

for profile in "${SHELL_PROFILES[@]}"; do
    if grep -q "borgclaw" "$profile" 2>/dev/null; then
        # Remove lines added by install.sh: the comment and the export PATH line
        sed -i '/# BorgClaw (added by install\.sh/d' "$profile" 2>/dev/null || true
        sed -i '/export PATH=.*borgclaw\|export PATH=.*\.local\/bin.*BorgClaw/d' "$profile" 2>/dev/null || true
        # Also clean up blank lines left behind (only trailing blank lines)
        log "✓ Cleaned PATH entries from $profile"
    fi
done

# ============================================================================
# PURGE (optional)
# ============================================================================

if [ "$PURGE" = true ]; then
    if [ -d "$CONFIG_DIR" ]; then
        rm -rf "$CONFIG_DIR"
        warn "✓ Removed config: $CONFIG_DIR"
    fi
    if [ -d "$STATE_DIR" ]; then
        rm -rf "$STATE_DIR"
        warn "✓ Removed state: $STATE_DIR"
    fi
fi

# ============================================================================
# COMPLETION
# ============================================================================

echo ""
section "═══════════════════════════════════════════════════════════════"
section "           ✅ Uninstall Complete"
section "═══════════════════════════════════════════════════════════════"
echo ""

if [ "$PURGE" = false ]; then
    info "Configuration and runtime state were preserved:"
    info "  Config: $CONFIG_DIR"
    info "  State:  $STATE_DIR"
    info ""
    info "To remove them as well, run: ./scripts/uninstall.sh --purge"
fi

info "Restart your terminal or run: source ~/.bashrc (or ~/.zshrc)"
echo ""
