use crate::a11y::FocusManager;
use crate::core::{Point, Rect, Size};
use crate::input::{CursorIcon, Event};
use crate::mss::parse_stylesheet_str;
use web_time::Instant;
use super::AppHandler;

/// Извлекает цвет переменной `--bg` из таблицы темы. Используется, чтобы
/// clear-color окна (а с ним полоса статус-бара и любые непокрытые области)
/// совпадал с фоном темы.
pub(super) fn parse_theme_bg(ss: &crate::mss::StyleSheet) -> Option<crate::core::Color> {
    let v = ss.get_variable("--bg")?;
    let c = v
        .as_color()
        .or_else(|| v.as_string().and_then(crate::mss::MssColor::parse))?;
    Some(crate::core::Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0))
}

impl AppHandler {
    pub(in crate::app) fn update_cursor(&mut self) {
        let new_cursor = self.tree.cursor_request.unwrap_or(CursorIcon::Default);
        if new_cursor != self.current_cursor {
            self.current_cursor = new_cursor;
            if let Some(window) = &self.window {
                window.set_cursor_icon(Self::map_cursor_icon(new_cursor));
            }
        }
    }

    pub(in crate::app) fn update_focus_from_click(&mut self, pos: Point) {
        let old_focus = self
            .focus_manager
            .current_focus()
            .or(self.tree.focused_element);

        if let Some(root_id) = self.root_id {
            let mut new_focus = None;
            let mut in_nonmodal_overlay = false;
            for entry in self.tree.overlay_stack.iter().rev() {
                if entry.modal {
                    new_focus = self.find_text_input_at(entry.element_id, pos);
                    break;
                }
                if entry.bounds.contains(pos) {
                    let candidate = self.find_text_input_at(entry.element_id, pos);
                    if candidate.is_some() {
                        new_focus = candidate;
                    } else {
                        in_nonmodal_overlay = true;
                    }
                    break;
                }
            }
            if in_nonmodal_overlay {
                return;
            }
            if new_focus.is_none() {
                new_focus = self.find_text_input_at(root_id, pos);
            }

            if new_focus == old_focus && new_focus.is_some() {
                return;
            }

            if let Some(old_id) = old_focus {
                if Some(old_id) != new_focus {
                    self.tree.dispatch_event_to(old_id, &Event::FocusLost);
                }
            }

            if let Some(element_id) = new_focus {
                if !self.focus_manager.set_focus(element_id) {
                    self.focus_manager.rebuild_tab_order(&self.tree, root_id);
                    self.focus_manager.set_focus(element_id);
                }
                self.tree.focused_element = Some(element_id);
                self.tree.dispatch_event_to(element_id, &Event::FocusGained);
                self.a11y_tree.update_focus(element_id);

                self.process_virtual_keyboard_request();
            } else {
                self.focus_manager.clear_focus();
                self.tree.focused_element = None;
                self.process_virtual_keyboard_request();
            }
            self.a11y_dirty = true;
        }
    }

    /// Autofocus (`focus_request_pending`) выставляет фокус только логически:
    /// элемент помечает себя focused, дерево — `focused_element`. Здесь этому
    /// элементу диспатчится настоящий `FocusGained` (и `FocusLost` прежнему),
    /// чтобы отработали побочные эффекты фокуса: запрос экранной клавиатуры
    /// Android, подсказка буфера обмена, a11y.
    pub(in crate::app) fn process_pending_autofocus(&mut self, root_id: crate::widget::ElementId) {
        let Some(id) = self.tree.pending_autofocus.take() else {
            return;
        };
        if !self.tree.elements.contains_key(&id) {
            return;
        }
        let old_focus = self.focus_manager.current_focus();
        if old_focus != Some(id) {
            if let Some(old_id) = old_focus {
                if self.tree.elements.contains_key(&old_id) {
                    self.tree.dispatch_event_to(old_id, &Event::FocusLost);
                }
            }
            if !self.focus_manager.set_focus(id) {
                self.focus_manager.rebuild_tab_order(&self.tree, root_id);
                self.focus_manager.set_focus(id);
            }
        }
        self.tree.focused_element = Some(id);
        self.tree.dispatch_event_to(id, &Event::FocusGained);
        self.a11y_tree.update_focus(id);
        self.a11y_dirty = true;
        self.process_virtual_keyboard_request();
    }

