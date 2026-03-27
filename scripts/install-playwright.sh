#!/usr/bin/env bash
# Install Playwright for browser automation
#
# DESCRIPTION:
#   Installs Playwright and Chromium browser for web automation capabilities.
#   Playwright enables BorgClaw to interact with web pages, take screenshots,
#   fill forms, and extract data programmatically.
#
# INSTALLATION:
#   ./scripts/install-playwright.sh
#
# POST-INSTALLATION:
#   Playwright is installed as a Node.js package in .local/tools/playwright/
#   The bridge script is copied to enable BorgClaw integration.
#
# USAGE IN BORGCLAW:
#   Configure browser skill in config.toml:
#     [skills.browser]
#     enabled = true
#     backend = "playwright"
#
#   Or use via BorgClaw tools:
#     - browser_navigate
#     - browser_screenshot
#     - browser_click
#     - browser_fill
#
# REQUIREMENTS:
#   - Node.js 18+ (install from https://nodejs.org)
#   - npm (comes with Node.js)
#   - Internet connection
#
# FILES CREATED:
#   - .local/tools/playwright/       - Playwright installation directory
#   - .local/tools/playwright/node_modules/
#   - .local/tools/playwright/playwright-bridge.js
#
# DOCUMENTATION:
#   - Playwright docs: https://playwright.dev/
#   - BorgClaw browser skill: docs/skills.md
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

log "Installing Playwright..."

if ! command -v node &> /dev/null; then
    error "✗ Node.js is required but not installed"
    error "  Install from: https://nodejs.org"
    error "  Or use: nvm install node"
    exit 1
fi

NODE_VERSION=$(node --version | cut -d'v' -f2 | cut -d'.' -f1)
if [ "$NODE_VERSION" -lt 18 ]; then
    warn "⚠ Node.js 18+ recommended (found: $(node --version))"
fi

PLAYWRIGHT_DIR="${TOOLS_DIR}/playwright"
mkdir -p "$PLAYWRIGHT_DIR"

cd "$PLAYWRIGHT_DIR"

if [ ! -f "package.json" ]; then
    log "Creating package.json..."
    cat > package.json << 'EOF'
{
  "name": "borgclaw-playwright",
  "version": "1.0.0",
  "private": true,
  "dependencies": {
    "playwright": "^1.40.0"
  }
}
EOF
fi

if [ ! -d "node_modules" ]; then
    log "Installing npm dependencies..."
    if ! npm install; then
        error "✗ npm install failed"
        exit 1
    fi
fi

log "Installing browser binaries (Chromium)..."
if ! npx playwright install chromium; then
    error "✗ Failed to install Chromium"
    exit 1
fi

# Copy bridge script if it exists
if [ -f "${ROOT_DIR}/scripts/playwright/playwright-bridge.js" ]; then
    cp "${ROOT_DIR}/scripts/playwright/playwright-bridge.js" "${PLAYWRIGHT_DIR}/"
    log "Bridge script copied"
fi

log ""
log "✓ Playwright installed successfully"
log ""
info "📁 Installation:"
log "  Directory: ${PLAYWRIGHT_DIR}"
log "  Chromium: $(npx playwright chromium --version 2>/dev/null || echo 'installed')"
if [ -f "${PLAYWRIGHT_DIR}/playwright-bridge.js" ]; then
    log "  Bridge: ${PLAYWRIGHT_DIR}/playwright-bridge.js"
fi

log ""
info "⚡ Usage:"
log "  In BorgClaw config (config.toml):"
log "    [skills.browser]"
log "    enabled = true"
log "    backend = \"playwright\""
log ""
log "  Or via BorgClaw tools:"
log "    - browser_navigate"
log "    - browser_screenshot"
log "    - browser_click"
log ""
info "📖 Documentation:"
log "  Playwright: https://playwright.dev/"
log "  BorgClaw skills: docs/skills.md"
