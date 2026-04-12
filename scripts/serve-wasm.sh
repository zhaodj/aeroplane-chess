#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"
PORT="${1:-8000}"

if [ ! -f "${DIST_DIR}/index.html" ]; then
  echo "error: ${DIST_DIR}/index.html not found."
  echo "run: ./scripts/build-wasm.sh"
  exit 1
fi

echo "serving ${DIST_DIR} at http://127.0.0.1:${PORT}"
cd "${DIST_DIR}"
python3 -m http.server "${PORT}"
