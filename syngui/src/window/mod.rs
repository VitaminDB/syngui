pub mod backdrop;
#[cfg(all(target_os = "linux", feature = "system-blur"))]
pub mod ext_background_effect;
pub mod window;

pub use backdrop::{set_backdrop, BackdropConfig, BackdropContrast, BackdropRegion};
pub use window::{Window, WindowBuilder, WindowEvent};

/// Состояние окна, которое влияет на оформление приложения: развёрнутость
/// меняет геометрию «шелла», фокус — вид кнопок титлбара.
///
/// Фреймворк держит его в сигнале, переданном через
/// [`AppBuilder::with_window_state`](crate::app::AppBuilder::with_window_state);
/// те же флаги доступны в MSS как псевдоклассы `:window-maximized`,
/// `:window-fullscreen`, `:window-focused`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowState {
    pub maximized: bool,
    pub fullscreen: bool,
    pub focused: bool,
}
