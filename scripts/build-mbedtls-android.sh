#!/usr/bin/env bash
# Кросс-сборка статического mbedTLS под Android — TLS для https в FFmpeg
# (`build-ffmpeg-android.sh` подхватывает префикс автоматически).
#
# Usage: scripts/build-mbedtls-android.sh [armeabi-v7a|arm64-v8a]
# Результат: android-deps/mbedtls/<abi>/{lib,include}
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ABI="${1:-armeabi-v7a}"
VERSION="${MBEDTLS_VERSION:-3.6.5}"
API="${ANDROID_API:-30}"
NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-/opt/android-ndk}}"

SRC_DIR="$ROOT/target/mbedtls-$VERSION"
BUILD_DIR="$ROOT/target/mbedtls-android-build/$ABI"
PREFIX="$ROOT/android-deps/mbedtls/$ABI"

if [[ ! -d "$SRC_DIR" ]]; then
    echo "==> Скачиваю mbedtls-$VERSION..."
    curl -sL -o "$ROOT/target/mbedtls.tar.bz2" \
        "https://github.com/Mbed-TLS/mbedtls/releases/download/mbedtls-$VERSION/mbedtls-$VERSION.tar.bz2"
    tar xf "$ROOT/target/mbedtls.tar.bz2" -C "$ROOT/target"
fi

mkdir -p "$BUILD_DIR" "$PREFIX"
cd "$BUILD_DIR"
cmake "$SRC_DIR" \
    -DCMAKE_TOOLCHAIN_FILE="$NDK/build/cmake/android.toolchain.cmake" \
    -DANDROID_ABI="$ABI" \
    -DANDROID_PLATFORM="android-$API" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$PREFIX" \
    -DENABLE_TESTING=OFF \
    -DENABLE_PROGRAMS=OFF \
    -DUSE_SHARED_MBEDTLS_LIBRARY=OFF \
    -DUSE_STATIC_MBEDTLS_LIBRARY=ON \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON >/dev/null
cmake --build . -j"$(nproc)" >/dev/null
cmake --install . >/dev/null
echo "==> Готово: $PREFIX"
ls "$PREFIX/lib"
