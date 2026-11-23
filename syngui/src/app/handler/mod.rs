mod android;
mod app_window;
mod appearance;
#[cfg(target_os = "ios")]
mod ios;
mod lifecycle;
pub(in crate::app) mod render;
mod styling;
mod windows;

pub(in crate::app) use app_window::AppWindow;

use super::builder::AppBuilder;
use super::window_handle::{WindowBuild, WindowHandle};
#[cfg(target_arch = "wasm32")]
use crate::gpu::GpuContext;
use crate::gpu::GpuShared;
use crate::input::Modifiers;
use crate::mss::StyleEngine;
use crate::widget::Widget;
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

/// Поле под экранной клавиатурой: шаг 1 — прокрутка ближайшего
/// скролл-контейнера, шаг 2 (кадром позже, по свежим позициям) — сдвиг
/// содержимого `ElementTree::keyboard_pan`, если поле всё ещё не видно.
#[cfg(any(target_os = "android", target_arch = "wasm32"))]
#[derive(Clone, Copy)]
pub(super) struct KeyboardReveal {
    pub element: crate::widget::ElementId,
    pub scrolled: bool,
}

/// IMPORTANT: Field order matters for drop safety!
pub(super) type RootFactory =
    std::sync::Arc<dyn Fn(&mut crate::widget::BuildContext) -> Box<dyn Widget> + 'static>;

pub(super) struct AppHandler {
    pub(super) config: AppBuilder,
    pub(super) root_factory: RootFactory,
    /// Активное окно: главное, а на время событий и кадра дополнительного —
    /// оно (см. [`AppHandler::with_window`]).
    pub(super) win: AppWindow,
    /// Таблица стилей приложения — общая для всех окон.
    pub(super) style_engine: StyleEngine,
    /// Общий GPU (instance, adapter, device, queue); поверхности — у окон.
    pub(super) gpu: Option<GpuShared>,
    pub(super) last_frame_time: Instant,
    /// Запрос «разбудить цикл через N» из update(): анимация ждёт окна
    /// frame-limit'а, либо Android-мост ввода ждёт следующего опроса.
    /// Потребляется в about_to_wait → ControlFlow::WaitUntil.
    pub(super) wakeup_after: Option<std::time::Duration>,
    pub(super) modifiers: Modifiers,
    /// Shift по xkb (`ModifiersChanged`) и физически зажатые Shift (бит 0 —
    /// левый, 1 — правый). При переключении раскладки по Ctrl+Shift xkb
    /// делает из Shift `ISO_Next_Group`, и модификатор не выставляется —
    /// Ctrl+Shift+T приходил бы как Ctrl+T.
    pub(super) xkb_shift: bool,
    pub(super) held_shift: u8,
    /// Нажатые клавиши (с аппаратным кодом): при потере фокуса окна их отпускания не придут — досылаются
    /// KeyUp (иначе у LiveView/ВМ залипает Alt после Alt+Tab, у игр — клавиши движения).
    pub(super) held_keys: Vec<(crate::input::Key, Option<u32>)>,
    pub(super) current_theme_is_dark: bool,
    pub(super) current_dynamic_theme: String,
    /// Порог двойного клика, разрешённый из настройки ОС/DE на старте
    /// (или override из билдера). См. [`crate::input::resolve_double_click_interval`].
    pub(super) double_click_interval: std::time::Duration,
    pub(super) app_start_time: Instant,
    /// Дополнительные окна, пока не активны.
    pub(super) secondary_windows: std::collections::HashMap<winit::window::WindowId, AppWindow>,
    /// Окна, ждущие готовности GPU (`add_window`, ранний `open_window`).
    pub(super) pending_windows: Vec<(WindowHandle, super::builder::WindowConfig, WindowBuild)>,
    pub(super) sticky_threshold: Option<f32>,
    pub(super) sticky_groups: Vec<std::collections::HashSet<winit::window::WindowId>>,
    pub(super) window_positions: std::collections::HashMap<winit::window::WindowId, (i32, i32)>,
    #[cfg(target_os = "android")]
    pub(super) android_app: Option<android_activity::AndroidApp>,
    /// Окно и частота, уже отданные `ANativeWindow_setFrameRate`.
    #[cfg(target_os = "android")]
    pub(super) applied_window_fps: Option<(usize, u32)>,
    #[cfg(target_os = "android")]
    pub(super) keyboard_height: f32,
    #[cfg(target_os = "android")]
    pub(super) keyboard_shown: bool,
    #[cfg(target_os = "android")]
    pub(super) composing_len: usize,
    /// Фокусное поле, которое нужно показать над экранной клавиатурой
    /// (см. `reveal_focused_under_keyboard`).
    #[cfg(any(target_os = "android", target_arch = "wasm32"))]
    pub(super) keyboard_reveal: Option<KeyboardReveal>,
    /// Веб: высота области под клавиатурой, для которой поле уже показывали;
    /// смена высоты (открытие, шаги анимации, поворот) показывает заново.
    #[cfg(target_arch = "wasm32")]
    pub(super) web_reveal_height: f32,
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

