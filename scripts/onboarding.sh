#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║              🤖 BorgClaw Onboarding Wizard 🤖                   ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

CONFIG_FILE="config.toml"

check_state() {
    if [ -f "$CONFIG_FILE" ]; then
        if grep -q "api_key" "$CONFIG_FILE" 2>/dev/null; then
            return 0
        fi
    fi
    return 1
}

if check_state; then
    echo -e "\033[0;32m✓\033[0m Configuration found at $CONFIG_FILE"
    echo ""
    echo "Your BorgClaw is already configured!"
    echo ""
    echo "Options:"
    echo "  [r] Reconfigure  - Run setup again"
    echo "  [s] Status       - Show current config"
    echo "  [q] Quit         - Exit without changes"
    echo ""
    read -p "Choice [r/s/q]: " choice
    
    case "$choice" in
        r|R)
            echo ""
            echo "[borgclaw] Starting reconfiguration..."
            ;;
        s|S)
            echo ""
            echo "[borgclaw] Current configuration:"
            cat "$CONFIG_FILE" 2>/dev/null || echo "  (unable to read)"
            exit 0
            ;;
        q|Q|*)
            echo "[borgclaw] Exiting..."
            exit 0
            ;;
    esac
else
    echo -e "\033[0;33m○\033[0m No configuration found. Starting setup..."
fi

echo ""
echo "[borgclaw] Running configuration wizard..."
cargo run --bin borgclaw -- init

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    ✅ Setup Complete!                          ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Next steps:"
echo "  • Start REPL:     ./scripts/repl.sh"
echo "  • Start Gateway:  ./scripts/gateway.sh"
echo "  • Check system:   ./scripts/doctor.sh"
echo ""
