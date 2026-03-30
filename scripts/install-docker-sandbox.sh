#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BASE_IMAGE_NAME="${1:-borgclaw-sandbox:base}"
REMOTE_IMAGE_NAME="${2:-borgclaw-sandbox:remote}"

if ! command -v docker >/dev/null 2>&1; then
    echo "[borgclaw] docker is required to build the sandbox image"
    exit 1
fi

echo "[borgclaw] Building Docker sandbox base image: ${BASE_IMAGE_NAME}"
docker build -t "${BASE_IMAGE_NAME}" -f docker/sandbox-base.Dockerfile .

echo "[borgclaw] Building Docker sandbox remote image: ${REMOTE_IMAGE_NAME}"
docker build -t "${REMOTE_IMAGE_NAME}" -f docker/sandbox-remote.Dockerfile .

echo "[borgclaw] Docker sandbox images ready: ${BASE_IMAGE_NAME}, ${REMOTE_IMAGE_NAME}"
