#!/usr/bin/env bash
# Install Bitwarden CLI (bw)
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_DIR="${ROOT_DIR}/.local/tools"
mkdir -p "$TOOLS_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log() { echo -e "${GREEN}[borgclaw]${NC} $*"; }
warn() { echo -e "${YELLOW}[borgclaw]${NC} $*"; }
error() { echo -e "${RED}[borgclaw]${NC} $*"; }

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
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64)
        ARCH="x64"
        ;;
    aarch64|arm64)
        ARCH="arm64"
        ;;
    *)
        error "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

BW_URL="https://github.com/bitwarden/clients/releases/download/cli-v2025.1.3/bw-${PLATFORM}-${ARCH}-2025.1.3.zip"
BW_ZIP="${TOOLS_DIR}/bw.zip"
BW_DIR="${TOOLS_DIR}/bitwarden"

log "Downloading Bitwarden CLI for ${PLATFORM}-${ARCH}..."

if command -v curl &> /dev/null; then
    curl -fsSL "$BW_URL" -o "$BW_ZIP"
elif command -v wget &> /dev/null; then
    wget -q "$BW_URL" -O "$BW_ZIP"
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
    exit 1
fi

rm "$BW_ZIP"

# Make executable
chmod +x "${BW_DIR}/bw"

# Create symlink in .local/bin
mkdir -p "${ROOT_DIR}/.local/bin"
ln -sf "${BW_DIR}/bw" "${ROOT_DIR}/.local/bin/bw"

# Verify installation
if "${BW_DIR}/bw" --version &> /dev/null; then
    BW_VERSION=$("${BW_DIR}/bw" --version)
    log "✓ Bitwarden CLI installed: version $BW_VERSION"
    log "  Location: ${BW_DIR}/bw"
    log "  Symlink: ${ROOT_DIR}/.local/bin/bw"
else
    error "✗ Installation failed"
    exit 1
fi

log ""
log "To use Bitwarden CLI:"
log "  1. Add to PATH: export PATH=\"${ROOT_DIR}/.local/bin:\$PATH\""
log "  2. Login: bw login"
log "  3. Set session: export BW_SESSION=\"\$(bw unlock --raw)\""
