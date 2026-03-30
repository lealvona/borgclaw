#!/usr/bin/env bash

if [[ -n "${BORGCLAW_BUILD_ENV_SH_LOADED:-}" ]]; then
    return 0 2>/dev/null || exit 0
fi
readonly BORGCLAW_BUILD_ENV_SH_LOADED=1

if [[ -z "${ROOT_DIR:-}" ]]; then
    ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fi

readonly BORGCLAW_CACHE_ROOT_DEFAULT="${ROOT_DIR}/.local/cache"
readonly BORGCLAW_TMP_ROOT_DEFAULT="${BORGCLAW_CACHE_ROOT_DEFAULT}/tmp"
readonly BORGCLAW_TARGET_DIR_DEFAULT="${ROOT_DIR}/target"
readonly BORGCLAW_TARGET_SOFT_LIMIT_GB_DEFAULT=12
readonly BORGCLAW_TMP_PRUNE_DAYS_DEFAULT=7

borgclaw_prepare_build_env() {
    export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$BORGCLAW_TARGET_DIR_DEFAULT}"
    export BORGCLAW_CACHE_ROOT="${BORGCLAW_CACHE_ROOT:-$BORGCLAW_CACHE_ROOT_DEFAULT}"
    export BORGCLAW_TMP_ROOT="${BORGCLAW_TMP_ROOT:-$BORGCLAW_TMP_ROOT_DEFAULT}"
    export TMPDIR="${TMPDIR:-$BORGCLAW_TMP_ROOT}"
    export TMP="${TMP:-$TMPDIR}"
    export TEMP="${TEMP:-$TMPDIR}"
    export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
    export BORGCLAW_TARGET_SOFT_LIMIT_GB="${BORGCLAW_TARGET_SOFT_LIMIT_GB:-$BORGCLAW_TARGET_SOFT_LIMIT_GB_DEFAULT}"
    export BORGCLAW_TMP_PRUNE_DAYS="${BORGCLAW_TMP_PRUNE_DAYS:-$BORGCLAW_TMP_PRUNE_DAYS_DEFAULT}"

    mkdir -p "$BORGCLAW_CACHE_ROOT" "$BORGCLAW_TMP_ROOT" "$CARGO_TARGET_DIR"

    borgclaw_prune_tmp_dir "$ROOT_DIR/.tmp"
    borgclaw_prune_tmp_dir "$BORGCLAW_TMP_ROOT"
    borgclaw_prune_incremental_if_oversized
}

borgclaw_target_dir() {
    printf '%s\n' "${CARGO_TARGET_DIR:-$BORGCLAW_TARGET_DIR_DEFAULT}"
}

borgclaw_prune_tmp_dir() {
    local dir="$1"

    [[ -d "$dir" ]] || return 0

    find "$dir" -mindepth 1 -maxdepth 1 -mtime +"${BORGCLAW_TMP_PRUNE_DAYS}" \
        \( -type d -o -type f \) \
        \( -name 'borgclaw_*' -o -name 'cargo-*' -o -name 'rustc*' \) \
        -exec rm -rf {} + 2>/dev/null || true
}

borgclaw_target_size_kb() {
    local target_dir
    target_dir="$(borgclaw_target_dir)"
    du -sk "$target_dir" 2>/dev/null | awk '{print $1}'
}

borgclaw_prune_incremental_if_oversized() {
    local target_dir size_kb soft_limit_kb

    target_dir="$(borgclaw_target_dir)"
    [[ -d "$target_dir" ]] || return 0

    size_kb="$(borgclaw_target_size_kb)"
    [[ -n "$size_kb" ]] || return 0

    soft_limit_kb=$((BORGCLAW_TARGET_SOFT_LIMIT_GB * 1024 * 1024))
    if (( size_kb <= soft_limit_kb )); then
        return 0
    fi

    rm -rf "$target_dir/debug/incremental" "$target_dir/release/incremental"
}

borgclaw_print_build_env() {
    local target_dir
    target_dir="$(borgclaw_target_dir)"

    echo "[borgclaw] Build hygiene:"
    echo "  target: ${target_dir}"
    echo "  temp:   ${TMPDIR:-$BORGCLAW_TMP_ROOT_DEFAULT}"
    echo "  incr:   ${CARGO_INCREMENTAL:-0}"
}
