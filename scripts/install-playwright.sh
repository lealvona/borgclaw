#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_DIR="${ROOT_DIR}/.local/tools"
mkdir -p "$TOOLS_DIR"

echo "[borgclaw] Installing Playwright..."

if ! command -v node &> /dev/null; then
    echo "[borgclaw] ERROR: Node.js is required. Install from https://nodejs.org"
    exit 1
fi

PLAYWRIGHT_DIR="${TOOLS_DIR}/playwright"
mkdir -p "$PLAYWRIGHT_DIR"

cd "$PLAYWRIGHT_DIR"

if [ ! -f "package.json" ]; then
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
    echo "[borgclaw] Installing npm dependencies..."
    npm install
fi

echo "[borgclaw] Installing browser binaries..."
npx playwright install chromium

cp "${ROOT_DIR}/scripts/playwright/playwright-bridge.js" "${PLAYWRIGHT_DIR}/"

echo "[borgclaw] Playwright installed to ${PLAYWRIGHT_DIR}"
echo "[borgclaw] Bridge: ${PLAYWRIGHT_DIR}/playwright-bridge.js"
