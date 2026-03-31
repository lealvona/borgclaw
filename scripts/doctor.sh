#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/build-env.sh"
source "$ROOT_DIR/scripts/lib/config.sh"
borgclaw_prepare_build_env

echo "[borgclaw] System Doctor"
echo "========================"
echo ""

check_command() {
    local cmd="$1"
    local desc="$2"
    local required="${3:-false}"
    
    if command -v "$cmd" &> /dev/null; then
        local version=""
        case "$cmd" in
            rustc) version="$(rustc --version 2>/dev/null || echo 'unknown')" ;;
            cargo) version="$(cargo --version 2>/dev/null || echo 'unknown')" ;;
            node) version="$(node --version 2>/dev/null || echo 'unknown')" ;;
            docker) version="$(docker --version 2>/dev/null || echo 'unknown')" ;;
            git) version="$(git --version 2>/dev/null || echo 'unknown')" ;;
            psql) version="$(psql --version 2>/dev/null || echo 'unknown')" ;;
            ollama) version="$(ollama --version 2>/dev/null || echo 'unknown')" ;;
            bw) version="$(bw --version 2>/dev/null | head -1 || echo 'unknown')" ;;
            op) version="$(op --version 2>/dev/null || echo 'unknown')" ;;
            signal-cli) version="$(signal-cli --version 2>/dev/null || echo 'unknown')" ;;
            *) version="installed" ;;
        esac
        echo -e "\033[0;32m✓\033[0m $desc: $version"
        return 0
    else
        if [ "$required" = "true" ]; then
            echo -e "\033[0;31m✗\033[0m $desc: NOT FOUND (required)"
            return 1
        else
            echo -e "\033[0;33m○\033[0m $desc: NOT FOUND (optional)"
            return 0
        fi
    fi
}

check_file() {
    local file="$1"
    local desc="$2"
    
    if [ -f "$file" ]; then
        echo -e "\033[0;32m✓\033[0m $desc: exists"
        return 0
    else
        echo -e "\033[0;33m○\033[0m $desc: missing"
        return 0
    fi
}

ERRORS=0

echo "=== Required Tools ==="
check_command rustc "Rust compiler" true || ERRORS=$((ERRORS + 1))
check_command cargo "Cargo build tool" true || ERRORS=$((ERRORS + 1))
check_command git "Git version control" true || ERRORS=$((ERRORS + 1))

echo ""
echo "=== Optional Tools ==="
check_command node "Node.js (for Playwright)"
check_command docker "Docker (for pgvector runtime and command sandbox)"
check_command psql "PostgreSQL client"
check_command ollama "Ollama (embeddings runtime)"
check_command signal-cli "Signal CLI (for Signal channel)"
check_command bw "Bitwarden CLI (vault)"
check_command op "1Password CLI (vault)"

echo ""
echo "=== Project Files ==="
check_file "Cargo.toml" "Workspace manifest"
check_file "borgclaw-core/Cargo.toml" "Core crate manifest"
check_file "borgclaw-cli/Cargo.toml" "CLI crate manifest"
check_file "borgclaw-gateway/Cargo.toml" "Gateway crate manifest"

# Runtime configuration
CONFIG_DIR="${HOME}/.config/borgclaw"
if [ -f "${CONFIG_DIR}/config.toml" ]; then
    echo -e "\033[0;32m✓\033[0m Runtime config: ${CONFIG_DIR}/config.toml"
else
    echo -e "\033[0;33m○\033[0m Runtime config: not configured (run ./scripts/onboarding.sh)"
fi

# Secrets encryption key (PR #196)
if [ -f "${CONFIG_DIR}/.secrets_key" ]; then
    echo -e "\033[0;32m✓\033[0m Secrets encryption key: ${CONFIG_DIR}/.secrets_key"
else
    echo -e "\033[0;33m○\033[0m Secrets encryption key: not initialized (will be created on first use)"
fi

