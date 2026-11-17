use super::AppHandler;
use crate::gpu::{GpuContext, GpuShared, Renderer, WindowSurface};
use crate::window::{Window, WindowBuilder};
use std::sync::Arc;

#[cfg(feature = "accessibility")]
use super::SynGuiActionHandler;
#[cfg(feature = "accessibility")]
use super::SynGuiActivationHandler;
#[cfg(feature = "accessibility")]
use super::SynGuiDeactivationHandler;

impl AppHandler {
    pub(in crate::app) fn init(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        #[cfg(feature = "splash")]
        if let Some(ref splash_config) = self.config.splash_config {
            if let Some(splash) =
                super::super::splash::SplashWindow::create(event_loop, splash_config)
            {
                splash.wait_and_close();
            }
        }

        #[cfg(not(target_os = "android"))]
        let (init_w, init_h) = resolve_initial_size(
            event_loop,
            self.config.width,
            self.config.height,
            self.config.width_ratio,
            self.config.height_ratio,
        );
        #[cfg(not(target_os = "android"))]
        let mut window_builder = WindowBuilder::new()
            .with_title(&self.config.title)
            .with_size(init_w, init_h)
            .with_min_size(self.config.min_width, self.config.min_height)
            .with_maximized(self.config.maximized)
            .with_decorations(self.config.decorations)
            .with_transparent(self.config.transparent)
            .with_fullscreen(self.config.fullscreen);
        #[cfg(not(target_os = "android"))]
        if let Some(id) = &self.config.app_id {
            window_builder = window_builder.with_app_id(id.clone());
        }

        #[cfg(target_os = "android")]
        let window_builder = WindowBuilder::new().with_title(&self.config.title);

        #[cfg(feature = "accessibility")]
        let window_builder = window_builder.with_visible(false);
        let window = Arc::new(Window::new(event_loop, window_builder));
        // Веб: пропуск F-клавиш браузеру — до того, как canvas получит фокус.
        #[cfg(target_arch = "wasm32")]
        crate::app::web_keys::install();
        // Веб: кнопка «Назад» браузера — событие BackPressed.
        #[cfg(target_arch = "wasm32")]
        if let Some(proxy) = self.event_loop_proxy.clone() {
            crate::app::web_history::install(proxy);
        }
        // Веб: вставка из системного буфера через DOM-событие `paste`.
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            if let Some(canvas) = window.winit_window().canvas() {
                crate::app::web_clipboard::install(canvas.clone());
                // Экранная клавиатура: скрытый <input> рядом с canvas.
                crate::app::web_text_agent::install(canvas);
            }
        }
        self.win.system_scale_factor = window.scale_factor();
        crate::scale::set_system_scale(self.win.system_scale_factor as f32);
        self.apply_design_scale();
        // Сбросить mSoftKeyboardActive у GameTextInput: пока он взведён,
        // InputConnection GameActivity перехватывает DPAD_LEFT/RIGHT пульта
        // (см. window/window.rs, set_ime_allowed).
        #[cfg(target_os = "android")]
        if let Some(app) = self.android_app.as_ref() {
            app.hide_soft_input(true);
        }
        self.win.scale_factor = self.effective_scale_factor();
        self.win.window = Some(window.clone());
        self.main_window_visible = true;
        crate::app::MAIN_WINDOW_VISIBLE.store(true, std::sync::atomic::Ordering::Relaxed);

