use super::lang::Lang;

const ENV_VARS: &[&str] = &["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"];

/// Язык системы: переменные окружения, затем платформенный источник; при неудаче `en`.
pub fn system_language() -> Lang {
    from_env().or_else(platform::detect).unwrap_or_else(Lang::en)
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
            let out = std::process::Command::new("getprop").arg(prop).output().ok()?;
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

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
mod platform {
    use super::Lang;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultLocaleName(name: *mut u16, len: i32) -> i32;
    }

    pub fn detect() -> Option<Lang> {
        let mut buf = [0u16; 85];
        let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if len <= 1 {
            return None;
        }
        Lang::parse(&String::from_utf16_lossy(&buf[..(len - 1) as usize]))
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "android", target_os = "macos", target_os = "windows"))
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
