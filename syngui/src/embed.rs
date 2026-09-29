//! Встраивание дерева syngui в чужой цикл событий — без winit.
//!
//! [`EmbedView`] — то, что `AppHandler` делает для окна winit, но для
//! произвольной поверхности: кадр (пересборки, стили, эффекты, раскладка,
//! display list), маршрутизация мыши и клавиатуры (hover, двойной клик,
//! фокус полей по клику, Tab, текстовый ввод, перетаскивание), анимации.
//! GPU и поверхность остаются на хосте: он получает готовый [`DisplayList`]
//! и рисует его своим [`crate::gpu::Renderer`]. Так работают layer-shell
//! поверхности оболочки рабочего стола (крейт `syngui-layer`).
//!
//! Все вызовы — из главного потока (сигналы thread-local).

use crate::a11y::FocusManager;
use crate::core::{Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, Modifiers, MouseButton};
use crate::layout::Constraints;
use crate::mss::{cascade, StyleEngine};
use crate::render::DisplayList;
use crate::widget::{ElementId, ElementTree, Widget};
use std::time::Duration;
use web_time::Instant;

/// Дерево элементов одной поверхности и состояние ввода над ним.
pub struct EmbedView {
    pub tree: ElementTree,
    root_id: Option<ElementId>,
    focus: FocusManager,
    tab_order_dirty: bool,
    cursor: Point,
    last_click: Option<(Instant, Point)>,
    /// Порог двойного клика.
    pub double_click_interval: Duration,
    modifiers: Modifiers,
    last_frame_sig: Option<u64>,
    touch: crate::input::TouchTracker,
}

impl Default for EmbedView {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbedView {
    pub fn new() -> Self {
        Self {
            tree: ElementTree::new(),
            root_id: None,
            focus: FocusManager::new(),
            tab_order_dirty: true,
            cursor: Point::new(-1.0, -1.0),
            last_click: None,
            double_click_interval: crate::input::resolve_double_click_interval(),
            modifiers: Modifiers::empty(),
            last_frame_sig: None,
            touch: crate::input::TouchTracker::new(),
        }
    }

    /// Смонтировать корневой виджет и применить стили.
    pub fn mount(&mut self, widget: Box<dyn Widget>, engine: &StyleEngine) {
        let element = widget.create_element();
        let type_id = widget.as_any().type_id();
        let root_id = self.tree.insert_with_type_id(element, None, type_id);
        widget.mount(&mut self.tree, root_id);
        self.tree.set_root(root_id);
        self.root_id = Some(root_id);
        cascade::apply_styles_dirty(&mut self.tree, engine);
        for _ in 0..8 {
            if !self.tree.rebuild_if_needed(root_id) {
                break;
            }
            cascade::apply_styles_dirty(&mut self.tree, engine);
        }
        self.tree.animations_armed = true;
        self.tab_order_dirty = true;
    }

    pub fn root(&self) -> Option<ElementId> {
        self.root_id
    }

    /// Есть ли в этом дереве элементы, помеченные сигналами к пересборке.
    pub fn has_dirty(&self) -> bool {
        crate::signal::has_dirty_elements()
            && crate::signal::dirty_element_ids()
                .into_iter()
                .any(|id| self.tree.elements.contains_key(&id))
    }

    /// Взведены ли анимации (с прошлого обхода могли начаться новые).
    pub fn animations_armed(&self) -> bool {
        self.tree.animations_armed
    }

    /// Тик анимаций; `true` — что-то изменилось, нужен кадр. Пустой обход
    /// снимает «взвод» — в простое дерево не обходится.
    pub fn animate(&mut self, dt: Duration) -> bool {
        let Some(root) = self.root_id else { return false };
        if !self.tree.animations_armed {
            return false;
        }
        let dt = dt.min(Duration::from_millis(64));
        if self.tree.animate(root, dt) {
            true
        } else {
            self.tree.animations_armed = false;
            false
        }
    }

    /// Сменилась таблица стилей: пересчитать стили всех элементов.
    pub fn restyle_all(&mut self, engine: &StyleEngine) {
        self.restyle_all_with_transition(engine, None);
    }

    /// То же, но изменившиеся цвета перетекают за `transition`
    /// (длительность, кривая), а не меняются скачком — плавная смена темы.
    pub fn restyle_all_with_transition(
        &mut self,
        engine: &StyleEngine,
        transition: Option<(Duration, crate::animation::Easing)>,
    ) {
        for node in self.tree.elements.values_mut() {
            node.styles_dirty = true;
        }
        cascade::with_theme_transition(transition, || cascade::apply_styles_dirty(&mut self.tree, engine));
        self.tree.mark_all_dirty(crate::widget::DirtyFlags::LAYOUT | crate::widget::DirtyFlags::RENDER);
        self.tree.animations_armed = true;
        self.last_frame_sig = None;
    }

