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
pub(crate) mod web_history;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web_keys;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web_text_agent;

pub use builder::{AppBuilder, WindowConfig};

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
static CONTROL_PROXY: std::sync::Mutex<
    Option<winit::event_loop::EventLoopProxy<user_event::SynGuiUserEvent>>,
> = std::sync::Mutex::new(None);

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub(crate) fn set_control_proxy(
    proxy: winit::event_loop::EventLoopProxy<user_event::SynGuiUserEvent>,
) {
    if let Ok(mut slot) = CONTROL_PROXY.lock() {
        *slot = Some(proxy);
    }
}

fn send_control(_event: user_event::SynGuiUserEvent) -> bool {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    if let Some(proxy) = CONTROL_PROXY.lock().ok().and_then(|g| g.clone()) {
        return proxy.send_event(_event).is_ok();
    }
    false
}

/// Показать главное окно (создать, если оно спрятано или ещё не создавалось)
/// и поднять его. Можно звать из любого потока; `false` — цикл событий ещё
/// не запущен.
pub fn show_main_window() -> bool {
    send_control(user_event::SynGuiUserEvent::Activate)
}

/// Спрятать главное окно; процесс и состояние вне дерева виджетов живут.
pub fn hide_main_window() -> bool {
    send_control(user_event::SynGuiUserEvent::TrayHide)
}

/// Видно ли сейчас главное окно.
pub fn main_window_visible() -> bool {
    MAIN_WINDOW_VISIBLE.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) static MAIN_WINDOW_VISIBLE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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

/// Телевизор ли устройство (`ui_mode = television` в конфигурации Android).
/// Раскладку для ТВ смотрят с дивана, и то же дизайнерское разрешение
/// ([`AppBuilder::design_size`]) на телефоне в руке даёт вдвое-втрое более
/// мелкий интерфейс — приложению нужно выбрать своё.
#[cfg(target_os = "android")]
pub fn is_television(app: &AndroidApp) -> bool {
    use android_activity::ndk::configuration::UiModeType;
    app.config().ui_mode_type() == UiModeType::Television
}

/// Есть ли сенсорный экран (Linux): устройство ввода с `INPUT_PROP_DIRECT`
/// и мультитач-координатами `ABS_MT_POSITION_X`. Тачпад ноутбука (без
/// DIRECT) не считается. Телефону с Linux нужна раскладка под палец —
/// прокрутка перетаскиванием вместо фокуса пульта или колеса мыши.
#[cfg(target_os = "linux")]
pub fn has_touchscreen() -> bool {
    const INPUT_PROP_DIRECT: usize = 0x01;
    const ABS_MT_POSITION_X: usize = 0x35;
    // Битовая карта sysfs: слова unsigned long в hex через пробел, старшее первым.
    fn bit(text: &str, n: usize) -> bool {
        let bits = usize::BITS as usize;
        text.split_whitespace()
            .rev()
            .nth(n / bits)
            .and_then(|w| usize::from_str_radix(w, 16).ok())
            .is_some_and(|w| w >> (n % bits) & 1 == 1)
    }
    let Ok(dir) = std::fs::read_dir("/sys/class/input") else {
        return false;
    };
    dir.flatten().any(|e| {
        let p = e.path();
        let read = |f: &str| std::fs::read_to_string(p.join(f)).unwrap_or_default();
        bit(&read("properties"), INPUT_PROP_DIRECT)
            && bit(&read("capabilities/abs"), ABS_MT_POSITION_X)
    })
}

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