        #[cfg(all(feature = "wayland-dnd", target_os = "linux"))]
        if self.wayland_dnd_handle.is_none() {
            if let Some(proxy) = self.event_loop_proxy.clone() {
                self.wayland_dnd_handle =
                    super::super::wayland_dnd::try_start_wayland_dnd(window.clone(), proxy);
            }
        }
        #[cfg(all(feature = "pen", target_os = "linux"))]
        if self.wayland_pen_handle.is_none() {
            if let Some(proxy) = self.event_loop_proxy.clone() {
                self.wayland_pen_handle = super::super::wayland_pen::try_start(&window, proxy);
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(bytes) = self.config.window_icon_png.clone() {
            if let Err(e) = window.set_window_icon_from_png(&bytes) {
                eprintln!("[syngui] failed to set window icon: {e}");
            }
        }

        #[cfg(feature = "accessibility")]
        {
            let ak_adapter = accesskit_winit::Adapter::with_direct_handlers(
                event_loop,
                window.winit_window(),
                SynGuiActivationHandler(self.event_loop_proxy.clone()),
                SynGuiActionHandler(self.event_loop_proxy.clone()),
                SynGuiDeactivationHandler(self.event_loop_proxy.clone()),
            );
            self.win.accesskit_adapter = Some(ak_adapter);
            window.set_visible(true);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(gpu) = self.gpu.as_ref() {
                // GPU пережил пересоздание главного окна (его держат
                // дополнительные окна) — поверхность на нём же.
                let surface =
                    create_window_surface(gpu, &window, self.config.vsync, self.config.transparent);
                self.complete_window_init(surface);
            } else {
                let gpu = pollster::block_on(Self::create_gpu_async(
                    &window,
                    self.config.vsync,
                    self.config.gpu_backend,
                    self.config.gpu_power,
                    self.config.transparent,
                ));
                self.complete_gpu_init(gpu);
            }

            self.create_pending_windows(event_loop);
        }

        #[cfg(target_arch = "wasm32")]
        {
            web_sys::console::log_1(&"[syngui] Starting async GPU init...".into());

            let pending = self.pending_gpu.clone();
            let window_clone = window.clone();
            let vsync = self.config.vsync;
            let backend = self.config.gpu_backend;
            let power = self.config.gpu_power;
            wasm_bindgen_futures::spawn_local(async move {
                web_sys::console::log_1(&"[syngui] create_gpu_async started".into());
                let gpu =
                    AppHandler::create_gpu_async(&window_clone, vsync, backend, power, false).await;
                web_sys::console::log_1(&"[syngui] GPU context created, storing...".into());
                *pending.borrow_mut() = Some(gpu);
                window_clone.request_redraw();
                web_sys::console::log_1(&"[syngui] GPU init done, redraw requested".into());
            });

            if let Some(url) = self.config.font_url.clone() {
                let pending_font = self.pending_font.clone();
                let window_clone = window.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match super::super::input_mapping::fetch_bytes(&url).await {
                        Ok(data) => {
                            *pending_font.borrow_mut() = Some(data);
                            window_clone.request_redraw();
                        }
                        Err(e) => {
                            log::error!("Failed to fetch font from '{}': {:?}", url, e);
                        }
                    }
                });
            } else {
                log::warn!("No font_url set — text won't render on WASM. Use .with_font_url()");
            }

            if let Some(url) = self.config.emoji_font_url.clone() {
                let pending_emoji = self.pending_emoji_font.clone();
                let window_clone = window.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match super::super::input_mapping::fetch_bytes(&url).await {
                        Ok(data) => {
                            *pending_emoji.borrow_mut() = Some(data);
                            window_clone.request_redraw();
                        }
                        Err(e) => {
                            log::error!("Failed to fetch emoji font from '{}': {:?}", url, e);
                        }
                    }
                });
            }

            for url in self.config.fallback_font_urls.clone() {
                let pending_fallback = self.pending_fallback_fonts.clone();
                let window_clone = window.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match super::super::input_mapping::fetch_bytes(&url).await {
                        Ok(data) => {
                            pending_fallback.borrow_mut().push(data);
                            window_clone.request_redraw();
                        }
                        Err(e) => {
                            log::error!("Failed to fetch fallback font from '{}': {:?}", url, e);
                        }
                    }
                });
            }
        }
    }

    #[allow(dead_code)]
    pub(in crate::app) fn recreate_surface(&mut self) {
        let window = self
            .win
            .window
            .as_ref()
            .expect("Window must exist for surface recreation");
        if let (Some(gpu), Some(surface)) = (self.gpu.as_ref(), self.win.surface.as_mut()) {
            let new_surface = gpu
                .instance
                .create_surface(window.clone())
                .expect("Failed to recreate surface");

            let (w, h) = window.size();
            surface.surface_config.width = w.max(1);
            surface.surface_config.height = h.max(1);
            new_surface.configure(&gpu.device, &surface.surface_config);
            surface.surface = new_surface;

            self.win.width = w.max(1);
            self.win.height = h.max(1);
            self.win.surface_valid = true;
            // Новая поверхность пуста: кадр, совпадающий с последним
            // показанным, пропускать нельзя — иначе после возврата из фона
            // (Home → снова в приложение) экран остаётся чёрным, пока
            // что-нибудь не изменится.
            self.win.last_frame_sig = None;
        }
    }

    pub(in crate::app) fn drop_surface(&mut self) {
        self.win.surface_valid = false;
    }

    pub(in crate::app) fn process_window_drag_request(&mut self) {
        if let Some(direction) = self.win.tree.window_resize_request.take() {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(window) = self.win.window.as_ref() {
                use crate::input::ResizeDirection as D;
                use winit::window::ResizeDirection as W;
                let direction = match direction {
                    D::North => W::North,
                    D::South => W::South,
                    D::East => W::East,
                    D::West => W::West,
                    D::NorthEast => W::NorthEast,
                    D::NorthWest => W::NorthWest,
                    D::SouthEast => W::SouthEast,
                    D::SouthWest => W::SouthWest,
                };
                #[cfg(all(feature = "wayland-dnd", target_os = "linux"))]
                let by_touch = super::super::wayland_dnd::start_touch_window_op(
                    window.winit_window(),
                    Some(direction),
                );
                #[cfg(not(all(feature = "wayland-dnd", target_os = "linux")))]
                let by_touch = false;
                if !by_touch {
                    if let Err(err) = window.winit_window().drag_resize_window(direction) {
                        eprintln!("[syngui] drag_resize_window failed: {err}");
                    }
                }
            }
        }
        if !self.win.tree.window_drag_request {
            return;
        }
        self.win.tree.window_drag_request = false;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(window) = self.win.window.as_ref() {
            #[cfg(all(feature = "wayland-dnd", target_os = "linux"))]
            if super::super::wayland_dnd::start_touch_window_op(window.winit_window(), None) {
                return;
            }
            if let Err(err) = window.winit_window().drag_window() {
                eprintln!("[syngui] drag_window failed: {err}");
            }
        }
    }

    pub(in crate::app) fn process_window_control_requests(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let hide = std::mem::take(&mut self.win.tree.window_hide_request);
            let show = std::mem::take(&mut self.win.tree.window_show_request);
            let toggle = std::mem::take(&mut self.win.tree.window_toggle_visibility_request);

            if let Some(window) = self.win.window.as_ref() {
                let winit_window = window.winit_window();
                if self.win.tree.window_minimize_request {
                    self.win.tree.window_minimize_request = false;
                    winit_window.set_minimized(true);
                }
                if self.win.tree.window_toggle_maximize_request {
                    self.win.tree.window_toggle_maximize_request = false;
                    winit_window.set_maximized(!winit_window.is_maximized());
                }
                if self.win.tree.window_toggle_fullscreen_request {
                    self.win.tree.window_toggle_fullscreen_request = false;
                    let is_fs = winit_window.fullscreen().is_some();
                    winit_window.set_fullscreen(if is_fs {
                        None
                    } else {
                        Some(winit::window::Fullscreen::Borderless(None))
                    });
                }
            }

            if self.win.is_main() {
                if hide {
                    self.hide_main_window();
                }
                if show {
                    self.show_main_window();
                }
                if toggle {
                    self.toggle_main_window_visibility();
                }
            } else if let Some(window) = self.win.window.as_ref() {
                // Дополнительное окно прячется и показывается само по себе.
                let visible = window.is_visible().unwrap_or(true);
                let target = if hide {
                    Some(false)
                } else if show {
                    Some(true)
                } else if toggle {
                    Some(!visible)
                } else {
                    None
                };
                if let Some(on) = target {
                    window.set_visible(on);
                    if on {
                        window.focus();
                    }
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            // В браузере у canvas нет свёртывания/разворачивания/скрытия;
            // полноэкранный режим есть — через Fullscreen API (winit).
            self.win.tree.window_minimize_request = false;
            self.win.tree.window_toggle_maximize_request = false;
            self.win.tree.window_hide_request = false;
            self.win.tree.window_show_request = false;
            self.win.tree.window_toggle_visibility_request = false;
            if self.win.tree.window_toggle_fullscreen_request {
                self.win.tree.window_toggle_fullscreen_request = false;
                if let Some(window) = self.win.window.as_ref() {
                    let winit_window = window.winit_window();
                    let is_fs = winit_window.fullscreen().is_some();
                    winit_window.set_fullscreen(if is_fs {
                        None
                    } else {
                        Some(winit::window::Fullscreen::Borderless(None))
                    });
                }
            }
        }
    }

    pub(in crate::app) fn take_window_close_request(&mut self) -> bool {
        let v = self.win.tree.window_close_request;
        self.win.tree.window_close_request = false;
        v
    }

    /// Снимает флаги окна (развёрнуто / полноэкранно / в фокусе) и разносит
    /// их в сигнал `WindowState` и MSS-псевдоклассы. В браузере «развёрнуто»
    /// всегда false, полноэкранность — по `document.fullscreenElement`.
    pub(in crate::app) fn sync_window_flags(&mut self) {
        {
            use crate::mss::window_flags as wf;
            let (maximized, fullscreen, focused, request_redraw) = {
                let Some(window) = self.win.window.as_ref() else {
                    return;
                };
                let winit_window = window.winit_window();
                (
                    winit_window.is_maximized(),
                    winit_window.fullscreen().is_some(),
                    winit_window.has_focus(),
                    window.clone(),
                )
            };
            // Веб: фокус — у страницы, а не у canvas: при наборе он на
            // скрытом агенте ввода, и окно не должно считаться неактивным.
            #[cfg(target_arch = "wasm32")]
            let focused = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.has_focus().ok())
                .unwrap_or(focused);
            self.sync_window_state(crate::window::WindowState {
                maximized,
                fullscreen,
                focused,
            });

            let mut flags = 0u8;
            if maximized {
                flags |= wf::MAXIMIZED;
            }
            if fullscreen {
                flags |= wf::FULLSCREEN;
            }
            if focused {
                flags |= wf::FOCUSED;
            }
            if !self.win.tree.set_window_flags(flags) {
                return;
            }
            if std::env::var("MGUI_WINDOW_FLAGS_LOG").is_ok() {
                eprintln!(
                    "[syngui] window_flags: maximized={maximized} fullscreen={fullscreen} focused={focused} (bits=0b{flags:08b})"
                );
            }
            if let Some(root_id) = self.win.root_id {
                self.apply_styles(root_id);
            }
            request_redraw.request_redraw();
        }
    }

    pub(super) fn complete_gpu_init(&mut self, gpu: GpuContext) {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&"[syngui] complete_gpu_init called".into());
        let (shared, surface) = gpu.split();
        self.gpu = Some(shared);
        self.complete_window_init(surface);
    }

    /// Главное окно получило поверхность на готовом GPU: рендерер, сигналы,
    /// службы приложения и корень дерева.
    pub(super) fn complete_window_init(&mut self, mut surface: WindowSurface) {
        let window = self
            .win
            .window
            .as_ref()
            .expect("Window must exist before GPU init");

        #[cfg(target_arch = "wasm32")]
        let (phys_w, phys_h) = {
            use winit::platform::web::WindowExtWebSys;
            let win = web_sys::window().expect("no global window");
            let dpr = win.device_pixel_ratio();
            let w = (win.inner_width().unwrap().as_f64().unwrap_or(1280.0) * dpr) as u32;
            let h = (win.inner_height().unwrap().as_f64().unwrap_or(900.0) * dpr) as u32;
            if let Some(canvas) = window.winit_window().canvas() {
                canvas.set_width(w);
                canvas.set_height(h);
            }
            (w.max(1), h.max(1))
        };
        #[cfg(not(target_arch = "wasm32"))]
        let (phys_w, phys_h) = {
            let (w, h) = window.size();
            (w.max(1), h.max(1))
        };
        self.win.width = phys_w;
        self.win.height = phys_h;

        let gpu = self
            .gpu
            .as_ref()
            .expect("GPU must exist before window init");
        surface.resize(&gpu.device, phys_w, phys_h);

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(
            &format!("[syngui] creating renderer {}x{}", phys_w, phys_h).into(),
        );

        let renderer = self.create_renderer(
            gpu,
            surface.surface_config.format,
            phys_w,
            phys_h,
            self.win.scale_factor,
        );
        Self::attach_renderer(&mut self.win.tree, &renderer);

        self.win.renderer = Some(renderer);
        self.win.surface = Some(surface);
        self.win.surface_valid = true;

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&"[syngui] surface_valid=true, registering signals".into());

        crate::signal::init_main_thread();
        if let Some(ref window) = self.win.window {
            crate::signal::set_window(window.clone());
            crate::async_runtime::set_async_window(window.clone());
        }

        self.start_appearance_watcher();
        self.start_backdrop_effect();

        #[cfg(all(
            feature = "tray",
            not(target_arch = "wasm32"),
            not(target_os = "android")
        ))]
        {
            if self.tray.is_none() {
                if let (Some(cfg), Some(proxy)) = (
                    self.config.tray_config.clone(),
                    self.event_loop_proxy.clone(),
                ) {
                    match super::super::tray::TrayManager::start(cfg, proxy) {
                        Ok(manager) => self.tray = Some(manager),
                        Err(e) => eprintln!("[syngui] system tray unavailable: {e}"),
                    }
                }
            }
        }

        #[cfg(any(target_os = "android", target_os = "ios"))]
        self.update_safe_area();

        #[cfg(target_os = "android")]
        self.set_status_bar_light_icons(self.current_theme_is_dark);

        #[cfg(target_os = "android")]
        self.register_back_handler();

        // Android: JNI-мост буфера обмена (ClipboardManager).
        #[cfg(target_os = "android")]
        if let Some(ref android_app) = self.android_app {
            crate::clipboard::set_android_ptrs(
                android_app.vm_as_ptr(),
                android_app.activity_as_ptr(),
            );
        }

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(
            &"[syngui] signals registered, deferring root widget until font loads".into(),
        );

        #[cfg(not(target_arch = "wasm32"))]
        {
            let factory = self.root_factory.clone();
            self.mount_root(move |ctx| factory(ctx));
        }
    }

    /// Рендерер окна на общем GPU: атлас шрифтов под масштаб окна, иконочные
    /// шрифты приложения.
    pub(super) fn create_renderer(
        &self,
        gpu: &GpuShared,
        format: wgpu::TextureFormat,
        phys_w: u32,
        phys_h: u32,
        scale: f64,
    ) -> Renderer {
        let logical_w = (phys_w as f64 / scale).max(1.0) as u32;
        let logical_h = (phys_h as f64 / scale).max(1.0) as u32;
        #[allow(unused_mut)]
        let mut renderer = Renderer::new(
            gpu,
            format,
            phys_w,
            phys_h,
            logical_w,
            logical_h,
            self.config.font_family.clone(),
        );
        renderer.set_staging_belt(self.config.staging_belt, &gpu.device);
        renderer
            .font_atlas
            .lock()
            .unwrap()
            .set_scale_factor(scale as f32);
        #[cfg(feature = "map")]
        renderer.ensure_tile_atlas(gpu);
        if let Some(icon_data) = self.config.icon_font_data {
            renderer
                .font_atlas
                .lock()
                .unwrap()
                .set_icon_font_data(icon_data.to_vec());
        }
        #[cfg(feature = "font-awesome")]
        if self.config.font_awesome {
            use crate::text::icon_fonts::awesome as fa;
            renderer.font_atlas.lock().unwrap().set_awesome_font_data(
                fa::SOLID_FONT_DATA.to_vec(),
                fa::REGULAR_FONT_DATA.to_vec(),
                fa::BRANDS_FONT_DATA.to_vec(),
            );
        }
        renderer
    }

    /// Дерево окна меряет текст и грузит картинки через его рендерер.
    pub(super) fn attach_renderer(tree: &mut crate::widget::ElementTree, renderer: &Renderer) {
        tree.text_measure =
            Some(renderer.font_atlas.clone()
                as std::sync::Arc<dyn crate::widget::context::TextMeasure>);
        tree.image_store = Some(renderer.image_store.clone());
        #[cfg(feature = "map")]
        {
            tree.tile_atlas = renderer.tile_atlas.clone();
        }
    }

    /// Собрать корень активного окна, применить стили и разложить по
    /// размеру окна. Эффекты сборщика дополнительного окна принадлежат ему
    /// и освобождаются при закрытии (см. `window_handle::mount_window_root`).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(in crate::app) fn mount_root(
        &mut self,
        build: impl FnOnce(&mut crate::widget::BuildContext) -> Box<dyn crate::widget::Widget>,
    ) {
        let root_id = crate::app::window_handle::mount_window_root(
            self.win.handle,
            &mut self.win.tree,
            build,
        );
        self.win.root_id = Some(root_id);

        self.sync_window_flags();

        self.apply_styles(root_id);

        loop {
            if !self.win.tree.rebuild_if_needed(root_id) {
                break;
            }
            self.apply_styles(root_id);
        }

        let logical_w = (self.win.width as f64 / self.win.scale_factor) as f32;
        let logical_h = (self.win.height as f64 / self.win.scale_factor) as f32;
        let safe = self.layout_insets();
        let layout_h = (logical_h - safe.top - safe.bottom).max(0.0);
        self.win.tree.root_offset = crate::core::Point::new(safe.left, safe.top);
        crate::viewport::publish_origin(self.win.tree.root_offset);
        crate::viewport::publish(crate::core::Size::new(
            logical_w - safe.left - safe.right,
            layout_h,
        ));
        let constraints =
            crate::layout::Constraints::new(0.0, logical_w - safe.left - safe.right, 0.0, layout_h);
        self.win.tree.layout(root_id, constraints);

        self.win.a11y_tree.sync(&self.win.tree, root_id);
        self.win
            .focus_manager
            .rebuild_tab_order(&self.win.tree, root_id);
        self.win.a11y_dirty = false;

        #[cfg(feature = "accessibility")]
        {
            if let Some(ref mut ak_adapter) = self.win.accesskit_adapter {
                if let Some(update) = self.win.a11y_tree.take_accesskit_update() {
                    ak_adapter.update_if_active(|| update);
                }
            }
        }
    }

    async fn create_gpu_async(
        window: &Arc<Window>,
        vsync: bool,
        backend: super::super::GpuBackend,
        power: super::super::GpuPowerPreference,
        transparent: bool,
    ) -> GpuContext {
        use super::super::{GpuBackend, GpuPowerPreference};

        let backends = if cfg!(target_arch = "wasm32") {
            wgpu::Backends::GL
        } else if cfg!(target_os = "android") {
            match backend {
                GpuBackend::Auto => wgpu::Backends::VULKAN | wgpu::Backends::GL,
                GpuBackend::Vulkan => wgpu::Backends::VULKAN,
                GpuBackend::Gl => wgpu::Backends::GL,
                _ => wgpu::Backends::VULKAN | wgpu::Backends::GL,
            }
        } else {
            match backend {
                GpuBackend::Auto => wgpu::Backends::all(),
                GpuBackend::Vulkan => wgpu::Backends::VULKAN,
                GpuBackend::Gl => wgpu::Backends::GL,
                GpuBackend::Dx12 => wgpu::Backends::DX12,
                GpuBackend::Metal => wgpu::Backends::METAL,
            }
        };

        let power_preference = match power {
            GpuPowerPreference::HighPerformance => wgpu::PowerPreference::HighPerformance,
            GpuPowerPreference::LowPower => wgpu::PowerPreference::LowPower,
        };

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            flags: crate::gpu::instance_flags(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");

        let preferred = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok();

        // HDR-окно: Vulkan WSI проприетарного NVIDIA объявляет FP16-окно как 1.0 = 80 нит при белом
        // 203 нит — интерфейс в нём в 2,5 раза темнее. Mesa (Intel, AMD) — 1.0 = белый. Окно с HDR —
        // на карте Mesa, если она есть и даёт FP16 (у гибридного ноутбука это ещё и карта, ведущая
        // экран: кадр окна не копируется между картами).
        #[cfg(not(target_arch = "wasm32"))]
        let preferred = match preferred {
            Some(a)
                if crate::gpu::hdr_requested()
                    && a.get_info().vendor == 0x10de
                    && !a.get_info().driver.contains("NVK") =>
            {
                let mesa = instance
                    .enumerate_adapters(backends)
                    .await
                    .into_iter()
                    .filter(|o| {
                        o.get_info().vendor != 0x10de
                            && o.get_info().backend == wgpu::Backend::Vulkan
                    })
                    .find(|o| {
                        o.is_surface_supported(&surface)
                            && surface
                                .get_capabilities(o)
                                .formats
                                .contains(&wgpu::TextureFormat::Rgba16Float)
                    });
                match mesa {
                    Some(m) => {
                        log::info!(
                            "gpu: HDR-окно — на «{}» вместо «{}»",
                            m.get_info().name,
                            a.get_info().name
                        );
                        Some(m)
                    }
                    None => Some(a),
                }
            }
            other => other,
        };

        let limits_for = |adapter: &wgpu::Adapter| {
            if cfg!(target_arch = "wasm32") || cfg!(target_os = "android") {
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
            } else if cfg!(target_os = "ios") {
                // Лимиты Metal на iPhone/iPad разные по поколениям GPU; запрос
                // больше возможного — отказ в устройстве и паника при запуске.
                adapter.limits()
            } else {
                wgpu::Limits::default()
            }
        };
        let open = |adapter: wgpu::Adapter| {
            let limits = limits_for(&adapter);
            // 16-битные текстуры: 10-битное видео (P010) переводится в цвет на GPU.
            let features = adapter.features() & wgpu::Features::TEXTURE_FORMAT_16BIT_NORM;
            async move {
                let r = adapter
                    .request_device(&wgpu::DeviceDescriptor {
                        required_features: features,
                        required_limits: limits,
                        label: Some("GPU Device"),
                        memory_hints: wgpu::MemoryHints::Performance,
                        trace: wgpu::Trace::Off,
                        experimental_features: wgpu::ExperimentalFeatures::disabled(),
                    })
                    .await;
                if r.is_ok() {
                    crate::gpu::set_yuv16_supported(!features.is_empty());
                }
                r.map(|(d, q)| (adapter, d, q))
            }
        };

        // Предпочтительная карта может не дать устройство (видеопамять занята
        // нейросетями — OutOfMemory): тогда остальные карты, подходящие окну,
        // — встроенная, прочие, программная — вместо паники.
        let mut opened = None;
        let mut tried: Vec<wgpu::AdapterInfo> = Vec::new();
        if let Some(a) = preferred {
            let info = a.get_info();
            match open(a).await {
                Ok(r) => opened = Some(r),
                Err(e) => log::warn!(
                    "gpu: «{}» не дала устройство: {e}; пробую другие карты",
                    info.name
                ),
            }
            tried.push(info);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if opened.is_none() {
            let mut others: Vec<wgpu::Adapter> = instance
                .enumerate_adapters(backends)
                .await
                .into_iter()
                .filter(|a| a.is_surface_supported(&surface))
                .filter(|a| {
                    let i = a.get_info();
                    !tried
                        .iter()
                        .any(|t| t.name == i.name && t.backend == i.backend && t.device == i.device)
                })
                .collect();
            let rank = |a: &wgpu::Adapter| match a.get_info().device_type {
                wgpu::DeviceType::IntegratedGpu => 0,
                wgpu::DeviceType::DiscreteGpu => 1,
                wgpu::DeviceType::VirtualGpu => 2,
                wgpu::DeviceType::Other => 3,
                wgpu::DeviceType::Cpu => 4,
            };
            others.sort_by_key(|a| (rank(a), a.get_info().backend != wgpu::Backend::Vulkan));
            for a in others {
                let info = a.get_info();
                match open(a).await {
                    Ok(r) => {
                        opened = Some(r);
                        break;
                    }
                    Err(e) => log::warn!(
                        "gpu: «{}» ({:?}) не дала устройство: {e}",
                        info.name,
                        info.backend
                    ),
                }
            }
        }
        let (adapter, device, queue) = opened.expect("gpu: ни одна видеокарта не дала устройство");
        // Нехватка видеопамяти (её заняли нейросети) — в журнал, а не паника
        // всей программы; картинки при этом грузятся уменьшенными
        // (ImageCache::upload). Прочие ошибки wgpu — как раньше, фатальны.
        device.on_uncaptured_error(std::sync::Arc::new(|e: wgpu::Error| match e {
            wgpu::Error::OutOfMemory { .. } => log::error!("gpu: {e}"),
            e => panic!("wgpu error: {e}"),
        }));
        {
            let info = adapter.get_info();
            log::info!(
                "gpu: адаптер «{}» ({:?}, {:?}), предпочтение {:?}",
                info.name,
                info.device_type,
                info.backend,
                power
            );
        }

        let surface_config = surface_config_for(&surface, &adapter, window, vsync, transparent);
        surface.configure(&device, &surface_config);

        GpuContext {
            shared: crate::gpu::GpuShared {
                instance,
                adapter,
                device,
                queue,
            },
            window_surface: crate::gpu::WindowSurface {
                surface,
                surface_config,
            },
        }
    }
}

/// Настройка поверхности окна: формат под адаптер, размер окна, vsync;
/// прозрачному окну (CSD) — премультиплицированная альфа, если есть.
fn surface_config_for(
    surface: &wgpu::Surface<'static>,
    adapter: &wgpu::Adapter,
    window: &Window,
    vsync: bool,
    transparent: bool,
) -> wgpu::SurfaceConfiguration {
    let surface_caps = surface.get_capabilities(adapter);
    let surface_format = crate::gpu::preferred_surface_format(&surface_caps, adapter);

    let (width, height) = window.size();
    let width = width.max(1);
    let height = height.max(1);
    let alpha_mode = if transparent {
        if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            surface_caps.alpha_modes[0]
        }
    } else {
        surface_caps.alpha_modes[0]
    };
    wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width,
        height,
        present_mode: if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        },
        alpha_mode,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    }
}

