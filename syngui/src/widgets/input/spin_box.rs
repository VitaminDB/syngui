use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use crate::core::sync::Mutex;

/// Id поля, которое сейчас правят с клавиатуры (0 — никто). Правка — это
/// состояние на всё приложение: одновременно мигающих кареток быть не должно.
static ACTIVE_EDITOR: AtomicU64 = AtomicU64::new(0);

const DEFAULT_BUTTON_WIDTH: f32 = 28.0;
const DEFAULT_HEIGHT: f32 = 40.0;
const DEFAULT_BORDER_RADIUS: f32 = 8.0;
const DEFAULT_BORDER_WIDTH: f32 = 1.0;
const DEFAULT_FONT_SIZE: f32 = 14.0;
const DEFAULT_VALUE_PADDING: f32 = 6.0;

const REPEAT_INITIAL_DELAY: f32 = 0.4;
const REPEAT_INTERVAL: f32 = 0.08;

const CURSOR_BLINK_RATE: f32 = 1.0;

pub struct SpinBox {
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    decimal_places: u8,
    disabled: bool,
    width: Option<Dimension>,
    on_change: Option<Arc<Mutex<dyn FnMut(f64) + Send>>>,
}

impl SpinBox {
    pub fn new() -> Self {
        Self {
            value: 0.0,
            min: f64::MIN,
            max: f64::MAX,
            step: 1.0,
            decimal_places: 0,
            disabled: false,
            width: None,
            on_change: None,
        }
    }

    pub fn value(mut self, value: f64) -> Self {
        self.value = value;
        self
    }

