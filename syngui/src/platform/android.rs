//! Android: JNI-мост к `syngui.android.SynGuiPlatformHandler` (in-memory DEX
//! фреймворка). Класс загружает `AppHandler::load_and_register_back` и
//! передаёт сюда через [`init`]; там же регистрируется нативный
//! `nativeOnPicked`, которым Java отдаёт результат пикера.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::Mutex;

use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::jlong;
use jni::{JNIEnv, NativeMethod};

use super::{MediaKind, PickCallback, PickedFile};

static VM_PTR: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
static CLASS: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static PENDING: Mutex<Option<HashMap<u64, PickCallback>>> = Mutex::new(None);

/// Загрузить класс из загрузчика фреймворка, вызвать `register(activity)`
/// и зарегистрировать натив. Повторный вызов (новое resume) заменяет класс.
pub(crate) unsafe fn init(
    env: &mut JNIEnv,
    vm_ptr: *mut std::ffi::c_void,
    loader: &JObject,
    activity: &JObject,
) -> Result<(), String> {
    let name: JObject = env
        .new_string("syngui.android.SynGuiPlatformHandler")
        .map_err(|e| format!("string: {e}"))?
        .into();
    let cls = env
        .call_method(
            loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        )
        .map_err(|e| format!("loadClass(Platform): {e}"))?
        .l()
        .map_err(|e| format!("cast: {e}"))?;
    let cls: JClass = cls.into();
    env.register_native_methods(
        &cls,
        &[NativeMethod {
            name: "nativeOnPicked".into(),
            sig: "(JLjava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V".into(),
            fn_ptr: native_on_picked as *mut std::ffi::c_void,
        }],
    )
    .map_err(|e| format!("RegisterNatives: {e}"))?;
    env.call_static_method(
        &cls,
        "register",
        "(Landroid/app/Activity;)V",
        &[JValue::Object(activity)],
    )
    .map_err(|e| format!("PlatformHandler.register: {e}"))?;
    let global = env
        .new_global_ref(cls)
        .map_err(|e| format!("global_ref: {e}"))?;
    let raw = global.as_raw() as *mut ();
    std::mem::forget(global);
    VM_PTR.store(vm_ptr as *mut (), Ordering::SeqCst);
    CLASS.store(raw, Ordering::SeqCst);
    Ok(())
}

fn with_env<R>(f: impl FnOnce(&mut JNIEnv, &JClass) -> Result<R, String>) -> Result<R, String> {
    let vm_ptr = VM_PTR.load(Ordering::SeqCst);
    let cls = CLASS.load(Ordering::SeqCst);
    if vm_ptr.is_null() || cls.is_null() {
        return Err("платформенный мост Android ещё не инициализирован".into());
    }
    unsafe {
        let vm = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM)
            .map_err(|e| format!("JavaVM: {e}"))?;
        let result = match vm.attach_current_thread_permanently() {
            Ok(mut env) => {
                let cls = JClass::from_raw(cls as jni::sys::jclass);
                let r = f(&mut env, &cls);
                if env.exception_check().unwrap_or(false) {
                    let _ = env.exception_describe();
                    let _ = env.exception_clear();
                }
                r
            }
            Err(e) => Err(format!("attach: {e}")),
        };
        std::mem::forget(vm);
        result
    }
}

pub(super) fn pick_media(kind: MediaKind, cb: PickCallback) {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(id, cb);
    let kind = match kind {
        MediaKind::Image => 0,
        MediaKind::Video => 1,
        MediaKind::ImageOrVideo => 2,
    };
    let r = with_env(|env, cls| {
        env.call_static_method(cls, "pickMedia", "(JI)V", &[JValue::Long(id as jlong), JValue::Int(kind)])
            .map(|_| ())
            .map_err(|e| format!("pickMedia: {e}"))
    });
    if let Err(e) = r {
        deliver(id, Err(e));
    }
}

fn deliver(id: u64, result: Result<Option<PickedFile>, String>) {
    let cb = PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .and_then(|m| m.remove(&id));
    match cb {
        Some(cb) => crate::async_runtime::run_on_main_thread(move || cb(result)),
        None => log::warn!("[syngui] pick_media: результат для неизвестного запроса {id}"),
    }
}