# Workspace and identity
echo ""
echo "=== Workspace Infrastructure ==="
if [ -d ".borgclaw/workspace" ]; then
    echo -e "\033[0;32m✓\033[0m Workspace directory: .borgclaw/workspace"
else
    echo -e "\033[0;33m○\033[0m Workspace directory: not created (run ./scripts/bootstrap.sh)"
fi

if [ -d ".borgclaw/skills" ]; then
    echo -e "\033[0;32m✓\033[0m Skills directory: .borgclaw/skills"
else
    echo -e "\033[0;33m○\033[0m Skills directory: not created (run ./scripts/bootstrap.sh)"
fi

if [ -f ".borgclaw/soul.md" ]; then
    echo -e "\033[0;32m✓\033[0m Identity document: .borgclaw/soul.md"
else
    echo -e "\033[0;33m○\033[0m Identity document: not created (run ./scripts/bootstrap.sh)"
fi

echo ""
echo "=== Build Status ==="
borgclaw_print_build_env
if cargo check --quiet 2>/dev/null; then
    echo -e "\033[0;32m✓\033[0m Code compiles successfully"
else
    echo -e "\033[0;31m✗\033[0m Code has compilation errors"
    ERRORS=$((ERRORS + 1))
fi

echo ""
echo "=== Optional Components ==="
if [ -d ".local/tools/playwright" ]; then
    echo -e "\033[0;32m✓\033[0m Playwright: installed"
else
    echo -e "\033[0;33m○\033[0m Playwright: not installed (run ./scripts/install-playwright.sh)"
fi

if [ -d ".local/tools/whisper.cpp" ]; then
    echo -e "\033[0;32m✓\033[0m whisper.cpp: installed"
else
    echo -e "\033[0;33m○\033[0m whisper.cpp: not installed (run ./scripts/install-whisper.sh)"
fi

if command -v docker >/dev/null 2>&1; then
    echo -e "\033[0;32m✓\033[0m pgvector runtime installer available (run ./scripts/install-pgvector.sh)"
    if docker image inspect borgclaw-sandbox:base >/dev/null 2>&1; then
        echo -e "\033[0;32m✓\033[0m Docker sandbox image: borgclaw-sandbox:base"
    else
        echo -e "\033[0;33m○\033[0m Docker sandbox image: not built (run ./scripts/install-docker-sandbox.sh)"
    fi
else
    echo -e "\033[0;33m○\033[0m pgvector runtime not installable via helper script until Docker is available"
    echo -e "\033[0;33m○\033[0m Docker command sandbox not installable via helper script until Docker is available"
fi

if command -v ollama >/dev/null 2>&1; then
    echo -e "\033[0;32m✓\033[0m Ollama embeddings runtime: installed"
else
    echo -e "\033[0;33m○\033[0m Ollama embeddings runtime: not installed (run ./scripts/install-ollama.sh)"
fi
echo -e "\033[0;32m✓\033[0m Build cache cleanup helper: ./scripts/clean-build-cache.sh"

echo ""
echo "=== Available Commands ==="
echo "  borgclaw status          - Show system status"
echo "  borgclaw doctor          - Run diagnostics"
echo "  borgclaw self-test       - Run self-test (exits 1 on failure)"
echo "  borgclaw schedules list  - List scheduled tasks"
echo "  borgclaw heartbeat list  - List heartbeat tasks"
echo "  borgclaw secrets list    - List stored secrets"
echo "  borgclaw backup export   - Export runtime state"

echo ""
echo "=== Gateway Web Interface ==="
WS_PORT=$(borgclaw_ws_port)
echo "  Start:    ./scripts/gateway.sh"
echo "  URL:      http://localhost:${WS_PORT}"
echo "  Config:   Press Ctrl+, in browser for visual editor"

echo ""
echo "========================"
if [ $ERRORS -eq 0 ]; then
    echo -e "\033[0;32m✅ All checks passed!\033[0m"
else
    echo -e "\033[0;31m❌ $ERRORS error(s) found\033[0m"
    exit 1
fi
