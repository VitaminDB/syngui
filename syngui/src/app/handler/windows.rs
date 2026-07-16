use std::sync::Arc;
use crate::a11y::FocusManager;
use crate::core::Point;
use crate::input::CursorIcon;
use crate::render::DisplayList;
use crate::widget::{ElementTree, Widget};
use crate::window::{Window, WindowBuilder};
use crate::gpu::Renderer;
use super::{AppHandler, SecondaryWindow};

impl AppHandler {
    pub(super) fn create_secondary_window(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        wc: super::super::builder::WindowConfig,
        build_fn: Box<dyn FnOnce(&mut crate::widget::BuildContext) -> Box<dyn Widget>>,
    ) {
        let gpu = match self.gpu.as_ref() {
            Some(g) => g,
            None => {
                log::error!("Cannot create secondary window: GPU not initialized");
                return;
            }
        };

        let mut wb = WindowBuilder::new()
            .with_title(if wc.title.is_empty() { &wc.name } else { &wc.title })
            .with_size(wc.width, wc.height)
            .with_min_size(wc.min_width, wc.min_height);

        if !wc.decorations {
            wb = wb.with_decorations(false);
        }

        let window = Arc::new(Window::new(event_loop, wb));
        let win_id = window.winit_window().id();

        if let Some((x, y)) = wc.position {
            window.winit_window().set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
        } else if let Some((dx, dy)) = wc.offset_from_main {
            if let Some(ref main_win) = self.window {
                if let Ok(main_pos) = main_win.winit_window().outer_position() {
                    let x = main_pos.x + dx;
                    let y = main_pos.y + dy;
                    window.winit_window().set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                }
            }
        }

        crate::signal::add_window(window.clone());

        let surface = gpu.shared.instance
            .create_surface(window.clone())
            .expect("Failed to create surface for secondary window");

        let surface_caps = surface.get_capabilities(&gpu.shared.adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let scale = window.scale_factor();
        let (phys_w, phys_h) = window.size();
        let phys_w = phys_w.max(1);
        let phys_h = phys_h.max(1);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: phys_w,
            height: phys_h,
            present_mode: if self.config.vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.shared.device, &surface_config);

        let win_surface = crate::gpu::WindowSurface { surface, surface_config };

        let logical_w = (phys_w as f64 / scale).max(1.0) as u32;
        let logical_h = (phys_h as f64 / scale).max(1.0) as u32;
        let renderer = Renderer::new(&gpu.shared, surface_format, phys_w, phys_h, logical_w, logical_h, self.config.font_family.clone());

        if let Some(icon_data) = self.config.icon_font_data {
            renderer.font_atlas.lock().unwrap().set_icon_font_data(icon_data.to_vec());
        }

        let mut build_ctx = crate::widget::BuildContext::root();
        let widget = build_fn(&mut build_ctx);
        let mut tree = ElementTree::new();
        tree.text_measure = Some(renderer.font_atlas.clone() as std::sync::Arc<dyn crate::widget::context::TextMeasure>);
        renderer.font_atlas.lock().unwrap().set_scale_factor(self.scale_factor as f32);
        tree.image_store = Some(renderer.image_store.clone());
        #[cfg(feature = "clipboard")]
        { tree.clipboard = self.clipboard.clone(); }

        let element = widget.create_element();
        let root_id = tree.insert_with_type_id(element, None, widget.as_any().type_id());
        widget.mount(&mut tree, root_id);

        let style_engine = self.style_engine.clone();

        Self::apply_styles_to_tree(&mut tree, &style_engine);

        loop {
            if !tree.rebuild_if_needed(root_id) { break; }
            Self::apply_styles_to_tree(&mut tree, &style_engine);
        }

        let safe = &tree.safe_area;
        let layout_h = (logical_h as f32 - safe.top - safe.bottom).max(0.0);
        let layout_w = logical_w as f32 - safe.left - safe.right;
        let constraints = crate::layout::Constraints::new(0.0, layout_w, 0.0, layout_h);
        tree.layout(root_id, constraints);

        let sw = SecondaryWindow {
            name: wc.name,
            window,
            surface: win_surface,
            renderer,
            tree,
            style_engine,
            root_id: Some(root_id),
            display_list: DisplayList::new(),
            cursor_position: Point::zero(),
            current_cursor: CursorIcon::Default,
            last_click_time: None,
            last_click_pos: None,
            double_click_interval: self.double_click_interval,
            focus_manager: FocusManager::new(),
            scale_factor: scale,
            width: phys_w,
            height: phys_h,
        };
        self.secondary_windows.insert(win_id, sw);
    }

