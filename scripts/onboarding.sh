#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

show_help() {
    cat << EOF
Usage: ./scripts/onboarding.sh [OPTIONS]

BorgClaw Onboarding Wizard

OPTIONS:
    --quick              Run minimal onboarding (skip integrations)
    --update             Reconfigure existing setup
    --features          Show available feature flags
    --enable <feature>  Enable a feature during onboarding
    --disable <feature> Disable a feature during onboarding
    -h, --help          Show this help message

FEATURES:
    github              GitHub integration
    google              Google Workspace integration
    browser             Browser automation (Playwright)
    stt                 Speech-to-text (whisper.cpp)
    tts                 Text-to-speech (ElevenLabs)
    image               Image generation (Stable Diffusion)
    url                 URL shortener
    telegram            Telegram channel
    signal              Signal channel
    webhook             Webhook channel

EXAMPLES:
    # Minimal onboarding (skip integrations)
    ./scripts/onboarding.sh --quick

    # Enable only Telegram and GitHub
    ./scripts/onboarding.sh --disable google --disable browser

    # Reconfigure existing setup
    ./scripts/onboarding.sh --update

    # Show available features
    ./scripts/onboarding.sh --features
EOF
}

show_features() {
    cat << EOF
Available Features:
  github              GitHub integration
  google              Google Workspace integration
  browser             Browser automation (Playwright)
  stt                 Speech-to-text (whisper.cpp)
  tts                 Text-to-speech (ElevenLabs)
  image               Image generation (Stable Diffusion)
  url                 URL shortener
  telegram            Telegram channel
  signal              Signal channel
  webhook             Webhook channel
  websocket           WebSocket channel (default: enabled)
EOF
}

QUICK_MODE=false
UPDATE_MODE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --quick)
            QUICK_MODE=true
            shift
            ;;
        --update)
            UPDATE_MODE=true
            shift
            ;;
        --features)
            show_features
            exit 0
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

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

if [ "$QUICK_MODE" = true ]; then
    echo "[borgclaw] Running in QUICK mode (minimal configuration)"
    cargo run --bin borgclaw -- init --quick
else
    cargo run --bin borgclaw -- init
fi

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
