use crate::core::Point;
use crate::input::{Event, EventResult};
use crate::widget::EventContext;
use super::{ElementId, ElementTree};

#[inline]
fn is_identity_transform(s: Point, k: f32) -> bool {
    s.x == 0.0 && s.y == 0.0 && (k - 1.0).abs() < f32::EPSILON
}

impl ElementTree {
    pub fn dispatch_event_to(&mut self, id: ElementId, event: &Event) -> EventResult {
        // Событие может запустить transition (hover и т.п.) — взводим обход анимаций.
        self.animations_armed = true;
        if let Some(node) = self.elements.get_mut(&id) {
            let mut ctx = EventContext::new(id);
            ctx.modifiers = self.modifiers;
            ctx.set_viewport_size(self.viewport_size);
            ctx.set_window_flags(self.window_flags);
            if let Some(ref tm) = self.text_measure {
                ctx.set_text_measure(tm.clone());
            }
            let result = node.element.handle_event(event, &mut ctx);
            let ctx_dirty = ctx.take_dirty_flags();
            let did_something = result.is_handled() || !ctx_dirty.is_empty() || ctx.has_side_effects();
            if !ctx_dirty.is_empty() {
                node.element.mark_dirty(ctx_dirty);
                if ctx_dirty.contains(crate::widget::DirtyFlags::LAYOUT) {
                    node.refresh_hint_cache();
                }
            }
            if let Some(text) = ctx.focused_text.take() {
                self.focused_text_content = Some(text);
            }
            if let Some(rect) = ctx.scroll_into_view_request.take() {
                self.scroll_element_into_view(id, rect);
            }
            self.process_overlay_commands(id, &mut ctx);
            if did_something {
                self.sync_registries_for(id);
            }
            result
        } else {
            EventResult::Ignored
        }
    }

    pub fn handle_event(&mut self, root_id: ElementId, event: &Event) -> EventResult {
        self.animations_armed = true;
        // TouchEnd снимает захват так же, как MouseUp: при скролле синтезиро-
        // ванного MouseUp не будет, и захватчик повис бы до следующего клика.
        let is_release = matches!(event, Event::MouseUp { .. } | Event::TouchEnd { .. });
        let result = self.do_handle_event(root_id, event);
        if is_release {
            self.mouse_captor = None;
        }
        result
    }

    fn do_handle_event(&mut self, root_id: ElementId, event: &Event) -> EventResult {
        self.cursor_request = None;

        if matches!(event, Event::DoubleClick { .. }) {
            if let Some(target) = self.last_mousedown_element {
                if self.elements.contains_key(&target) {
                    let (s, k) = self.accumulated_event_transform(target);
                    let adjusted = if is_identity_transform(s, k) {
                        event.clone()
                    } else {
                        event.with_inverse_transform(s, k)
                    };
                    // РЕКУРСИВНЫЙ `dispatch_event`, а не одиночный
                    // `dispatch_event_to`: `last_mousedown_element` может быть
                    // корнем overlay/Portal — модалка ставит его именно в
                    // overlay-ветке ниже (`entry.element_id`), а не глубоким
                    // элементом. `dispatch_event_to` дёрнул бы `handle_event`
                    // только у Portal (который DoubleClick игнорирует), и
                    // двойной клик по контенту модалки терялся. `dispatch_event`
                    // спускается по поддереву target'а до реального элемента под
                    // курсором. Если поддерево не обработало (курсор ушёл) —
                    // фолбэк на позиционный обход от корня.
                    let r = self.dispatch_event(target, &adjusted);
                    if r.is_handled() {
                        return r;
                    }
                }
            }
            return self.dispatch_event(root_id, event);
        }

        if matches!(event, Event::MouseDown { .. }) {
            self.last_mousedown_element = None;
        }

        let event_pos = event.position();

        if event_pos.is_none() {
            for entry in self.overlay_stack.iter().rev() {
                if entry.modal {
                    return self.dispatch_event_to_element(entry.element_id, event);
                }
            }
            if let Some(focused) = self.focused_element {
                if self.elements.contains_key(&focused) {
                    return self.dispatch_focus_bubble(focused, event);
                }
            }
        }

        if let Some(pos) = event_pos {
            let overlay_len = self.overlay_stack.len();
            for i in (0..overlay_len).rev() {
                if i >= self.overlay_stack.len() {
                    continue;
                }
                let entry = self.overlay_stack[i].clone();
                let hits = entry.bounds.contains(pos);
                if hits || entry.modal {
                    if matches!(event, Event::MouseMove(_)) {
                        let old_path = std::mem::take(&mut self.last_hovered_path);
                        let off_screen = crate::core::Point::new(-1.0, -1.0);
                        for id in old_path {
                            self.dispatch_event_to(id, &Event::MouseMove(off_screen));
                        }
                    }
                    let result = self.dispatch_event_to_element(entry.element_id, event);
                    if matches!(event, Event::MouseDown { .. }) && result.is_handled() {
                        self.last_mousedown_element = Some(entry.element_id);
                        self.mouse_captor = Some(entry.element_id);
                    }
                    return result;
                } else if matches!(event, Event::MouseDown { .. }) {
                    self.dispatch_event_to_element(entry.element_id, event);
                }
            }
        }

        if let Event::MouseMove(pos) = event {
            return self.dispatch_mouse_move(root_id, *pos);
        }
        if let Some(pos) = event_pos {
            return self.dispatch_positional(root_id, event, pos);
        }
        self.dispatch_event(root_id, event)
    }

