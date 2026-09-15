mod builder;
mod event_handling;
mod handler;
pub(crate) mod input_mapping;
#[cfg(feature = "splash")]
mod splash;

#[cfg(all(
    feature = "single-instance",
    not(target_arch = "wasm32"),
    not(target_os = "android")
))]
pub(crate) mod single_instance;
pub mod tray;
pub mod user_event;
#[cfg(all(feature = "wayland-dnd", target_os = "linux"))]
pub(crate) mod wayland_dnd;

#[cfg(target_os = "android")]
pub mod notification;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web_clipboard;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web_keys;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web_text_agent;

pub use builder::{AppBuilder, WindowConfig};

/// Заявка на «тик без кадра»: цикл событий проснётся через `after`, снова
/// обойдёт анимации (`Element::animate`) и не будет рисовать кадр, если
/// анимации сами не пометят элементы грязными. Нужен элементам, которым
/// надо регулярно что-то делать, но не перерисовываться: например,
/// `VideoView` в режиме Surface планирует показ кадров, которые рисует
/// сам кодек. Вызывать из `animate()`; заявка одноразовая.
pub fn request_tick(after: std::time::Duration) {
    if let Ok(mut slot) = TICK_REQUEST.lock() {
        *slot = Some(match *slot {
            Some(d) => d.min(after),
            None => after,
        });
    }
}

pub(crate) fn take_tick_request() -> Option<std::time::Duration> {
    TICK_REQUEST.lock().ok().and_then(|mut s| s.take())
}

static TICK_REQUEST: std::sync::Mutex<Option<std::time::Duration>> = std::sync::Mutex::new(None);
pub use tray::{TrayCloseAction, TrayConfig, TrayMenuItem};
pub use user_event::SynGuiUserEvent;

#[cfg(target_os = "android")]
pub use android_activity::AndroidApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    Auto,
    Vulkan,
    Gl,
    Dx12,
    Metal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuPowerPreference {
    HighPerformance,
    LowPower,
}

pub struct App;

impl App {
    pub fn new() -> AppBuilder {
        AppBuilder::new()
    }
}