    pub(super) event_loop_proxy:
        Option<winit::event_loop::EventLoopProxy<super::user_event::SynGuiUserEvent>>,

    #[cfg(all(
        feature = "tray",
        not(target_arch = "wasm32"),
        not(target_os = "android")
    ))]
    pub(super) tray: Option<super::tray::TrayManager>,

    #[cfg(all(
        feature = "single-instance",
        not(target_arch = "wasm32"),
        not(target_os = "android")
    ))]
    pub(super) single_instance: Option<super::single_instance::SingleInstanceLock>,

    /// Слежение за системным оформлением; жив, пока живо приложение.
    pub(super) appearance_watcher: Option<crate::appearance::AppearanceWatcher>,
    /// Последняя применённая пара (настройка фона, размер окна) — фильтр
    /// повторных запросов к композитору при resize.
    pub(super) last_backdrop: Option<(crate::window::BackdropConfig, (u32, u32))>,

    pub(super) main_window_visible: bool,

    pub(super) pending_show: bool,
    /// Первый resumed при `start_hidden` уже пропущен.
    pub(super) started_hidden: bool,

    /// Android: активити в фоне (surface уничтожен). winit в этом состоянии
    /// игнорирует wake-up'ы EventLoopProxy («ignore wake ups while suspended»),
    /// поэтому цикл переводится на медленный пульс WaitUntil — см. about_to_wait.
    #[cfg(target_os = "android")]
    pub(super) android_suspended: bool,

    #[cfg(all(feature = "wayland-dnd", target_os = "linux"))]
    pub(super) wayland_dnd_handle: Option<std::thread::JoinHandle<()>>,

    /// Поток пера (zwp_tablet_v2); автомат штриха — у каждого окна свой.
    #[cfg(all(feature = "pen", target_os = "linux"))]
    pub(super) wayland_pen_handle: Option<std::thread::JoinHandle<()>>,
}

pub(super) fn is_wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
}

impl AppHandler {
    /// Таймер долгого нажатия: сработал — кадр; ещё ждём — проснуться к
    /// сроку (`wakeup_after`), даже если анимаций нет.
    pub(in crate::app) fn poll_touch(&mut self) {
        let Some(root_id) = self.win.root_id else {
            return;
        };
        let tree = &mut self.win.tree;
        let fired = self.win.touch.poll(&mut |e| tree.handle_event(root_id, e));
        if fired {
            if let Some(window) = &self.win.window {
                crate::perf::redraw_from(file!(), line!());
                window.request_redraw();
            }
        }
        if let Some(deadline) = self.win.touch.deadline() {
            let delay = deadline.saturating_duration_since(web_time::Instant::now());
            self.wakeup_after = Some(self.wakeup_after.map_or(delay, |d| d.min(delay)));
        }
    }

    /// Отступы корневого layout: safe area, либо нули в режиме edge-to-edge
    /// (корень на всё окно). Заодно публикует настоящую safe area для
    /// [`crate::viewport::safe_area`].
    pub(in crate::app) fn layout_insets(&self) -> crate::core::EdgeInsets {
        crate::viewport::publish_safe_area(self.win.tree.safe_area);
        if self.config.edge_to_edge {
            crate::core::EdgeInsets::zero()
        } else {
            self.win.tree.safe_area
        }
    }