    pub(in crate::app) fn handle_secondary_window_event(
        &mut self,
        window_id: winit::window::WindowId,
        event: &winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent;
        use crate::input::Event as UiEvent;

        match event {
            WindowEvent::CloseRequested => {
                self.secondary_windows.remove(&window_id);
            }
            WindowEvent::Resized(physical_size) => {
                if physical_size.width == 0 || physical_size.height == 0 {
                    return;
                }
                if let Some(sw) = self.secondary_windows.get_mut(&window_id) {
                    if let Some(gpu) = self.gpu.as_ref() {
                        sw.width = physical_size.width;
                        sw.height = physical_size.height;
                        sw.surface.resize(&gpu.shared.device, physical_size.width, physical_size.height);
                        let logical_w = (physical_size.width as f64 / sw.scale_factor) as u32;
                        let logical_h = (physical_size.height as f64 / sw.scale_factor) as u32;
                        sw.renderer.resize(&gpu.shared.device, physical_size.width, physical_size.height, logical_w, logical_h);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(sw) = self.secondary_windows.get_mut(&window_id) {
                    let sf = sw.scale_factor as f32;
                    sw.cursor_position = Point::new(position.x as f32 / sf, position.y as f32 / sf);
                    let evt = UiEvent::MouseMove(sw.cursor_position);
                    if let Some(root_id) = sw.root_id {
                        sw.tree.handle_event(root_id, &evt);
                        let new_cursor = sw.tree.cursor_request.unwrap_or(CursorIcon::Default);
                        if new_cursor != sw.current_cursor {
                            sw.current_cursor = new_cursor;
                            sw.window.set_cursor_icon(Self::map_cursor_icon(new_cursor));
                        }
                    }
                    sw.window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(sw) = self.secondary_windows.get_mut(&window_id) {
                    let pos = sw.cursor_position;
                    let btn = super::super::input_mapping::map_mouse_button(*button);
                    let evt = match state {
                        winit::event::ElementState::Pressed => {
                            let now = web_time::Instant::now();
                            let is_double = sw.last_click_time
                                .map(|t| now.duration_since(t) < sw.double_click_interval)
                                .unwrap_or(false)
                                && sw.last_click_pos
                                    .map(|p| (p.x - pos.x).abs() < 5.0 && (p.y - pos.y).abs() < 5.0)
                                    .unwrap_or(false);
                            if is_double {
                                // Сброс, чтобы тройной клик не дал второй
                                // «двойной» на третьем нажатии (как в основном
                                // хендлере event_handling.rs).
                                sw.last_click_time = None;
                                sw.last_click_pos = None;
                                UiEvent::DoubleClick { position: pos, button: btn }
                            } else {
                                sw.last_click_time = Some(now);
                                sw.last_click_pos = Some(pos);
                                UiEvent::MouseDown { position: pos, button: btn }
                            }
                        }
                        winit::event::ElementState::Released => {
                            UiEvent::MouseUp { position: pos, button: btn }
                        }
                    };
                    if let Some(root_id) = sw.root_id {
                        sw.tree.handle_event(root_id, &evt);
                    }
                    sw.window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                if let Some(sw) = self.secondary_windows.get_mut(&window_id) {
                    if key_event.state == winit::event::ElementState::Pressed {
                        if let Some(ref text) = key_event.text {
                            for ch in text.chars() {
                                if !ch.is_control() {
                                    let evt = UiEvent::CharInput(ch);
                                    if let Some(root_id) = sw.root_id {
                                        sw.tree.handle_event(root_id, &evt);
                                    }
                                }
                            }
                        }
                    }
                    let key = match key_event.physical_key {
                        winit::keyboard::PhysicalKey::Code(code) => super::super::input_mapping::map_key_code(code),
                        winit::keyboard::PhysicalKey::Unidentified(_) => return,
                    };
                    let evt = match key_event.state {
                        winit::event::ElementState::Pressed => UiEvent::KeyDown(key),
                        winit::event::ElementState::Released => UiEvent::KeyUp(key),
                    };
                    if let Some(root_id) = sw.root_id {
                        sw.tree.handle_event(root_id, &evt);
                    }
                    sw.window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(sw) = self.secondary_windows.get_mut(&window_id) {
                    let (dx, dy) = match delta {
                        winit::event::MouseScrollDelta::LineDelta(x, y) => (*x * 40.0, *y * 40.0),
                        winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                    };
                    let pos = sw.cursor_position;
                    let evt = UiEvent::MouseWheel { delta: dy, delta_x: dx, position: pos };
                    if let Some(root_id) = sw.root_id {
                        sw.tree.handle_event(root_id, &evt);
                    }
                    sw.window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = crate::input::Modifiers {
                    shift: state.shift_key(),
                    ctrl: state.control_key(),
                    alt: state.alt_key(),
                    meta: state.super_key(),
                };
                for sw in self.secondary_windows.values_mut() {
                    sw.tree.modifiers = self.modifiers;
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_secondary_window(window_id);
            }
            _ => {}
        }
    }

    pub(in crate::app) fn render_secondary_window(&mut self, window_id: winit::window::WindowId) {
        let gpu = match self.gpu.as_ref() {
            Some(g) => g,
            None => return,
        };

        let sw = match self.secondary_windows.get_mut(&window_id) {
            Some(sw) => sw,
            None => return,
        };

        if let Some(root_id) = sw.root_id {
            let mut any_rebuilt = false;
            for _ in 0..8 {
                if !sw.tree.rebuild_if_needed(root_id) { break; }
                any_rebuilt = true;
                Self::apply_styles_to_tree(&mut sw.tree, &sw.style_engine);
            }
            let _ = any_rebuilt;
        }

        crate::signal::drain_and_run_effects();

        if let Some(root_id) = sw.root_id {
            let logical_w = (sw.width as f64 / sw.scale_factor) as f32;
            let logical_h = (sw.height as f64 / sw.scale_factor) as f32;
            let constraints = crate::layout::Constraints::new(0.0, logical_w, 0.0, logical_h);
            sw.tree.layout(root_id, constraints);

            sw.display_list.clear();
            let surface_size = crate::core::Size::new(logical_w, logical_h);
            sw.display_list.set_surface_size(surface_size);
            sw.display_list.set_scale_factor(sw.scale_factor as f32);
            let clip = crate::core::Rect::new(crate::core::Point::zero(), surface_size);
            sw.tree.build_display_list(root_id, &mut sw.display_list, clip);

            sw.renderer.render(&gpu.shared, &sw.surface, &sw.display_list, self.config.background_color);
        }
    }

    pub(in crate::app) fn handle_window_moved(&mut self, moved_id: winit::window::WindowId, new_x: i32, new_y: i32) {
        let threshold = match self.sticky_threshold {
            Some(t) => t as i32,
            None => return,
        };

        let old_pos = self.window_positions.get(&moved_id).copied();
        self.window_positions.insert(moved_id, (new_x, new_y));

        let mut all_windows: Vec<(winit::window::WindowId, (i32, i32), (i32, i32))> = Vec::new();
        if let Some(ref win) = self.window {
            let id = win.winit_window().id();
            let (w, h) = win.size();
            if let Ok(pos) = win.winit_window().outer_position() {
                self.window_positions.insert(id, (pos.x, pos.y));
                all_windows.push((id, (pos.x, pos.y), (w as i32, h as i32)));
            }
        }
        for (&id, sw) in &self.secondary_windows {
            let (w, h) = (sw.width as i32, sw.height as i32);
            if let Ok(pos) = sw.window.winit_window().outer_position() {
                self.window_positions.insert(id, (pos.x, pos.y));
                all_windows.push((id, (pos.x, pos.y), (w, h)));
            }
        }

        let moved_idx = all_windows.iter().position(|(id, _, _)| *id == moved_id);
        if moved_idx.is_none() { return; }
        let moved_idx = moved_idx.unwrap();
        let (_, moved_pos, moved_size) = all_windows[moved_idx];

        let delta = if let Some((ox, oy)) = old_pos {
            (new_x - ox, new_y - oy)
        } else {
            (0, 0)
        };

        if delta.0 != 0 || delta.1 != 0 {
            let group = self.sticky_groups.iter().find(|g| g.contains(&moved_id)).cloned();
            if let Some(group) = group {
                for &gid in &group {
                    if gid == moved_id { continue; }
                    if let Some(&(gx, gy)) = self.window_positions.get(&gid) {
                        let new_gx = gx + delta.0;
                        let new_gy = gy + delta.1;
                        self.set_window_position(gid, new_gx, new_gy);
                        self.window_positions.insert(gid, (new_gx, new_gy));
                    }
                }
            }
        }

        let mut snap_x = moved_pos.0;
        let mut snap_y = moved_pos.1;
        let mut snapped_to: Option<winit::window::WindowId> = None;

        for (i, &(other_id, other_pos, other_size)) in all_windows.iter().enumerate() {
            if i == moved_idx { continue; }

            let r2l = (moved_pos.0 + moved_size.0 - other_pos.0).abs();
            let l2r = (moved_pos.0 - (other_pos.0 + other_size.0)).abs();
            let b2t = (moved_pos.1 + moved_size.1 - other_pos.1).abs();
            let t2b = (moved_pos.1 - (other_pos.1 + other_size.1)).abs();

            let v_overlap = moved_pos.1 < other_pos.1 + other_size.1 && moved_pos.1 + moved_size.1 > other_pos.1;
            let h_overlap = moved_pos.0 < other_pos.0 + other_size.0 && moved_pos.0 + moved_size.0 > other_pos.0;

            if v_overlap {
                if r2l < threshold { snap_x = other_pos.0 - moved_size.0; snapped_to = Some(other_id); }
                if l2r < threshold { snap_x = other_pos.0 + other_size.0; snapped_to = Some(other_id); }
            }
            if h_overlap {
                if b2t < threshold { snap_y = other_pos.1 - moved_size.1; snapped_to = Some(other_id); }
                if t2b < threshold { snap_y = other_pos.1 + other_size.1; snapped_to = Some(other_id); }
            }

            if v_overlap && (r2l < threshold || l2r < threshold) {
                if (moved_pos.1 - other_pos.1).abs() < threshold {
                    snap_y = other_pos.1;
                }
            }
            if h_overlap && (b2t < threshold || t2b < threshold) {
                if (moved_pos.0 - other_pos.0).abs() < threshold {
                    snap_x = other_pos.0;
                }
            }
        }

        if snap_x != moved_pos.0 || snap_y != moved_pos.1 {
            self.set_window_position(moved_id, snap_x, snap_y);
            self.window_positions.insert(moved_id, (snap_x, snap_y));

            if let Some(other_id) = snapped_to {
                self.merge_sticky_group(moved_id, other_id);
            }
        } else {
            let detach_threshold = threshold * 3;
            let should_detach = if let Some(group) = self.sticky_groups.iter().find(|g| g.contains(&moved_id)) {
                group.iter().filter(|&&gid| gid != moved_id).all(|&gid| {
                    if let Some(&(gx, gy)) = self.window_positions.get(&gid) {
                        let dx = (moved_pos.0 - gx).abs();
                        let dy = (moved_pos.1 - gy).abs();
                        dx > detach_threshold * 3 || dy > detach_threshold * 3
                    } else {
                        true
                    }
                })
            } else {
                false
            };
            if should_detach {
                self.remove_from_sticky_group(moved_id);
            }
        }
    }

    fn set_window_position(&self, id: winit::window::WindowId, x: i32, y: i32) {
        if let Some(ref win) = self.window {
            if win.winit_window().id() == id {
                win.winit_window().set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                return;
            }
        }
        if let Some(sw) = self.secondary_windows.get(&id) {
            sw.window.winit_window().set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
        }
    }

    fn merge_sticky_group(&mut self, a: winit::window::WindowId, b: winit::window::WindowId) {
        let group_a = self.sticky_groups.iter().position(|g| g.contains(&a));
        let group_b = self.sticky_groups.iter().position(|g| g.contains(&b));

        match (group_a, group_b) {
            (Some(ga), Some(gb)) if ga != gb => {
                let group_b_set = self.sticky_groups.remove(gb.max(ga));
                let ga_idx = if gb < ga { ga - 1 } else { ga };
                let group_a_set = self.sticky_groups.remove(ga_idx.min(gb));
                let mut merged = group_a_set;
                merged.extend(group_b_set);
                self.sticky_groups.push(merged);
            }
            (Some(_), Some(_)) => {}
            (Some(ga), None) => {
                self.sticky_groups[ga].insert(b);
            }
            (None, Some(gb)) => {
                self.sticky_groups[gb].insert(a);
            }
            (None, None) => {
                let mut group = std::collections::HashSet::new();
                group.insert(a);
                group.insert(b);
                self.sticky_groups.push(group);
            }
        }
    }

    fn remove_from_sticky_group(&mut self, id: winit::window::WindowId) {
        for group in &mut self.sticky_groups {
            group.remove(&id);
        }
        self.sticky_groups.retain(|g| g.len() > 1);
    }
}
