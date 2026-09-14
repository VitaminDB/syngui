//! Android-специфика FFmpeg: MediaCodec-декодеры (`h264_mediacodec` и др.)
//! ходят в Java-класс `android.media.MediaCodec` через JNI и без
//! зарегистрированной JavaVM отказываются открываться («no jni set»).
//! `AppBuilder::with_android_app` вызывает [`set_java_vm`] один раз на старте.

use std::ffi::c_void;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, Ordering};

// `libavcodec/jni.h` не входит в заголовки, по которым ffmpeg-sys-next
// генерирует привязки, поэтому объявляем символ сами: он есть в статической
// libavcodec, собранной с `--enable-jni`.
extern "C" {
    fn av_jni_set_java_vm(vm: *mut c_void, log_ctx: *mut c_void) -> c_int;
}

static VM_SET: AtomicBool = AtomicBool::new(false);

/// Передать FFmpeg указатель на JavaVM процесса. Повторные вызовы — no-op.
pub fn set_java_vm(vm: *mut c_void) {
    if vm.is_null() || VM_SET.swap(true, Ordering::SeqCst) {
        return;
    }
    // SAFETY: vm — живой JavaVM* из android-activity; FFmpeg только
    // сохраняет указатель и позже делает AttachCurrentThread.
    let rc = unsafe { av_jni_set_java_vm(vm, std::ptr::null_mut()) };
    if rc < 0 {
        log::warn!("ffmpeg: av_jni_set_java_vm вернул {rc} — MediaCodec будет недоступен");
    } else {
        log::info!("ffmpeg: JavaVM передана libavcodec (MediaCodec доступен)");
    }
}