    fn dispatch_focus_bubble(&mut self, focused: ElementId, event: &Event) -> EventResult {
        let mut chain: Vec<ElementId> = Vec::with_capacity(8);
        let mut cur = Some(focused);
        while let Some(id) = cur {
            chain.push(id);
            cur = self.elements.get(&id).and_then(|n| n.parent);
        }
        for id in chain {
            if !self.elements.contains_key(&id) { continue; }
            let r = self.dispatch_event_to(id, event);
            if r.is_handled() {
                return r;
            }
        }
        EventResult::Ignored
    }

    fn dispatch_positional(&mut self, root_id: ElementId, event: &Event, pos: crate::core::Point) -> EventResult {
        // Touch-события тоже идут захватчику: на тачскринах MouseDown
        // синтезируется на TouchStart (ставит mouse_captor), а движение пальца
        // приходит только как TouchMove — без приоритета захватчика drag,
        // начатый на слайдере/скроллбаре, «отваливался» бы, стоило пальцу
        // покинуть его границы. Если захватчик событие игнорирует, оно, как и
        // прежде, уходит вниз по hit-test.
        if matches!(
            event,
            Event::MouseUp { .. }
                | Event::MouseMove(_)
                | Event::TouchMove { .. }
                | Event::TouchEnd { .. }
        ) {
            if let Some(cap) = self.mouse_captor {
                if self.elements.contains_key(&cap) {
                    let (s, k) = self.accumulated_event_transform(cap);
                    let adj = if is_identity_transform(s, k) {
                        event.clone()
                    } else {
                        event.with_inverse_transform(s, k)
                    };
                    let r = self.dispatch_event_to(cap, &adj);
                    if r.is_handled() {
                        return r;
                    }
                }
            }
        }

        let mut path: Vec<ElementId> = Vec::new();
        self.hit_test_path(root_id, pos, &mut path);

        if let Some(cut) = path.iter().position(|id| {
            self.elements
                .get(id)
                .map(|n| n.element.intercepts_child_events())
                .unwrap_or(false)
        }) {
            path.truncate(cut + 1);
        }

        for &id in path.iter().rev() {
            if !self.elements.contains_key(&id) {
                continue;
            }
            let (s, k) = self.accumulated_event_transform(id);
            let adj = if is_identity_transform(s, k) {
                event.clone()
            } else {
                event.with_inverse_transform(s, k)
            };
            let r = self.dispatch_event_to(id, &adj);
            if r.is_handled() {
                if matches!(event, Event::MouseDown { .. }) {
                    self.last_mousedown_element = Some(id);
                    self.mouse_captor = Some(id);
                }
                // Захват тач-жеста: кто заклеймил TouchStart (слайдер,
                // ScrollView), тот получает и последующие TouchMove/TouchEnd,
                // даже когда палец уходит за границы виджета.
                if matches!(event, Event::TouchStart { .. }) {
                    self.mouse_captor = Some(id);
                }
                return r;
            }
        }
        EventResult::Ignored
    }

