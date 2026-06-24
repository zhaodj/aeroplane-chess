#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"
WEB_DIR="${ROOT_DIR}/web"
BUILD_MODE="${1:-release}"

case "${BUILD_MODE}" in
  dev)
    CARGO_PROFILE_DIR="debug"
    RUN_WASM_OPT=false
    EMIT_COMPRESSED=false
    ;;
  release)
    CARGO_PROFILE_DIR="wasm-release"
    RUN_WASM_OPT=true
    EMIT_COMPRESSED=true
    ;;
  *)
    echo "usage: $0 [dev|release]"
    exit 1
    ;;
esac

TARGET_DIR="${ROOT_DIR}/target/wasm32-unknown-unknown/${CARGO_PROFILE_DIR}"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "error: wasm-bindgen CLI not found."
  echo "install: cargo install wasm-bindgen-cli"
  exit 1
fi

if [ "${RUN_WASM_OPT}" = true ] && ! command -v wasm-opt >/dev/null 2>&1; then
  echo "error: wasm-opt not found."
  echo "install: brew install binaryen"
  exit 1
fi

rustup target add wasm32-unknown-unknown >/dev/null

echo "[1/5] building wasm binary (${BUILD_MODE})..."
cargo_build_args=(build --target wasm32-unknown-unknown --lib)
if [ "${BUILD_MODE}" = release ]; then
  cargo_build_args=(build --profile wasm-release --target wasm32-unknown-unknown --lib)
fi
RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo "${cargo_build_args[@]}"

WASM_FILE=""
for candidate in "${TARGET_DIR}/aeroplane_chess.wasm" "${TARGET_DIR}/aeroplane-chess.wasm"; do
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

echo "[2/5] running wasm-bindgen..."
mkdir -p "${DIST_DIR}"
bindgen_args=(
  --target web
  --no-typescript
  --out-dir "${DIST_DIR}"
  --out-name aeroplane_chess
)
if [ "${BUILD_MODE}" = release ]; then
  bindgen_args+=(--remove-name-section --remove-producers-section)
fi
wasm-bindgen \
  "${bindgen_args[@]}" \
  "${WASM_FILE}"

if [ "${RUN_WASM_OPT}" = true ]; then
  echo "[3/5] optimizing wasm..."
  wasm-opt -Oz \
    "${DIST_DIR}/aeroplane_chess_bg.wasm" \
    -o "${DIST_DIR}/aeroplane_chess_bg.wasm"
else
  echo "[3/5] skipping wasm-opt for dev build..."
fi

echo "[4/5] copying web shell..."
cp "${WEB_DIR}/index.html" "${DIST_DIR}/index.html"
find "${WEB_DIR}" -maxdepth 1 -type f \( -name "*.png" -o -name "*.ico" -o -name "*.webmanifest" \) -exec cp {} "${DIST_DIR}/" \;
rm -rf "${DIST_DIR}/assets"
cp -R "${ROOT_DIR}/assets" "${DIST_DIR}/assets"

rm -f "${DIST_DIR}/aeroplane_chess_bg.wasm.gz" "${DIST_DIR}/aeroplane_chess_bg.wasm.br"
if [ "${EMIT_COMPRESSED}" = true ]; then
  echo "[5/5] writing compressed wasm variants..."
  gzip -9 -c "${DIST_DIR}/aeroplane_chess_bg.wasm" > "${DIST_DIR}/aeroplane_chess_bg.wasm.gz"
  if command -v brotli >/dev/null 2>&1; then
    brotli -f -q 11 -o "${DIST_DIR}/aeroplane_chess_bg.wasm.br" "${DIST_DIR}/aeroplane_chess_bg.wasm"
  else
    echo "warning: brotli not found; skipped .br output"
  fi
else
  echo "[5/5] skipping compressed variants for dev build..."
fi

echo "wasm size:"
ls -lh "${DIST_DIR}/aeroplane_chess_bg.wasm" \
  "${DIST_DIR}/aeroplane_chess_bg.wasm.gz" \
  "${DIST_DIR}/aeroplane_chess_bg.wasm.br" 2>/dev/null || true
echo "done: ${DIST_DIR}"
