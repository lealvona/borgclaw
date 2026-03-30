#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

IMAGE_NAME="${1:-borgclaw-sandbox:base}"
DOCKERFILE_PATH="${DOCKERFILE_PATH:-docker/sandbox-base.Dockerfile}"

if ! command -v docker >/dev/null 2>&1; then
    echo "[borgclaw] docker is required to build the sandbox image"
    exit 1
fi

echo "[borgclaw] Building Docker sandbox image: ${IMAGE_NAME}"
docker build -t "${IMAGE_NAME}" -f "${DOCKERFILE_PATH}" .

echo "[borgclaw] Docker sandbox image ready: ${IMAGE_NAME}"