    pub(super) fn new(
        mut config: AppBuilder,
        root_factory: RootFactory,
        style_engine: StyleEngine,
        initial_is_dark: bool,
    ) -> Self {
        let pending_windows: Vec<_> = std::mem::take(&mut config.extra_windows)
            .into_iter()
            .map(|(wc, build)| (super::window_handle::register(&wc), wc, build))
            .collect();
        super::window_handle::register_main(config.window_state);
        // Живые стили (`with_live_styles`) уже загружены как базовые.
        let initial_dynamic = match (config.dynamic_replaces_base, config.dynamic_theme_mss) {
            (true, Some(sig)) => sig.get_untracked(),
            _ => String::new(),
        };
        let sticky_threshold = config.sticky_threshold;
        // Масштаб интерфейса — до создания окна: первый layout уже должен
        // считать логический вьюпорт с ним.
        crate::scale::set_initial_ui_scale(config.ui_scale);
        let double_click_interval = config
            .double_click_interval
            .unwrap_or_else(crate::input::resolve_double_click_interval);
        log::debug!(
            "double-click interval resolved to {:?}",
            double_click_interval
        );

        // Начальный clear-color = фон стартовой темы. Иначе при запуске сразу в
        // тёмной теме (theme_state==current) блок смены не срабатывает и полоса
        // статус-бара осталась бы дефолтной светлой. Прозрачное окно (своя рамка со
        // скруглёнными углами, CSD) фон рисует само — его clear-color не трогаем, иначе
        // углы за скруглением закрашивались фоном темы.
        if !config.transparent {
            let init_ss = if initial_is_dark {
                config.dark_stylesheet.as_ref()
            } else {
                config.light_stylesheet.as_ref()
            };
            if let Some(bg) = init_ss.and_then(render::parse_theme_bg) {
                config.background_color = bg;
            }
        }

        let mut win = AppWindow::new(
            WindowHandle::MAIN,
            config.background_color,
            config.transparent,
            config.debug_overlay,
            config.devtools,
        );
        win.width = config.width;
        win.height = config.height;

        Self {
            config,
            root_factory,
            win,
            style_engine,
            gpu: None,
            last_frame_time: Instant::now(),
            wakeup_after: None,
            modifiers: Modifiers::empty(),
            xkb_shift: false,
            held_shift: 0,
            held_keys: Vec::new(),
            current_theme_is_dark: initial_is_dark,
            current_dynamic_theme: initial_dynamic,
            double_click_interval,
            app_start_time: Instant::now(),
            #[cfg(target_os = "android")]
            android_app: None,
            #[cfg(target_os = "android")]
            applied_window_fps: None,
            #[cfg(target_os = "android")]
            keyboard_height: 0.0,
            #[cfg(target_os = "android")]
            keyboard_shown: false,
            #[cfg(target_os = "android")]
            composing_len: 0,
            #[cfg(any(target_os = "android", target_arch = "wasm32"))]
            keyboard_reveal: None,
            #[cfg(target_arch = "wasm32")]
            web_reveal_height: 0.0,
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
            secondary_windows: std::collections::HashMap::new(),
            pending_windows,
            sticky_threshold,
            sticky_groups: Vec::new(),
            window_positions: std::collections::HashMap::new(),

            event_loop_proxy: None,
            #[cfg(all(
                feature = "tray",
                not(target_arch = "wasm32"),
                not(target_os = "android")
            ))]
            tray: None,
            #[cfg(all(
                feature = "single-instance",
                not(target_arch = "wasm32"),
                not(target_os = "android")
            ))]
            single_instance: None,
            appearance_watcher: None,
            last_backdrop: None,
            main_window_visible: true,
            pending_show: false,
            started_hidden: false,
            #[cfg(target_os = "android")]
            android_suspended: false,
            #[cfg(all(feature = "wayland-dnd", target_os = "linux"))]
            wayland_dnd_handle: None,
            #[cfg(all(feature = "pen", target_os = "linux"))]
            wayland_pen_handle: None,
        }
    }

    /// Даёт приложению отменить закрытие (несохранённое, живые процессы).
    /// Без guard'а — закрываем, как раньше.
    pub(super) fn close_allowed(&self) -> bool {
        match self.config.close_guard.as_ref() {
            Some(guard) => guard(),
            None => true,
        }
    }

    pub(super) fn should_hide_on_close(&self) -> bool {
        match super::super::HIDE_ON_CLOSE.load(std::sync::atomic::Ordering::Relaxed) {
            1 => return false,
            2 => return true,
            _ => {}
        }
        if self.config.hide_on_close {
            return true;
        }
        #[cfg(all(
            feature = "tray",
            not(target_arch = "wasm32"),
            not(target_os = "android")
        ))]
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
        self.win.last_frame_sig = None;
        // Wayland не даёт программе развернуть свёрнутое окно или поднять
        // его поверх других: `set_visible`/`focus` у свёрнутого окна ничего
        // не делают, и «Показать окно» из трея или второй запуск оставляли
        // его свёрнутым. Окно без фокуса пересоздаётся — новое появляется
        // поверх, как после сворачивания в трей.
        if is_wayland_session()
            && self
                .win
                .window
                .as_ref()
                .is_some_and(|w| !w.winit_window().has_focus())
        {
            self.destroy_window_and_gpu();
        }
        if self.win.window.is_some() {
            if let Some(window) = self.win.window.as_ref() {
                window.set_visible(true);
                window.focus();
                crate::perf::redraw_from(file!(), line!());
                window.request_redraw();
            }
            self.main_window_visible = true;
            super::super::MAIN_WINDOW_VISIBLE.store(true, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.pending_show = true;
        }
    }

    pub(super) fn hide_main_window(&mut self) {
        if is_wayland_session() {
            self.destroy_window_and_gpu();
        } else if let Some(window) = self.win.window.as_ref() {
            window.set_visible(false);
        }
        self.main_window_visible = false;
        super::super::MAIN_WINDOW_VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    pub(super) fn destroy_window_and_gpu(&mut self) {
        self.win.last_frame_sig = None;
        self.win.tree = crate::widget::ElementTree::new();
        self.win.root_id = None;
        self.win.surface_valid = false;

        #[cfg(feature = "winit")]
        crate::signal::clear_window();
        #[cfg(feature = "winit")]
        crate::async_runtime::clear_async_window();

        self.win.renderer.take();
        self.win.surface.take();
        // Дополнительные окна рисуют на том же GPU — при них он остаётся,
        // а новое главное окно получит поверхность на нём.
        if self.secondary_windows.is_empty() {
            self.gpu.take();
        }
        self.win.window.take();

        self.win.focus_manager = crate::a11y::FocusManager::new();
        #[cfg(feature = "accessibility")]
        {
            self.win.a11y_tree =
                crate::a11y::A11yTree::new(Box::new(crate::a11y::AccessKitAdapter::new()));
            self.win.accesskit_adapter = None;
        }
        #[cfg(not(feature = "accessibility"))]
        {
            self.win.a11y_tree = crate::a11y::A11yTree::new(Box::new(crate::a11y::LoggingAdapter));
        }
    }

    pub(super) fn toggle_main_window_visibility(&mut self) {
        if self.main_window_visible {
            self.hide_main_window();
        } else {
            self.show_main_window();
        }
    }

    /// `None` — указатель скрыт.
    pub(super) fn map_cursor_icon(
        cursor: crate::input::CursorIcon,
    ) -> Option<winit::window::CursorIcon> {
        Some(match cursor {
            crate::input::CursorIcon::None | crate::input::CursorIcon::Custom(_) => return None,
            crate::input::CursorIcon::Default => winit::window::CursorIcon::Default,
            crate::input::CursorIcon::Pointer => winit::window::CursorIcon::Pointer,
            crate::input::CursorIcon::Text => winit::window::CursorIcon::Text,
            crate::input::CursorIcon::Grab => winit::window::CursorIcon::Grab,
            crate::input::CursorIcon::Grabbing => winit::window::CursorIcon::Grabbing,
            crate::input::CursorIcon::Move => winit::window::CursorIcon::Move,
            crate::input::CursorIcon::NotAllowed => winit::window::CursorIcon::NotAllowed,
            crate::input::CursorIcon::Crosshair => winit::window::CursorIcon::Crosshair,
            crate::input::CursorIcon::ColResize => winit::window::CursorIcon::ColResize,
            crate::input::CursorIcon::RowResize => winit::window::CursorIcon::RowResize,
            crate::input::CursorIcon::NwResize => winit::window::CursorIcon::NwResize,
            crate::input::CursorIcon::NeResize => winit::window::CursorIcon::NeResize,
            crate::input::CursorIcon::SeResize => winit::window::CursorIcon::SeResize,
            crate::input::CursorIcon::SwResize => winit::window::CursorIcon::SwResize,
            crate::input::CursorIcon::NResize => winit::window::CursorIcon::NResize,
            crate::input::CursorIcon::EResize => winit::window::CursorIcon::EResize,
            crate::input::CursorIcon::SResize => winit::window::CursorIcon::SResize,
            crate::input::CursorIcon::WResize => winit::window::CursorIcon::WResize,
        })
    }

    /// Системный DPI-масштаб окна, умноженный на пользовательский масштаб
    /// интерфейса. Именно он переводит логические единицы в физические
    /// пиксели — и в рендере, и при разборе координат ввода.
    pub(in crate::app) fn effective_scale_factor(&self) -> f64 {
        self.win.system_scale_factor * crate::scale::ui_scale() as f64
    }

    /// Подобрать пользовательский масштаб под [`AppBuilder::design_size`]:
    /// логический вьюпорт (физика / системный DPI) делится на дизайнерское
    /// разрешение, берётся меньшее отношение сторон. Вызывается при создании
    /// окна, resize и смене системного DPI; применяется следующим кадром
    /// через [`AppHandler::apply_ui_scale`].
    pub(in crate::app) fn apply_design_scale(&mut self) {
        // Дизайнерское разрешение — по главному окну; масштаб у приложения
        // один на все окна.
        if !self.win.is_main()
            || self.win.width == 0
            || self.win.height == 0
            || self.win.system_scale_factor <= 0.0
        {
            return;
        }
        let portrait = self.win.height > self.win.width;
        let size = match (portrait, self.config.design_size_portrait) {
            (true, Some(p)) => Some(p),
            _ => self.config.design_size,
        };
        let Some((dw, dh)) = size else {
            return;
        };
        let lw = self.win.width as f64 / self.win.system_scale_factor;
        let lh = self.win.height as f64 / self.win.system_scale_factor;
        let scale = if crate::scale::design_size_paused() {
            1.0
        } else {
            (lw / dw as f64).min(lh / dh as f64) as f32
        };
        if (scale - crate::scale::ui_scale()).abs() > 1e-4 {
            log::info!(
                "design_size {dw}×{dh}: логический экран {lw:.0}×{lh:.0} → ui_scale {scale:.3}"
            );
            crate::scale::set_ui_scale(scale);
        }
    }

    /// Применить сменившийся масштаб интерфейса: перенастроить рендерер и
    /// атлас шрифта под новый размер физического пикселя и запросить полный
    /// пересчёт раскладки. Сам layout и публикация вьюпорта происходят
    /// дальше по кадру — в [`AppHandler::render`].
    pub(in crate::app) fn apply_ui_scale(&mut self) {
        let effective = self.effective_scale_factor();
        if (effective - self.win.scale_factor).abs() < 1e-6 {
            return;
        }
        self.win.scale_factor = effective;

        let (phys_w, phys_h) = (self.win.width, self.win.height);
        if let Some(renderer) = self.win.renderer.as_mut() {
            let logical_w = (phys_w as f64 / effective).max(1.0) as u32;
            let logical_h = (phys_h as f64 / effective).max(1.0) as u32;
            if let Some(gpu) = self.gpu.as_ref() {
                renderer.resize(&gpu.device, phys_w, phys_h, logical_w, logical_h);
            }
            if let Ok(mut atlas) = renderer.font_atlas.lock() {
                atlas.set_scale_factor(effective as f32);
            }
        }

        // Размер глифа в физических пикселях изменился — закэшированные
        // измерения текста и раскладка целиком считаются заново.
        self.win
            .tree
            .mark_all_dirty(crate::widget::DirtyFlags::LAYOUT);
        self.win.tree.force_full_measure = true;

        if let Some(window) = &self.win.window {
            crate::perf::redraw_from(file!(), line!());
            window.request_redraw();
        }
    }
}

