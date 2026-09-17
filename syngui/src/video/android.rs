//! Android-специфика FFmpeg: MediaCodec-декодеры (`h264_mediacodec` и др.)
//! ходят в Java-класс `android.media.MediaCodec` через JNI и без
//! зарегистрированной JavaVM отказываются открываться («no jni set»).
//! `AppBuilder::with_android_app` вызывает [`set_java_vm`] один раз на старте.

use std::ffi::c_void;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Mutex;

use ffmpeg_next::ffi;

// `libavcodec/jni.h` и `libavcodec/mediacodec.h` не входят в заголовки, по
// которым ffmpeg-sys-next генерирует привязки, поэтому объявляем символы
// сами: они есть в статической libavcodec, собранной с `--enable-jni
// --enable-mediacodec`.
extern "C" {
    fn av_jni_set_java_vm(vm: *mut c_void, log_ctx: *mut c_void) -> c_int;
    fn av_mediacodec_release_buffer(buffer: *mut c_void, render: c_int) -> c_int;
    fn av_mediacodec_render_buffer_at_time(buffer: *mut c_void, time: i64) -> c_int;
}

static VM_SET: AtomicBool = AtomicBool::new(false);
static VM_PTR: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ACTIVITY_PTR: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static SURFACE_REF: Mutex<Option<jni::objects::GlobalRef>> = Mutex::new(None);
static SURFACE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Запомнить JavaVM и Activity для JNI-вызовов видео-Surface
/// ([`video_surface`], [`set_video_rect`]). Делает `with_android_app`.
pub fn set_activity(vm: *mut c_void, activity: *mut c_void) {
    VM_PTR.store(vm, Ordering::Release);
    ACTIVITY_PTR.store(activity, Ordering::Release);
}

fn with_env<R>(f: impl FnOnce(&mut jni::JNIEnv, &jni::objects::JObject) -> Option<R>) -> Option<R> {
    let vm_ptr = VM_PTR.load(Ordering::Acquire);
    let activity_ptr = ACTIVITY_PTR.load(Ordering::Acquire);
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return None;
    }
    // SAFETY: указатели получены от android-activity и живут всё время
    // работы приложения.
    let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }.ok()?;
    let mut env = vm.attach_current_thread_permanently().ok()?;
    let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr as jni::sys::jobject) };
    let out = f(&mut env, &activity);
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
        return None;
    }
    out
}

/// `android.view.Surface` для вывода видео напрямую из MediaCodec, минуя
/// GPU-текстуры: метод `getVideoSurface()` активити приложения (см. эталон
/// `tv_rezka/android/.../TvActivity.java` — `SurfaceView` под GL-слоем).
/// Возвращает jobject (global ref) для `AVMediaCodecDeviceContext.surface`;
/// `None`, если активити такого метода не имеет или surface ещё не создан.
pub fn video_surface() -> Option<*mut c_void> {
    let global = with_env(|env, activity| {
        let obj = env
            .call_method(activity, "getVideoSurface", "()Landroid/view/Surface;", &[])
            .ok()?
            .l()
            .ok()?;
        if obj.as_raw().is_null() {
            return None;
        }
        env.new_global_ref(obj).ok()
    })?;
    let raw = global.as_obj().as_raw() as *mut c_void;
    if let Ok(mut slot) = SURFACE_REF.lock() {
        *slot = Some(global);
    }
    Some(raw)
}

/// Положение видео-Surface в долях окна (x, y, w, h ∈ 0..1): вызывает
/// `setVideoRect(float,float,float,float)` активити. Зовёт `VideoView`,
/// когда его fit-прямоугольник меняется.
pub fn set_video_rect(x: f32, y: f32, w: f32, h: f32) {
    use jni::objects::JValue;
    let _ = with_env(|env, activity| {
        env.call_method(
            activity,
            "setVideoRect",
            "(FFFF)V",
            &[
                JValue::Float(x),
                JValue::Float(y),
                JValue::Float(w),
                JValue::Float(h),
            ],
        )
        .ok()
        .map(|_| ())
    });
}

/// Сейчас на экране видео идёт через Surface под GL-слоем: UI над ним обязан
/// быть прозрачным (корневой фон и clear-color окна).
pub fn set_surface_video_active(active: bool) {
    SURFACE_ACTIVE.store(active, Ordering::Release);
}

pub fn surface_video_active() -> bool {
    SURFACE_ACTIVE.load(Ordering::Acquire)
}

/// Отдать выходной буфер MediaCodec (`AVFrame.data[3]`) на Surface
/// (`render = 1`) либо вернуть кодеку без показа.
pub(crate) unsafe fn release_mediacodec_buffer(buffer: *mut c_void, render: bool) {
    if !buffer.is_null() {
        av_mediacodec_release_buffer(buffer, render as c_int);
    }
}

/// Показать буфер MediaCodec в заданный момент (наносекунды часов
/// `CLOCK_MONOTONIC`, как `System.nanoTime()`): кодек сам выведет кадр на
/// ближайшем к этому времени vsync.
pub(crate) unsafe fn render_mediacodec_buffer_at(buffer: *mut c_void, time_ns: i64) {
    if !buffer.is_null() {
        av_mediacodec_render_buffer_at_time(buffer, time_ns);
    }
}

/// Текущее время часов `CLOCK_MONOTONIC` в наносекундах.
pub fn monotonic_ns() -> i64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: обычный вызов clock_gettime с валидным указателем.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as i64 * 1_000_000_000 + ts.tv_nsec as i64
}

/// Создать hw-device-контекст MEDIACODEC с заданным Surface и подвесить его
/// на ещё не открытый кодек-контекст: декодер `*_mediacodec` тогда отдаёт
/// кадры `AV_PIX_FMT_MEDIACODEC`, которые показываются на Surface без
/// копирования (см. `HwAccel::MediaCodecSurface`).
pub(crate) unsafe fn attach_surface_device(
    codec_ctx: *mut ffi::AVCodecContext,
    surface: *mut c_void,
) -> Result<(), String> {
    let dev = ffi::av_hwdevice_ctx_alloc(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_MEDIACODEC);
    if dev.is_null() {
        return Err("av_hwdevice_ctx_alloc(MEDIACODEC) вернул NULL".into());
    }
    let dev_ctx = (*dev).data as *mut ffi::AVHWDeviceContext;
    let mc = (*dev_ctx).hwctx as *mut ffi::AVMediaCodecDeviceContext;
    (*mc).surface = surface;
    let rc = ffi::av_hwdevice_ctx_init(dev);
    if rc < 0 {
        let mut dev = dev;
        ffi::av_buffer_unref(&mut dev);
        return Err(format!("av_hwdevice_ctx_init: {rc}"));
    }
    (*codec_ctx).hw_device_ctx = ffi::av_buffer_ref(dev);
    let mut dev = dev;
    ffi::av_buffer_unref(&mut dev);
    Ok(())
}

/// Передать FFmpeg указатель на JavaVM процесса. Повторные вызовы — no-op.
pub fn set_java_vm(vm: *mut c_void) {
    if vm.is_null() || VM_SET.swap(true, Ordering::SeqCst) {
        return;
    }
    VM_PTR.store(vm, Ordering::Release);
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
            log::info!(
                "ffmpeg: CA bundle: {count} сертификатов → {}",
                out.display()
            );
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
