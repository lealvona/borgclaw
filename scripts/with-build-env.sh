#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
source "$ROOT_DIR/scripts/lib/build-env.sh"
borgclaw_prepare_build_env

if [[ $# -eq 0 ]]; then
    echo "Usage: ./scripts/with-build-env.sh <command> [args...]"
    exit 1
fi

exec "$@"
