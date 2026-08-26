mod android;
mod appearance;
mod styling;
mod windows;
mod lifecycle;
mod render;

use crate::a11y::{A11yTree, FocusManager};
#[cfg(not(feature = "accessibility"))]
use crate::a11y::LoggingAdapter;
#[cfg(feature = "accessibility")]
use crate::a11y::AccessKitAdapter;
use crate::core::Point;
use crate::gpu::{GpuContext, WindowSurface, Renderer};
use crate::input::{CursorIcon, Modifiers};
use crate::mss::StyleEngine;
use crate::render::DisplayList;
use crate::widget::{ElementTree, Widget};
use crate::window::Window;
use std::sync::Arc;
use web_time::Instant;
use super::builder::AppBuilder;

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

pub(super) struct SecondaryWindow {
    #[allow(dead_code)]
    pub name: String,
    pub window: Arc<Window>,
    pub surface: WindowSurface,
    pub renderer: Renderer,
    pub tree: ElementTree,
    pub style_engine: StyleEngine,
    pub root_id: Option<crate::widget::ElementId>,
    pub display_list: DisplayList,
    pub cursor_position: Point,
    pub current_cursor: CursorIcon,
    pub last_click_time: Option<Instant>,
    pub last_click_pos: Option<Point>,
    pub double_click_interval: std::time::Duration,
    #[allow(dead_code)]
    pub focus_manager: FocusManager,
    pub scale_factor: f64,
    pub width: u32,
    pub height: u32,
}

/// IMPORTANT: Field order matters for drop safety!
pub(super) type RootFactory =
    std::sync::Arc<dyn Fn(&mut crate::widget::BuildContext) -> Box<dyn Widget> + 'static>;

pub(super) struct AppHandler {
    pub(super) config: AppBuilder,
    pub(super) root_factory: RootFactory,
    pub(super) tree: ElementTree,
    pub(super) style_engine: StyleEngine,
    pub(super) renderer: Option<Renderer>,
    pub(super) gpu: Option<GpuContext>,
    pub(super) window: Option<Arc<Window>>,
    pub(super) root_id: Option<crate::widget::ElementId>,
    pub(super) last_frame_time: Instant,
    pub(super) last_paced_redraw: Option<Instant>,
    pub(super) cursor_position: Point,
    pub(super) scale_factor: f64,
    pub(super) focus_manager: FocusManager,
    pub(super) a11y_tree: A11yTree,
    pub(super) modifiers: Modifiers,
    pub(super) a11y_dirty: bool,
    #[cfg(feature = "accessibility")]
    pub(super) accesskit_adapter: Option<accesskit_winit::Adapter>,
    pub(super) current_theme_is_dark: bool,
    pub(super) current_dynamic_theme: String,
    pub(super) current_cursor: CursorIcon,
    pub(super) last_click_time: Option<Instant>,
    pub(super) last_click_pos: Option<Point>,
    /// Порог двойного клика, разрешённый из настройки ОС/DE на старте
    /// (или override из билдера). См. [`crate::input::resolve_double_click_interval`].
    pub(super) double_click_interval: std::time::Duration,
    #[cfg(feature = "clipboard")]
    pub(super) clipboard: Option<std::sync::Arc<crate::core::sync::Mutex<arboard::Clipboard>>>,
    pub(super) debug_overlay: Option<crate::debug::DebugOverlay>,
    pub(super) devtools: Option<crate::devtools::DevTools>,
    pub(super) app_start_time: Instant,
    pub(super) display_list: DisplayList,
    pub(super) surface_valid: bool,
    pub(super) main_window_id: Option<winit::window::WindowId>,
    pub(super) secondary_windows: std::collections::HashMap<winit::window::WindowId, SecondaryWindow>,
    pub(super) pending_windows: Vec<(super::builder::WindowConfig, Box<dyn FnOnce(&mut crate::widget::BuildContext) -> Box<dyn Widget>>)>,
    pub(super) sticky_threshold: Option<f32>,
    pub(super) sticky_groups: Vec<std::collections::HashSet<winit::window::WindowId>>,
    pub(super) window_positions: std::collections::HashMap<winit::window::WindowId, (i32, i32)>,
    #[cfg(target_os = "android")]
    pub(super) android_app: Option<android_activity::AndroidApp>,
    #[cfg(target_os = "android")]
    pub(super) keyboard_height: f32,
    #[cfg(target_os = "android")]
    pub(super) keyboard_shown: bool,
    #[cfg(target_os = "android")]
    pub(super) composing_len: usize,
    #[cfg(target_os = "android")]
    pub(super) pending_scroll_element: Option<crate::widget::ElementId>,
    #[cfg(target_arch = "wasm32")]
    pub(super) pending_gpu: Rc<RefCell<Option<GpuContext>>>,
    #[cfg(target_arch = "wasm32")]
    pub(super) pending_font: Rc<RefCell<Option<Vec<u8>>>>,
    #[cfg(target_arch = "wasm32")]
    pub(super) pending_emoji_font: Rc<RefCell<Option<Vec<u8>>>>,
    #[cfg(target_arch = "wasm32")]
    pub(super) pending_fallback_fonts: Rc<RefCell<Vec<Vec<u8>>>>,
    #[cfg(target_arch = "wasm32")]
    pub(super) wasm_font_ready: bool,

