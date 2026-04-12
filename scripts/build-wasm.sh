#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${ROOT_DIR}/target/wasm32-unknown-unknown/release"
DIST_DIR="${ROOT_DIR}/dist"
WEB_DIR="${ROOT_DIR}/web"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "error: wasm-bindgen CLI not found."
  echo "install: cargo install wasm-bindgen-cli"
  exit 1
fi

rustup target add wasm32-unknown-unknown >/dev/null

echo "[1/3] building wasm binary..."
RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo build --release --target wasm32-unknown-unknown

WASM_FILE=""
for candidate in "${TARGET_DIR}/aeroplane-chess.wasm" "${TARGET_DIR}/aeroplane_chess.wasm"; do
  if [ -f "${candidate}" ]; then
    WASM_FILE="${candidate}"
    break
  fi
done
if [ -z "${WASM_FILE}" ]; then
  WASM_FILE="$(find "${TARGET_DIR}" -maxdepth 1 -type f -name "*.wasm" | head -n 1 || true)"
fi
if [ -z "${WASM_FILE}" ] || [ ! -f "${WASM_FILE}" ]; then
  echo "error: wasm output not found in ${TARGET_DIR}"
  exit 1
fi

echo "[2/3] running wasm-bindgen..."
mkdir -p "${DIST_DIR}"
wasm-bindgen \
  --target web \
  --no-typescript \
  --out-dir "${DIST_DIR}" \
  --out-name aeroplane_chess \
  "${WASM_FILE}"

echo "[3/3] copying web shell..."
cp "${WEB_DIR}/index.html" "${DIST_DIR}/index.html"

echo "done: ${DIST_DIR}"