    pub(in crate::app) fn find_text_input_at(&self, element_id: crate::widget::ElementId, pos: Point) -> Option<crate::widget::ElementId> {
        let node = self.tree.elements.get(&element_id)?;
        if !node.element.is_visible() {
            return None;
        }
        let is_portal = matches!(node.element.layout_hint(), crate::widget::LayoutHint::Portal { .. });
        let is_passthrough = is_portal || node.element.passthrough_hit_test();
        if !is_passthrough && !node.element.hit_test(pos) {
            return None;
        }
        let child_pos = if is_portal {
            pos
        } else {
            let scroll = node.element.scroll_offset();
            let scale = node.element.event_scale();
            if scroll.x == 0.0 && scroll.y == 0.0 && (scale - 1.0).abs() < f32::EPSILON {
                pos
            } else {
                let k = scale.max(f32::EPSILON);
                Point::new((pos.x + scroll.x) / k, (pos.y + scroll.y) / k)
            }
        };
        for &child_id in node.children.iter().rev() {
            if let Some(found) = self.find_text_input_at(child_id, child_pos) {
                return Some(found);
            }
        }
        if is_passthrough && !is_portal && !node.element.hit_test(pos) {
            return None;
        }
        if let Some(info) = node.element.accessibility_info() {
            if matches!(
                info.role,
                crate::a11y::Role::TextField
                    | crate::a11y::Role::ComboBox
                    | crate::a11y::Role::Terminal
            ) && !info.state.disabled
            {
                return Some(element_id);
            }
        }
        None
    }

    #[allow(dead_code)]
    fn find_focusable_at(&self, element_id: crate::widget::ElementId, pos: Point) -> Option<crate::widget::ElementId> {
        let node = self.tree.elements.get(&element_id)?;
        if !node.element.is_visible() {
            return None;
        }
        let is_portal = matches!(node.element.layout_hint(), crate::widget::LayoutHint::Portal { .. });
        let is_passthrough = is_portal || node.element.passthrough_hit_test();
        if !is_passthrough && !node.element.hit_test(pos) {
            return None;
        }

        let child_pos = if is_portal {
            pos
        } else {
            let scroll = node.element.scroll_offset();
            let scale = node.element.event_scale();
            if scroll.x == 0.0 && scroll.y == 0.0 && (scale - 1.0).abs() < f32::EPSILON {
                pos
            } else {
                let k = scale.max(f32::EPSILON);
                Point::new((pos.x + scroll.x) / k, (pos.y + scroll.y) / k)
            }
        };

        for &child_id in node.children.iter().rev() {
            if let Some(found) = self.find_focusable_at(child_id, child_pos) {
                return Some(found);
            }
        }

        if is_passthrough && !is_portal && !node.element.hit_test(pos) {
            return None;
        }

        if let Some(info) = node.element.accessibility_info() {
            if FocusManager::is_focusable(&info.role) && !info.state.disabled {
                return Some(element_id);
            }
        }

        None
    }

