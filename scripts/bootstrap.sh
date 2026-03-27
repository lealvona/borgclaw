#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

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
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

check_command() {
    if command -v "$1" &> /dev/null; then
        echo -e "\033[0;32m✓\033[0m $1: $(command -v "$1")"
        return 0
    else
        echo -e "\033[0;31m✗\033[0m $1: NOT FOUND"
        return 1
    fi
}

echo "[borgclaw] Checking prerequisites..."
MISSING=0

check_command rustc || MISSING=1
check_command cargo || MISSING=1
check_command git || MISSING=1

if [ $MISSING -eq 1 ]; then
    echo ""
    echo "[borgclaw] ERROR: Missing required tools."
    echo "[borgclaw] Install Rust from: https://rustup.rs"
    exit 1
fi

echo ""
echo "[borgclaw] Rust versions:"
echo "  rustc: $(rustc --version)"
echo "  cargo: $(cargo --version)"

echo ""
echo "[borgclaw] Optional tools:"
check_command node || true
check_command signal-cli || true
check_command bw || true
check_command op || true

echo ""
echo "[borgclaw] Checking for previous builds..."
if [ -d "target" ]; then
    echo ""
    echo "WARNING: Found existing target/ directory with previous build artifacts."
    echo "         This can cause issues with stale dependencies."
    echo ""
    read -p "Delete target/ directory and all build artifacts? [y/N] " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "  Removing target/ directory..."
        rm -rf target
        echo "  ✓ Clean complete"
    else
        echo "  Skipping cleanup (may use stale artifacts)"
    fi
else
    echo "  ✓ No previous builds found"
fi

echo ""
echo "[borgclaw] Building workspace..."
cargo build --release

echo ""
echo "[borgclaw] Creating .local directory structure..."
mkdir -p .local/tools
mkdir -p .local/data
mkdir -p .local/cache

if [ ! -f ".gitignore" ] || ! grep -q "^\.local" .gitignore; then
    echo ".local/" >> .gitignore
fi

echo ""
echo "[borgclaw] Checking optional components..."

# Check for Playwright
if [ -f ".local/tools/playwright/playwright-bridge.js" ] && [ -d ".local/tools/playwright/node_modules" ]; then
    echo ""
    echo -e "\033[0;32m✓\033[0m Playwright: Installed (.local/tools/playwright)"
else
    if command -v node &> /dev/null; then
        echo ""
        echo "[borgclaw] Node.js detected. Install Playwright?"
        echo "  ./scripts/install-playwright.sh"
    fi
fi

echo ""
echo "[borgclaw] ✅ Bootstrap complete!"
echo ""
echo "[borgclaw] Next steps:"
echo "  1. Run onboarding:    ./scripts/onboarding.sh"
echo "  2. Check system:      ./scripts/doctor.sh"
echo "  3. Start REPL:        ./scripts/repl.sh"
echo "  4. Start Gateway:     ./scripts/gateway.sh"
echo ""
