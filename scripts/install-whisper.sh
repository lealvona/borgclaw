#!/usr/bin/env bash
set -euo pipefail

# Fix libcurl version warnings by using system libraries
# (Some systems have conflicting libcurl in /usr/local/lib)
export LD_LIBRARY_PATH=""

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_DIR="${ROOT_DIR}/.local/tools"
mkdir -p "$TOOLS_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    echo -e "${GREEN}[borgclaw]${NC} $*"
}

warn() {
    echo -e "${YELLOW}[borgclaw]${NC} $*"
}

error() {
    echo -e "${RED}[borgclaw]${NC} $*"
}

log "Installing whisper.cpp..."

# Check prerequisites
if ! command -v git &> /dev/null; then
    error "✗ git is required but not installed"
    error "  Install git: https://git-scm.com/downloads"
    exit 1
fi

if ! command -v cmake &> /dev/null; then
    error "✗ cmake is required but not installed"
    error "  Install cmake:"
    error "    Ubuntu/Debian: sudo apt-get install cmake"
    error "    macOS: brew install cmake"
    error "    Fedora: sudo dnf install cmake"
    exit 1
fi

if ! command -v make &> /dev/null; then
    error "✗ make is required but not installed"
    error "  Install build-essential (Ubuntu/Debian) or Xcode Command Line Tools (macOS)"
    exit 1
fi

# Check available disk space (need at least 2GB)
AVAILABLE_KB=$(df -k "$TOOLS_DIR" | tail -1 | awk '{print $4}')
AVAILABLE_GB=$((AVAILABLE_KB / 1024 / 1024))
if [ "$AVAILABLE_GB" -lt 2 ]; then
    warn "⚠ Low disk space: ${AVAILABLE_GB}GB available (recommended: 2GB+)"
    read -p "Continue anyway? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

WHISPER_DIR="${TOOLS_DIR}/whisper.cpp"
WHISPER_REPO="https://github.com/ggerganov/whisper.cpp"

if [ -d "$WHISPER_DIR" ]; then
    log "whisper.cpp already exists at ${WHISPER_DIR}"
    log "Pulling latest changes..."
    cd "$WHISPER_DIR"
    if ! git pull; then
        warn "Failed to pull updates, continuing with existing version"
    fi
else
    log "Cloning whisper.cpp..."
    if ! git clone --depth 1 "$WHISPER_REPO" "$WHISPER_DIR"; then
        error "✗ Failed to clone whisper.cpp repository"
        error "  Check your internet connection and try again"
        exit 1
    fi
    cd "$WHISPER_DIR"
fi

log "Building whisper.cpp..."
cd "$WHISPER_DIR"

# Clean previous build if exists
if [ -d "build" ]; then
    log "Cleaning previous build..."
    rm -rf build
fi

# Build with optimizations
if ! cmake -B build -DCMAKE_BUILD_TYPE=Release; then
    error "✗ cmake configuration failed"
    exit 1
fi

# Get number of CPU cores for parallel build
NPROC=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
log "Building with ${NPROC} parallel jobs..."

if ! cmake --build build --config Release -j"${NPROC}"; then
    error "✗ Build failed"
    error "  Common causes:"
    error "    - Missing build tools (g++, clang)"
    error "    - Out of memory during compilation"
    error "    - Corrupted clone (try deleting ${WHISPER_DIR} and re-running)"
    exit 1
fi

mkdir -p "${WHISPER_DIR}/build/bin"
cp "${WHISPER_DIR}/build/bin/whisper-cli" "${WHISPER_DIR}/build/bin/main" 2>/dev/null || true

# Download base model
log "Downloading base model (base.en)..."
if [ ! -f "models/ggml-base.en.bin" ]; then
    if ! bash ./models/download-ggml-model.sh base.en; then
        warn "Failed to download model automatically"
        warn "You can download it manually later:"
        warn "  cd ${WHISPER_DIR} && bash ./models/download-ggml-model.sh base.en"
    fi
else
    log "Model already exists, skipping download"
fi

# Create symlink for easy access
mkdir -p "${ROOT_DIR}/.local/bin"
if [ -L "${ROOT_DIR}/.local/bin/whisper-cli" ]; then
    rm "${ROOT_DIR}/.local/bin/whisper-cli"
fi
ln -sf "${WHISPER_DIR}/build/bin/whisper-cli" "${ROOT_DIR}/.local/bin/whisper-cli"

# Verify installation
if [ -f "${WHISPER_DIR}/build/bin/whisper-cli" ]; then
    log "✓ whisper.cpp installed successfully"
    log "  Binary: ${WHISPER_DIR}/build/bin/whisper-cli"
    log "  Symlink: ${ROOT_DIR}/.local/bin/whisper-cli"
    if [ -f "models/ggml-base.en.bin" ]; then
        log "  Model: ${WHISPER_DIR}/models/ggml-base.en.bin"
    fi
    
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
            echo "# BorgClaw tools (added by install-whisper.sh)" >> "$RC_FILE"
            echo "$PATH_EXPORT" >> "$RC_FILE"
            log "✓ PATH added to ${RC_FILE}"
            warn "⚠ Run 'source ${RC_FILE}' or restart your shell to use 'whisper-cli' command"
        else
            log "PATH already configured in ${RC_FILE}"
        fi
    fi
else
    error "✗ Installation verification failed"
    exit 1
fi

log ""
log "For better quality, download larger models:"
log "  cd ${WHISPER_DIR} && bash ./models/download-ggml-model.sh small"
log "  cd ${WHISPER_DIR} && bash ./models/download-ggml-model.sh medium"
