#!/usr/bin/env bash
# Build Android APK for Calculator
# Usage: ./build-apk.sh [--debug|--release]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ANDROID_DIR="$SCRIPT_DIR/android"
JNILIBS="$ANDROID_DIR/app/src/main/jniLibs"

export JAVA_HOME="/usr/lib/jvm/java-21-openjdk"
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"

# Ensure local.properties exists
if [[ ! -f "$ANDROID_DIR/local.properties" ]]; then
    echo "sdk.dir=$ANDROID_HOME" > "$ANDROID_DIR/local.properties"
fi

CARGO_FLAGS="--release"
GRADLE_TASK="assembleDebug"
if [[ "${1:-}" == "--debug" ]]; then
    CARGO_FLAGS=""
elif [[ "${1:-}" == "--release" ]]; then
    GRADLE_TASK="assembleRelease"
fi

echo "==> Building native library (arm64${CARGO_FLAGS:+, release})..."
cd "$PROJECT_ROOT"
cargo ndk -t arm64-v8a -o "$JNILIBS" build $CARGO_FLAGS -p calculator --no-default-features --features android

echo "==> Building APK ($GRADLE_TASK)..."
cd "$ANDROID_DIR"
./gradlew "$GRADLE_TASK"

if [[ "$GRADLE_TASK" == "assembleRelease" ]]; then
    APK="$ANDROID_DIR/app/build/outputs/apk/release/app-release.apk"
else
    APK="$ANDROID_DIR/app/build/outputs/apk/debug/app-debug.apk"
fi
echo "==> Done: $APK"