    pub(super) event_loop_proxy: Option<winit::event_loop::EventLoopProxy<super::user_event::SynGuiUserEvent>>,

    #[cfg(all(feature = "tray", not(target_arch = "wasm32"), not(target_os = "android")))]
    pub(super) tray: Option<super::tray::TrayManager>,

    #[cfg(all(feature = "single-instance", not(target_arch = "wasm32"), not(target_os = "android")))]
    pub(super) single_instance: Option<super::single_instance::SingleInstanceLock>,

    /// Слежение за системным оформлением; жив, пока живо приложение.
    pub(super) appearance_watcher: Option<crate::appearance::AppearanceWatcher>,
    /// Последняя применённая пара (настройка фона, размер окна) — фильтр
    /// повторных запросов к композитору при resize.
    pub(super) last_backdrop: Option<(crate::window::BackdropConfig, (u32, u32))>,

    pub(super) main_window_visible: bool,

    pub(super) pending_show: bool,

    /// Android: активити в фоне (surface уничтожен). winit в этом состоянии
    /// игнорирует wake-up'ы EventLoopProxy («ignore wake ups while suspended»),
    /// поэтому цикл переводится на медленный пульс WaitUntil — см. about_to_wait.
    #[cfg(target_os = "android")]
    pub(super) android_suspended: bool,

    /// Отложенный tap-синтез для тача: (id первого пальца, точка старта,
    /// превышен ли slop). Клик синтезируется на отпускании и только если палец
    /// не сдвинулся — иначе скролл списка «проваливался» в строку под пальцем.
    pub(super) touch_tap: Option<(u64, crate::core::Point, bool)>,

    #[cfg(all(feature = "wayland-dnd", target_os = "linux"))]
    pub(super) wayland_dnd_handle: Option<std::thread::JoinHandle<()>>,
}

pub(super) fn is_wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
}

