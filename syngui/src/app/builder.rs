use crate::core::Color;
use crate::mss::{StyleSheet, StyleEngine, load_stylesheet, parse_stylesheet_str};
use crate::signal::RwSignal;
use crate::widget::{BuildContext, Widget};
use super::{GpuBackend, GpuPowerPreference};

#[derive(Clone, Debug)]
pub struct WindowConfig {
    pub name: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub position: Option<(i32, i32)>,
    pub offset_from_main: Option<(i32, i32)>,
    pub decorations: bool,
}

impl WindowConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: String::new(),
            width: 400,
            height: 300,
            min_width: 100,
            min_height: 100,
            position: None,
            offset_from_main: None,
            decorations: true,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn min_size(mut self, width: u32, height: u32) -> Self {
        self.min_width = width;
        self.min_height = height;
        self
    }

    pub fn position(mut self, x: i32, y: i32) -> Self {
        self.position = Some((x, y));
        self
    }

    pub fn offset_from_main(mut self, dx: i32, dy: i32) -> Self {
        self.offset_from_main = Some((dx, dy));
        self
    }

    pub fn decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }
}

#[cfg(target_os = "android")]
pub use android_activity::AndroidApp;

pub struct AppBuilder {
    pub(super) title: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) min_width: u32,
    pub(super) min_height: u32,
    pub(super) background_color: Color,
    pub(super) vsync: bool,
    pub(super) frame_limit: u32,
    pub(super) stylesheet: Option<StyleSheet>,
    pub(super) light_stylesheet: Option<StyleSheet>,
    pub(super) dark_stylesheet: Option<StyleSheet>,
    pub(super) theme_state: Option<RwSignal<bool>>,
    pub(super) font_url: Option<String>,
    pub(super) emoji_font_url: Option<String>,
    pub(super) additional_stylesheets: Vec<StyleSheet>,
    pub(super) gpu_backend: GpuBackend,
    pub(super) gpu_power: GpuPowerPreference,
    pub(super) font_family: Option<String>,
    pub(super) icon_font_data: Option<&'static [u8]>,
    pub(super) maximized: bool,
    pub(super) debug_overlay: bool,
    pub(super) staging_belt: bool,
    pub(super) devtools: bool,
    pub(super) dynamic_theme_mss: Option<RwSignal<String>>,
    pub(super) system_appearance: Option<RwSignal<crate::appearance::SystemAppearance>>,
    pub(super) backdrop: Option<RwSignal<crate::window::BackdropConfig>>,
    pub(super) window_state: Option<RwSignal<crate::window::WindowState>>,
    #[cfg(feature = "splash")]
    pub(super) splash_config: Option<super::splash::SplashConfig>,
    #[cfg(target_os = "android")]
    pub(super) android_app: Option<AndroidApp>,
    pub(super) extra_windows: Vec<(WindowConfig, Box<dyn FnOnce(&mut BuildContext) -> Box<dyn Widget>>)>,
    pub(super) sticky_threshold: Option<f32>,
    /// Явный override интервала двойного клика. `None` = взять из настройки
    /// ОС/DE в рантайме ([`crate::input::resolve_double_click_interval`]).
    pub(super) double_click_interval: Option<std::time::Duration>,
    pub(super) decorations: bool,
    pub(super) transparent: bool,
    pub(super) fullscreen: bool,
    pub(super) width_ratio: Option<f32>,
    pub(super) height_ratio: Option<f32>,
    pub(super) window_icon_png: Option<Vec<u8>>,
    pub(super) tray_config: Option<crate::app::tray::TrayConfig>,
    pub(super) single_instance_id: Option<String>,
}

