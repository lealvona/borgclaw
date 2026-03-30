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
check_command docker || true
check_command psql || true
check_command ollama || true
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
echo "[borgclaw] Checking memory runtimes..."
if command -v docker &> /dev/null; then
    echo "  PostgreSQL + pgvector runtime: ./scripts/install-pgvector.sh"
else
    echo "  Docker not found; pgvector convenience runtime installer requires Docker"
fi

if command -v ollama &> /dev/null; then
    echo -e "\033[0;32m✓\033[0m Ollama: Installed"
else
    echo "  Embeddings runtime (recommended for hybrid search): ./scripts/install-ollama.sh"
fi

# Check for secrets/encryption setup
echo ""
echo "[borgclaw] Checking secret store configuration..."

# Check if .env exists with secrets that should be migrated
if [ -f ".env" ]; then
    ENV_SECRETS=$(grep -E '(_API_KEY|_TOKEN|_SECRET|_PASSWORD|CLIENT_ID|CLIENT_SECRET)=' .env 2>/dev/null | wc -l)
    if [ "$ENV_SECRETS" -gt 0 ]; then
        echo ""
        echo -e "\033[0;33m⚠ Found $ENV_SECRETS secret(s) in .env file\033[0m"
        echo "[borgclaw] Secrets should be stored in the encrypted vault for security."
        echo ""
        read -p "Migrate .env secrets to encrypted store now? [y/N] " -n 1 -r
        echo ""
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            ./scripts/migrate-env-to-secrets.sh
        else
            echo "  You can migrate later with: ./scripts/migrate-env-to-secrets.sh"
        fi
    fi
fi

# Check if encryption key exists
BORGCLAW_WORKSPACE="${HOME}/.borgclaw"
SECRETS_FILE="${BORGCLAW_WORKSPACE}/secrets.enc"
KEY_FILE="${BORGCLAW_WORKSPACE}/secrets.enc.key"

if [ -f "$SECRETS_FILE" ] && [ -f "$KEY_FILE" ]; then
    echo -e "\033[0;32m✓\033[0m Encrypted secret store: Configured"
    echo "  Location: $SECRETS_FILE"
    echo "  Key file: $KEY_FILE"
elif [ -f "$SECRETS_FILE" ] && [ ! -f "$KEY_FILE" ]; then
    echo ""
    echo -e "\033[0;31m✗\033[0m Warning: Secrets file exists but encryption key is missing!"
    echo "  Secrets: $SECRETS_FILE"
    echo "  Key: $KEY_FILE (NOT FOUND)"
    echo ""
    echo "[borgclaw] Run onboarding to regenerate the encryption key."
else
    echo ""
    echo -e "\033[0;33m○\033[0m Encrypted secret store: Not initialized"
    echo "[borgclaw] Secret store will be created during onboarding."
fi

# Check for provider API key
if command -v ./target/release/borgclaw &> /dev/null || command -v ./target/debug/borgclaw &> /dev/null; then
    BORGCLAW_BIN="./target/release/borgclaw"
    [ ! -f "$BORGCLAW_BIN" ] && BORGCLAW_BIN="./target/debug/borgclaw"
    
    # Check if any provider key is configured
    if ! $BORGCLAW_BIN secrets list 2>/dev/null | grep -q "_API_KEY"; then
        echo ""
        echo -e "\033[0;33m○\033[0m No API keys configured"
        echo "[borgclaw] You'll need an API key to use LLM features."
        echo ""
        read -p "Set up provider API key now? [y/N] " -n 1 -r
        echo ""
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            ./scripts/setup-provider-key.sh
        else
            echo "  You can set up later with: ./scripts/setup-provider-key.sh"
        fi
    else
        echo -e "\033[0;32m✓\033[0m API key(s) configured"
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
