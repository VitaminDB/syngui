//! Буфер обмена: единая точка входа для всех платформ.
//!
//! - desktop (feature `clipboard`): arboard, один глобальный экземпляр;
//! - wasm32: локальный кэш + async Clipboard API браузера. Синхронный
//!   `paste()` отдаёт кэш; кэш наполняется DOM-событием `paste`
//!   (см. `app/web_clipboard.rs`), собственными `copy()` и фоновым
//!   `readText()` из [`request_refresh`];
//! - android: JNI-мост к `ClipboardManager` (указатели VM/Activity
//!   выставляет `AppHandler` через [`set_android_ptrs`]).

#[cfg(all(
    feature = "clipboard",
    not(target_arch = "wasm32"),
    not(target_os = "android")
))]
mod imp {
    use std::sync::{Arc, Mutex, OnceLock};

    fn handle() -> Option<Arc<Mutex<arboard::Clipboard>>> {
        static H: OnceLock<Option<Arc<Mutex<arboard::Clipboard>>>> = OnceLock::new();
        H.get_or_init(|| match arboard::Clipboard::new() {
            Ok(cb) => Some(Arc::new(Mutex::new(cb))),
            Err(e) => {
                log::warn!("clipboard: arboard::Clipboard::new() failed: {e}");
                None
            }
        })
        .clone()
    }

    pub fn copy(text: &str) {
        let Some(h) = handle() else { return };
        let mut g = match h.lock() {
            Ok(g) => g,
            Err(_) => {
                log::warn!("clipboard: mutex poisoned");
                return;
            }
        };
        if let Err(e) = g.set_text(text.to_string()) {
            log::warn!("clipboard: set_text не удался: {e}");
        }
    }

    pub fn paste() -> Option<String> {
        let h = handle()?;
        let result = h.lock().ok().and_then(|mut g| g.get_text().ok());
        result
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::{Cell, RefCell};

    use wasm_bindgen::{JsCast, JsValue};

    thread_local! {
        static CACHE: RefCell<Option<String>> = const { RefCell::new(None) };
        static REFRESHED: Cell<bool> = const { Cell::new(false) };
    }

    pub(crate) fn set_cached(text: String) {
        CACHE.with(|c| *c.borrow_mut() = Some(text));
    }

    /// Копирует в кэш и в системный буфер через `navigator.clipboard.writeText`
    /// (fire-and-forget: промис не ждём, ошибка означает лишь то, что текст
    /// останется только во внутреннем кэше).
    pub fn copy(text: &str) {
        set_cached(text.to_string());
        if let Some(win) = web_sys::window() {
            let _ = win.navigator().clipboard().write_text(text);
        }
    }

    /// Синхронное чтение — только кэш. Свежий системный буфер попадает в кэш
    /// через DOM-событие `paste` и [`request_refresh`].
    pub fn paste() -> Option<String> {
        CACHE.with(|c| c.borrow().clone())
    }

    /// Фоновое обновление кэша из `navigator.clipboard.readText()`.
    /// Метод вызывается через Reflect: в Firefox `readText` страницам
    /// недоступен, и прямой биндинг web-sys бросил бы исключение.
    pub fn request_refresh() {
        let Some(win) = web_sys::window() else { return };
        let clipboard = win.navigator().clipboard();
        let Ok(f) = js_sys::Reflect::get(&clipboard, &JsValue::from_str("readText")) else {
            return;
        };
        let Ok(f) = f.dyn_into::<js_sys::Function>() else {
            return;
        };
        let Ok(promise) = f.call0(&clipboard) else { return };
        let Ok(promise) = promise.dyn_into::<js_sys::Promise>() else {
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            let Ok(value) = wasm_bindgen_futures::JsFuture::from(promise).await else {
                return;
            };
            let Some(text) = value.as_string() else { return };
            if text.is_empty() {
                return;
            }
            let changed = CACHE.with(|c| c.borrow().as_deref() != Some(text.as_str()));
            if changed {
                set_cached(text);
                REFRESHED.set(true);
                if let Some(win) = crate::signal::primary_window() {
                    win.request_redraw();
                }
            }
        });
    }

    /// Снимает флаг «кэш обновился асинхронно». `AppHandler::update()`
    /// по нему повторяет `FocusGained` фокусному элементу, чтобы подсказка
    /// буфера обмена появилась с уже подгруженным текстом.
    pub(crate) fn take_refreshed() -> bool {
        REFRESHED.replace(false)
    }
}

#[cfg(target_os = "android")]
mod imp {
    use std::sync::atomic::{AtomicPtr, Ordering};

    use jni::objects::{JObject, JValue};

    static VM_PTR: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
    static ACTIVITY_PTR: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

    pub(crate) fn set_android_ptrs(vm: *mut std::ffi::c_void, activity: *mut std::ffi::c_void) {
        VM_PTR.store(vm as *mut (), Ordering::SeqCst);
        ACTIVITY_PTR.store(activity as *mut (), Ordering::SeqCst);
    }