fn opt_string(env: &mut JNIEnv, s: &JString) -> Option<String> {
    if s.is_null() {
        return None;
    }
    env.get_string(s).ok().map(|s| s.into())
}

extern "system" fn native_on_picked<'l>(
    mut env: JNIEnv<'l>,
    _cls: JClass<'l>,
    id: jlong,
    path: JString<'l>,
    name: JString<'l>,
    mime: JString<'l>,
    error: JString<'l>,
) {
    let result = if let Some(err) = opt_string(&mut env, &error) {
        Err(err)
    } else if let Some(path) = opt_string(&mut env, &path) {
        let path = PathBuf::from(path);
        let name = opt_string(&mut env, &name).unwrap_or_else(|| {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let mime = opt_string(&mut env, &mime)
            .unwrap_or_else(|| super::mime_for_path(&path).to_string());
        Ok(Some(PickedFile { path, name, mime }))
    } else {
        Ok(None)
    };
    deliver(id as u64, result);
}

/// Вызвать статический `String f(String…)` (null — успех).
fn call_status(method: &str, sig: &str, args: &[&str]) -> Result<(), String> {
    with_env(|env, cls| {
        let mut jargs = Vec::with_capacity(args.len());
        for a in args {
            let s: JObject = env.new_string(a).map_err(|e| format!("{e}"))?.into();
            jargs.push(s);
        }
        let values: Vec<JValue> = jargs.iter().map(JValue::Object).collect();
        let ret = env
            .call_static_method(cls, method, sig, &values)
            .map_err(|e| format!("{method}: {e}"))?
            .l()
            .map_err(|e| format!("{method}: {e}"))?;
        if ret.is_null() {
            Ok(())
        } else {
            let s = JString::from(ret);
            Err(env.get_string(&s).map(|s| s.into()).unwrap_or_else(|_| format!("{method}: ошибка")))
        }
    })
}

const S2: &str = "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;";
const S3: &str = "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;";

pub(super) fn share_text(title: &str, text: &str) -> Result<(), String> {
    call_status("shareText", S2, &[title, text])
}

pub(super) fn share_file(path: &Path, mime: &str, title: &str) -> Result<(), String> {
    call_status("shareFile", S3, &[&path.to_string_lossy(), mime, title])
}

pub(super) fn open_path(path: &Path) -> Result<(), String> {
    call_status("openPath", "(Ljava/lang/String;)Ljava/lang/String;", &[&path.to_string_lossy()])
}

/// Декодировать картинку (HEIF/AVIF) системным `ImageDecoder`: RGBA8,
/// ширина, высота и признак умноженной альфы. `max_side` > 0 — большая
/// сторона уменьшается до него при декодировании.
pub(crate) fn decode_image(bytes: &[u8], max_side: u32) -> Result<(u32, u32, Vec<u8>, bool), String> {
    use jni::objects::{JByteArray, JObjectArray};
    with_env(|env, cls| {
        // Локальный кадр: поток декодирования прикреплён навсегда, ссылки
        // иначе копились бы до его выхода.
        env.with_local_frame(8, |env| -> jni::errors::Result<Result<_, String>> {
            let input = env.byte_array_from_slice(bytes)?;
            let info = env.new_int_array(3)?;
            let error = env.new_object_array(1, "java/lang/String", JObject::null())?;
            let out = env
                .call_static_method(
                    cls,
                    "decodeRgba",
                    "([BI[I[Ljava/lang/String;)[B",
                    &[
                        JValue::Object(&input),
                        JValue::Int(max_side.min(i32::MAX as u32) as i32),
                        JValue::Object(&info),
                        JValue::Object(&error),
                    ],
                )?
                .l()?;
            if out.is_null() {
                let msg = env.get_object_array_element(&JObjectArray::from(error), 0)?;
                let msg = if msg.is_null() {
                    "ImageDecoder: ошибка".to_string()
                } else {
                    env.get_string(&JString::from(msg))?.into()
                };
                return Ok(Err(msg));
            }
            let mut sz = [0i32; 3];
            env.get_int_array_region(&info, 0, &mut sz)?;
            let data = env.convert_byte_array(JByteArray::from(out))?;
            Ok(Ok((sz[0] as u32, sz[1] as u32, data, sz[2] != 0)))
        })
        .map_err(|e| format!("ImageDecoder: {e}"))?
    })
}