impl AppHandler {
    pub(super) fn new(mut config: AppBuilder, root_factory: RootFactory, style_engine: StyleEngine, initial_is_dark: bool) -> Self {
        let pending_windows = std::mem::take(&mut config.extra_windows);
        let sticky_threshold = config.sticky_threshold;
        let double_click_interval = config
            .double_click_interval
            .unwrap_or_else(crate::input::resolve_double_click_interval);
        log::debug!("double-click interval resolved to {:?}", double_click_interval);
        let debug_overlay = if config.debug_overlay {
            Some(crate::debug::DebugOverlay::new())
        } else {
            None
        };

        // Начальный clear-color = фон стартовой темы. Иначе при запуске сразу в
        // тёмной теме (theme_state==current) блок смены не срабатывает и полоса
        // статус-бара осталась бы дефолтной светлой.
        {
            let init_ss = if initial_is_dark {
                config.dark_stylesheet.as_ref()
            } else {
                config.light_stylesheet.as_ref()
            };
            if let Some(bg) = init_ss.and_then(render::parse_theme_bg) {
                config.background_color = bg;
            }
        }

        let devtools = {
            let mut dt = crate::devtools::DevTools::new();
            if config.devtools { dt.toggle(); }
            Some(dt)
        };

        Self {
            config,
            root_factory,
            renderer: None,
            gpu: None,
            window: None,
            tree: ElementTree::new(),
            style_engine,
            root_id: None,
            last_frame_time: Instant::now(),
            last_paced_redraw: None,
            cursor_position: Point::zero(),
            scale_factor: 1.0,
            focus_manager: FocusManager::new(),
            #[cfg(feature = "accessibility")]
            a11y_tree: A11yTree::new(Box::new(AccessKitAdapter::new())),
            #[cfg(not(feature = "accessibility"))]
            a11y_tree: A11yTree::new(Box::new(LoggingAdapter)),
            modifiers: Modifiers::empty(),
            a11y_dirty: true,
            #[cfg(feature = "accessibility")]
            accesskit_adapter: None,
            current_theme_is_dark: initial_is_dark,
            current_dynamic_theme: String::new(),
            current_cursor: CursorIcon::Default,
            last_click_time: None,
            last_click_pos: None,
            double_click_interval,
            #[cfg(feature = "clipboard")]
            clipboard: {
                match arboard::Clipboard::new() {
                    Ok(c) => {
                        Some(std::sync::Arc::new(crate::core::sync::Mutex::new(c)))
                    }
                    Err(e) => {
                        log::warn!("Failed to initialize clipboard: {}", e);
                        None
                    }
                }
            },
            debug_overlay,
            devtools,
            app_start_time: Instant::now(),
            display_list: DisplayList::new(),
            surface_valid: false,
            #[cfg(target_os = "android")]
            android_app: None,
            #[cfg(target_os = "android")]
            keyboard_height: 0.0,
            #[cfg(target_os = "android")]
            keyboard_shown: false,
            #[cfg(target_os = "android")]
            composing_len: 0,
            #[cfg(target_os = "android")]
            pending_scroll_element: None,
            #[cfg(target_arch = "wasm32")]
            pending_gpu: Rc::new(RefCell::new(None)),
            #[cfg(target_arch = "wasm32")]
            pending_font: Rc::new(RefCell::new(None)),
            #[cfg(target_arch = "wasm32")]
            pending_emoji_font: Rc::new(RefCell::new(None)),
            #[cfg(target_arch = "wasm32")]
            pending_fallback_fonts: Rc::new(RefCell::new(Vec::new())),
            #[cfg(target_arch = "wasm32")]
            wasm_font_ready: false,
            main_window_id: None,
            secondary_windows: std::collections::HashMap::new(),
            pending_windows,
            sticky_threshold,
            sticky_groups: Vec::new(),
            window_positions: std::collections::HashMap::new(),

            event_loop_proxy: None,
            #[cfg(all(feature = "tray", not(target_arch = "wasm32"), not(target_os = "android")))]
            tray: None,
            #[cfg(all(feature = "single-instance", not(target_arch = "wasm32"), not(target_os = "android")))]
            single_instance: None,
            appearance_watcher: None,
            last_backdrop: None,
            main_window_visible: true,
            pending_show: false,
            #[cfg(target_os = "android")]
            android_suspended: false,
            touch_tap: None,
            #[cfg(all(feature = "wayland-dnd", target_os = "linux"))]
            wayland_dnd_handle: None,
        }
    }

    pub(super) fn should_hide_on_close(&self) -> bool {
        #[cfg(all(feature = "tray", not(target_arch = "wasm32"), not(target_os = "android")))]
        {
            if self.tray.is_some() {
                if let Some(cfg) = self.config.tray_config.as_ref() {
                    return cfg.close_action_value() == super::tray::TrayCloseAction::HideToTray;
                }
            }
        }
        false
    }

