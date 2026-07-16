#!/usr/bin/env bash
# Build, install and launch Calculator on connected device
# Usage: ./run-device.sh [--no-build]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ANDROID_DIR="$SCRIPT_DIR/android"
PACKAGE="com.syngui.calculator"
ACTIVITY="com.google.androidgamesdk.GameActivity"

# Pick device (prefer physical over emulator)
DEVICE=$(adb devices | grep -v emulator | grep 'device$' | head -1 | awk '{print $1}')
if [[ -z "$DEVICE" ]]; then
    DEVICE=$(adb devices | grep 'device$' | head -1 | awk '{print $1}')
fi
if [[ -z "$DEVICE" ]]; then
    echo "No Android device found."
    exit 1
fi
echo "==> Device: $DEVICE"

# Build unless --no-build
if [[ "${1:-}" != "--no-build" ]]; then
    "$SCRIPT_DIR/build-apk.sh"
fi

# Find APK
APK=$(find "$ANDROID_DIR/app/build/outputs/apk" -name "*.apk" | sort | tail -1)
if [[ -z "$APK" ]]; then
    echo "APK not found. Run build-apk.sh first."
    exit 1
fi

echo "==> Installing..."
adb -s "$DEVICE" install -r "$APK"

echo "==> Launching..."
adb -s "$DEVICE" shell am start -S -n "$PACKAGE/$ACTIVITY"

# Wait for process
for _ in 1 2 3; do
    sleep 1
    PID=$(adb -s "$DEVICE" shell pidof "$PACKAGE" 2>/dev/null | tr -d '[:space:]')
    [[ -n "$PID" ]] && break
done

if [[ -n "${PID:-}" ]]; then
    echo "==> Logcat PID=$PID (Ctrl+C to stop):"
    adb -s "$DEVICE" logcat --pid="$PID" | grep -iE "syngui|calculator|GameActivity"
else
    echo "==> App may have crashed. Check: adb logcat -b crash -t 50"
fi