    fn ptrs() -> Option<(*mut std::ffi::c_void, *mut std::ffi::c_void)> {
        let vm = VM_PTR.load(Ordering::SeqCst);
        let activity = ACTIVITY_PTR.load(Ordering::SeqCst);
        if vm.is_null() || activity.is_null() {
            None
        } else {
            Some((vm as *mut std::ffi::c_void, activity as *mut std::ffi::c_void))
        }
    }

    /// `Context.getSystemService("clipboard")` → ClipboardManager.
    fn clipboard_manager<'l>(
        env: &mut jni::JNIEnv<'l>,
        activity: &JObject,
    ) -> Result<JObject<'l>, jni::errors::Error> {
        let name: JObject = env.new_string("clipboard")?.into();
        env.call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&name)],
        )?
        .l()
    }

    pub fn copy(text: &str) {
        let Some((vm_ptr, activity_ptr)) = ptrs() else { return };
        let result = unsafe { copy_jni(vm_ptr, activity_ptr, text) };
        if let Err(e) = result {
            log::warn!("clipboard: setPrimaryClip не удался: {e}");
        }
    }

    unsafe fn copy_jni(
        vm_ptr: *mut std::ffi::c_void,
        activity_ptr: *mut std::ffi::c_void,
        text: &str,
    ) -> Result<(), jni::errors::Error> {
        let vm = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM)?;
        let mut env = match vm.attach_current_thread_permanently() {
            Ok(env) => env,
            Err(e) => {
                std::mem::forget(vm);
                return Err(e);
            }
        };
        let activity = JObject::from_raw(activity_ptr as jni::sys::jobject);

        let result = (|| {
            let cm = clipboard_manager(&mut env, &activity)?;
            let label: JObject = env.new_string("syngui")?.into();
            let value: JObject = env.new_string(text)?.into();
            let clip = env
                .call_static_method(
                    "android/content/ClipData",
                    "newPlainText",
                    "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
                    &[JValue::Object(&label), JValue::Object(&value)],
                )?
                .l()?;
            env.call_method(
                &cm,
                "setPrimaryClip",
                "(Landroid/content/ClipData;)V",
                &[JValue::Object(&clip)],
            )?;
            Ok(())
        })();
        if result.is_err() {
            let _ = env.exception_clear();
        }
        std::mem::forget(vm);
        result
    }

    pub fn paste() -> Option<String> {
        let (vm_ptr, activity_ptr) = ptrs()?;
        match unsafe { paste_jni(vm_ptr, activity_ptr) } {
            Ok(text) => text,
            Err(e) => {
                log::warn!("clipboard: getPrimaryClip не удался: {e}");
                None
            }
        }
    }

    unsafe fn paste_jni(
        vm_ptr: *mut std::ffi::c_void,
        activity_ptr: *mut std::ffi::c_void,
    ) -> Result<Option<String>, jni::errors::Error> {
        let vm = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM)?;
        let mut env = match vm.attach_current_thread_permanently() {
            Ok(env) => env,
            Err(e) => {
                std::mem::forget(vm);
                return Err(e);
            }
        };
        let activity = JObject::from_raw(activity_ptr as jni::sys::jobject);

        let result = (|| {
            let cm = clipboard_manager(&mut env, &activity)?;
            let has = env.call_method(&cm, "hasPrimaryClip", "()Z", &[])?.z()?;
            if !has {
                return Ok(None);
            }
            let clip = env
                .call_method(&cm, "getPrimaryClip", "()Landroid/content/ClipData;", &[])?
                .l()?;
            if clip.is_null() {
                return Ok(None);
            }
            let count = env.call_method(&clip, "getItemCount", "()I", &[])?.i()?;
            if count == 0 {
                return Ok(None);
            }
            let item = env
                .call_method(
                    &clip,
                    "getItemAt",
                    "(I)Landroid/content/ClipData$Item;",
                    &[JValue::Int(0)],
                )?
                .l()?;
            let cs = env
                .call_method(
                    &item,
                    "coerceToText",
                    "(Landroid/content/Context;)Ljava/lang/CharSequence;",
                    &[JValue::Object(&activity)],
                )?
                .l()?;
            if cs.is_null() {
                return Ok(None);
            }
            let jstr = env
                .call_method(&cs, "toString", "()Ljava/lang/String;", &[])?
                .l()?;
            let text: String = env.get_string(&jstr.into())?.into();
            Ok(if text.is_empty() { None } else { Some(text) })
        })();
        if result.is_err() {
            let _ = env.exception_clear();
        }
        std::mem::forget(vm);
        result
    }
}

#[cfg(all(
    not(feature = "clipboard"),
    not(target_arch = "wasm32"),
    not(target_os = "android")
))]
mod imp {
    pub fn copy(_text: &str) {
        log::warn!("clipboard::copy: feature `clipboard` отключена");
    }
    pub fn paste() -> Option<String> {
        None
    }
}

pub use imp::{copy, paste};

#[cfg(target_arch = "wasm32")]
pub use imp::request_refresh;
#[cfg(target_arch = "wasm32")]
pub(crate) use imp::{set_cached, take_refreshed};

#[cfg(target_os = "android")]
pub(crate) use imp::set_android_ptrs;
