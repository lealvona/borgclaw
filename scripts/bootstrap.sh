#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[borgclaw] Checking Rust toolchain..."
cargo --version
rustc --version

echo "[borgclaw] Building workspace..."
cargo build

echo "[borgclaw] Initializing config if missing..."
cargo run --bin borgclaw -- init

echo "[borgclaw] Done. Start REPL with: ./scripts/repl.sh"