    /// Пересборки + стили + эффекты — начало любого кадра.
    fn prepare(&mut self, engine: &StyleEngine) {
        let Some(root) = self.root_id else { return };
        for _ in 0..8 {
            if !self.tree.rebuild_if_needed(root) {
                break;
            }
            cascade::apply_styles_dirty(&mut self.tree, engine);
            self.tab_order_dirty = true;
        }
        self.process_pending_autofocus();
        crate::signal::drain_and_run_effects();
    }

    fn layout_with(&mut self, engine: &StyleEngine, constraints: Constraints) -> Size {
        let Some(root) = self.root_id else { return Size::zero() };
        self.tree.root_offset = Point::zero();
        self.tree.set_pixel_snap_scale(0.0);
        let mut size = self.tree.layout(root, constraints);
        if self.tree.rebuild_if_needed(root) {
            cascade::apply_styles_dirty(&mut self.tree, engine);
            size = self.tree.layout(root, constraints);
            self.tab_order_dirty = true;
        }
        self.tree.force_full_measure = false;
        size
    }

    /// Предпочтительный размер содержимого при свободных ограничениях до
    /// `max` — для поверхностей, подгоняющих размер под содержимое.
    pub fn measure(&mut self, engine: &StyleEngine, max: Size) -> Size {
        self.prepare(engine);
        self.tree.viewport_size = max;
        self.layout_with(engine, Constraints::new(0.0, max.width, 0.0, max.height))
    }

    /// Кадр: пересборки, стили, эффекты, раскладка под `size` (логические
    /// единицы; корень растягивается на всю поверхность), display list.
    /// Возвращает `false`, если кадр совпал с
    /// прошлым отданным (`DisplayList::frame_signature`) — рисовать не нужно.
    pub fn frame(&mut self, engine: &StyleEngine, size: Size, scale: f32, list: &mut DisplayList) -> bool {
        self.tree.animations_armed = true;
        let Some(root) = self.root_id else { return false };
        self.prepare(engine);
        self.tree.viewport_size = size;
        crate::viewport::publish(size);
        crate::viewport::publish_origin(Point::zero());
        self.layout_with(engine, Constraints::tight(size));
        if self.tab_order_dirty {
            self.focus.rebuild_tab_order(&self.tree, root);
            self.tab_order_dirty = false;
        }
        list.clear();
        list.set_surface_size(size);
        list.set_scale_factor(scale);
        self.tree.build_display_list(root, list, Rect::new(Point::zero(), size));
        self.tree.build_drag_overlay(list);
        self.tree.sync_overlay_stack();
        let sig = list.frame_signature();
        if sig.is_some() && sig == self.last_frame_sig {
            return false;
        }
        self.last_frame_sig = sig;
        true
    }

    /// Забыть подпись прошлого кадра — следующий кадр будет нарисован
    /// (поверхность пересоздана, сменился масштаб, кадр не дошёл до экрана).
    pub fn invalidate(&mut self) {
        self.last_frame_sig = None;
    }

    /// Курсор, который просят элементы под указателем.
    pub fn cursor_icon(&self) -> CursorIcon {
        self.tree.cursor_request.unwrap_or(CursorIcon::Default)
    }

    pub fn pointer_position(&self) -> Point {
        self.cursor
    }

    fn dispatch(&mut self, event: &Event) -> EventResult {
        let Some(root) = self.root_id else { return EventResult::Ignored };
        self.tree.animations_armed = true;
        self.tree.handle_event(root, event)
    }

    pub fn pointer_motion(&mut self, pos: Point) -> EventResult {
        self.cursor = pos;
        let r = self.dispatch(&Event::MouseMove(pos));
        if self.tree.drag_state.is_some() {
            if let Some(ref mut drag) = self.tree.drag_state {
                drag.current_pos = pos;
            }
            let data = self.tree.drag_state.as_ref().map(|d| d.data.clone());
            if let Some(data) = data {
                self.tree.dispatch_drag_event(&Event::DragMove { position: pos, data });
            }
        }
        r
    }

    /// Указатель ушёл с поверхности: снять hover.
    pub fn pointer_leave(&mut self) {
        self.cursor = Point::new(-1.0, -1.0);
        self.dispatch(&Event::MouseMove(Point::new(-1.0, -1.0)));
    }

