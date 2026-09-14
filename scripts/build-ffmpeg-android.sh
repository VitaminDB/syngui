#!/usr/bin/env bash
# Кросс-сборка статического FFmpeg под Android для фичи `ffmpeg` syngui.
#
# Usage: scripts/build-ffmpeg-android.sh [armeabi-v7a|arm64-v8a] [--clean]
#
# Результат: android-deps/ffmpeg/<abi>/{lib,include} — префикс, который
# ffmpeg-sys-next берёт через переменную окружения FFMPEG_DIR:
#
#   FFMPEG_DIR=$SYNGUI/android-deps/ffmpeg/armeabi-v7a \
#   cargo ndk -t armeabi-v7a -P 30 build --features "android ffmpeg-static"
#
# Требуется: ANDROID_NDK_HOME (или /opt/android-ndk), make, yasm не нужен
# (ARM-ассемблер собирает clang). Исходники берутся из ffmpeg.org, версия
# должна совпадать с major ffmpeg-next (9.x → FFmpeg 9.0.x), иначе bindgen
# сгенерирует привязки к другим заголовкам.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ABI="${1:-armeabi-v7a}"
CLEAN=0
for arg in "$@"; do
    [[ "$arg" == "--clean" ]] && CLEAN=1
done

FFMPEG_VERSION="${FFMPEG_VERSION:-9.0.1}"
API="${ANDROID_API:-30}"
NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-/opt/android-ndk}}"
TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/linux-x86_64"
[[ -d "$TOOLCHAIN" ]] || { echo "NDK toolchain не найден: $TOOLCHAIN" >&2; exit 1; }

case "$ABI" in
    armeabi-v7a)
        ARCH=arm
        CPU=armv7-a
        TRIPLE=armv7a-linux-androideabi
        LIB_TRIPLE=arm-linux-androideabi
        EXTRA_CFLAGS="-mfpu=neon -mfloat-abi=softfp -mthumb"
        EXTRA_CONFIGURE="--enable-neon --enable-thumb"
        ;;
    arm64-v8a)
        ARCH=aarch64
        CPU=armv8-a
        TRIPLE=aarch64-linux-android
        LIB_TRIPLE=aarch64-linux-android
        EXTRA_CFLAGS=""
        EXTRA_CONFIGURE="--enable-neon"
        ;;
    *)
        echo "Неизвестный ABI: $ABI (armeabi-v7a | arm64-v8a)" >&2
        exit 1
        ;;
esac

SRC_DIR="$ROOT/target/ffmpeg-src/ffmpeg-$FFMPEG_VERSION"
BUILD_DIR="$ROOT/target/ffmpeg-android-build/$ABI"
PREFIX="$ROOT/android-deps/ffmpeg/$ABI"

if [[ ! -d "$SRC_DIR" ]]; then
    mkdir -p "$(dirname "$SRC_DIR")"
    echo "==> Скачиваю ffmpeg-$FFMPEG_VERSION..."
    curl -sL -o "$SRC_DIR.tar.xz" "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz"
    tar xf "$SRC_DIR.tar.xz" -C "$(dirname "$SRC_DIR")"
fi

if [[ "$CLEAN" == 1 ]]; then
    rm -rf "$BUILD_DIR" "$PREFIX"
fi
mkdir -p "$BUILD_DIR" "$PREFIX"

CC="$TOOLCHAIN/bin/${TRIPLE}${API}-clang"
CXX="$TOOLCHAIN/bin/${TRIPLE}${API}-clang++"
[[ -x "$CC" ]] || { echo "Компилятор не найден: $CC" >&2; exit 1; }

echo "==> configure ($ABI, api $API, prefix $PREFIX)"
cd "$BUILD_DIR"

# Что включено и почему:
#  * только декодирование + demux + protocols: приложению нужен playback;
#  * network + http/https(без TLS)/tcp/udp/rtsp/hls: сетевые источники;
#  * mediacodec + jni: аппаратное декодирование h264/hevc/vp8/vp9/av1
#    (требует av_jni_set_java_vm на старте — syngui делает это в
#    app/handler/android.rs при фиче ffmpeg);
#  * без avfilter/avdevice/postproc — ffmpeg-next в syngui собирается
#    с features ["codec","format","software-resampling","software-scaling"].
"$SRC_DIR/configure" \
    --prefix="$PREFIX" \
    --target-os=android \
    --arch="$ARCH" \
    --cpu="$CPU" \
    --enable-cross-compile \
    --sysroot="$TOOLCHAIN/sysroot" \
    --cc="$CC" \
    --cxx="$CXX" \
    --ar="$TOOLCHAIN/bin/llvm-ar" \
    --nm="$TOOLCHAIN/bin/llvm-nm" \
    --ranlib="$TOOLCHAIN/bin/llvm-ranlib" \
    --strip="$TOOLCHAIN/bin/llvm-strip" \
    --extra-cflags="-O3 -fPIC -DANDROID -D__ANDROID_API__=$API $EXTRA_CFLAGS" \
    --extra-ldflags="-fPIC" \
    --enable-static \
    --disable-shared \
    --enable-pic \
    --enable-pthreads \
    $EXTRA_CONFIGURE \
    --disable-programs \
    --disable-doc \
    --disable-debug \
    --disable-autodetect \
    --disable-avdevice \
    --disable-avfilter \
    --disable-encoders \
    --disable-muxers \
    --disable-devices \
    --disable-filters \
    --enable-network \
    --enable-zlib \
    --enable-jni \
    --enable-mediacodec \
    --enable-decoder=h264_mediacodec \
    --enable-decoder=hevc_mediacodec \
    --enable-decoder=vp8_mediacodec \
    --enable-decoder=vp9_mediacodec \
    --enable-decoder=av1_mediacodec \
    --enable-decoder=mpeg4_mediacodec \
    --enable-hwaccels \
    --enable-protocol=file,http,httpproxy,tcp,udp,rtp,rtmp,hls,data,crypto,concat,pipe

echo "==> make -j$(nproc)"
make -j"$(nproc)" >/dev/null
make install >/dev/null

# ffmpeg-sys-next генерирует привязки к libavutil/hwcontext_*.h через свои
# заглушки SDK (vulkan, VideoToolbox, VAAPI, QSV), рассчитанные на 64-битные
# desktop-платформы: на armv7 static_assert по размеру VkPhysicalDeviceFeatures2
# роняет bindgen. FFmpeg ставит эти заголовки даже без поддержки самих API,
# а на Android они бессмысленны — убираем, оставляя только hwcontext_mediacodec.
for h in vulkan videotoolbox vaapi qsv cuda d3d11va d3d12va dxva2 opencl vdpau amf drm; do
    rm -f "$PREFIX/include/libavutil/hwcontext_$h.h"
done

# ffmpeg-sys-next (FFMPEG_DIR) не читает EXTRALIBS из config.mak — системные
# библиотеки Android линкует сам syngui (см. syngui/src/lib.rs, блок
# `#[link]` под cfg(target_os = "android", feature = "ffmpeg")). Список
# сохраняем рядом для справки.
grep '^EXTRALIBS' ffbuild/config.mak > "$PREFIX/extralibs.txt" || true

echo "==> Готово: $PREFIX"
ls "$PREFIX/lib"
