#!/usr/bin/env bash
# Install Bitwarden CLI (bw)
#
# DESCRIPTION:
#   Downloads and installs the official Bitwarden CLI tool for secure secret
#   management. The CLI enables integration with Bitwarden vaults for storing
#   and retrieving API keys, passwords, and other sensitive configuration.
#
# INSTALLATION:
#   ./scripts/install-bitwarden.sh
#
# POST-INSTALLATION SETUP:
#   1. Add to PATH:
#      export PATH="$PWD/.local/bin:$PATH"
#
#   2. Login to Bitwarden:
#      bw login                    # Interactive login
#      bw login username password  # Non-interactive
#
#   3. Unlock vault and set session:
#      export BW_SESSION="$(bw unlock --raw)"
#
#   4. Verify installation:
#      bw status                   # Should show "unlocked"
#
# USAGE IN BORGCLAW:
#   Configure BorgClaw to use Bitwarden vault in config.toml:
#     [security.vault]
#     provider = "bitwarden"
#
#   Or set environment variable:
#     export BW_SESSION="$(bw unlock --raw)"
#
# SUPPORTED PLATFORMS:
#   - Linux: x64, arm64
#   - macOS: x64, arm64 (Apple Silicon)
#
# REQUIREMENTS:
#   - curl or wget
#   - unzip
#   - Internet connection
#
# FILES CREATED:
#   - .local/tools/bitwarden/bw    - The Bitwarden CLI binary
#   - .local/bin/bw                - Symlink for PATH access
#
# DOCUMENTATION:
#   - Bitwarden CLI docs: https://bitwarden.com/help/cli/
#   - BorgClaw vault config: docs/security.md
#
# LICENSE: MIT

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_DIR="${ROOT_DIR}/.local/tools"
mkdir -p "$TOOLS_DIR"

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

log "Installing Bitwarden CLI (bw)..."

# Check if already installed
if command -v bw &> /dev/null; then
    BW_VERSION=$(bw --version 2>/dev/null || echo "unknown")
    log "Bitwarden CLI already installed: version $BW_VERSION"
    log "Location: $(which bw)"
    exit 0
fi

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    linux)
        PLATFORM="linux"
        ;;
    darwin)
        PLATFORM="macos"
        ;;
    *)
        error "Unsupported OS: $OS"
        error "Supported: Linux, macOS"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64)
        # x64 uses platform name only (no arch suffix)
        ASSET_SUFFIX=""
        ;;
    aarch64|arm64)
        # ARM64 uses -arm64 suffix
        ASSET_SUFFIX="-arm64"
        ;;
    *)
        error "Unsupported architecture: $ARCH"
        error "Supported: x86_64, arm64"
        exit 1
        ;;
esac

# Bitwarden CLI version - fetched from GitHub API for latest
BW_VERSION="2026.2.0"
BW_URL="https://github.com/bitwarden/clients/releases/download/cli-v${BW_VERSION}/bw-${PLATFORM}${ASSET_SUFFIX}-${BW_VERSION}.zip"
BW_ZIP="${TOOLS_DIR}/bw.zip"
BW_DIR="${TOOLS_DIR}/bitwarden"

log "Downloading Bitwarden CLI v${BW_VERSION} for ${PLATFORM}${ASSET_SUFFIX}..."

if command -v curl &> /dev/null; then
    if ! curl -fsSL "$BW_URL" -o "$BW_ZIP"; then
        error "Failed to download Bitwarden CLI"
        error "URL: $BW_URL"
        exit 1
    fi
elif command -v wget &> /dev/null; then
    if ! wget -q "$BW_URL" -O "$BW_ZIP"; then
        error "Failed to download Bitwarden CLI"
        error "URL: $BW_URL"
        exit 1
    fi
else
    error "curl or wget is required"
    exit 1
fi

log "Extracting..."
mkdir -p "$BW_DIR"
if command -v unzip &> /dev/null; then
    unzip -q "$BW_ZIP" -d "$BW_DIR"
else
    error "unzip is required"
    rm -f "$BW_ZIP"
    exit 1
fi

rm -f "$BW_ZIP"

# Make executable
chmod +x "${BW_DIR}/bw"

# Create symlink in .local/bin
mkdir -p "${ROOT_DIR}/.local/bin"
if [ -L "${ROOT_DIR}/.local/bin/bw" ]; then
    rm "${ROOT_DIR}/.local/bin/bw"
fi
ln -sf "${BW_DIR}/bw" "${ROOT_DIR}/.local/bin/bw"

# Verify installation
if "${BW_DIR}/bw" --version &> /dev/null; then
    INSTALLED_VERSION=$("${BW_DIR}/bw" --version)
    log "✓ Bitwarden CLI v${INSTALLED_VERSION} installed successfully"
    log ""
    info "📁 Installation paths:"
    log "  Binary: ${BW_DIR}/bw"
    log "  Symlink: ${ROOT_DIR}/.local/bin/bw"
    log ""
    
    # Auto-configure PATH in shell rc file
    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
        zsh)
            RC_FILE="$HOME/.zshrc"
            ;;
        bash)
            RC_FILE="$HOME/.bashrc"
            ;;
        *)
            RC_FILE="$HOME/.profile"
            ;;
    esac
    
    PATH_EXPORT="export PATH=\"${ROOT_DIR}/.local/bin:\$PATH\""
    
    if [ -f "$RC_FILE" ]; then
        if ! grep -qF "$PATH_EXPORT" "$RC_FILE" 2>/dev/null; then
            log "Adding PATH to ${RC_FILE}..."
            echo "" >> "$RC_FILE"
            echo "# BorgClaw tools (added by install-bitwarden.sh)" >> "$RC_FILE"
            echo "$PATH_EXPORT" >> "$RC_FILE"
            log "✓ PATH added to ${RC_FILE}"
            warn "⚠ Run 'source ${RC_FILE}' or restart your shell to use 'bw' command"
        else
            log "PATH already configured in ${RC_FILE}"
        fi
    else
        warn "Could not find ${RC_FILE}"
        log "Add this to your shell configuration:"
        log "  ${PATH_EXPORT}"
    fi
    
    log ""
    info "⚡ Quick start:"
    log "  1. Login to Bitwarden:"
    log "     bw login"
    log ""
    log "  2. Set session (required for vault access):"
    log "     export BW_SESSION=\"\$(bw unlock --raw)\""
    log ""
    log "  3. Verify:"
    log "     bw status"
    log ""
    info "📖 Documentation:"
    log "  Bitwarden CLI: https://bitwarden.com/help/cli/"
    log "  BorgClaw vault: docs/security.md"
else
    error "✗ Installation verification failed"
    exit 1
fi
