use std::sync::Arc;
use crate::gpu::{GpuContext, Renderer};
use crate::window::{Window, WindowBuilder};
use super::AppHandler;

#[cfg(feature = "accessibility")]
use super::SynGuiActivationHandler;
#[cfg(feature = "accessibility")]
use super::SynGuiActionHandler;
#[cfg(feature = "accessibility")]
use super::SynGuiDeactivationHandler;

impl AppHandler {
    pub(in crate::app) fn init(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        #[cfg(feature = "splash")]
        if let Some(ref splash_config) = self.config.splash_config {
            if let Some(splash) = super::super::splash::SplashWindow::create(event_loop, splash_config) {
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
        let window_builder = WindowBuilder::new()
            .with_title(&self.config.title)
            .with_size(init_w, init_h)
            .with_min_size(self.config.min_width, self.config.min_height)
            .with_maximized(self.config.maximized)
            .with_decorations(self.config.decorations)
            .with_transparent(self.config.transparent)
            .with_fullscreen(self.config.fullscreen);

        #[cfg(target_os = "android")]
        let window_builder = WindowBuilder::new()
            .with_title(&self.config.title);

        let window = Arc::new(Window::new(event_loop, window_builder));
        // Веб: пропуск F-клавиш браузеру — до того, как canvas получит фокус.
        #[cfg(target_arch = "wasm32")]
        crate::app::web_keys::install();
        self.scale_factor = window.scale_factor();
        self.main_window_id = Some(window.winit_window().id());
        self.window = Some(window.clone());
        self.main_window_visible = true;

        #[cfg(all(feature = "wayland-dnd", target_os = "linux"))]
        if self.wayland_dnd_handle.is_none() {
            if let Some(proxy) = self.event_loop_proxy.clone() {
                self.wayland_dnd_handle = super::super::wayland_dnd::try_start_wayland_dnd(
                    window.clone(),
                    proxy,
                );
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
                window.winit_window(),
                SynGuiActivationHandler,
                SynGuiActionHandler,
                SynGuiDeactivationHandler,
            );
            self.accesskit_adapter = Some(ak_adapter);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let gpu = pollster::block_on(Self::create_gpu_async(
                &window,
                self.config.vsync,
                self.config.gpu_backend,
                self.config.gpu_power,
                self.config.transparent,
            ));
            self.complete_gpu_init(gpu);

            if !self.pending_windows.is_empty() {
                let pending = std::mem::take(&mut self.pending_windows);
                for (wc, build_fn) in pending {
                    self.create_secondary_window(event_loop, wc, build_fn);
                }
            }
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
                let gpu = AppHandler::create_gpu_async(&window_clone, vsync, backend, power, false).await;
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
        let window = self.window.as_ref().expect("Window must exist for surface recreation");
        if let Some(gpu) = self.gpu.as_mut() {
            let new_surface = gpu.shared.instance
                .create_surface(window.clone())
                .expect("Failed to recreate surface");

            let (w, h) = window.size();
            gpu.window_surface.surface_config.width = w.max(1);
            gpu.window_surface.surface_config.height = h.max(1);
            new_surface.configure(&gpu.shared.device, &gpu.window_surface.surface_config);
            gpu.window_surface.surface = new_surface;

            self.config.width = w.max(1);
            self.config.height = h.max(1);
            self.surface_valid = true;
        }
    }

    pub(in crate::app) fn drop_surface(&mut self) {
        self.surface_valid = false;
    }

    pub(in crate::app) fn process_window_drag_request(&mut self) {
        if let Some(direction) = self.tree.window_resize_request.take() {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(window) = self.window.as_ref() {
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
                if let Err(err) = window.winit_window().drag_resize_window(direction) {
                    eprintln!("[syngui] drag_resize_window failed: {err}");
                }
            }
        }
        if !self.tree.window_drag_request {
            return;
        }
        self.tree.window_drag_request = false;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(window) = self.window.as_ref() {
            if let Err(err) = window.winit_window().drag_window() {
                eprintln!("[syngui] drag_window failed: {err}");
            }
        }
    }

    pub(in crate::app) fn process_window_control_requests(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let hide = std::mem::take(&mut self.tree.window_hide_request);
            let show = std::mem::take(&mut self.tree.window_show_request);
            let toggle = std::mem::take(&mut self.tree.window_toggle_visibility_request);

            if let Some(window) = self.window.as_ref() {
                let winit_window = window.winit_window();
                if self.tree.window_minimize_request {
                    self.tree.window_minimize_request = false;
                    winit_window.set_minimized(true);
                }
                if self.tree.window_toggle_maximize_request {
                    self.tree.window_toggle_maximize_request = false;
                    winit_window.set_maximized(!winit_window.is_maximized());
                }
                if self.tree.window_toggle_fullscreen_request {
                    self.tree.window_toggle_fullscreen_request = false;
                    let is_fs = winit_window.fullscreen().is_some();
                    winit_window.set_fullscreen(if is_fs {
                        None
                    } else {
                        Some(winit::window::Fullscreen::Borderless(None))
                    });
                }
            }

            if hide {
                self.hide_main_window();
            }
            if show {
                self.show_main_window();
            }
            if toggle {
                self.toggle_main_window_visibility();
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            // В браузере у canvas нет свёртывания/разворачивания/скрытия;
            // полноэкранный режим есть — через Fullscreen API (winit).
            self.tree.window_minimize_request = false;
            self.tree.window_toggle_maximize_request = false;
            self.tree.window_hide_request = false;
            self.tree.window_show_request = false;
            self.tree.window_toggle_visibility_request = false;
            if self.tree.window_toggle_fullscreen_request {
                self.tree.window_toggle_fullscreen_request = false;
                if let Some(window) = self.window.as_ref() {
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
        let v = self.tree.window_close_request;
        self.tree.window_close_request = false;
        v
    }

    /// Снимает флаги окна (развёрнуто / полноэкранно / в фокусе) и разносит
    /// их в сигнал `WindowState` и MSS-псевдоклассы. В браузере «развёрнуто»
    /// всегда false, полноэкранность — по `document.fullscreenElement`.
    pub(in crate::app) fn sync_window_flags(&mut self) {
        {
            use crate::mss::window_flags as wf;
            let (maximized, fullscreen, focused, request_redraw) = {
                let Some(window) = self.window.as_ref() else {
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
            self.sync_window_state(crate::window::WindowState {
                maximized,
                fullscreen,
                focused,
            });

            let mut flags = 0u8;
            if maximized  { flags |= wf::MAXIMIZED;  }
            if fullscreen { flags |= wf::FULLSCREEN; }
            if focused    { flags |= wf::FOCUSED;    }
            if !self.tree.set_window_flags(flags) {
                return;
            }
            if std::env::var("MGUI_WINDOW_FLAGS_LOG").is_ok() {
                eprintln!(
                    "[syngui] window_flags: maximized={maximized} fullscreen={fullscreen} focused={focused} (bits=0b{flags:08b})"
                );
            }
            if let Some(root_id) = self.root_id {
                self.apply_styles(root_id);
            }
            request_redraw.request_redraw();
        }
    }

    pub(super) fn complete_gpu_init(&mut self, mut gpu: GpuContext) {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&"[syngui] complete_gpu_init called".into());

        let window = self.window.as_ref().expect("Window must exist before GPU init");

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
        self.config.width = phys_w;
        self.config.height = phys_h;

        gpu.resize(phys_w, phys_h);

        let logical_w = (phys_w as f64 / self.scale_factor).max(1.0) as u32;
        let logical_h = (phys_h as f64 / self.scale_factor).max(1.0) as u32;
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&format!("[syngui] creating renderer {}x{} (logical {}x{})", phys_w, phys_h, logical_w, logical_h).into());

        let surface_format = gpu.window_surface.surface_config.format;
        #[allow(unused_mut)]
        let mut renderer = Renderer::new(&gpu.shared, surface_format, phys_w, phys_h, logical_w, logical_h, self.config.font_family.clone());

        renderer.set_staging_belt(self.config.staging_belt, &gpu.shared.device);

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&"[syngui] renderer created, setting up tree".into());

        self.tree.text_measure = Some(renderer.font_atlas.clone() as std::sync::Arc<dyn crate::widget::context::TextMeasure>);
        renderer.font_atlas.lock().unwrap().set_scale_factor(self.scale_factor as f32);
        #[cfg(feature = "clipboard")]
        { self.tree.clipboard = self.clipboard.clone(); }

        self.tree.image_store = Some(renderer.image_store.clone());

        #[cfg(feature = "map")]
        {
            renderer.ensure_tile_atlas(&gpu.shared);
            self.tree.tile_atlas = renderer.tile_atlas.clone();
        }

        if let Some(icon_data) = self.config.icon_font_data {
            renderer.font_atlas.lock().unwrap().set_icon_font_data(icon_data.to_vec());
        }

        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.surface_valid = true;

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&"[syngui] surface_valid=true, registering signals".into());

        crate::signal::init_main_thread();
        if let Some(ref window) = self.window {
            crate::signal::set_window(window.clone());
            crate::async_runtime::set_async_window(window.clone());
        }

        self.start_appearance_watcher();
        self.start_backdrop_effect();

        #[cfg(all(feature = "tray", not(target_arch = "wasm32"), not(target_os = "android")))]
        {
            if self.tray.is_none() {
                if let (Some(cfg), Some(proxy)) =
                    (self.config.tray_config.clone(), self.event_loop_proxy.clone())
                {
                    match super::super::tray::TrayManager::start(cfg, proxy) {
                        Ok(manager) => self.tray = Some(manager),
                        Err(e) => eprintln!("[syngui] system tray unavailable: {e}"),
                    }
                }
            }
        }

        #[cfg(target_os = "android")]
        self.update_safe_area();

        #[cfg(target_os = "android")]
        self.set_status_bar_light_icons(self.current_theme_is_dark);

        #[cfg(target_os = "android")]
        self.register_back_handler();

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&"[syngui] signals registered, deferring root widget until font loads".into());

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut ctx = crate::widget::BuildContext::root();
            let widget = (self.root_factory)(&mut ctx);
            let element = widget.create_element();
            let type_id = widget.as_any().type_id();
            let root_id = self.tree.insert_with_type_id(element, None, type_id);

            widget.mount(&mut self.tree, root_id);
            self.root_id = Some(root_id);

            self.sync_window_flags();

            self.apply_styles(root_id);

            loop {
                if !self.tree.rebuild_if_needed(root_id) {
                    break;
                }
                self.apply_styles(root_id);
            }

            let logical_w = (self.config.width as f64 / self.scale_factor) as f32;
            let logical_h = (self.config.height as f64 / self.scale_factor) as f32;
            let safe = &self.tree.safe_area;
            let layout_h = (logical_h - safe.top - safe.bottom).max(0.0);
            self.tree.root_offset = crate::core::Point::new(safe.left, safe.top);
            crate::viewport::publish(crate::core::Size::new(
                logical_w - safe.left - safe.right,
                layout_h,
            ));
            let constraints = crate::layout::Constraints::new(
                0.0, logical_w - safe.left - safe.right,
                0.0, layout_h,
            );
            self.tree.layout(root_id, constraints);

            self.a11y_tree.sync(&self.tree, root_id);
            self.focus_manager.rebuild_tab_order(&self.tree, root_id);
            self.a11y_dirty = false;

            #[cfg(feature = "accessibility")]
            {
                if let Some(ref mut ak_adapter) = self.accesskit_adapter {
                    if let Some(update) = self.a11y_tree.take_accesskit_update() {
                        ak_adapter.update_if_active(|| update);
                    }
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
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).expect("Failed to create surface");

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.expect("Failed to find suitable adapter");

        let limits = if cfg!(target_arch = "wasm32") {
            wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter.limits())
        } else if cfg!(target_os = "android") {
            wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter.limits())
        } else {
            wgpu::Limits::default()
        };

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                label: Some("GPU Device"),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            },
        ).await.expect("Failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let (width, height) = window.size();
        let width = width.max(1);
        let height = height.max(1);
        let alpha_mode = if transparent {
            if surface_caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else if surface_caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
                wgpu::CompositeAlphaMode::PostMultiplied
            } else {
                surface_caps.alpha_modes[0]
            }
        } else {
            surface_caps.alpha_modes[0]
        };
        let surface_config = wgpu::SurfaceConfiguration {
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
        };

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

#[cfg(not(target_os = "android"))]
fn resolve_initial_size(
    event_loop: &winit::event_loop::ActiveEventLoop,
    width: u32,
    height: u32,
    width_ratio: Option<f32>,
    height_ratio: Option<f32>,
) -> (u32, u32) {
    let monitor = event_loop.primary_monitor().or_else(|| event_loop.available_monitors().next());
    let monitor_logical = monitor.map(|m| {
        let size = m.size();
        let sf = m.scale_factor().max(0.1);
        ((size.width as f64 / sf) as f32, (size.height as f64 / sf) as f32)
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