    pub fn pointer_button(&mut self, button: MouseButton, pressed: bool) -> EventResult {
        let pos = self.cursor;
        if pressed {
            self.update_focus_from_click(pos);
        }
        if !pressed && self.tree.drag_state.is_some() {
            if let Some(root) = self.root_id {
                self.tree.end_drag(root, pos, false);
            }
            return EventResult::Handled;
        }
        if pressed {
            let is_double = self.last_click.is_some_and(|(t, p)| {
                let d = ((pos.x - p.x).powi(2) + (pos.y - p.y).powi(2)).sqrt();
                t.elapsed() < self.double_click_interval && d < 4.0
            });
            if is_double {
                self.last_click = None;
                self.dispatch(&Event::DoubleClick { button, position: pos })
            } else {
                self.last_click = Some((Instant::now(), pos));
                self.dispatch(&Event::MouseDown { button, position: pos })
            }
        } else {
            self.dispatch(&Event::MouseUp { button, position: pos })
        }
    }

    /// Палец коснулся поверхности (`id` — номер касания у хоста).
    /// Прокрутки, слайдеры и жесты получают сырые `Touch*`, тап и долгое
    /// нажатие синтезируются (см. [`crate::input::touch`]).
    pub fn touch_down(&mut self, id: u64, pos: Point) -> EventResult {
        self.cursor = pos;
        let Some(root) = self.root_id else { return EventResult::Ignored };
        self.tree.animations_armed = true;
        let tree = &mut self.tree;
        self.touch.down(id, pos, &mut |e| tree.handle_event(root, e));
        EventResult::Handled
    }

    pub fn touch_motion(&mut self, id: u64, pos: Point) -> EventResult {
        self.cursor = pos;
        let Some(root) = self.root_id else { return EventResult::Ignored };
        self.tree.animations_armed = true;
        let tree = &mut self.tree;
        self.touch.motion(id, pos, &mut |e| tree.handle_event(root, e));
        EventResult::Handled
    }

    /// Палец поднят; `pos` — `None`, если хост точку не знает (`wl_touch.up`
    /// её не передаёт) — берётся последняя.
    pub fn touch_up(&mut self, id: u64, pos: Option<Point>) -> EventResult {
        let Some(root) = self.root_id else { return EventResult::Ignored };
        self.tree.animations_armed = true;
        if self.touch.would_tap(id) {
            self.update_focus_from_click(pos.unwrap_or(self.cursor));
        }
        let tree = &mut self.tree;
        self.touch.up(id, pos, &mut |e| tree.handle_event(root, e));
        EventResult::Handled
    }

    /// Хост отменил касания (жест забрал композитор).
    pub fn touch_cancel(&mut self) {
        let Some(root) = self.root_id else { return };
        self.tree.animations_armed = true;
        let tree = &mut self.tree;
        self.touch.cancel(&mut |e| tree.handle_event(root, e));
    }

    /// Когда проверить долгое нажатие ([`Self::touch_poll`]); `None` — не нужно.
    pub fn touch_deadline(&self) -> Option<Instant> {
        self.touch.deadline()
    }

    /// Проверить таймер долгого нажатия; `true` — сработало, нужен кадр.
    pub fn touch_poll(&mut self) -> bool {
        let Some(root) = self.root_id else { return false };
        let tree = &mut self.tree;
        let fired = self.touch.poll(&mut |e| tree.handle_event(root, e));
        if fired {
            self.tree.animations_armed = true;
        }
        fired
    }

    /// Сколько пальцев сейчас на поверхности.
    pub fn touch_count(&self) -> usize {
        self.touch.active()
    }

    /// Прокрутка: `dy > 0` — вверх (как `LineDelta` winit, уже в пикселях).
    pub fn wheel(&mut self, dx: f32, dy: f32) -> EventResult {
        let pos = self.cursor;
        self.dispatch(&Event::MouseWheel { delta: dy, delta_x: dx, position: pos })
    }

    pub fn set_modifiers(&mut self, m: Modifiers) {
        self.modifiers = m;
        self.tree.modifiers = m;
    }

