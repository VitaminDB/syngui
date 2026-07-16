#[cfg(feature = "clipboard")]
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

#[cfg(not(feature = "clipboard"))]
mod imp {
    pub fn copy(_text: &str) {
        log::warn!("clipboard::copy: feature `clipboard` отключена");
    }
    pub fn paste() -> Option<String> {
        None
    }
}

pub use imp::{copy, paste};