    pub(super) fn show_main_window(&mut self) {
        if self.window.is_some() {
            if let Some(window) = self.window.as_ref() {
                window.set_visible(true);
                window.focus();
                window.request_redraw();
            }
            self.main_window_visible = true;
        } else {
            self.pending_show = true;
        }
    }

    pub(super) fn hide_main_window(&mut self) {
        if is_wayland_session() {
            self.destroy_window_and_gpu();
        } else if let Some(window) = self.window.as_ref() {
            window.set_visible(false);
        }
        self.main_window_visible = false;
    }

    pub(super) fn destroy_window_and_gpu(&mut self) {
        self.tree = crate::widget::ElementTree::new();
        self.root_id = None;
        self.surface_valid = false;

        #[cfg(feature = "winit")]
        crate::signal::clear_window();
        #[cfg(feature = "winit")]
        crate::async_runtime::clear_async_window();

        self.renderer.take();
        self.gpu.take();
        self.window.take();
        self.main_window_id = None;

        self.focus_manager = crate::a11y::FocusManager::new();
        #[cfg(feature = "accessibility")]
        {
            self.a11y_tree = crate::a11y::A11yTree::new(Box::new(crate::a11y::AccessKitAdapter::new()));
            self.accesskit_adapter = None;
        }
        #[cfg(not(feature = "accessibility"))]
        {
            self.a11y_tree = crate::a11y::A11yTree::new(Box::new(crate::a11y::LoggingAdapter));
        }
    }

    pub(super) fn toggle_main_window_visibility(&mut self) {
        if self.main_window_visible {
            self.hide_main_window();
        } else {
            self.show_main_window();
        }
    }

    pub(super) fn map_cursor_icon(cursor: CursorIcon) -> winit::window::CursorIcon {
        match cursor {
            CursorIcon::Default => winit::window::CursorIcon::Default,
            CursorIcon::Pointer => winit::window::CursorIcon::Pointer,
            CursorIcon::Text => winit::window::CursorIcon::Text,
            CursorIcon::Grab => winit::window::CursorIcon::Grab,
            CursorIcon::Grabbing => winit::window::CursorIcon::Grabbing,
            CursorIcon::Move => winit::window::CursorIcon::Move,
            CursorIcon::NotAllowed => winit::window::CursorIcon::NotAllowed,
            CursorIcon::Crosshair => winit::window::CursorIcon::Crosshair,
            CursorIcon::ColResize => winit::window::CursorIcon::ColResize,
            CursorIcon::RowResize => winit::window::CursorIcon::RowResize,
            CursorIcon::NwResize => winit::window::CursorIcon::NwResize,
            CursorIcon::NeResize => winit::window::CursorIcon::NeResize,
            CursorIcon::SeResize => winit::window::CursorIcon::SeResize,
            CursorIcon::SwResize => winit::window::CursorIcon::SwResize,
            CursorIcon::NResize => winit::window::CursorIcon::NResize,
            CursorIcon::EResize => winit::window::CursorIcon::EResize,
            CursorIcon::SResize => winit::window::CursorIcon::SResize,
            CursorIcon::WResize => winit::window::CursorIcon::WResize,
        }
    }

}

#[cfg(feature = "accessibility")]
pub(super) struct SynGuiActivationHandler;

#[cfg(feature = "accessibility")]
impl accesskit::ActivationHandler for SynGuiActivationHandler {
    fn request_initial_tree(&mut self) -> Option<accesskit::TreeUpdate> {
        None
    }
}

#[cfg(feature = "accessibility")]
pub(super) struct SynGuiActionHandler;

#[cfg(feature = "accessibility")]
impl accesskit::ActionHandler for SynGuiActionHandler {
    fn do_action(&mut self, _request: accesskit::ActionRequest) {
    }
}

#[cfg(feature = "accessibility")]
pub(super) struct SynGuiDeactivationHandler;

#[cfg(feature = "accessibility")]
impl accesskit::DeactivationHandler for SynGuiDeactivationHandler {
    fn deactivate_accessibility(&mut self) {
    }
}
