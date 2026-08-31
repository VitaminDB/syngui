use crate::input::{DragData, Event, Modifiers};
use super::handler::AppHandler;
use super::input_mapping::{map_mouse_button, map_key_code};
use super::user_event::SynGuiUserEvent;
use crate::core::Point;
use web_time::Instant;

impl winit::application::ApplicationHandler<SynGuiUserEvent> for AppHandler {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&"[syngui] resumed() called".into());

        if self.window.is_none() {
            self.init(event_loop);
        } else {
            #[cfg(target_os = "android")]
            {
                self.recreate_surface();
            }
        }
        #[cfg(target_os = "android")]
        {
            self.android_suspended = false;
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn suspended(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        self.drop_surface();
        // Android: в suspended winit игнорирует wake-up'ы EventLoopProxy, и
        // run_on_main_thread-колбэки (фоновые тики плеера, команды медиа-сессии)
        // зависали бы до resume. Медленный пульс по таймауту дренирует их.
        #[cfg(target_os = "android")]
        {
            self.android_suspended = true;
            _event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                std::time::Instant::now() + std::time::Duration::from_millis(1000),
            ));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        if let winit::event::WindowEvent::Moved(pos) = &event {
            self.handle_window_moved(window_id, pos.x, pos.y);
        }

        if self.main_window_id.is_some() && Some(window_id) != self.main_window_id {
            self.handle_secondary_window_event(window_id, &event);
            return;
        }

        #[cfg(feature = "accessibility")]
        {
            if let (Some(ref mut ak_adapter), Some(ref window)) = (&mut self.accesskit_adapter, &self.window) {
                ak_adapter.process_event(window.winit_window(), &event);
            }
        }

        match event {
            winit::event::WindowEvent::CloseRequested => {
                #[cfg(target_os = "android")]
                {
                    let back_event = Event::BackPressed;
                    if let Some(root_id) = self.root_id {
                        let result = self.tree.handle_event(root_id, &back_event);
                        if result.is_handled() {
                            if let Some(ref window) = self.window {
                                window.request_redraw();
                            }
                            return;
                        }
                    }
                    event_loop.exit();
                }
                #[cfg(not(target_os = "android"))]
                {
                    if self.should_hide_on_close() {
                        self.hide_main_window();
                    } else {
                        #[cfg(all(feature = "tray", not(target_arch = "wasm32"), not(target_os = "android")))]
                        { self.tray.take(); }
                        #[cfg(all(feature = "single-instance", not(target_arch = "wasm32"), not(target_os = "android")))]
                        { self.single_instance.take(); }
                        event_loop.exit();
                    }
                }
            }
            winit::event::WindowEvent::Resized(physical_size) => {

                if physical_size.width == 0 || physical_size.height == 0 {
                    return;
                }

                self.sync_window_flags();

                self.config.width = physical_size.width;
                self.config.height = physical_size.height;

                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.window_surface.surface_config.width = physical_size.width;
                    gpu.window_surface.surface_config.height = physical_size.height;
                    gpu.window_surface.surface.configure(&gpu.shared.device, &gpu.window_surface.surface_config);
                }

                if let Some(renderer) = self.renderer.as_mut() {
                    let logical_w = (physical_size.width as f64 / self.scale_factor) as u32;
                    let logical_h = (physical_size.height as f64 / self.scale_factor) as u32;
                    if let Some(gpu) = self.gpu.as_ref() {
                        renderer.resize(&gpu.shared.device, self.config.width, self.config.height, logical_w, logical_h);
                    }
                }

                if let Some(root_id) = self.root_id {
                    let logical_w = (physical_size.width as f64 / self.scale_factor) as f32;
                    let logical_h = (physical_size.height as f64 / self.scale_factor) as f32;
                    let safe = &self.tree.safe_area;
                    let layout_h = (logical_h - safe.top - safe.bottom).max(0.0);
                    let layout_w = logical_w - safe.left - safe.right;
                    self.tree.root_offset = crate::core::Point::new(safe.left, safe.top);
                    crate::viewport::publish(crate::core::Size::new(layout_w, layout_h));
                    let constraints = crate::layout::Constraints::new(
                        0.0, layout_w,
                        0.0, layout_h,
                    );
                    self.tree.layout(root_id, constraints);
                    self.a11y_dirty = true;
                }

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            winit::event::WindowEvent::Focused(_) => {
                self.sync_window_flags();
            }
            winit::event::WindowEvent::ThemeChanged(theme) => {
                self.handle_theme_changed(theme);
            }
            winit::event::WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor;

                if let Some(renderer) = self.renderer.as_mut() {
                    let logical_w = (self.config.width as f64 / scale_factor) as u32;
                    let logical_h = (self.config.height as f64 / scale_factor) as u32;
                    if let Some(gpu) = self.gpu.as_ref() {
                        renderer.resize(&gpu.shared.device, self.config.width, self.config.height, logical_w, logical_h);
                    }
                    if let Ok(mut atlas) = renderer.font_atlas.lock() {
                        atlas.set_scale_factor(scale_factor as f32);
                    }
                    self.tree.mark_all_dirty(crate::widget::DirtyFlags::LAYOUT);
                }

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            winit::event::WindowEvent::CursorLeft { .. } => {
                if let Some(root_id) = self.root_id {
                    let off_screen = Event::MouseMove(Point::new(-1.0, -1.0));
                    let _ = self.tree.handle_event(root_id, &off_screen);
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                let sf = self.scale_factor as f32;
                self.cursor_position = Point::new(position.x as f32 / sf, position.y as f32 / sf);

                if self.devtools_handle_cursor_moved() {
                    return;
                }

                let event = Event::MouseMove(self.cursor_position);
                if let Some(root_id) = self.root_id {
                    crate::perf::incr(crate::perf::Counter::MmDispatch);
                    let _mm_t = web_time::Instant::now();
                    let result = self.tree.handle_event(root_id, &event);
                    crate::perf::add_time(crate::perf::TimeKind::MouseMoveHandleEvent, _mm_t.elapsed());
                    if result.is_handled() {
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }

                    if self.tree.drag_state.is_some() {
                        let pos = self.cursor_position;
                        if let Some(ref mut drag) = self.tree.drag_state {
                            drag.current_pos = pos;
                        }
                        let data = self.tree.drag_state.as_ref().unwrap().data.clone();
                        let drag_move = Event::DragMove { position: pos, data };
                        self.tree.dispatch_drag_event(&drag_move);
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }

                    self.update_cursor();
                }
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                let pos = self.cursor_position;
                let is_press = state == winit::event::ElementState::Pressed;

                if self.devtools_handle_mouse_input(is_press, button) {
                    return;
                }

                if is_press {
                    self.update_focus_from_click(pos);
                }

                if !is_press && self.tree.drag_state.is_some() {
                    if let Some(root_id) = self.root_id {
                        let data = self.tree.drag_state.as_ref().unwrap().data.clone();
                        let drop_event = Event::Drop { position: pos, data };
                        self.tree.dispatch_drag_event(&drop_event);
                        let drag_end = Event::DragEnd { cancelled: false };
                        self.tree.handle_event(root_id, &drag_end);
                        self.tree.drag_state = None;
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                } else {
                    let mapped_button = map_mouse_button(button);

                    if is_press {
                        let is_double = if let (Some(last_time), Some(last_pos)) =
                            (self.last_click_time, self.last_click_pos)
                        {
                            let elapsed = Instant::now().duration_since(last_time);
                            let dx = pos.x - last_pos.x;
                            let dy = pos.y - last_pos.y;
                            let dist = (dx * dx + dy * dy).sqrt();
                            elapsed < self.double_click_interval && dist < 4.0
                        } else {
                            false
                        };

                        if is_double {
                            let dbl_event = Event::DoubleClick { button: mapped_button, position: pos };
                            if let Some(root_id) = self.root_id {
                                self.tree.handle_event(root_id, &dbl_event);
                            }
                            self.last_click_time = None;
                            self.last_click_pos = None;
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        } else {
                            self.last_click_time = Some(Instant::now());
                            self.last_click_pos = Some(pos);
                            let event = Event::MouseDown { button: mapped_button, position: pos };
                            if let Some(root_id) = self.root_id {
                                let result = self.tree.handle_event(root_id, &event);
                                self.devtools_log_event(&format!("MouseDown({:?})", mapped_button), &result);
                                let _ = result;
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                        }
                    } else {
                        let event = Event::MouseUp { button: mapped_button, position: pos };
                        if let Some(root_id) = self.root_id {
                            let result = self.tree.handle_event(root_id, &event);
                            self.devtools_log_event(&format!("MouseUp({:?})", mapped_button), &result);
                            if result.is_handled() {
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                        }
                    }
                }

                self.process_virtual_keyboard_request();
                self.process_window_drag_request();
                self.process_window_control_requests();
                if self.take_window_close_request() {
                    if self.should_hide_on_close() {
                        self.hide_main_window();
                    } else {
                        #[cfg(all(feature = "tray", not(target_arch = "wasm32"), not(target_os = "android")))]
                        { self.tray.take(); }
                        #[cfg(all(feature = "single-instance", not(target_arch = "wasm32"), not(target_os = "android")))]
                        { self.single_instance.take(); }
                        event_loop.exit();
                    }
                    return;
                }
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                let (scroll_delta, scroll_delta_x) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (y * 40.0, x * 40.0),
                    winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.y as f32, pos.x as f32),
                };

                if self.devtools_handle_mouse_wheel(scroll_delta) {
                    return;
                }

                let event = Event::MouseWheel {
                    delta: scroll_delta,
                    delta_x: scroll_delta_x,
                    position: self.cursor_position,
                };
                if let Some(root_id) = self.root_id {
                    let result = self.tree.handle_event(root_id, &event);
                    if result.is_handled() {
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                }
            }
            winit::event::WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = Modifiers {
                    shift: mods.state().shift_key(),
                    ctrl: mods.state().control_key(),
                    alt: mods.state().alt_key(),
                    meta: mods.state().super_key(),
                };
                self.tree.modifiers = self.modifiers;
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::PhysicalKey;

                #[cfg(target_os = "android")]
                {
                    return;
                }

                if event.state == winit::event::ElementState::Pressed {
                    if let winit::keyboard::Key::Named(winit::keyboard::NamedKey::BrowserBack) = &event.logical_key {
                        let back_event = Event::BackPressed;
                        if let Some(root_id) = self.root_id {
                            let result = self.tree.handle_event(root_id, &back_event);
                            if !result.is_handled() {
                                event_loop.exit();
                            }
                        }
                        return;
                    }
                }

                // В браузере сюда F11 попадает, только если приложение её
                // перехватило (`AppBuilder::capture_function_keys`); иначе
                // клавишу обрабатывает сам браузер.
                #[cfg(not(target_os = "android"))]
                if event.state == winit::event::ElementState::Pressed {
                    if let PhysicalKey::Code(winit::keyboard::KeyCode::F11) = event.physical_key {
                        if let Some(window) = &self.window {
                            let win = window.winit_window();
                            let is_fs = win.fullscreen().is_some();
                            win.set_fullscreen(if is_fs {
                                None
                            } else {
                                Some(winit::window::Fullscreen::Borderless(None))
                            });
                            window.request_redraw();
                        }
                        return;
                    }
                }

                if event.state == winit::event::ElementState::Pressed {
                    if let PhysicalKey::Code(winit::keyboard::KeyCode::F12) = event.physical_key {
                        if let Some(ref mut devtools) = self.devtools {
                            let was_enabled = devtools.is_enabled();
                            devtools.toggle();
                            if !was_enabled && devtools.is_enabled() && devtools.expanded_nodes_count() == 0 {
                                devtools.auto_expand(&self.tree, 4);
                            }
                        } else {
                            let mut dt = crate::devtools::DevTools::new();
                            dt.toggle();
                            dt.auto_expand(&self.tree, 4);
                            self.devtools = Some(dt);
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                        return;
                    }
                    if self.modifiers.ctrl && self.modifiers.alt {
                        if let PhysicalKey::Code(winit::keyboard::KeyCode::KeyC) = event.physical_key {
                            if let Some(ref mut devtools) = self.devtools {
                                if devtools.is_enabled() {
                                    devtools.toggle_picking();
                                    if let Some(window) = &self.window {
                                        window.request_redraw();
                                    }
                                    return;
                                }
                            }
                        }
                    }
                }

                if event.state == winit::event::ElementState::Pressed {
                    if let PhysicalKey::Code(winit::keyboard::KeyCode::Tab) = event.physical_key {
                        let focused_wants_tab = self
                            .tree
                            .focused_element
                            .and_then(|id| self.tree.get(id))
                            .map(|el| el.wants_tab())
                            .unwrap_or(false);

                        if !focused_wants_tab {
                            let old_focus = self
                                .focus_manager
                                .current_focus()
                                .or(self.tree.focused_element);
                            let new_focus = if self.modifiers.shift {
                                self.focus_manager.previous_focus()
                            } else {
                                self.focus_manager.next_focus()
                            };

                            if let Some(new_id) = new_focus {
                                if let Some(old_id) = old_focus {
                                    if old_id != new_id {
                                        self.tree.dispatch_event_to(old_id, &Event::FocusLost);
                                    }
                                }
                                self.tree.dispatch_event_to(new_id, &Event::FocusGained);
                                self.tree.focused_element = Some(new_id);
                                self.a11y_tree.update_focus(new_id);
                                self.a11y_dirty = true;

                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                            return;
                        }
                    }
                }

                #[cfg(not(target_os = "android"))]
                if event.state == winit::event::ElementState::Pressed {
                    if let Some(text) = event.text {
                        for ch in text.chars() {
                            if !ch.is_control() {
                                let evt = Event::CharInput(ch);
                                if let Some(root_id) = self.root_id {
                                    self.tree.handle_event(root_id, &evt);
                                }
                            }
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                }

                let key = match event.physical_key {
                    PhysicalKey::Code(code) => map_key_code(code),
                    PhysicalKey::Unidentified(_) => return,
                };

                let evt = match event.state {
                    winit::event::ElementState::Pressed => Event::KeyDown(key),
                    winit::event::ElementState::Released => Event::KeyUp(key),
                };

                if let Some(root_id) = self.root_id {
                    let result = self.tree.handle_event(root_id, &evt);
                    self.devtools_log_event(&format!("{:?}", evt), &result);
                    if result.is_handled() {
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                }
            }
            winit::event::WindowEvent::Ime(ime_event) => {
                #[cfg(target_os = "android")]
                { let _ = ime_event; }
                #[cfg(not(target_os = "android"))]
                {
                    let event = match ime_event {
                        winit::event::Ime::Commit(text) => Event::ImeCommit(text),
                        winit::event::Ime::Preedit(text, cursor) => Event::ImePreedit { text, cursor },
                        winit::event::Ime::Enabled => Event::ImeEnabled,
                        winit::event::Ime::Disabled => Event::ImeDisabled,
                    };
                    if let Some(root_id) = self.root_id {
                        self.tree.handle_event(root_id, &event);
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }

            winit::event::WindowEvent::Touch(touch) => {
                let sf = self.scale_factor as f32;
                let position = Point::new(
                    touch.location.x as f32 / sf,
                    touch.location.y as f32 / sf,
                );
                let id = touch.id;

                // Порог, после которого жест считается скроллом, а не тапом.
                const TAP_SLOP: f32 = 8.0;

                match touch.phase {
                    winit::event::TouchPhase::Started => {
                        self.cursor_position = position;
                        // Клик НЕ синтезируется здесь: жест ещё может оказаться
                        // скроллом. Тап синтезируется на отпускании (ниже).
                        if self.touch_tap.is_none() {
                            self.touch_tap = Some((id, position, false));
                        }
                        if let Some(root_id) = self.root_id {
                            let touch_event = Event::TouchStart { id, position };
                            self.tree.handle_event(root_id, &touch_event);
                        }
                    }
                    winit::event::TouchPhase::Moved => {
                        self.cursor_position = position;
                        if let Some((tid, start, moved)) = &mut self.touch_tap {
                            if *tid == id
                                && !*moved
                                && ((position.x - start.x).abs() > TAP_SLOP
                                    || (position.y - start.y).abs() > TAP_SLOP)
                            {
                                *moved = true;
                            }
                        }
                        if let Some(root_id) = self.root_id {
                            let touch_event = Event::TouchMove { id, position };
                            self.tree.handle_event(root_id, &touch_event);
                        }
                    }
                    winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                        if let Some(root_id) = self.root_id {
                            let touch_event = Event::TouchEnd { id, position };
                            self.tree.handle_event(root_id, &touch_event);
                        }
                        let is_tap = match self.touch_tap {
                            Some((tid, _, moved)) if tid == id => {
                                self.touch_tap = None;
                                !moved
                                    && matches!(touch.phase, winit::event::TouchPhase::Ended)
                            }
                            _ => false,
                        };
                        if is_tap {
                            // Палец не сдвинулся — это клик: down+up в точке отпускания.
                            self.update_focus_from_click(position);
                            if let Some(root_id) = self.root_id {
                                let down = Event::MouseDown {
                                    button: crate::input::MouseButton::Left,
                                    position,
                                };
                                self.tree.handle_event(root_id, &down);
                                let up = Event::MouseUp {
                                    button: crate::input::MouseButton::Left,
                                    position,
                                };
                                self.tree.handle_event(root_id, &up);
                            }
                        }
                    }
                }

                if let Some(window) = &self.window {
                    window.request_redraw();
                }

                self.process_virtual_keyboard_request();

            }

            winit::event::WindowEvent::RedrawRequested => {
                #[cfg(target_arch = "wasm32")]
                web_sys::console::log_1(&format!("[syngui] RedrawRequested, gpu={}", self.gpu.is_some()).into());
                self.render();
            }

            winit::event::WindowEvent::HoveredFile(path) => {
                let data = DragData::external_file(&path);
                let pos = self.cursor_position;
                if self.root_id.is_some() {
                    self.tree.dispatch_drag_event(&Event::DragEnter {
                        position: pos,
                        data,
                    });
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            winit::event::WindowEvent::HoveredFileCancelled => {
                if self.root_id.is_some() {
                    self.tree.dispatch_drag_event(&Event::DragLeave);
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            winit::event::WindowEvent::DroppedFile(path) => {
                let data = DragData::external_file(&path);
                let pos = self.cursor_position;
                if self.root_id.is_some() {
                    self.tree.dispatch_drag_event(&Event::Drop {
                        position: pos,
                        data,
                    });
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.pending_show && self.window.is_none() {
            self.pending_show = false;
            self.init(event_loop);
            self.main_window_visible = true;
            if let Some(window) = self.window.as_ref() {
                window.focus();
                window.request_redraw();
            }
        }

        self.update();
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        // Android-фон: продлеваем пульс, пока не возобновимся (см. suspended()).
        #[cfg(target_os = "android")]
        if self.android_suspended {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                std::time::Instant::now() + std::time::Duration::from_millis(1000),
            ));
        }
    }

    fn user_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        event: SynGuiUserEvent,
    ) {
        match event {
            SynGuiUserEvent::TrayShow | SynGuiUserEvent::Activate => {
                self.show_main_window();
            }
            SynGuiUserEvent::TrayHide => {
                self.hide_main_window();
            }
            SynGuiUserEvent::TrayToggle => {
                self.toggle_main_window_visibility();
            }
            SynGuiUserEvent::TrayExit => {
                #[cfg(all(feature = "tray", not(target_arch = "wasm32"), not(target_os = "android")))]
                { self.tray.take(); }
                #[cfg(all(feature = "single-instance", not(target_arch = "wasm32"), not(target_os = "android")))]
                { self.single_instance.take(); }
                event_loop.exit();
            }
            SynGuiUserEvent::MenuItem(id) => {
                log::debug!("[syngui] tray menu item: {id}");
            }
            SynGuiUserEvent::MainThreadWake => {
                // Очередь run_on_main_thread: дренируем прямо здесь — рендера
                // (и его дренажа) в фоне может не быть вовсе.
                crate::async_runtime::poll_main_thread_callbacks();
            }
            #[cfg(feature = "wayland-dnd")]
            SynGuiUserEvent::WaylandDnd(ev) => {
                self.handle_wayland_dnd(ev);
            }
        }
    }
}

#[cfg(feature = "wayland-dnd")]
impl AppHandler {
    fn handle_wayland_dnd(&mut self, ev: super::user_event::WaylandDndEvent) {
        use super::user_event::WaylandDndEvent;
        if self.root_id.is_none() {
            return;
        }
        match ev {
            WaylandDndEvent::Enter { x, y } => {
                let data = crate::input::DragData::new(
                    crate::input::DragData::TYPE_FILE,
                    "",
                    0,
                );
                self.cursor_position = Point::new(x, y);
                self.tree.dispatch_drag_event(&Event::DragEnter {
                    position: Point::new(x, y),
                    data,
                });
                self.request_redraw();
            }
            WaylandDndEvent::Motion { x, y } => {
                let data = crate::input::DragData::new(
                    crate::input::DragData::TYPE_FILE,
                    "",
                    0,
                );
                self.cursor_position = Point::new(x, y);
                self.tree.dispatch_drag_event(&Event::DragMove {
                    position: Point::new(x, y),
                    data,
                });
                self.request_redraw();
            }
            WaylandDndEvent::Leave => {
                self.tree.dispatch_drag_event(&Event::DragLeave);
                self.request_redraw();
            }
            WaylandDndEvent::Drop { x, y, paths } => {
                self.cursor_position = Point::new(x, y);
                for path in paths {
                    let data = crate::input::DragData::external_file(&path);
                    self.tree.dispatch_drag_event(&Event::Drop {
                        position: Point::new(x, y),
                        data,
                    });
                }
                self.request_redraw();
            }
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl AppHandler {
    pub(super) fn devtools_log_event(&mut self, event_type: &str, result: &crate::input::EventResult) {
        if let Some(ref mut devtools) = self.devtools {
            if devtools.is_enabled() && !devtools.event_log_paused() {
                let result_str = match result {
                    crate::input::EventResult::Handled => "Handled",
                    crate::input::EventResult::Captured => "Captured",
                    crate::input::EventResult::Ignored => "Ignored",
                };
                devtools.log_event(crate::devtools::EventLogEntry {
                    timestamp_ms: self.app_start_time.elapsed().as_secs_f64() * 1000.0,
                    event_type: event_type.to_string(),
                    result: result_str.to_string(),
                });
            }
        }
    }

    pub(super) fn devtools_handle_cursor_moved(&mut self) -> bool {
        let surface = self.logical_surface_size();
        let cursor = self.cursor_position;
        let devtools = match self.devtools.as_mut() {
            Some(d) if d.is_enabled() => d,
            _ => return false,
        };

        devtools.update_mouse_pos(cursor);

        if devtools.is_picking() && !devtools.contains_point(cursor, surface) {
            devtools.update_picking_hover(&self.tree, cursor);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }

        if devtools.contains_point(cursor, surface) || devtools.is_resizing() {
            let move_event = Event::MouseMove(cursor);
            devtools.handle_mouse_event(&move_event, surface, &self.tree);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return true;
        }

        false
    }

    pub(super) fn devtools_handle_mouse_input(
        &mut self,
        is_press: bool,
        button: winit::event::MouseButton,
    ) -> bool {
        let surface = self.logical_surface_size();
        let pos = self.cursor_position;
        let devtools = match self.devtools.as_mut() {
            Some(d) if d.is_enabled() => d,
            _ => return false,
        };

        if is_press && devtools.is_picking() && !devtools.contains_point(pos, surface) {
            devtools.complete_pick(&self.tree, pos);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return true;
        }

        if devtools.contains_point(pos, surface) || devtools.is_resizing() {
            let mapped = map_mouse_button(button);
            let evt = if is_press {
                Event::MouseDown { button: mapped, position: pos }
            } else {
                Event::MouseUp { button: mapped, position: pos }
            };
            devtools.handle_mouse_event(&evt, surface, &self.tree);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return true;
        }

        false
    }

    pub(super) fn devtools_handle_mouse_wheel(&mut self, scroll_delta: f32) -> bool {
        let surface = self.logical_surface_size();
        let pos = self.cursor_position;
        let devtools = match self.devtools.as_mut() {
            Some(d) if d.is_enabled() => d,
            _ => return false,
        };

        if devtools.contains_point(pos, surface) {
            let evt = Event::MouseWheel { delta: scroll_delta, delta_x: 0.0, position: pos };
            devtools.handle_mouse_event(&evt, surface, &self.tree);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return true;
        }

        false
    }
}