    fn dispatch_mouse_move(&mut self, root_id: ElementId, pos: crate::core::Point) -> EventResult {
        let mut new_path: Vec<ElementId> = Vec::new();
        self.hit_test_path(root_id, pos, &mut new_path);

        let old_path = std::mem::take(&mut self.last_hovered_path);
        let new_set: std::collections::HashSet<ElementId> = new_path.iter().copied().collect();

        let mut any_handled = false;
        let dispatch_one = |tree: &mut Self, id: ElementId, any: &mut bool| {
            if !tree.elements.contains_key(&id) {
                return;
            }
            let (s, k) = tree.accumulated_event_transform(id);
            let adjusted = if is_identity_transform(s, k) {
                Event::MouseMove(pos)
            } else {
                let safe_k = k.max(f32::EPSILON);
                Event::MouseMove(crate::core::Point::new(
                    (pos.x + s.x) / safe_k,
                    (pos.y + s.y) / safe_k,
                ))
            };
            if tree.dispatch_event_to(id, &adjusted).is_handled() {
                *any = true;
            }
        };

        for id in old_path.iter().copied() {
            if !new_set.contains(&id) {
                dispatch_one(self, id, &mut any_handled);
            }
        }
        for id in new_path.iter().copied() {
            dispatch_one(self, id, &mut any_handled);
        }
        if let Some(captor_id) = self.mouse_captor {
            if !new_set.contains(&captor_id) && !old_path.contains(&captor_id) {
                dispatch_one(self, captor_id, &mut any_handled);
            }
        }

        self.last_hovered_path = new_path;
        if any_handled { EventResult::Handled } else { EventResult::Ignored }
    }

    fn hit_test_path(&self, id: ElementId, pos: crate::core::Point, out: &mut Vec<ElementId>) {
        let node = match self.elements.get(&id) {
            Some(n) => n,
            None => return,
        };
        if !node.element.is_visible() {
            return;
        }
        let is_portal = matches!(node.element.layout_hint(), crate::widget::LayoutHint::Portal { .. });
        let is_passthrough = is_portal || node.element.passthrough_hit_test();
        if !is_passthrough && !node.element.hit_test(pos) {
            return;
        }
        let self_idx = out.len();
        out.push(id);

        let scroll = node.element.scroll_offset();
        let scale = node.element.event_scale();
        let child_pos = if scroll.x == 0.0
            && scroll.y == 0.0
            && (scale - 1.0).abs() < f32::EPSILON
        {
            pos
        } else {
            let k = scale.max(f32::EPSILON);
            crate::core::Point::new((pos.x + scroll.x) / k, (pos.y + scroll.y) / k)
        };

        let active = node.element.active_child_count();
        let children_len = node.children.len().min(active);

        match node.element.child_at_position(child_pos) {
            crate::widget::ChildHit::None => {
                if is_passthrough {
                    out.truncate(self_idx);
                }
                return;
            }
            crate::widget::ChildHit::Index(i) if i < children_len => {
                let before = out.len();
                self.hit_test_path(node.children[i], child_pos, out);
                if out.len() > before {
                    return;
                }
            }
            _ => {}
        }

        for &child_id in node.children.iter().take(active).rev() {
            let before = out.len();
            self.hit_test_path(child_id, child_pos, out);
            if out.len() > before {
                return;
            }
        }

        if is_passthrough && out.len() == self_idx + 1 {
            out.truncate(self_idx);
        }
    }

    fn dispatch_event_to_element(&mut self, id: ElementId, event: &Event) -> EventResult {
        let (s, k) = self.accumulated_event_transform(id);
        let adj = if is_identity_transform(s, k) {
            event.clone()
        } else {
            event.with_inverse_transform(s, k)
        };
        self.dispatch_event(id, &adj)
    }

    fn accumulated_event_transform(&self, id: ElementId) -> (crate::core::Point, f32) {
        let mut chain: Vec<(crate::core::Point, f32)> = Vec::new();
        let mut current = self.elements.get(&id).and_then(|n| n.parent);
        while let Some(parent_id) = current {
            if let Some(parent_node) = self.elements.get(&parent_id) {
                chain.push((
                    parent_node.element.scroll_offset(),
                    parent_node.element.event_scale(),
                ));
                current = parent_node.parent;
            } else {
                break;
            }
        }
        let mut s = crate::core::Point::zero();
        let mut k: f32 = 1.0;
        for (s_i, k_i) in chain.iter().rev() {
            s = crate::core::Point::new(s.x + s_i.x * k, s.y + s_i.y * k);
            k *= k_i.max(f32::EPSILON);
        }
        (s, k)
    }

