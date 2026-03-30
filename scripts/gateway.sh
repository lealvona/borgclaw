#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/build-env.sh"
source "$ROOT_DIR/scripts/lib/config.sh"
borgclaw_prepare_build_env

# Get configured ports
WS_PORT=$(borgclaw_ws_port)
WEBHOOK_PORT=$(borgclaw_webhook_port)

echo "🚀 Starting BorgClaw Gateway..."
echo ""
echo "  🌐 Web Dashboard: http://localhost:${WS_PORT}"
echo "  ⚙️  Config Editor: Press Ctrl+, in the browser"
echo "  📡 API Endpoints:  http://localhost:${WS_PORT}/api/"
echo "  🔗 WebSocket:      ws://localhost:${WS_PORT}/ws"

# Show webhook info if different port
if [[ "$WEBHOOK_PORT" != "$WS_PORT" ]]; then
    echo "  📡 Webhook:        http://localhost:${WEBHOOK_PORT}/webhook"
fi

echo ""
echo "  Configuration: $(borgclaw_config_path)"
echo "  Documentation: docs/gateway.md"
echo ""

cargo run --bin borgclaw-gateway