impl AppBuilder {
    pub fn new() -> Self {
        Self {
            title: "SYNGUI Application".to_string(),
            width: 1280,
            height: 720,
            min_width: 400,
            min_height: 300,
            background_color: Color::from_hex("#F9FAFB"),
            vsync: true,
            frame_limit: 0,
            stylesheet: None,
            light_stylesheet: None,
            dark_stylesheet: None,
            theme_state: None,
            font_url: None,
            emoji_font_url: None,
            font_family: None,
            icon_font_data: None,
            additional_stylesheets: Vec::new(),
            gpu_backend: GpuBackend::Auto,
            gpu_power: GpuPowerPreference::HighPerformance,
            maximized: false,
            debug_overlay: false,
            staging_belt: false,
            devtools: false,
            dynamic_theme_mss: None,
            system_appearance: None,
            backdrop: None,
            window_state: None,
            #[cfg(feature = "splash")]
            splash_config: None,
            #[cfg(target_os = "android")]
            android_app: None,
            extra_windows: Vec::new(),
            sticky_threshold: None,
            double_click_interval: None,
            decorations: true,
            transparent: false,
            fullscreen: false,
            width_ratio: None,
            height_ratio: None,
            window_icon_png: None,
            tray_config: None,
            single_instance_id: None,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn min_size(mut self, width: u32, height: u32) -> Self {
        self.min_width = width;
        self.min_height = height;
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background_color = color;
        self
    }

    pub fn vsync(mut self, enabled: bool) -> Self {
        self.vsync = enabled;
        self
    }

    pub fn frame_limit(mut self, fps: u32) -> Self {
        self.frame_limit = fps;
        self
    }

    pub fn staging_belt(mut self, enabled: bool) -> Self {
        self.staging_belt = enabled;
        self
    }

    pub fn with_styles<P: AsRef<std::path::Path>>(mut self, path: P) -> Self {
        match load_stylesheet(path) {
            Ok(stylesheet) => {
                self.stylesheet = Some(stylesheet);
            }
            Err(e) => {
                log::warn!("Failed to load stylesheet: {}", e);
            }
        }
        self
    }

    pub fn with_styles_str(mut self, content: &str) -> Self {
        match parse_stylesheet_str(content) {
            Ok(stylesheet) => {
                self.stylesheet = Some(stylesheet);
            }
            Err(e) => {
                log::warn!("Failed to parse stylesheet: {:?}", e);
            }
        }
        self
    }

    pub fn with_theme_styles(mut self, light: &str, dark: &str, theme: RwSignal<bool>) -> Self {
        match parse_stylesheet_str(light) {
            Ok(ss) => {
                self.light_stylesheet = Some(ss);
            }
            Err(e) => log::warn!("Failed to parse light stylesheet: {:?}", e),
        }
        match parse_stylesheet_str(dark) {
            Ok(ss) => {
                self.dark_stylesheet = Some(ss);
            }
            Err(e) => log::warn!("Failed to parse dark stylesheet: {:?}", e),
        }
        self.theme_state = Some(theme);
        self
    }

    pub fn with_additional_styles_str(mut self, content: &str) -> Self {
        match parse_stylesheet_str(content) {
            Ok(stylesheet) => {
                self.additional_stylesheets.push(stylesheet);
            }
            Err(e) => {
                log::warn!("Failed to parse additional stylesheet: {:?}", e);
            }
        }
        self
    }

    pub fn with_additional_styles<P: AsRef<std::path::Path>>(mut self, path: P) -> Self {
        match load_stylesheet(path) {
            Ok(stylesheet) => {
                self.additional_stylesheets.push(stylesheet);
            }
            Err(e) => {
                log::warn!("Failed to load additional stylesheet: {}", e);
            }
        }
        self
    }

    pub fn with_dynamic_theme(mut self, signal: RwSignal<String>) -> Self {
        self.dynamic_theme_mss = Some(signal);
        self
    }

    /// Держит сигнал в согласии с системным оформлением рабочего стола:
    /// светлая/тёмная схема, акцент DE, повышенный контраст, reduced-motion.
    ///
    /// Значение выставляется до построения дерева виджетов, поэтому приложение
    /// стартует уже в системной теме, а дальше обновляется на каждое изменение
    /// настроек (на Linux — сигнал портала, на Windows/macOS — winit
    /// `ThemeChanged`). Приложение обычно связывает его с
    /// [`with_dynamic_theme`](Self::with_dynamic_theme) через свой `create_effect`.
    pub fn with_system_appearance(
        mut self,
        signal: RwSignal<crate::appearance::SystemAppearance>,
    ) -> Self {
        self.system_appearance = Some(signal);
        self
    }

    /// Просит композитор размывать фон за окном («стекло»), пока сигнал этого
    /// требует. Работает только у прозрачного окна
    /// ([`transparent`](Self::transparent)) и там, где такой эффект вообще есть
    /// — сейчас это KWin (Wayland и X11); в остальных окружениях запрос молча
    /// игнорируется, окно остаётся просто прозрачным.
    pub fn with_backdrop(mut self, signal: RwSignal<crate::window::BackdropConfig>) -> Self {
        self.backdrop = Some(signal);
        self
    }

    /// Держит сигнал в согласии с состоянием окна (развёрнуто / во весь экран /
    /// в фокусе). Приложению это нужно там, где оформление зависит от состояния:
    /// геометрия «шелла» под размытие, иконка «восстановить» у кнопки окна,
    /// приглушённые кнопки неактивного окна.
    pub fn with_window_state(mut self, signal: RwSignal<crate::window::WindowState>) -> Self {
        self.window_state = Some(signal);
        self
    }

    pub fn with_font_family(mut self, family: impl Into<String>) -> Self {
        self.font_family = Some(family.into());
        self
    }

    pub fn with_font_url(mut self, url: impl Into<String>) -> Self {
        self.font_url = Some(url.into());
        self
    }

    pub fn with_emoji_font_url(mut self, url: impl Into<String>) -> Self {
        self.emoji_font_url = Some(url.into());
        self
    }

    pub fn with_icon_font(mut self, data: &'static [u8]) -> Self {
        self.icon_font_data = Some(data);
        self
    }

    pub fn gpu_backend(mut self, backend: GpuBackend) -> Self {
        self.gpu_backend = backend;
        self
    }

    pub fn gpu_power(mut self, power: GpuPowerPreference) -> Self {
        self.gpu_power = power;
        self
    }

    pub fn maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    pub fn decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    pub fn frameless(self) -> Self {
        self.decorations(false)
    }

    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    pub fn fullscreen(mut self, fullscreen: bool) -> Self {
        self.fullscreen = fullscreen;
        self
    }

    pub fn size_ratio(mut self, width: f32, height: f32) -> Self {
        self.width_ratio = Some(width.clamp(0.05, 1.0));
        self.height_ratio = Some(height.clamp(0.05, 1.0));
        self
    }

    pub fn with_debug_overlay(mut self, enabled: bool) -> Self {
        self.debug_overlay = enabled;
        self
    }

    pub fn with_dev_tools(mut self, enabled: bool) -> Self {
        self.devtools = enabled;
        self
    }

    #[cfg(feature = "splash")]
    pub fn with_splash(mut self, image_bytes: &'static [u8]) -> Self {
        self.splash_config = Some(super::splash::SplashConfig::new(image_bytes));
        self
    }

    #[cfg(feature = "splash")]
    pub fn splash_size(mut self, width: u32, height: u32) -> Self {
        if let Some(ref mut cfg) = self.splash_config {
            cfg.window_width = width;
            cfg.window_height = height;
        }
        self
    }

    #[cfg(feature = "splash")]
    pub fn splash_background(mut self, r: u8, g: u8, b: u8) -> Self {
        if let Some(ref mut cfg) = self.splash_config {
            cfg.background_color = [r, g, b];
        }
        self
    }

    #[cfg(feature = "splash")]
    pub fn splash_transparent(mut self, transparent: bool) -> Self {
        if let Some(ref mut cfg) = self.splash_config {
            cfg.transparent = transparent;
        }
        self
    }

    #[cfg(feature = "splash")]
    pub fn splash_min_duration(mut self, ms: u64) -> Self {
        if let Some(ref mut cfg) = self.splash_config {
            cfg.min_display_ms = ms;
        }
        self
    }

    #[cfg(target_os = "android")]
    pub fn with_android_app(mut self, app: AndroidApp) -> Self {
        self.android_app = Some(app);
        self
    }

    pub fn add_window<F>(mut self, config: WindowConfig, build: F) -> Self
    where
        F: FnOnce(&mut BuildContext) -> Box<dyn Widget> + 'static,
    {
        self.extra_windows.push((config, Box::new(build)));
        self
    }

    pub fn sticky_windows(mut self, threshold_px: f32) -> Self {
        self.sticky_threshold = Some(threshold_px);
        self
    }

    /// Явно задать интервал двойного клика, переопределяя настройку ОС/DE.
    /// По умолчанию (без вызова) интервал берётся из системы —
    /// [`crate::input::resolve_double_click_interval`].
    pub fn double_click_interval(mut self, interval: std::time::Duration) -> Self {
        self.double_click_interval = Some(interval);
        self
    }

    pub fn with_window_icon_png(mut self, png: &[u8]) -> Self {
        self.window_icon_png = Some(png.to_vec());
        self
    }

    pub fn with_tray(mut self, config: crate::app::tray::TrayConfig) -> Self {
        self.tray_config = Some(config);
        self
    }

    pub fn with_single_instance(mut self, id: impl Into<String>) -> Self {
        self.single_instance_id = Some(id.into());
        self
    }

    pub fn run<F>(self, build_root: F)
    where
        F: Fn(&mut BuildContext) -> Box<dyn Widget> + 'static,
    {
        let root_factory: std::sync::Arc<dyn Fn(&mut BuildContext) -> Box<dyn Widget> + 'static> =
            std::sync::Arc::new(build_root);

        let initial_is_dark = self.theme_state
            .map(|t| t.get_untracked())
            .unwrap_or(false);

        let mut style_engine = if self.theme_state.is_some() {
            let ss = if initial_is_dark {
                self.dark_stylesheet.clone().unwrap_or_default()
            } else {
                self.light_stylesheet.clone().unwrap_or_default()
            };
            StyleEngine::new(ss)
        } else {
            self.stylesheet
                .clone()
                .map(StyleEngine::new)
                .unwrap_or_else(StyleEngine::empty)
        };

        for additional in &self.additional_stylesheets {
            style_engine.load_additional_stylesheet(additional.clone());
        }

        #[cfg(target_arch = "wasm32")]
        console_error_panic_hook::set_once();

        #[cfg(target_os = "android")]
        {
            let mut app_handler = super::handler::AppHandler::new(self, root_factory.clone(), style_engine, initial_is_dark);
            use winit::platform::android::EventLoopBuilderExtAndroid;
            let android_app = app_handler.config.android_app.clone()
                .expect("AndroidApp must be provided via .with_android_app() on Android");
            app_handler.android_app = Some(android_app.clone());
            let event_loop = winit::event_loop::EventLoop::<super::user_event::SynGuiUserEvent>::with_user_event()
                .with_android_app(android_app)
                .build()
                .expect("Failed to create event loop");
            app_handler.event_loop_proxy = Some(event_loop.create_proxy());
            event_loop.run_app(&mut app_handler).expect("Event loop error");
        }

        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        {
            let event_loop = winit::event_loop::EventLoop::<super::user_event::SynGuiUserEvent>::with_user_event()
                .build()
                .expect("Failed to create event loop");
            let proxy = event_loop.create_proxy();

            #[cfg(all(feature = "single-instance", not(target_arch = "wasm32"), not(target_os = "android")))]
            let single_instance = match self.single_instance_id.as_deref() {
                Some(id) => match super::single_instance::SingleInstanceLock::try_acquire(id, proxy.clone()) {
                    Ok(Some(lock)) => Some(lock),
                    Ok(None) => {
                        if let Err(e) = super::single_instance::notify_running_instance(id) {
                            eprintln!("[syngui] failed to notify running instance: {e}");
                        }
                        return;
                    }
                    Err(e) => {
                        eprintln!("[syngui] single-instance setup failed, continuing without lock: {e}");
                        None
                    }
                },
                None => None,
            };

            let mut app_handler = super::handler::AppHandler::new(self, root_factory.clone(), style_engine, initial_is_dark);
            app_handler.event_loop_proxy = Some(proxy);

            #[cfg(all(feature = "single-instance", not(target_arch = "wasm32"), not(target_os = "android")))]
            {
                app_handler.single_instance = single_instance;
            }

            event_loop.run_app(&mut app_handler).expect("Event loop error");
        }

        #[cfg(target_arch = "wasm32")]
        {
            let mut app_handler = super::handler::AppHandler::new(self, root_factory.clone(), style_engine, initial_is_dark);
            use winit::platform::web::EventLoopExtWebSys;
            let event_loop = winit::event_loop::EventLoop::<super::user_event::SynGuiUserEvent>::with_user_event()
                .build()
                .expect("Failed to create event loop");
            app_handler.event_loop_proxy = Some(event_loop.create_proxy());
            event_loop.spawn_app(app_handler);
        }
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}