    fn process_overlay_commands(&mut self, element_id: ElementId, ctx: &mut EventContext) {
        if let Some((bounds, modal)) = ctx.overlay_register.take() {
            let (s, k) = self.accumulated_event_transform(element_id);
            let adjusted = if is_identity_transform(s, k) {
                bounds
            } else {
                crate::core::Rect::new(
                    crate::core::Point::new(
                        k * bounds.origin.x - s.x,
                        k * bounds.origin.y - s.y,
                    ),
                    crate::core::Size::new(k * bounds.size.width, k * bounds.size.height),
                )
            };
            self.register_overlay(element_id, adjusted, modal);
        }
        if ctx.overlay_unregister {
            self.unregister_overlay(element_id);
            ctx.overlay_unregister = false;
        }
        if let Some(data) = ctx.start_drag.take() {
            let pos = ctx.cursor_position;
            let source_id = ElementId(data.source_id);
            let source_bounds = self.elements.get(&source_id)
                .map(|n| n.element.bounds())
                .unwrap_or(crate::core::Rect::zero());
            let drag_offset = Point::new(
                pos.x - source_bounds.origin.x,
                pos.y - source_bounds.origin.y,
            );
            self.drag_state = Some(super::DragState {
                data,
                start_pos: pos,
                current_pos: pos,
                hover_element: None,
                drag_offset,
                source_bounds,
            });
        }
        if let Some(cursor) = ctx.cursor_icon.take() {
            self.cursor_request = Some(cursor);
        }
        if let Some(show) = ctx.show_virtual_keyboard.take() {
            self.virtual_keyboard_request = Some(show);
            // Скрытый ввод объявляет только поле пароля; остальные виджеты
            // молчат — сбрасываем, чтобы флаг не пережил смену фокуса.
            if show {
                self.keyboard_secret = false;
            }
        }
        if let Some(numeric) = ctx.numeric_keyboard.take() {
            self.keyboard_numeric = numeric;
        }
        if let Some(secret) = ctx.secret_keyboard.take() {
            self.keyboard_secret = secret;
        }
        if ctx.start_window_drag {
            self.window_drag_request = true;
            ctx.start_window_drag = false;
        }
        if let Some(direction) = ctx.start_window_resize.take() {
            self.window_resize_request = Some(direction);
        }
        if ctx.close_window {
            self.window_close_request = true;
            ctx.close_window = false;
        }
        if ctx.minimize_window {
            self.window_minimize_request = true;
            ctx.minimize_window = false;
        }
        if ctx.toggle_maximize_window {
            self.window_toggle_maximize_request = true;
            ctx.toggle_maximize_window = false;
        }
        if ctx.toggle_fullscreen_window {
            self.window_toggle_fullscreen_request = true;
            ctx.toggle_fullscreen_window = false;
        }
        if ctx.hide_window {
            self.window_hide_request = true;
            ctx.hide_window = false;
        }
        if ctx.show_window {
            self.window_show_request = true;
            ctx.show_window = false;
        }
        if ctx.toggle_window_visibility {
            self.window_toggle_visibility_request = true;
            ctx.toggle_window_visibility = false;
        }
    }

