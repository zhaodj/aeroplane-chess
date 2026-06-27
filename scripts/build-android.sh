#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ANDROID_DIR="${ROOT_DIR}/platforms/android"
APP_DIR="${ANDROID_DIR}/app"
PROFILE="${1:-debug}"
ABI="${ANDROID_ABI:-arm64-v8a}"
DEFAULT_ANDROID_SDK_ROOT="${HOME}/Library/Android/sdk"
DEFAULT_NDK_VERSION="26.1.10909125"
ANDROID_PLATFORM="${ANDROID_PLATFORM:-31}"
DEFAULT_JAVA_HOME="/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home"
HOMEBREW_GRADLE="/opt/homebrew/opt/gradle@8/bin/gradle"

if [ -z "${ANDROID_SDK_ROOT:-}" ] && [ -d "${DEFAULT_ANDROID_SDK_ROOT}" ]; then
  export ANDROID_SDK_ROOT="${DEFAULT_ANDROID_SDK_ROOT}"
fi

if [ -n "${ANDROID_SDK_ROOT:-}" ]; then
  export ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT}}"
fi

if [ -z "${ANDROID_NDK_ROOT:-}" ] && [ -d "${ANDROID_SDK_ROOT:-}/ndk/${DEFAULT_NDK_VERSION}" ]; then
  export ANDROID_NDK_ROOT="${ANDROID_SDK_ROOT}/ndk/${DEFAULT_NDK_VERSION}"
fi

if [ -n "${ANDROID_NDK_ROOT:-}" ]; then
  export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT}}"
  if [ -d "${ANDROID_NDK_ROOT}/toolchains/llvm/prebuilt/darwin-x86_64/bin" ]; then
    export PATH="${ANDROID_NDK_ROOT}/toolchains/llvm/prebuilt/darwin-x86_64/bin:${PATH}"
  fi
fi

if [ -d "${DEFAULT_JAVA_HOME}" ]; then
  export JAVA_HOME="${DEFAULT_JAVA_HOME}"
fi

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
    CARGO_BUILD_ARGS=(build --lib)
    GRADLE_TASK="assembleDebug"
    ;;
  release)
    CARGO_BUILD_ARGS=(build --lib --release)
    GRADLE_TASK="assembleRelease"
    ;;
  *)
    echo "usage: $0 [debug|release]"
    exit 1
    ;;
esac

MISSING=0
GRADLE_CMD=""

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

if [ -x "${ANDROID_DIR}/gradlew" ]; then
  GRADLE_CMD="${ANDROID_DIR}/gradlew"
elif command -v gradle >/dev/null 2>&1; then
  GRADLE_CMD="$(command -v gradle)"
elif [ -x "${HOMEBREW_GRADLE}" ]; then
  GRADLE_CMD="${HOMEBREW_GRADLE}"
fi

if [ -z "${GRADLE_CMD}" ]; then
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
  -P "${ANDROID_PLATFORM}" \
  -o "${APP_DIR}/src/main/jniLibs" \
  "${CARGO_BUILD_ARGS[@]}"

pushd "${ANDROID_DIR}" >/dev/null

echo "[2/2] assembling Android APK (${PROFILE})..."
"${GRADLE_CMD}" "${GRADLE_TASK}"
popd >/dev/null

echo "done: ${ANDROID_DIR}/app/build/outputs/apk/${PROFILE}/"