    pub fn min(mut self, min: f64) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn decimal_places(mut self, decimal_places: u8) -> Self {
        self.decimal_places = decimal_places;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(Dimension::Px(width));
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(f64) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Default for SpinBox {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for SpinBox {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(SpinBoxElement {
            id: ElementId::new(),
            value: self.value,
            min: self.min,
            max: self.max,
            step: self.step,
            decimal_places: self.decimal_places,
            disabled: self.disabled,
            width: self.width,
            on_change: self.on_change.clone(),
            bounds: Rect::zero(),
            minus_hovered: false,
            plus_hovered: false,
            minus_pressed: false,
            plus_pressed: false,
            repeat_direction: None,
            repeat_elapsed: 0.0,
            repeat_initial_done: false,
            editing: false,
            edit_text: String::new(),
            edit_cursor: 0,
            cursor_blink: 0.0,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
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

pub struct SpinBoxElement {
    id: ElementId,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    decimal_places: u8,
    disabled: bool,
    width: Option<Dimension>,
    on_change: Option<Arc<Mutex<dyn FnMut(f64) + Send>>>,
    bounds: Rect,
    minus_hovered: bool,
    plus_hovered: bool,
    minus_pressed: bool,
    plus_pressed: bool,
    repeat_direction: Option<i8>,
    repeat_elapsed: f32,
    repeat_initial_done: bool,
    editing: bool,
    edit_text: String,
    edit_cursor: usize,
    cursor_blink: f32,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl SpinBoxElement {
    fn button_width(&self) -> f32 {
        self.mss.padding_left.unwrap_or(DEFAULT_BUTTON_WIDTH)
    }

    fn value_padding(&self) -> f32 {
        self.mss.padding_right.unwrap_or(DEFAULT_VALUE_PADDING)
    }

    fn resolved_height(&self) -> f32 {
        self.mss.height.map(|d| d.resolve(f32::INFINITY)).unwrap_or(DEFAULT_HEIGHT)
    }

    fn resolved_radius(&self) -> f32 {
        self.mss.border_radius_uniform(self.bounds.size.width.min(self.bounds.size.height), DEFAULT_BORDER_RADIUS)
    }

    fn resolved_border_width(&self) -> f32 {
        self.mss.border_width.unwrap_or(DEFAULT_BORDER_WIDTH)
    }

    fn value_font_size(&self) -> f32 {
        self.mss.font_size_or(DEFAULT_FONT_SIZE)
    }

    fn button_font_size(&self) -> f32 {
        self.mss.font_size_or(DEFAULT_FONT_SIZE) + 2.0
    }

    fn minus_rect(&self) -> Rect {
        let bw = self.button_width();
        Rect::new(
            self.bounds.origin,
            Size::new(bw, self.bounds.size.height),
        )
    }

    fn plus_rect(&self) -> Rect {
        let bw = self.button_width();
        Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - bw, self.bounds.y()),
            Size::new(bw, self.bounds.size.height),
        )
    }

    fn value_rect(&self) -> Rect {
        let bw = self.button_width();
        let pad = self.value_padding();
        Rect::new(
            Point::new(self.bounds.x() + bw + pad, self.bounds.y()),
            Size::new((self.bounds.size.width - bw * 2.0 - pad * 2.0).max(0.0), self.bounds.size.height),
        )
    }

    fn set_value(&mut self, new_value: f64) {
        let clamped = new_value.clamp(self.min, self.max);
        if (clamped - self.value).abs() > f64::EPSILON {
            self.value = clamped;
            self.trigger_change();
        }
    }

    fn trigger_change(&mut self) {
        if let Some(ref callback) = self.on_change {
            if let Ok(mut cb) = callback.lock() {
                cb(self.value);
            }
        }
    }

    fn formatted_value(&self) -> String {
        format!("{:.prec$}", self.value, prec = self.decimal_places as usize)
    }

    fn start_editing(&mut self) {
        self.editing = true;
        self.edit_text = self.formatted_value();
        self.edit_cursor = self.edit_text.len();
        self.cursor_blink = 0.0;
        ACTIVE_EDITOR.store(self.id.0, Ordering::Relaxed);
    }

    fn release_editor_slot(&self) {
        let _ = ACTIVE_EDITOR.compare_exchange(
            self.id.0,
            0,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }

    fn commit_editing(&mut self) {
        if self.editing {
            self.editing = false;
            self.release_editor_slot();
            if let Ok(v) = self.edit_text.parse::<f64>() {
                self.set_value(v);
            }
        }
    }

    fn cancel_editing(&mut self) {
        self.editing = false;
        self.release_editor_slot();
    }

    fn cursor_byte_pos(&self) -> usize {
        self.edit_text.char_indices()
            .nth(self.edit_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.edit_text.len())
    }

    fn start_repeat(&mut self, direction: i8) {
        self.repeat_direction = Some(direction);
        self.repeat_elapsed = 0.0;
        self.repeat_initial_done = false;
    }

    fn stop_repeat(&mut self) {
        self.repeat_direction = None;
    }
}

impl Element for SpinBoxElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(spin_box) = widget.as_any().downcast_ref::<SpinBox>() {
            if !self.editing {
                self.value = spin_box.value;
            }
            self.min = spin_box.min;
            self.max = spin_box.max;
            self.step = spin_box.step;
            self.decimal_places = spin_box.decimal_places;
            self.disabled = spin_box.disabled;
            self.width = spin_box.width;
            self.on_change = spin_box.on_change.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = self.width.map(|d| d.resolve(constraints.max_width))
            .unwrap_or(constraints.max_width).min(constraints.max_width);
        let height = self.resolved_height();
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg = self.mss.color.unwrap_or_else(|| Color::from_hex("#1F2937"));
        let border_color = self.mss.border_color.unwrap_or_else(|| Color::from_hex("#D1D5DB"));
        let hover_bg = if self.mss.background_color.is_some() { bg.lighten(0.1) } else { Color::from_hex("#F3F4F6") };
        let pressed_bg = if self.mss.background_color.is_some() { bg.lighten(0.15) } else { Color::from_hex("#E5E7EB") };
        let divider_color = if self.mss.border_color.is_some() { border_color } else { Color::from_hex("#E5E7EB") };
        let accent = self.mss.accent_color.unwrap_or_else(|| Color::from_hex("#3B82F6"));

        let bg_color = if self.disabled { bg.darken(0.05) } else { bg };
        let text_color = if self.disabled { fg.with_alpha(0.5) } else { fg };

        let radius = self.resolved_radius();
        let bw = self.button_width();
        let border_w = self.resolved_border_width();
        let h = self.bounds.size.height;

        let active_border = if self.editing { accent } else { border_color };
        let active_bw = if self.editing { 2.0 } else { border_w };
        list.push_rect_bordered(
            self.bounds, bg_color, [radius; 4],
            Border { width: active_bw, color: active_border },
        );

        let inner_radius = (radius - border_w).max(0.0);
        if !self.disabled {
            if self.minus_pressed || self.minus_hovered {
                let color = if self.minus_pressed { pressed_bg } else { hover_bg };
                let r = Rect::new(
                    Point::new(self.bounds.x() + border_w, self.bounds.y() + border_w),
                    Size::new(bw - border_w, h - border_w * 2.0),
                );
                list.push_rect(r, color, [inner_radius, 0.0, 0.0, inner_radius]);
            }
            if self.plus_pressed || self.plus_hovered {
                let color = if self.plus_pressed { pressed_bg } else { hover_bg };
                let r = Rect::new(
                    Point::new(self.bounds.x() + self.bounds.size.width - bw, self.bounds.y() + border_w),
                    Size::new(bw - border_w, h - border_w * 2.0),
                );
                list.push_rect(r, color, [0.0, inner_radius, inner_radius, 0.0]);
            }
        }

        let left_div = Rect::new(
            Point::new(self.bounds.x() + bw, self.bounds.y()),
            Size::new(1.0, h),
        );
        list.push_rect(left_div, divider_color, [0.0; 4]);

        let right_div = Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - bw, self.bounds.y()),
            Size::new(1.0, h),
        );
        list.push_rect(right_div, divider_color, [0.0; 4]);

        let btn_fs = self.button_font_size();
        list.push_text_centered("\u{2212}", self.minus_rect(), text_color, btn_fs);
        list.push_text_centered("+", self.plus_rect(), text_color, btn_fs);

        let val_fs = self.value_font_size();
        let vr = self.value_rect();

        if self.editing {
            let text_rect = Rect::new(
                Point::new(vr.x() + 2.0, vr.y() + (vr.size.height - val_fs) / 2.0),
                Size::new(vr.size.width - 4.0, val_fs + 2.0),
            );
            list.push_text(&self.edit_text, text_rect, text_color, val_fs);

            let blink_phase = (self.cursor_blink * CURSOR_BLINK_RATE * 2.0) % 2.0;
            if blink_phase < 1.0 {
                let prefix: String = self.edit_text.chars().take(self.edit_cursor).collect();
                let char_w = val_fs * 0.6;
                let cursor_x = text_rect.x() + prefix.len() as f32 * char_w;
                let cursor_rect = Rect::new(
                    Point::new(cursor_x, vr.y() + (vr.size.height - val_fs) / 2.0),
                    Size::new(1.5, val_fs),
                );
                list.push_rect(cursor_rect, accent, [0.0; 4]);
            }
        } else {
            list.push_text_centered(&self.formatted_value(), vr, text_color, val_fs);
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled {
            return EventResult::Ignored;
        }

        match event {
            Event::MouseMove(pos) => {
                let was_minus = self.minus_hovered;
                let was_plus = self.plus_hovered;

                self.minus_hovered = self.minus_rect().contains(*pos);
                self.plus_hovered = self.plus_rect().contains(*pos);

                if self.minus_hovered || self.plus_hovered {
                    ctx.set_cursor(CursorIcon::Pointer);
                } else if self.value_rect().contains(*pos) {
                    ctx.set_cursor(CursorIcon::Text);
                }

                if self.minus_hovered != was_minus || self.plus_hovered != was_plus {
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.bounds.contains(*pos) {
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left {
                    if self.minus_rect().contains(*position) {
                        if self.editing { self.commit_editing(); }
                        self.minus_pressed = true;
                        self.set_value(self.value - self.step);
                        self.start_repeat(-1);
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    if self.plus_rect().contains(*position) {
                        if self.editing { self.commit_editing(); }
                        self.plus_pressed = true;
                        self.set_value(self.value + self.step);
                        self.start_repeat(1);
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    if self.value_rect().contains(*position) {
                        if !self.editing {
                            self.start_editing();
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    if self.editing {
                        self.commit_editing();
                        ctx.request_paint();
                    }
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } => {
                if *button == MouseButton::Left {
                    let was_pressed = self.minus_pressed || self.plus_pressed;
                    self.minus_pressed = false;
                    self.plus_pressed = false;
                    self.stop_repeat();

                    if was_pressed {
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::CharInput(ch) if self.editing => {
                if ch.is_control() { return EventResult::Ignored; }
                if ch.is_ascii_digit() || *ch == '.' || *ch == '-' {
                    let byte_pos = self.cursor_byte_pos();
                    self.edit_text.insert(byte_pos, *ch);
                    self.edit_cursor += 1;
                    self.cursor_blink = 0.0;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Handled
            }
            Event::KeyDown(key) if self.editing => {
                match key {
                    Key::Backspace => {
                        if self.edit_cursor > 0 {
                            self.edit_cursor -= 1;
                            let byte_pos = self.cursor_byte_pos();
                            self.edit_text.remove(byte_pos);
                            self.cursor_blink = 0.0;
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    Key::Delete => {
                        let byte_pos = self.cursor_byte_pos();
                        if byte_pos < self.edit_text.len() {
                            self.edit_text.remove(byte_pos);
                            self.cursor_blink = 0.0;
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    Key::Left => {
                        if self.edit_cursor > 0 {
                            self.edit_cursor -= 1;
                            self.cursor_blink = 0.0;
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    Key::Right => {
                        let char_count = self.edit_text.chars().count();
                        if self.edit_cursor < char_count {
                            self.edit_cursor += 1;
                            self.cursor_blink = 0.0;
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    Key::Home => {
                        self.edit_cursor = 0;
                        self.cursor_blink = 0.0;
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::End => {
                        self.edit_cursor = self.edit_text.chars().count();
                        self.cursor_blink = 0.0;
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Enter => {
                        self.commit_editing();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Escape => {
                        self.cancel_editing();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    _ => EventResult::Handled,
                }
            }
            Event::FocusGained => EventResult::Handled,
            Event::FocusLost => {
                if self.editing {
                    self.commit_editing();
                }
                self.minus_hovered = false;
                self.plus_hovered = false;
                self.stop_repeat();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(Key::Left) | Event::KeyDown(Key::Down) => {
                self.set_value(self.value - self.step);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(Key::Right) | Event::KeyDown(Key::Up) => {
                self.set_value(self.value + self.step);
                ctx.request_paint();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let dt_secs = dt.as_secs_f32();
        let mut needs_anim = false;

        if let Some(dir) = self.repeat_direction {
            self.repeat_elapsed += dt_secs;
            if !self.repeat_initial_done {
                if self.repeat_elapsed >= REPEAT_INITIAL_DELAY {
                    self.repeat_initial_done = true;
                    self.repeat_elapsed -= REPEAT_INITIAL_DELAY;
                    self.set_value(self.value + dir as f64 * self.step);
                }
            } else {
                while self.repeat_elapsed >= REPEAT_INTERVAL {
                    self.repeat_elapsed -= REPEAT_INTERVAL;
                    self.set_value(self.value + dir as f64 * self.step);
                }
            }
            self.mark_dirty(DirtyFlags::RENDER);
            needs_anim = true;
        }

        if self.editing {
            // Клик в соседний SpinBox сюда не приходит (позиционная
            // доставка), а `FocusLost` полю с ролью Slider не шлют — без
            // этой проверки в панели одновременно мигали две каретки.
            if ACTIVE_EDITOR.load(Ordering::Relaxed) != self.id.0 {
                self.commit_editing();
                self.mark_dirty(DirtyFlags::RENDER);
                return needs_anim;
            }
            self.cursor_blink += dt_secs;
            self.mark_dirty(DirtyFlags::RENDER);
            needs_anim = true;
        }

        needs_anim
    }

    fn needs_repaint(&self) -> bool {
        self.repeat_direction.is_some() || self.editing
    }

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        (
            self.width.map(|d| d.resolve(f32::INFINITY)),
            self.mss.height.map(|d| d.resolve(f32::INFINITY)),
        )
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
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

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str { "SpinBox" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Slider,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: Some(format!("SpinBox: {}", self.formatted_value())),
                value: Some(self.value.to_string()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for SpinBoxElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
