#!/usr/bin/env bash
# Пересобрать syngui/src/app/android/syngui_framework.dex из Java-исходников
# рядом с ним (JNI-мост: обработчики «назад», ввода, уведомлений, версия).
# Dex вшивается в библиотеку (include_bytes!) и грузится InMemoryDexClassLoader,
# поэтому после правки любого .java его нужно пересобрать и закоммитить.
# Usage: scripts/build-android-dex.sh  (нужны javac, d8 и android.jar из SDK)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$SCRIPT_DIR/../syngui/src/app/android"
ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
PLATFORM="${ANDROID_PLATFORM:-$(ls -d "$ANDROID_HOME"/platforms/android-* | sort -V | tail -1)}"
D8="${D8:-$(command -v d8 || ls "$ANDROID_HOME"/build-tools/*/d8 | sort -V | tail -1)}"

# SynGuiForegroundService — Service, его объявляет манифест приложения и
# компилирует само приложение; в dex фреймворка он не входит.
CLASSES=(SynGuiBackHandler SynGuiInputHandler SynGuiNotificationHandler SynGuiVersionHelper)
SOURCES=()
for c in "${CLASSES[@]}"; do SOURCES+=("$SRC/$c.java"); done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
echo "==> javac ($PLATFORM)"
javac --release 17 -Xlint:-options -classpath "$PLATFORM/android.jar" -d "$TMP/classes" "${SOURCES[@]}"
echo "==> d8"
mkdir -p "$TMP/dex"
"$D8" --release --min-api 30 --lib "$PLATFORM/android.jar" --output "$TMP/dex" "$TMP"/classes/syngui/android/*.class
cp "$TMP/dex/classes.dex" "$SRC/syngui_framework.dex"
echo "==> $SRC/syngui_framework.dex ($(stat -c %s "$SRC/syngui_framework.dex") байт)"
