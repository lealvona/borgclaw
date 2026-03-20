#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_DIR="${ROOT_DIR}/.local/tools"
mkdir -p "$TOOLS_DIR"

echo "[borgclaw] Installing whisper.cpp..."

WHISPER_DIR="${TOOLS_DIR}/whisper.cpp"
WHISPER_REPO="https://github.com/ggerganov/whisper.cpp"

if [ -d "$WHISPER_DIR" ]; then
    echo "[borgclaw] whisper.cpp already exists at ${WHISPER_DIR}"
    echo "[borgclaw] Pulling latest changes..."
    cd "$WHISPER_DIR" && git pull
else
    echo "[borgclaw] Cloning whisper.cpp..."
    git clone --depth 1 "$WHISPER_REPO" "$WHISPER_DIR"
    cd "$WHISPER_DIR"
fi

echo "[borgclaw] Building whisper.cpp..."
cd "$WHISPER_DIR"
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release -j$(nproc)

mkdir -p "${WHISPER_DIR}/build/bin"
cp "${WHISPER_DIR}/build/bin/whisper-cli" "${WHISPER_DIR}/build/bin/main" 2>/dev/null || true

echo "[borgclaw] Downloading base model (base.en)..."
if [ ! -f "models/ggml-base.en.bin" ]; then
    bash ./models/download-ggml-model.sh base.en
fi

echo "[borgclaw] whisper.cpp installed to ${WHISPER_DIR}"
echo "[borgclaw] Binary: ${WHISPER_DIR}/build/bin/whisper-cli"
echo "[borgclaw] Model: ${WHISPER_DIR}/models/ggml-base.en.bin"
echo ""
echo "[borgclaw] For better quality, download larger models:"
echo "  cd ${WHISPER_DIR} && bash ./models/download-ggml-model.sh small"
echo "  cd ${WHISPER_DIR} && bash ./models/download-ggml-model.sh medium"
