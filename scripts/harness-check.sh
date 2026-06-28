#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-quick}"

cd "${ROOT_DIR}"

usage() {
  cat <<'USAGE'
usage: ./scripts/harness-check.sh [quick|standard|wasm|android|all]

quick     cargo fmt --check + cargo test --lib
standard  quick checks + cargo check --all-targets + cargo clippy --all-targets
wasm      wasm32 check + ./scripts/build-wasm.sh dev
android   ./scripts/build-android.sh debug
all       standard + wasm + android
USAGE
}

run() {
  echo
  echo "+ $*"
  "$@"
}

run_fmt() {
  run cargo fmt --all -- --check
}

run_tests() {
  run cargo test --lib
}

run_native_check() {
  run cargo check --all-targets
}

run_clippy() {
  run cargo clippy --all-targets
}

run_wasm() {
  run rustup target add wasm32-unknown-unknown
  echo
  echo '+ RUSTFLAGS=--cfg getrandom_backend="wasm_js" cargo check --target wasm32-unknown-unknown --lib'
  RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo check --target wasm32-unknown-unknown --lib
  run ./scripts/build-wasm.sh dev
}

run_android() {
  run ./scripts/build-android.sh debug
}

run_quick() {
  run_fmt
  run_tests
}

run_standard() {
  run_fmt
  run_native_check
  run_clippy
  run_tests
}

case "${MODE}" in
  quick)
    run_quick
    ;;
  standard)
    run_standard
    ;;
  wasm)
    run_wasm
    ;;
  android)
    run_android
    ;;
  all)
    run_standard
    run_wasm
    run_android
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage
    exit 1
    ;;
esac

