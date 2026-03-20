#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

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
            git) version="$(git --version 2>/dev/null || echo 'unknown')" ;;
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
check_command signal-cli "Signal CLI (for Signal channel)"
check_command bw "Bitwarden CLI (vault)"
check_command op "1Password CLI (vault)"

echo ""
echo "=== Project Files ==="
check_file "Cargo.toml" "Workspace manifest"
check_file "borgclaw-core/Cargo.toml" "Core crate manifest"
check_file "borgclaw-cli/Cargo.toml" "CLI crate manifest"
check_file "borgclaw-gateway/Cargo.toml" "Gateway crate manifest"
echo -e "\033[0;33m○\033[0m Runtime config: user-specific (created at ~/.config/borgclaw/config.toml)"

echo ""
echo "=== Build Status ==="
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

echo ""
echo "========================"
if [ $ERRORS -eq 0 ]; then
    echo -e "\033[0;32m✅ All checks passed!\033[0m"
else
    echo -e "\033[0;31m❌ $ERRORS error(s) found\033[0m"
    exit 1
fi