/// Поверхность нового окна на уже готовом GPU (дополнительное окно или
/// пересозданное главное).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(super) fn create_window_surface(
    gpu: &GpuShared,
    window: &Arc<Window>,
    vsync: bool,
    transparent: bool,
) -> WindowSurface {
    let surface = gpu
        .instance
        .create_surface(window.clone())
        .expect("Failed to create surface for window");
    let surface_config = surface_config_for(&surface, &gpu.adapter, window, vsync, transparent);
    surface.configure(&gpu.device, &surface_config);
    WindowSurface {
        surface,
        surface_config,
    }
}

#[cfg(not(target_os = "android"))]
fn resolve_initial_size(
    event_loop: &winit::event_loop::ActiveEventLoop,
    width: u32,
    height: u32,
    width_ratio: Option<f32>,
    height_ratio: Option<f32>,
) -> (u32, u32) {
    let monitor = event_loop
        .primary_monitor()
        .or_else(|| event_loop.available_monitors().next());
    let monitor_logical = monitor.map(|m| {
        let size = m.size();
        let sf = m.scale_factor().max(0.1);
        (
            (size.width as f64 / sf) as f32,
            (size.height as f64 / sf) as f32,
        )
    });
    let w = match (width_ratio, monitor_logical) {
        (Some(r), Some((mw, _))) => (mw * r).round().max(1.0) as u32,
        _ => width,
    };
    let h = match (height_ratio, monitor_logical) {
        (Some(r), Some((_, mh))) => (mh * r).round().max(1.0) as u32,
        _ => height,
    };
    (w, h)
}