// Обработчики AccessKit зовутся с его собственного потока (AT-SPI на Linux):
// дерево и элементы живут в цикле событий, поэтому запросы уходят туда через
// прокси, а не выполняются здесь.
#[cfg(feature = "accessibility")]
pub(super) struct SynGuiActivationHandler(
    pub(super) Option<winit::event_loop::EventLoopProxy<super::user_event::SynGuiUserEvent>>,
);

#[cfg(feature = "accessibility")]
impl accesskit::ActivationHandler for SynGuiActivationHandler {
    fn request_initial_tree(&mut self) -> Option<accesskit::TreeUpdate> {
        // Синхронно дерево отсюда не собрать — отдаём его ближайшим кадром.
        if let Some(p) = &self.0 {
            let _ = p.send_event(super::user_event::SynGuiUserEvent::A11yActivated);
        }
        None
    }
}

#[cfg(feature = "accessibility")]
pub(super) struct SynGuiActionHandler(
    pub(super) Option<winit::event_loop::EventLoopProxy<super::user_event::SynGuiUserEvent>>,
);

#[cfg(feature = "accessibility")]
impl accesskit::ActionHandler for SynGuiActionHandler {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        if let Some(p) = &self.0 {
            let _ = p.send_event(super::user_event::SynGuiUserEvent::A11yAction(request));
        }
    }
}

#[cfg(feature = "accessibility")]
pub(super) struct SynGuiDeactivationHandler(
    pub(super) Option<winit::event_loop::EventLoopProxy<super::user_event::SynGuiUserEvent>>,
);

#[cfg(feature = "accessibility")]
impl accesskit::DeactivationHandler for SynGuiDeactivationHandler {
    fn deactivate_accessibility(&mut self) {
        if let Some(p) = &self.0 {
            let _ = p.send_event(super::user_event::SynGuiUserEvent::A11yDeactivated);
        }
    }
}