    /// Клавиша. `text` — набранный текст (только при нажатии). Tab двигает
    /// фокус, если фокусный элемент не забирает Tab себе.
    pub fn key(&mut self, key: Key, pressed: bool, text: Option<&str>) -> EventResult {
        if pressed && key == Key::Tab {
            let wants_tab = self
                .tree
                .focused_element
                .and_then(|id| self.tree.get(id))
                .map(|el| el.wants_tab())
                .unwrap_or(false);
            if !wants_tab {
                let old = self.focus.current_focus().or(self.tree.focused_element);
                let new = if self.modifiers.shift { self.focus.previous_focus() } else { self.focus.next_focus() };
                if let Some(new_id) = new {
                    if let Some(old_id) = old {
                        if old_id != new_id {
                            self.tree.dispatch_event_to(old_id, &Event::FocusLost);
                        }
                    }
                    self.tree.dispatch_event_to(new_id, &Event::FocusGained);
                    self.tree.focused_element = Some(new_id);
                    self.tree.animations_armed = true;
                    return EventResult::Handled;
                }
            }
        }
        let combo = (self.modifiers.ctrl && !self.modifiers.alt) || self.modifiers.meta;
        if pressed && !combo {
            if let Some(text) = text {
                for ch in text.chars().filter(|c| !c.is_control()) {
                    self.dispatch(&Event::CharInput(ch));
                }
            }
        }
        let ev = if pressed { Event::KeyDown(key) } else { Event::KeyUp(key) };
        self.dispatch(&ev)
    }

    /// Поверхность потеряла клавиатуру.
    pub fn keyboard_leave(&mut self) {
        if let Some(id) = self.tree.focused_element {
            if self.tree.elements.contains_key(&id) {
                self.tree.dispatch_event_to(id, &Event::FocusLost);
            }
        }
        self.modifiers = Modifiers::empty();
        self.tree.modifiers = self.modifiers;
    }

    /// Поверхность получила клавиатуру: вернуть фокус полю, если было.
    pub fn keyboard_enter(&mut self) {
        if let Some(id) = self.tree.focused_element {
            if self.tree.elements.contains_key(&id) {
                self.tree.dispatch_event_to(id, &Event::FocusGained);
            }
        }
    }

    fn process_pending_autofocus(&mut self) {
        let Some(id) = self.tree.pending_autofocus.take() else { return };
        if !self.tree.elements.contains_key(&id) {
            return;
        }
        let old = self.focus.current_focus();
        if old != Some(id) {
            if let Some(old_id) = old {
                if self.tree.elements.contains_key(&old_id) {
                    self.tree.dispatch_event_to(old_id, &Event::FocusLost);
                }
            }
            if !self.focus.set_focus(id) {
                if let Some(root) = self.root_id {
                    self.focus.rebuild_tab_order(&self.tree, root);
                }
                self.focus.set_focus(id);
            }
        }
        self.tree.focused_element = Some(id);
        self.tree.dispatch_event_to(id, &Event::FocusGained);
    }

    fn update_focus_from_click(&mut self, pos: Point) {
        let Some(root) = self.root_id else { return };
        let old = self.focus.current_focus().or(self.tree.focused_element);
        let mut new_focus = None;
        let mut in_nonmodal_overlay = false;
        self.tree.refresh_overlay_bounds();
        for entry in self.tree.overlay_stack.iter().rev() {
            if entry.modal {
                new_focus = self.text_input_at(entry.element_id, pos);
                break;
            }
            if entry.bounds.contains(pos) {
                let c = self.text_input_at(entry.element_id, pos);
                if c.is_some() {
                    new_focus = c;
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
            new_focus = self.text_input_at(root, pos);
        }
        if new_focus == old && new_focus.is_some() {
            return;
        }
        if let Some(old_id) = old {
            if Some(old_id) != new_focus && self.tree.elements.contains_key(&old_id) {
                self.tree.dispatch_event_to(old_id, &Event::FocusLost);
            }
        }
        if let Some(id) = new_focus {
            if !self.focus.set_focus(id) {
                self.focus.rebuild_tab_order(&self.tree, root);
                self.focus.set_focus(id);
            }
            self.tree.focused_element = Some(id);
            self.tree.dispatch_event_to(id, &Event::FocusGained);
        } else {
            self.focus.clear_focus();
            self.tree.focused_element = None;
        }
    }

    /// Поле ввода под точкой (как `AppHandler::find_text_input_at`).
    fn text_input_at(&self, element_id: ElementId, pos: Point) -> Option<ElementId> {
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
        for &child in node.children.iter().rev() {
            if let Some(found) = self.text_input_at(child, child_pos) {
                return Some(found);
            }
        }
        if is_passthrough && !is_portal && !node.element.hit_test(pos) {
            return None;
        }
        if let Some(info) = node.element.accessibility_info() {
            if matches!(
                info.role,
                crate::a11y::Role::TextField | crate::a11y::Role::ComboBox | crate::a11y::Role::Terminal
            ) && !info.state.disabled
                && node.element.text_input_hit(pos)
            {
                return Some(element_id);
            }
        }
        None
    }
}