    fn dispatch_event(&mut self, id: ElementId, event: &Event) -> EventResult {
        crate::perf::incr(crate::perf::Counter::DispatchVisit);
        if matches!(event, Event::MouseMove(_)) {
            crate::perf::incr(crate::perf::Counter::MmDispatchVisit);
        }
        if let Some(node) = self.elements.get(&id) {
            if !node.element.is_visible() {
                return EventResult::Ignored;
            }
        }

        let (scroll, scale) = self.elements.get(&id)
            .map(|n| (n.element.scroll_offset(), n.element.event_scale()))
            .unwrap_or((Point::zero(), 1.0));

        let intercepts = self.elements.get(&id)
            .map(|n| n.element.intercepts_child_events())
            .unwrap_or(false);

        let children = self.elements.get(&id)
            .map(|n| {
                let active = n.element.active_child_count();
                n.children.iter().take(active).copied().collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let child_event = if is_identity_transform(scroll, scale) {
            event.clone()
        } else {
            event.with_inverse_transform(scroll, scale)
        };

        let is_broadcast = matches!(event, Event::MouseMove(_));
        let mut child_handled = false;

        for &child_id in children.iter().rev() {
            if intercepts { break; }
            let result = self.dispatch_event(child_id, &child_event);
            if result.is_handled() {
                if is_broadcast {
                    child_handled = true;
                } else {
                    return result;
                }
            }
        }

        let child_cursor = if child_handled { self.cursor_request } else { None };

        if let Some(node) = self.elements.get_mut(&id) {
            let mut ctx = EventContext::new(id);
            ctx.modifiers = self.modifiers;
            ctx.set_viewport_size(self.viewport_size);
            ctx.set_window_flags(self.window_flags);
            if let Some(ref tm) = self.text_measure {
                ctx.set_text_measure(tm.clone());
            }
            let result = node.element.handle_event(event, &mut ctx);
            let ctx_dirty = ctx.take_dirty_flags();
            let did_something = result.is_handled() || !ctx_dirty.is_empty() || ctx.has_side_effects();
            if !ctx_dirty.is_empty() {
                node.element.mark_dirty(ctx_dirty);
                if ctx_dirty.contains(crate::widget::DirtyFlags::LAYOUT) {
                    node.refresh_hint_cache();
                }
            }
            self.process_overlay_commands(id, &mut ctx);
            if let Some(rect) = ctx.scroll_into_view_request.take() {
                self.scroll_element_into_view(id, rect);
            }
            if let Some(cc) = child_cursor {
                self.cursor_request = Some(cc);
            }
            let final_result = if child_handled { EventResult::Handled } else { result };
            if matches!(event, Event::MouseDown { .. }) && final_result.is_handled() {
                self.last_mousedown_element = Some(id);
                self.mouse_captor = Some(id);
            }
            if did_something {
                self.sync_registries_for(id);
            }
            final_result
        } else {
            if child_handled { EventResult::Handled } else { EventResult::Ignored }
        }
    }

    /// Drag-события (DragMove/DragEnter/Drop) идут **самой глубокой** цели
    /// под курсором, и только если она их не взяла — следующей по глубине:
    /// вложенная DropArea (карточка доски внутри редактора страницы) получает
    /// дроп одна, а не вместе с внешней. Остальные цели, которые точку
    /// содержат, но перекрыты, и цели вне точки получают `DragLeave` на
    /// движении — так они снимают подсветку. `DragLeave`/`DragEnd` — всем.
    pub fn dispatch_drag_event(&mut self, event: &Event) -> EventResult {
        let targets = self.drop_targets.clone();
        let positional = matches!(
            event,
            Event::DragMove { .. } | Event::DragEnter { .. } | Event::Drop { .. }
        );
        if !positional {
            let mut result = EventResult::Ignored;
            for target_id in &targets {
                if self.deliver_drag_event(*target_id, event).is_handled() {
                    result = EventResult::Handled;
                }
            }
            return result;
        }

        // Кандидаты: цели, содержащие точку в своих координатах; глубокие
        // первыми, из равных — зарегистрированная позже.
        let mut hits: Vec<(usize, usize, ElementId, Event)> = Vec::new();
        let mut misses: Vec<ElementId> = Vec::new();
        for (order, target_id) in targets.iter().enumerate() {
            let Some(node) = self.elements.get(target_id) else { continue };
            let (s, k) = self.accumulated_event_transform(*target_id);
            let adjusted = if is_identity_transform(s, k) {
                event.clone()
            } else {
                event.with_inverse_transform(s, k)
            };
            let inside = adjusted
                .position()
                .map(|p| node.element.bounds().contains(p))
                .unwrap_or(false);
            if inside {
                hits.push((self.depth_of(*target_id), order, *target_id, adjusted));
            } else {
                misses.push(*target_id);
            }
        }
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

        let is_drop = matches!(event, Event::Drop { .. });
        let mut result = EventResult::Ignored;
        for (_, _, target_id, adjusted) in &hits {
            if result.is_handled() {
                if !is_drop {
                    self.deliver_drag_event(*target_id, &Event::DragLeave);
                }
                continue;
            }
            if self.deliver_drag_event(*target_id, adjusted).is_handled() {
                result = EventResult::Handled;
            }
        }
        if !is_drop {
            for target_id in misses {
                self.deliver_drag_event(target_id, &Event::DragLeave);
            }
        }
        result
    }

    /// Закончить drag дерева в точке `position` — то, что приложение делает
    /// на отпускании кнопки: `Drop` целям (если не отмена), `DragEnd` —
    /// источнику напрямую (обход от фокуса до Draggable внутри редактора-в-
    /// фокусе не доходит, а ему надо снять захват и закончить жест) и
    /// дереву; состояние переноса и захват мыши снимаются (MouseUp дереву при
    /// drag'е не шлётся). `false` — переноса не было.
    pub fn end_drag(&mut self, root_id: ElementId, position: crate::core::Point, cancelled: bool) -> bool {
        let Some(data) = self.drag_state.as_ref().map(|d| d.data.clone()) else { return false };
        if !cancelled {
            self.dispatch_drag_event(&Event::Drop { position, data: data.clone() });
        }
        let drag_end = Event::DragEnd { cancelled };
        let source = ElementId(data.source_id);
        if data.source_id != 0 && self.elements.contains_key(&source) {
            self.dispatch_event_to(source, &drag_end);
        }
        self.handle_event(root_id, &drag_end);
        self.drag_state = None;
        self.mouse_captor = None;
        true
    }

    /// Глубина элемента (число предков).
    fn depth_of(&self, id: ElementId) -> usize {
        let mut depth = 0;
        let mut current = self.elements.get(&id).and_then(|n| n.parent);
        while let Some(parent_id) = current {
            depth += 1;
            current = self.elements.get(&parent_id).and_then(|n| n.parent);
        }
        depth
    }

    fn deliver_drag_event(&mut self, target_id: ElementId, event: &Event) -> EventResult {
        if self.elements.get(&target_id).is_none() {
            return EventResult::Ignored;
        }
        let mut ctx = EventContext::new(target_id);
        ctx.modifiers = self.modifiers;
        ctx.set_viewport_size(self.viewport_size);
        ctx.set_window_flags(self.window_flags);
        if let Some(ref tm) = self.text_measure {
            ctx.set_text_measure(tm.clone());
        }
        let mut result = EventResult::Ignored;
        if let Some(node) = self.elements.get_mut(&target_id) {
            let r = node.element.handle_event(event, &mut ctx);
            let ctx_dirty = ctx.take_dirty_flags();
            let did_something = r.is_handled() || !ctx_dirty.is_empty();
            if !ctx_dirty.is_empty() {
                node.element.mark_dirty(ctx_dirty);
            }
            if r.is_handled() {
                result = EventResult::Handled;
            }
            if did_something {
                self.sync_registries_for(target_id);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Rect, Size};
    use crate::input::{Event, EventResult, MouseButton};
    use crate::layout::Constraints;
    use crate::signal::use_signal;
    use crate::widget::{
        DirtyFlags, Element, ElementId, ElementTree, UpdateContext, Widget,
    };
    use crate::widget::context::EventContext;
    use crate::widgets::containers::PanZoomViewport;
    use crate::widgets::ScrollView;
    use std::any::Any;
    use std::sync::{Arc, Mutex};

    type SpyLog = Arc<Mutex<Option<crate::core::Point>>>;

    struct SpyTarget {
        size: Size,
        log: SpyLog,
    }

    impl SpyTarget {
        fn new(size: Size) -> (Self, SpyLog) {
            let log: SpyLog = Arc::new(Mutex::new(None));
            (
                SpyTarget {
                    size,
                    log: log.clone(),
                },
                log,
            )
        }
    }

    impl Widget for SpyTarget {
        fn create_element(&self) -> Box<dyn Element> {
            Box::new(SpyTargetElement {
                id: ElementId::new(),
                bounds: Rect::zero(),
                size: self.size,
                log: self.log.clone(),
                dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            })
        }
        fn can_update(&self, other: &dyn Any) -> bool {
            other.is::<Self>()
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
    }

    struct SpyTargetElement {
        id: ElementId,
        bounds: Rect,
        size: Size,
        log: SpyLog,
        dirty_flags: DirtyFlags,
    }

    impl Element for SpyTargetElement {
        fn update(&mut self, _w: &dyn Widget, _ctx: &mut UpdateContext) {}
        fn layout(&mut self, _c: Constraints) -> Size {
            self.bounds = Rect::new(self.bounds.origin, self.size);
            self.size
        }
        fn build_display_list(
            &self,
            _list: &mut crate::render::DisplayList,
            _clip: Rect,
        ) {
        }
        fn handle_event(&mut self, event: &Event, _ctx: &mut EventContext) -> EventResult {
            if let Event::MouseDown { position, .. } = event {
                *self.log.lock().unwrap() = Some(*position);
                return EventResult::Handled;
            }
            EventResult::Ignored
        }
        fn children(&self) -> &[ElementId] {
            &[]
        }
        fn bounds(&self) -> Rect {
            self.bounds
        }
        fn set_position(&mut self, pos: crate::core::Point) {
            self.bounds.origin = pos;
        }
        fn mark_dirty(&mut self, flags: DirtyFlags) {
            self.dirty_flags |= flags;
        }
        fn clear_dirty(&mut self, flags: DirtyFlags) {
            self.dirty_flags.remove(flags);
        }
        fn is_dirty(&self, flags: DirtyFlags) -> bool {
            self.dirty_flags.contains(flags)
        }
        fn id(&self) -> ElementId {
            self.id
        }
        fn set_id(&mut self, id: ElementId) {
            self.id = id;
        }
        fn mount(&mut self, _tree: &mut ElementTree) {}
    }
    impl crate::widget::StyledElement for SpyTargetElement {
        fn apply_style(&mut self, _: &crate::mss::ComputedStyle) {}
        fn classes(&self) -> &[String] {
            &[]
        }
        fn set_classes(&mut self, _: Vec<String>) {}
    }

    fn build_and_layout(widget: Box<dyn Widget>) -> (ElementTree, ElementId, Box<dyn Widget>) {
        let mut tree = ElementTree::new();
        let root_elem = widget.create_element();
        let root_id = tree.insert(root_elem, None);
        widget.mount(&mut tree, root_id);
        let constraints = Constraints::loose(Size::new(800.0, 600.0));
        tree.layout(root_id, constraints);
        (tree, root_id, widget)
    }

    fn click_at(tree: &mut ElementTree, root_id: ElementId, x: f32, y: f32) {
        tree.handle_event(
            root_id,
            &Event::MouseDown {
                button: MouseButton::Left,
                position: Point::new(x, y),
            },
        );
    }

    fn make_panzoom_with_spy(
        zoom: f32,
        pan: crate::core::Point,
    ) -> (Box<dyn Widget>, SpyLog) {
        let (spy, log) = SpyTarget::new(Size::new(800.0, 600.0));
        let zoom_sig = use_signal(zoom);
        let pan_sig = use_signal(pan);
        let viewport = PanZoomViewport::new()
            .pan(pan_sig)
            .zoom(zoom_sig)
            .zoom_range(0.1, 10.0)
            .child(spy);
        (Box::new(viewport), log)
    }

    #[test]
    fn hit_test_with_zoom_only() {
        let (widget, log) = make_panzoom_with_spy(2.0, Point::zero());
        let (mut tree, root_id, _w) = build_and_layout(widget);
        click_at(&mut tree, root_id, 220.0, 220.0);
        let pos = log.lock().unwrap().expect("spy must receive MouseDown");
        assert!((pos.x - 110.0).abs() < 1e-3 && (pos.y - 110.0).abs() < 1e-3,
            "expected (110, 110), got ({}, {})", pos.x, pos.y);
    }

    #[test]
    fn hit_test_with_pan_and_zoom() {
        let (widget, log) = make_panzoom_with_spy(2.0, Point::new(50.0, 50.0));
        let (mut tree, root_id, _w) = build_and_layout(widget);
        click_at(&mut tree, root_id, 250.0, 250.0);
        let pos = log.lock().unwrap().expect("spy must receive MouseDown");
        assert!((pos.x - 100.0).abs() < 1e-3 && (pos.y - 100.0).abs() < 1e-3,
            "expected (100, 100), got ({}, {})", pos.x, pos.y);
    }

    #[test]
    fn hit_test_with_zoom_out() {
        let (widget, log) = make_panzoom_with_spy(0.5, Point::zero());
        let (mut tree, root_id, _w) = build_and_layout(widget);
        click_at(&mut tree, root_id, 200.0, 200.0);
        let pos = log.lock().unwrap().expect("spy must receive MouseDown");
        assert!((pos.x - 400.0).abs() < 1e-3 && (pos.y - 400.0).abs() < 1e-3,
            "expected (400, 400), got ({}, {})", pos.x, pos.y);
    }

    #[test]
    fn composition_scroll_inside_panzoom() {
        let (spy, log) = SpyTarget::new(Size::new(800.0, 600.0));
        let scroll = ScrollView::new().child(spy);
        let zoom_sig = use_signal(2.0_f32);
        let pan_sig = use_signal(Point::zero());
        let viewport = PanZoomViewport::new()
            .pan(pan_sig)
            .zoom(zoom_sig)
            .zoom_range(0.1, 10.0)
            .child(scroll);
        let (mut tree, root_id, _w) = build_and_layout(Box::new(viewport));
        click_at(&mut tree, root_id, 200.0, 200.0);
        let pos = log.lock().unwrap();
        assert!(pos.is_some(), "spy must receive MouseDown through ScrollView+PanZoom");
        let p = pos.unwrap();
        assert!((p.x - 100.0).abs() < 1.0 && (p.y - 100.0).abs() < 1.0,
            "expected ~(100, 100), got ({}, {})", p.x, p.y);
    }

    #[test]
    fn regression_zoom_one_with_pan() {
        let (widget, log) = make_panzoom_with_spy(1.0, Point::new(50.0, 30.0));
        let (mut tree, root_id, _w) = build_and_layout(widget);
        click_at(&mut tree, root_id, 150.0, 130.0);
        let pos = log.lock().unwrap().expect("spy must receive MouseDown");
        assert!((pos.x - 100.0).abs() < 1e-3 && (pos.y - 100.0).abs() < 1e-3,
            "regression: expected (100, 100), got ({}, {})", pos.x, pos.y);
    }

    #[test]
    fn regression_no_transforms() {
        let (spy, log) = SpyTarget::new(Size::new(800.0, 600.0));
        let (mut tree, root_id, _w) = build_and_layout(Box::new(spy));
        click_at(&mut tree, root_id, 110.0, 110.0);
        let pos = log.lock().unwrap().expect("regression: spy without transforms");
        assert!((pos.x - 110.0).abs() < 1e-3 && (pos.y - 110.0).abs() < 1e-3,
            "regression: identity transform must pass screen coords through");
    }

    #[test]
    fn mouse_up_captor_through_scale() {
        let (widget, log) = make_panzoom_with_spy(2.0, Point::zero());
        let (mut tree, root_id, _w) = build_and_layout(widget);
        click_at(&mut tree, root_id, 220.0, 220.0);
        let _ = tree.handle_event(
            root_id,
            &Event::MouseUp {
                button: MouseButton::Left,
                position: Point::new(400.0, 400.0),
            },
        );
        assert!(log.lock().unwrap().is_some());
    }

    #[test]
    fn accumulated_event_transform_identity_for_plain_tree() {
        let (spy, _log) = SpyTarget::new(Size::new(800.0, 600.0));
        let (tree, root_id, _w) = build_and_layout(Box::new(spy));

        let mut id = root_id;
        loop {
            let next = tree
                .elements
                .get(&id)
                .and_then(|n| n.children.first().copied());
            match next {
                Some(c) => id = c,
                None => break,
            }
        }
        let (s, k) = tree.accumulated_event_transform(id);
        assert_eq!(s.x, 0.0);
        assert_eq!(s.y, 0.0);
        assert!((k - 1.0).abs() < 1e-6);
    }

    #[test]
    fn accumulated_event_transform_zoom_only() {
        let (widget, _log) = make_panzoom_with_spy(2.0, Point::zero());
        let (tree, root_id, _w) = build_and_layout(widget);

        let child = tree
            .elements
            .get(&root_id)
            .and_then(|n| n.children.first().copied())
            .expect("PanZoom must have a child");
        let (s, k) = tree.accumulated_event_transform(child);
        assert!((k - 2.0).abs() < 1e-3, "expected K=2, got {}", k);
        assert!(s.x.abs() < 1e-3 && s.y.abs() < 1e-3, "expected S=(0,0), got ({}, {})", s.x, s.y);
    }
}