    pub(in crate::app) fn render(&mut self) {
        // Кадр может применить стили или собрать новые элементы — то и другое
        // способно запустить анимацию; взводим обход в следующем update().
        self.tree.animations_armed = true;

        #[cfg(target_arch = "wasm32")]
        if self.gpu.is_none() {
            let has_pending = self.pending_gpu.borrow().is_some();
            web_sys::console::log_1(&format!("[syngui] render: gpu=None, pending_gpu.is_some()={}, Rc strong_count={}",
                has_pending, std::rc::Rc::strong_count(&self.pending_gpu)).into());
            let gpu = self.pending_gpu.borrow_mut().take();
            if let Some(gpu) = gpu {
                web_sys::console::log_1(&"[syngui] render: GPU ready, completing init".into());
                self.complete_gpu_init(gpu);
            } else {
                return;
            }
        }

        if !self.surface_valid {
            return;
        }

        #[cfg(target_arch = "wasm32")]
        {
            let mut font_changed = false;
            if let Some(renderer) = self.renderer.as_mut() {
                if let Some(font_data) = self.pending_font.borrow_mut().take() {
                    renderer.font_atlas.lock().unwrap().set_font_data(font_data);
                    font_changed = true;
                }
                if let Some(emoji_data) = self.pending_emoji_font.borrow_mut().take() {
                    renderer.font_atlas.lock().unwrap().set_emoji_font_data(emoji_data);
                    font_changed = true;
                }
                let fallback_fonts = std::mem::take(&mut *self.pending_fallback_fonts.borrow_mut());
                for data in fallback_fonts {
                    renderer.font_atlas.lock().unwrap().add_fallback_font(data, 0);
                    font_changed = true;
                }
            }
            if font_changed {
                web_sys::console::log_1(&"[syngui] Font data received".into());
            }
            let all_fonts_ready = {
                let atlas = self.renderer.as_ref().unwrap().font_atlas.lock().unwrap();
                let has_primary = atlas.has_font();
                let has_emoji = atlas.has_emoji_font();
                let emoji_configured = self.config.emoji_font_url.is_some();
                has_primary && (!emoji_configured || has_emoji)
            };
            if !all_fonts_ready {
                return;
            }
            if self.root_id.is_none() {
                {
                    let mut ctx = crate::widget::BuildContext::root();
                    let widget = (self.root_factory)(&mut ctx);
                    web_sys::console::log_1(&"[syngui] Building widget tree with font...".into());
                    let element = widget.create_element();
                    let type_id = widget.as_any().type_id();
                    let root_id = self.tree.insert_with_type_id(element, None, type_id);
                    widget.mount(&mut self.tree, root_id);
                    self.root_id = Some(root_id);
                    self.apply_styles(root_id);
                    let logical_w = (self.config.width as f64 / self.scale_factor) as f32;
                    let logical_h = (self.config.height as f64 / self.scale_factor) as f32;
                    let safe = &self.tree.safe_area;
                    let layout_h = (logical_h - safe.top - safe.bottom).max(0.0);
                    self.tree.root_offset = crate::core::Point::new(safe.left, safe.top);
                    let constraints = crate::layout::Constraints::new(
                        0.0, logical_w - safe.left - safe.right,
                        0.0, layout_h,
                    );
                    self.tree.layout(root_id, constraints);
                    self.a11y_tree.sync(&self.tree, root_id);
                    self.focus_manager.rebuild_tab_order(&self.tree, root_id);
                    web_sys::console::log_1(&format!("[syngui] Widget tree built: {} elements", self.tree.elements.len()).into());
                }
            }
        }

        if self.config.width == 0 || self.config.height == 0 {
            return;
        }

        let t_rebuild = Instant::now();
        if let Some(root_id) = self.root_id {
            let mut any_rebuilt = false;
            for _ in 0..8 {
                if !self.tree.rebuild_if_needed(root_id) {
                    break;
                }
                any_rebuilt = true;
                self.apply_styles(root_id);
            }
            if any_rebuilt {
                self.tree.force_full_measure = true;
                self.a11y_dirty = true;
            }
            self.process_pending_autofocus(root_id);
        }
        let rebuild_elapsed = t_rebuild.elapsed();

        crate::signal::drain_and_run_effects();

        #[cfg(target_os = "android")]
        let keyboard_height_changed;
        #[cfg(target_os = "android")]
        {
            let kb = self.query_keyboard_height();
            keyboard_height_changed = (kb - self.keyboard_height).abs() > 1.0 && kb > self.keyboard_height;
            if (kb - self.keyboard_height).abs() > 1.0 {
                self.keyboard_height = kb;
            }
        }

        let layout_elapsed;
        if let Some(root_id) = self.root_id {
            let logical_w = (self.config.width as f64 / self.scale_factor) as f32;
            let logical_h = (self.config.height as f64 / self.scale_factor) as f32;
            let safe = &self.tree.safe_area;
            #[cfg(target_os = "android")]
            let keyboard_h = self.keyboard_height;
            #[cfg(not(target_os = "android"))]
            let keyboard_h = 0.0f32;
            let layout_h = (logical_h - safe.top - safe.bottom - keyboard_h).max(0.0);
            let layout_w = logical_w - safe.left - safe.right;
            self.tree.root_offset = crate::core::Point::new(safe.left, safe.top);
            // Публикация размера вьюпорта здесь покрывает пути без отдельного
            // Resized-обработчика (wasm, Android): любой ресайз ведёт к redraw,
            // а set() дедуплицирует — подписчики будятся только при изменении.
            crate::viewport::publish(crate::core::Size::new(layout_w, layout_h));
            let constraints = crate::layout::Constraints::new(
                0.0, layout_w,
                0.0, layout_h,
            );

            self.tree.set_pixel_snap_scale(0.0);

            let t_layout = Instant::now();
            self.tree.layout(root_id, constraints);

            if self.tree.rebuild_if_needed(root_id) {
                self.apply_styles(root_id);
                self.tree.force_full_measure = true;
                self.tree.layout(root_id, constraints);
                self.a11y_dirty = true;
            }
            self.tree.force_full_measure = false;
            layout_elapsed = t_layout.elapsed();

            #[cfg(target_os = "android")]
            {
                let kb_ready = keyboard_height_changed || self.keyboard_height > 100.0;
                if kb_ready {
                    if let Some(element_id) = self.pending_scroll_element.take() {
                        self.tree.ensure_element_visible(element_id);
                    }
                }
            }

            if self.a11y_dirty {
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
        } else {
            layout_elapsed = std::time::Duration::ZERO;
        }

        if let (Some(root_id), Some(gpu), Some(renderer)) = (self.root_id, self.gpu.as_ref(), self.renderer.as_mut()) {
            let logical_w = (self.config.width as f64 / self.scale_factor) as f32;
            let logical_h = (self.config.height as f64 / self.scale_factor) as f32;

            let t_dl = Instant::now();
            self.display_list.clear();
            let surface_size = Size::new(logical_w, logical_h);
            self.display_list.set_surface_size(surface_size);
            self.display_list.set_scale_factor(self.scale_factor as f32);
            let clip = Rect::new(Point::zero(), surface_size);
            self.tree.build_display_list(root_id, &mut self.display_list, clip);
            self.tree.build_drag_overlay(&mut self.display_list);
            let dl_elapsed = t_dl.elapsed();

            if let Some(ref debug) = self.debug_overlay {
                debug.build_display_list(&mut self.display_list);
            }

            if let Some(ref mut devtools) = self.devtools {
                if devtools.is_enabled() {
                    if devtools.expanded_nodes_count() == 0 {
                        devtools.auto_expand(&self.tree, 4);
                    }
                    devtools.build_display_list(&mut self.display_list, &self.tree, &self.style_engine);
                }
            }

            let t_render = Instant::now();
            let render_stats = renderer.render(&gpu.shared, &gpu.window_surface, &self.display_list, self.config.background_color);
            let render_elapsed = t_render.elapsed();

            let font_stats = renderer.font_atlas_stats();
            let dl_stats = self.display_list.stats();

            if let Some(ref mut debug) = self.debug_overlay {
                debug.update_stats(crate::debug::FrameStats {
                    draw_calls: render_stats.draw_calls,
                    vertex_count: render_stats.vertex_count,
                    element_count: self.tree.elements.len(),
                    font_atlas_glyphs: font_stats.glyph_count,
                    font_atlas_mem_kb: font_stats.total_bytes / 1024,
                    display_list_commands: dl_stats.command_count + dl_stats.overlay_command_count,
                });
            }

            crate::perf::record_frame(
                rebuild_elapsed,
                layout_elapsed,
                dl_elapsed,
                render_elapsed,
                render_stats.draw_calls,
                render_stats.vertex_count,
                dl_stats.command_count + dl_stats.overlay_command_count,
            );

            if let Some(ref mut devtools) = self.devtools {
                if devtools.is_enabled() {
                    devtools.record_frame_timing(crate::devtools::FrameTiming {
                        layout_us: layout_elapsed.as_micros() as u64,
                        display_list_us: dl_elapsed.as_micros() as u64,
                        batch_render_us: render_elapsed.as_micros() as u64,
                        total_us: (layout_elapsed + dl_elapsed + render_elapsed).as_micros() as u64,
                        element_count: self.tree.elements.len(),
                        draw_calls: render_stats.draw_calls,
                        vertex_count: render_stats.vertex_count,
                        font_atlas_glyphs: font_stats.glyph_count,
                        font_atlas_mem_kb: font_stats.total_bytes / 1024,
                        display_list_commands: dl_stats.command_count + dl_stats.overlay_command_count,
                    });
                }
            }
        }
    }

    pub(in crate::app) fn update(&mut self) {
        crate::async_runtime::poll_main_thread_callbacks();

        // Веб: readText() обновил кэш буфера обмена — повторяем FocusGained
        // фокусному элементу, чтобы подсказка буфера показала свежий текст.
        #[cfg(target_arch = "wasm32")]
        if crate::clipboard::take_refreshed() {
            if let Some(focused) = self.tree.focused_element {
                if self.tree.elements.contains_key(&focused) {
                    self.tree.dispatch_event_to(focused, &crate::input::Event::FocusGained);
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
        }

        let now = Instant::now();
        let dt = now - self.last_frame_time;
        self.last_frame_time = now;

        if let Some(ref mut debug) = self.debug_overlay {
            debug.record_frame(dt.as_secs_f32());
        }

        #[cfg(feature = "i18n")]
        if let Some(renderer) = self.renderer.as_ref() {
            let lang = crate::i18n::language();
            let base = lang.base();
            renderer
                .font_atlas
                .lock()
                .unwrap()
                .set_preferred_cjk(base == "ja", base == "ko");
        }

        if let Some(theme_state) = self.config.theme_state {
            let is_dark = theme_state.get_untracked();
            if is_dark != self.current_theme_is_dark {
                self.current_theme_is_dark = is_dark;
                let new_ss = if is_dark {
                    self.config.dark_stylesheet.clone().unwrap_or_default()
                } else {
                    self.config.light_stylesheet.clone().unwrap_or_default()
                };
                // Clear-color окна = фон темы: полоса статус-бара и непокрытые
                // области больше не остаются дефолтным светлым фоном.
                if let Some(bg) = parse_theme_bg(&new_ss) {
                    self.config.background_color = bg;
                }
                self.style_engine.load_stylesheet(new_ss);
                for additional in &self.config.additional_stylesheets {
                    self.style_engine.load_additional_stylesheet(additional.clone());
                }
                crate::signal::mark_all_reactive_dirty();
                for node in self.tree.elements.values_mut() {
                    node.styles_dirty = true;
                }
                if let Some(root_id) = self.root_id {
                    self.apply_styles(root_id);
                }
                #[cfg(target_os = "android")]
                self.set_status_bar_light_icons(is_dark);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }

        if let Some(sig) = self.config.dynamic_theme_mss {
            let new_mss = sig.get_untracked();
            if new_mss != self.current_dynamic_theme {
                self.current_dynamic_theme = new_mss.clone();
                if let Some(ref base) = self.config.stylesheet {
                    self.style_engine.load_stylesheet(base.clone());
                }
                for additional in &self.config.additional_stylesheets {
                    self.style_engine.load_additional_stylesheet(additional.clone());
                }
                if !new_mss.is_empty() {
                    match parse_stylesheet_str(&new_mss) {
                        Ok(theme_ss) => {
                            self.style_engine.load_additional_stylesheet(theme_ss);
                        }
                        Err(e) => log::warn!("Failed to parse dynamic theme: {:?}", e),
                    }
                }
                let is_dark = new_mss.find("--bg:")
                    .and_then(|pos| {
                        let after = &new_mss[pos + 5..];
                        let val = after.trim_start().split(';').next()?.trim();
                        if val.starts_with('#') && val.len() >= 7 {
                            let r = u8::from_str_radix(&val[1..3], 16).ok()?;
                            let g = u8::from_str_radix(&val[3..5], 16).ok()?;
                            let b = u8::from_str_radix(&val[5..7], 16).ok()?;
                            let lum = 0.299 * (r as f32) + 0.587 * (g as f32) + 0.114 * (b as f32);
                            Some(lum < 128.0)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(false);
                if is_dark != self.current_theme_is_dark {
                    self.current_theme_is_dark = is_dark;
                    #[cfg(target_os = "android")]
                    self.set_status_bar_light_icons(is_dark);
                }

                for node in self.tree.elements.values_mut() {
                    node.styles_dirty = true;
                }
                if let Some(root_id) = self.root_id {
                    self.apply_styles(root_id);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }

        if let Some(ref image_store) = self.tree.image_store {
            if image_store.lock().unwrap().has_loading() {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }

        #[cfg(target_os = "android")]
        {
            self.poll_input_handler();
            self.poll_android_back();
            // Пульс опроса IME-моста и кнопки «назад» вместо безусловного
            // request_redraw: раньше приложение рендерило полный кадр на
            // каждом vsync даже в простое. Теперь кадры рисуются только по
            // грязным элементам/анимациям, а Java-мост опрашивается таймером
            // без рендера. Тачи приходят событиями winit и в опросе не
            // нуждаются.
            let poll_pulse = std::time::Duration::from_millis(12);
            self.wakeup_after = Some(match self.wakeup_after {
                Some(d) => d.min(poll_pulse),
                None => poll_pulse,
            });
            // Пока открыта экранная клавиатура, кадры продолжаются: layout
            // подстраивается под её высоту (query в render), а высота меняется
            // без событий — анимация IME.
            if self.keyboard_shown {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }

        let frame_limit = self.config.frame_limit;
        let paced_allowed = if frame_limit == 0 {
            true
        } else {
            let min_interval = std::time::Duration::from_secs_f32(1.0 / frame_limit as f32);
            self.last_paced_redraw
                .map_or(true, |t| now.duration_since(t) >= min_interval)
        };

        if let Some(root_id) = self.root_id {
            // Обход всех элементов — только пока «взведено» (см. animations_armed):
            // события, рендер и новые элементы взводят, пустой обход даёт отбой.
            // В простое update() не трогает дерево вовсе.
            if self.tree.animations_armed {
                if self.tree.animate(root_id, dt) {
                    if paced_allowed {
                        self.last_paced_redraw = Some(now);
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    } else if frame_limit > 0 {
                        // Иначе анимация замирала до следующего события (движения
                        // мыши): кадр не запрошен, а сам по себе цикл не проснётся.
                        // Просим пробуждение к моменту, когда пейсер разрешит кадр.
                        let min_interval =
                            std::time::Duration::from_secs_f32(1.0 / frame_limit as f32);
                        let elapsed = self
                            .last_paced_redraw
                            .map(|t| now.duration_since(t))
                            .unwrap_or(min_interval);
                        let delay = min_interval.saturating_sub(elapsed).max(
                            std::time::Duration::from_millis(1),
                        );
                        self.wakeup_after = Some(match self.wakeup_after {
                            Some(d) => d.min(delay),
                            None => delay,
                        });
                    }
                } else {
                    self.tree.animations_armed = false;
                }
            }
        }

        if crate::signal::has_dirty_elements() {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }

        let needs_continuous = self.debug_overlay.is_some()
            || self.devtools.as_ref().map_or(false, |d| d.is_enabled());
        if needs_continuous && paced_allowed {
            self.last_paced_redraw = Some(now);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}
