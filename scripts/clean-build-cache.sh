#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/build-env.sh"
borgclaw_prepare_build_env

FULL_CLEAN=false
if [[ "${1:-}" == "--all" ]]; then
    FULL_CLEAN=true
fi

target_dir="$(borgclaw_target_dir)"

echo "[borgclaw] Cleaning build scratch..."
rm -rf "$target_dir/debug/incremental" "$target_dir/release/incremental"
borgclaw_prune_tmp_dir "$ROOT_DIR/.tmp"
borgclaw_prune_tmp_dir "$BORGCLAW_TMP_ROOT"

if [[ "$FULL_CLEAN" == true ]]; then
    echo "[borgclaw] Running cargo clean..."
    cargo clean
fi

echo "[borgclaw] Remaining build directories:"
du -sh "$target_dir" "$BORGCLAW_TMP_ROOT" 2>/dev/null || true
