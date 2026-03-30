#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/build-env.sh"
borgclaw_prepare_build_env

echo "🚀 Starting BorgClaw Gateway..."
echo ""
echo "  🌐 Web Dashboard: http://localhost:3000"
echo "  ⚙️  Config Editor: Press Ctrl+, in the browser"
echo "  📡 API Endpoints:  http://localhost:3000/api/"
echo "  🔗 WebSocket:      ws://localhost:3000/ws"
echo ""
echo "  Documentation: docs/gateway.md"
echo ""

cargo run --bin borgclaw-gateway
