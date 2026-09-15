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

/// Единый PEM-файл системных корневых сертификатов для mbedTLS в FFmpeg.
/// Android хранит их отдельными файлами в `/system/etc/security/cacerts` и,
/// с Android 14, в APEX conscrypt; mbedtls_x509_crt_parse_file читает только
/// один файл — склеиваем в `cache_dir/ca-bundle.pem` и переиспользуем.
///
/// Путь выставляется в переменную окружения `SSL_CERT_FILE`: FFmpeg из
/// `scripts/build-ffmpeg-android.sh` пропатчен так, что TLS-контекст берёт
/// её как `ca_file` по умолчанию. Это нужно потому, что HLS/DASH открывают
/// сегменты новыми https-соединениями, до которых опция `ca_file`,
/// переданная в `open_with_options`, не доходит. Вызывать до открытия
/// первого https-источника (например, в `android_main`).
pub fn system_ca_bundle(cache_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let out = cache_dir.join("ca-bundle.pem");
    if out.exists() {
        export_ca_env(&out);
        return Some(out);
    }
    let dirs = [
        "/apex/com.android.conscrypt/cacerts",
        "/system/etc/security/cacerts",
    ];
    let mut bundle = String::new();
    let mut count = 0usize;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Ok(text) = std::fs::read_to_string(entry.path()) {
                // Файлы Android содержат PEM + текстовый дамп; берём PEM.
                for block in text.split("-----BEGIN CERTIFICATE-----").skip(1) {
                    if let Some(end) = block.find("-----END CERTIFICATE-----") {
                        bundle.push_str("-----BEGIN CERTIFICATE-----");
                        bundle.push_str(&block[..end]);
                        bundle.push_str("-----END CERTIFICATE-----\n");
                        count += 1;
                    }
                }
            }
        }
    }
    if count == 0 {
        log::warn!("ffmpeg: системные сертификаты не найдены, https без проверки не откроется");
        return None;
    }
    let _ = std::fs::create_dir_all(cache_dir);
    match std::fs::write(&out, bundle) {
        Ok(()) => {
            log::info!("ffmpeg: CA bundle: {count} сертификатов → {}", out.display());
            export_ca_env(&out);
            Some(out)
        }
        Err(e) => {
            log::warn!("ffmpeg: не удалось записать CA bundle: {e}");
            None
        }
    }
}

fn export_ca_env(path: &std::path::Path) {
    if std::env::var_os("SSL_CERT_FILE").is_none() {
        std::env::set_var("SSL_CERT_FILE", path);
    }
}
