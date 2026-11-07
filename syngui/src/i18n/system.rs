use super::lang::Lang;

const ENV_VARS: &[&str] = &["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"];

/// Язык системы: переменные окружения, затем платформенный источник; при неудаче `en`.
pub fn system_language() -> Lang {
    from_env()
        .or_else(platform::detect)
        .unwrap_or_else(Lang::en)
}

fn from_env() -> Option<Lang> {
    ENV_VARS.iter().find_map(|var| {
        let value = std::env::var(var).ok()?;
        let first = value.split(':').next().unwrap_or("");
        Lang::parse(first)
    })
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use super::Lang;

    pub fn detect() -> Option<Lang> {
        let navigator = js_sys::Reflect::get(&js_sys::global(), &"navigator".into()).ok()?;
        let language = js_sys::Reflect::get(&navigator, &"language".into()).ok()?;
        Lang::parse(&language.as_string()?)
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
mod platform {
    use super::Lang;

    const PROPS: &[&str] = &["persist.sys.locale", "ro.product.locale"];

    pub fn detect() -> Option<Lang> {
        PROPS.iter().find_map(|prop| {
            let out = std::process::Command::new("getprop")
                .arg(prop)
                .output()
                .ok()?;
            Lang::parse(String::from_utf8_lossy(&out.stdout).trim())
        })
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
mod platform {
    use super::Lang;

    pub fn detect() -> Option<Lang> {
        let out = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleLocale"])
            .output()
            .ok()?;
        Lang::parse(String::from_utf8_lossy(&out.stdout).trim())
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
mod platform {
    use super::Lang;
    use std::ffi::{c_char, c_void};

    type CFIndex = isize;
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> *const c_void;
        fn CFArrayGetCount(a: *const c_void) -> CFIndex;
        fn CFArrayGetValueAtIndex(a: *const c_void, i: CFIndex) -> *const c_void;
        fn CFStringGetCString(s: *const c_void, buf: *mut c_char, len: CFIndex, enc: u32) -> u8;
        fn CFRelease(o: *const c_void);
    }
    const UTF8: u32 = 0x0800_0100;

    /// Первый язык из «Настройки → Основные → Язык и регион» («ru-RU», «en-US»).
    pub fn detect() -> Option<Lang> {
        // SAFETY: Copy-функция отдаёт массив во владение — освобождается ниже;
        // элементы массива — CFString, буфер с запасом и нулём на конце.
        unsafe {
            let arr = CFLocaleCopyPreferredLanguages();
            if arr.is_null() {
                return None;
            }
            let mut out = None;
            if CFArrayGetCount(arr) > 0 {
                let s = CFArrayGetValueAtIndex(arr, 0);
                let mut buf = [0 as c_char; 64];
                if CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as CFIndex, UTF8) != 0 {
                    let tag = std::ffi::CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned();
                    out = Lang::parse(&tag);
                }
            }
            CFRelease(arr);
            out
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
mod platform {
    use super::Lang;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
        fn LCIDToLocaleName(lcid: u32, name: *mut u16, len: i32, flags: u32) -> i32;
        fn GetUserDefaultLocaleName(name: *mut u16, len: i32) -> i32;
    }

    /// Язык интерфейса Windows (язык отображения: «Параметры → Время и язык → Язык»), а не региональный
    /// формат: английская Windows с форматом ru-RU — английский. Формат — только если язык интерфейса не узнать.
    pub fn detect() -> Option<Lang> {
        let mut buf = [0u16; 85];
        let ui = unsafe { GetUserDefaultUILanguage() } as u32;
        if ui != 0 {
            let len = unsafe { LCIDToLocaleName(ui, buf.as_mut_ptr(), buf.len() as i32, 0) };
            if len > 1 {
                if let Some(l) = Lang::parse(&String::from_utf16_lossy(&buf[..(len - 1) as usize])) {
                    return Some(l);
                }
            }
        }
        let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if len <= 1 {
            return None;
        }
        Lang::parse(&String::from_utf16_lossy(&buf[..(len - 1) as usize]))
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "android", target_os = "macos", target_os = "ios", target_os = "windows"))
))]
mod platform {
    use super::Lang;

    pub fn detect() -> Option<Lang> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_language_never_fails() {
        let lang = system_language();
        assert!(!lang.tag().is_empty());
    }
}
