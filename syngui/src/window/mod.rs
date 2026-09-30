pub mod backdrop;
#[cfg(all(target_os = "linux", feature = "system-blur"))]
pub mod ext_background_effect;
pub mod window;

pub use backdrop::{set_backdrop, BackdropConfig, BackdropContrast, BackdropRegion};
pub use window::{Window, WindowBuilder, WindowEvent};

static PENDING_SIZE: std::sync::Mutex<Option<(u32, u32)>> = std::sync::Mutex::new(None);

/// Попросить оконную систему изменить размер главного окна (логические
/// пиксели) — из любого потока. Применяется в цикле событий; композитор
/// может не согласиться (развёрнутое окно, тайлинг). Так окно следует за
/// содержимым, например за поворотом экрана транслируемого устройства.
pub fn request_size(width: u32, height: u32) {
    if let Ok(mut p) = PENDING_SIZE.lock() {
        *p = Some((width.max(1), height.max(1)));
    }
    crate::async_runtime::run_on_main_thread(|| {});
}

pub(crate) fn take_pending_size() -> Option<(u32, u32)> {
    PENDING_SIZE.lock().ok()?.take()
}

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
