#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ANDROID_DIR="${ROOT_DIR}/platforms/android"
APP_DIR="${ANDROID_DIR}/app"
PROFILE="${1:-debug}"
ABI="${ANDROID_ABI:-arm64-v8a}"

case "${ABI}" in
  arm64-v8a)
    RUST_TARGET="aarch64-linux-android"
    ;;
  *)
    echo "error: unsupported ANDROID_ABI=${ABI}; currently supported: arm64-v8a"
    exit 1
    ;;
esac

case "${PROFILE}" in
  debug)
    CARGO_PROFILE_ARGS=()
    GRADLE_TASK="assembleDebug"
    ;;
  release)
    CARGO_PROFILE_ARGS=(--release)
    GRADLE_TASK="assembleRelease"
    ;;
  *)
    echo "usage: $0 [debug|release]"
    exit 1
    ;;
esac

MISSING=0

if ! command -v cargo-ndk >/dev/null 2>&1; then
  echo "error: cargo-ndk not found."
  echo "install: cargo install cargo-ndk"
  MISSING=1
fi

if [ -z "${ANDROID_SDK_ROOT:-}" ] || [ ! -d "${ANDROID_SDK_ROOT:-}" ]; then
  echo "error: ANDROID_SDK_ROOT is not set to a valid Android SDK directory."
  MISSING=1
fi

if [ -z "${ANDROID_NDK_ROOT:-}" ] || [ ! -d "${ANDROID_NDK_ROOT:-}" ]; then
  echo "error: ANDROID_NDK_ROOT is not set to a valid Android NDK directory."
  MISSING=1
fi

if [ ! -x "${ANDROID_DIR}/gradlew" ] && ! command -v gradle >/dev/null 2>&1; then
  echo "error: Gradle not found. Install Android Studio/Gradle or add a Gradle wrapper in ${ANDROID_DIR}."
  MISSING=1
fi

if [ "${MISSING}" -ne 0 ]; then
  exit 1
fi

rustup target add "${RUST_TARGET}" >/dev/null
mkdir -p "${APP_DIR}/src/main/jniLibs"

echo "[1/2] building Rust shared library for ${ABI}..."
cargo ndk \
  -t "${ABI}" \
  -o "${APP_DIR}/src/main/jniLibs" \
  build \
  --lib \
  "${CARGO_PROFILE_ARGS[@]}"

pushd "${ANDROID_DIR}" >/dev/null
if [ -x "./gradlew" ]; then
  GRADLE_CMD="./gradlew"
else
  GRADLE_CMD="gradle"
fi

echo "[2/2] assembling Android APK (${PROFILE})..."
"${GRADLE_CMD}" "${GRADLE_TASK}"
popd >/dev/null

echo "done: ${ANDROID_DIR}/app/build/outputs/apk/${PROFILE}/"
