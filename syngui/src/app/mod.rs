mod builder;
mod handler;
mod event_handling;
pub(crate) mod input_mapping;
#[cfg(feature = "splash")]
mod splash;

pub mod tray;
pub mod user_event;
#[cfg(all(feature = "wayland-dnd", target_os = "linux"))]
pub(crate) mod wayland_dnd;
#[cfg(all(
    feature = "single-instance",
    not(target_arch = "wasm32"),
    not(target_os = "android")
))]
pub(crate) mod single_instance;

#[cfg(target_os = "android")]
pub mod notification;

pub use builder::{AppBuilder, WindowConfig};
pub use tray::{TrayCloseAction, TrayConfig, TrayMenuItem};
pub use user_event::MguiUserEvent;

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
