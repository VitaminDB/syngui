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
use crate::core::sync::Mutex;

/// Ширина зоны числового значения (см. [`Slider::show_value`]) по умолчанию.
const DEFAULT_VALUE_WIDTH: f32 = 44.0;
/// Зазор между треком и зоной значения.
const VALUE_GAP: f32 = 6.0;
/// Скорость мигания caret'а в режиме текстового ввода (циклов/сек).
const CURSOR_BLINK_RATE: f32 = 1.0;

pub struct Slider {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub disabled: bool,
    pub width: Option<Dimension>,
    pub vertical: bool,
    pub bipolar: bool,
    pub show_value: bool,
    pub decimals: u8,
    pub value_width: Option<f32>,
    pub on_change: Option<Arc<Mutex<dyn FnMut(f32) + Send>>>,
}

impl Slider {
    pub fn new() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            disabled: false,
            width: None,
            vertical: false,
            bipolar: false,
            show_value: false,
            decimals: 0,
            value_width: None,
            on_change: None,
        }
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value;
        self
    }

    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = step;
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

    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    pub fn bipolar(mut self) -> Self {
        self.bipolar = true;
        self
    }

    /// Числовое значение справа от трека. Клик по числу открывает
    /// инлайн-редактор: точный ввод с клавиатуры, Enter/клик-мимо —
    /// применить (значение снапится к `step` и клампится в диапазон),
    /// Escape — отменить. Только для горизонтального слайдера.
    /// Стилизация через MSS: `label-color` (цвет числа), `value-font-size`
    /// или `font-size` (кегль), `caret-color` (курсор ввода).
    pub fn show_value(mut self, decimals: u8) -> Self {
        self.show_value = true;
        self.decimals = decimals;
        self
    }

    /// Ширина зоны значения (по умолчанию 44 px).
    pub fn value_width(mut self, width: f32) -> Self {
        self.value_width = Some(width);
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(f32) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Default for Slider {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Slider {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(SliderElement {
            id: ElementId::new(),
            value: self.value,
            min: self.min,
            max: self.max,
            step: self.step,
            disabled: self.disabled,
            width: self.width,
            vertical: self.vertical,
            bipolar: self.bipolar,
            show_value: self.show_value,
            decimals: self.decimals,
            value_width: self.value_width,
            bounds: Rect::zero(),
            track_bounds: Rect::zero(),
            dragging: false,
            touch_id: None,
            hover: false,
            focused: false,
            editing: false,
            edit_text: String::new(),
            edit_cursor: 0,
            cursor_blink: 0.0,
            label_color: None,
            value_font_size: None,
            on_change: self.on_change.clone(),
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

pub struct SliderElement {
    id: ElementId,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    disabled: bool,
    width: Option<Dimension>,
    vertical: bool,
    bipolar: bool,
    show_value: bool,
    decimals: u8,
    value_width: Option<f32>,
    bounds: Rect,
    track_bounds: Rect,
    dragging: bool,
    /// Палец, ведущий текущий drag (тачскрины; см. Touch-ветки handle_event).
    touch_id: Option<u64>,
    hover: bool,
    focused: bool,
    editing: bool,
    edit_text: String,
    edit_cursor: usize,
    cursor_blink: f32,
    label_color: Option<Color>,
    value_font_size: Option<f32>,
    on_change: Option<Arc<Mutex<dyn FnMut(f32) + Send>>>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl SliderElement {
    /// Ширина зоны readout'а (0 если выключен или слайдер вертикальный).
    fn value_zone_width(&self) -> f32 {
        if self.show_value && !self.vertical {
            self.value_width.unwrap_or(DEFAULT_VALUE_WIDTH)
        } else {
            0.0
        }
    }

    fn value_rect(&self) -> Rect {
        let w = self.value_zone_width();
        Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - w, self.bounds.y()),
            Size::new(w, self.bounds.size.height),
        )
    }

    fn formatted_value(&self) -> String {
        format!("{:.*}", self.decimals as usize, self.value)
    }

    /// Кламп + снап к step — та же формула, что у [`Self::pos_to_value`],
    /// чтобы введённое с клавиатуры значение совпадало с достижимым мышью.
    fn snap(&self, v: f32) -> f32 {
        let snapped = if self.step > 0.0 {
            (v / self.step).round() * self.step
        } else {
            v
        };
        snapped.clamp(self.min, self.max)
    }

    fn start_editing(&mut self) {
        self.editing = true;
        self.edit_text = self.formatted_value();
        self.edit_cursor = self.edit_text.chars().count();
        self.cursor_blink = 0.0;
    }

    fn commit_editing(&mut self) {
        if !self.editing {
            return;
        }
        self.editing = false;
        if let Ok(v) = self.edit_text.trim().parse::<f32>() {
            let new_value = self.snap(v);
            if (new_value - self.value).abs() > f32::EPSILON {
                self.value = new_value;
                self.trigger_change();
            }
        }
    }

    fn cancel_editing(&mut self) {
        self.editing = false;
    }

    fn cursor_byte_pos(&self) -> usize {
        self.edit_text
            .char_indices()
            .nth(self.edit_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.edit_text.len())
    }
    fn value_to_pos(&self, value: f32) -> f32 {
        let range = self.max - self.min;
        if self.vertical {
            if range <= 0.0 {
                return self.track_bounds.y() + self.track_bounds.size.height;
            }
            let percent = (value - self.min) / range;
            self.track_bounds.y() + (1.0 - percent) * self.track_bounds.size.height
        } else {
            if range <= 0.0 {
                return self.track_bounds.x();
            }
            let percent = (value - self.min) / range;
            self.track_bounds.x() + percent * self.track_bounds.size.width
        }
    }

    fn pos_to_value(&self, pos: f32) -> f32 {
        let range = self.max - self.min;
        let raw_value = if self.vertical {
            if range <= 0.0 || self.track_bounds.size.height <= 0.0 {
                return self.min;
            }
            let percent =
                ((self.track_bounds.y() + self.track_bounds.size.height - pos)
                    / self.track_bounds.size.height)
                    .clamp(0.0, 1.0);
            self.min + percent * range
        } else {
            if range <= 0.0 || self.track_bounds.size.width <= 0.0 {
                return self.min;
            }
            let percent =
                ((pos - self.track_bounds.x()) / self.track_bounds.size.width).clamp(0.0, 1.0);
            self.min + percent * range
        };

        if self.step > 0.0 {
            (raw_value / self.step).round() * self.step
        } else {
            raw_value
        }
    }

    fn trigger_change(&mut self) {
        if let Some(ref callback) = self.on_change {
            if let Ok(mut cb) = callback.lock() {
                cb(self.value);
            }
        }
    }
}

impl Element for SliderElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(slider) = widget.as_any().downcast_ref::<Slider>() {
            // Пока идёт drag или текстовый ввод, внешнее значение не
            // затирает локальное — иначе rebuild (напр. Reactive на тот же
            // сигнал) сбрасывал бы незакоммиченный ввод.
            if !self.dragging && !self.editing {
                self.value = slider.value;
            }
            self.show_value = slider.show_value;
            self.decimals = slider.decimals;
            self.value_width = slider.value_width;
            self.min = slider.min;
            self.max = slider.max;
            self.step = slider.step;
            self.disabled = slider.disabled;
            self.width = slider.width;
            self.vertical = slider.vertical;
            self.bipolar = slider.bipolar;
            self.on_change = slider.on_change.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        if self.vertical {
            let mss_width = self.mss.width.map(|d| d.resolve(constraints.max_width));
            let width = mss_width.unwrap_or(24.0).min(constraints.max_width);
            let mss_height = self.mss.height.map(|d| d.resolve(constraints.max_height));
            let height = mss_height
                .unwrap_or(120.0)
                .min(constraints.max_height);

            self.bounds = Rect::new(Point::zero(), Size::new(width, height));

            let track_w = self.mss.min_width.map(|d| d.resolve(width)).unwrap_or(4.0);
            self.track_bounds = Rect::new(
                Point::new((width - track_w) / 2.0, 8.0),
                Size::new(track_w, height - 16.0),
            );

            Size::new(width, height)
        } else {
            let mss_height = self.mss.height.map(|d| d.resolve(constraints.max_height));
            let height = mss_height.unwrap_or(24.0);
            let width = self
                .width
                .map(|d| d.resolve(constraints.max_width))
                .unwrap_or(constraints.max_width)
                .min(constraints.max_width);

            self.bounds = Rect::new(Point::zero(), Size::new(width, height));

            let vz = self.value_zone_width();
            let vz_total = if vz > 0.0 { vz + VALUE_GAP } else { 0.0 };
            let track_height = self.mss.min_height.map(|d| d.resolve(height)).unwrap_or(4.0);
            self.track_bounds = Rect::new(
                Point::new(8.0, (height - track_height) / 2.0),
                Size::new((width - 16.0 - vz_total).max(1.0), track_height),
            );

            Size::new(width, height)
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let track_base = self.mss.background_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let fill_base = self.mss.color.unwrap_or(Color::from_hex("#3B82F6"));
        let track_color = if self.disabled {
            track_base.darken(0.1)
        } else {
            track_base
        };
        let fill_color = if self.disabled { track_base.darken(0.2) } else { fill_base };

        let track_radius_basis = if self.vertical {
            self.track_bounds.size.width
        } else {
            self.track_bounds.size.height
        };
        let track_radius = self
            .mss
            .border_radius
            .map(|r| r.map(|d| d.resolve(track_radius_basis)))
            .unwrap_or([2.0; 4]);

        if let Some(ref gradient) = self.mss.background_gradient {
            list.push_gradient_rect(self.track_bounds, gradient.clone(), track_radius);
        } else {
            list.push_rect(self.track_bounds, track_color, track_radius);
        }

        let thumb_pos = self.value_to_pos(self.value);

        if self.mss.background_gradient.is_none() {
            let bipolar_ok = self.bipolar && self.min < 0.0 && self.max > 0.0;
            if self.vertical {
                if bipolar_ok {
                    let zero_y = self.value_to_pos(0.0);
                    let (top, bottom) = if thumb_pos <= zero_y {
                        (thumb_pos, zero_y)
                    } else {
                        (zero_y, thumb_pos)
                    };
                    let h = bottom - top;
                    if h > 0.0 {
                        let fill_rect = Rect::new(
                            Point::new(self.track_bounds.x(), top),
                            Size::new(self.track_bounds.size.width, h),
                        );
                        list.push_rect(fill_rect, fill_color, track_radius);
                    }
                } else {
                    let bottom = self.track_bounds.y() + self.track_bounds.size.height;
                    let h = bottom - thumb_pos;
                    if h > 0.0 {
                        let fill_rect = Rect::new(
                            Point::new(self.track_bounds.x(), thumb_pos),
                            Size::new(self.track_bounds.size.width, h),
                        );
                        list.push_rect(fill_rect, fill_color, track_radius);
                    }
                }
            } else if bipolar_ok {
                let zero_x = self.value_to_pos(0.0);
                let (left, right) = if thumb_pos <= zero_x {
                    (thumb_pos, zero_x)
                } else {
                    (zero_x, thumb_pos)
                };
                let w = right - left;
                if w > 0.0 {
                    let fill_rect = Rect::new(
                        Point::new(left, self.track_bounds.y()),
                        Size::new(w, self.track_bounds.size.height),
                    );
                    list.push_rect(fill_rect, fill_color, track_radius);
                }
            } else {
                let fill_width = thumb_pos - self.track_bounds.x();
                if fill_width > 0.0 {
                    let fill_rect = Rect::new(
                        self.track_bounds.origin,
                        Size::new(fill_width, self.track_bounds.size.height),
                    );
                    list.push_rect(fill_rect, fill_color, track_radius);
                }
            }
        }

        let thumb_color = if self.disabled {
            track_base.darken(0.2)
        } else {
            self.mss.accent_color.unwrap_or(Color::WHITE)
        };
        let thumb_border_color = self.mss.border_color.unwrap_or(fill_color);
        let thumb_border = if self.disabled {
            track_color
        } else if self.hover || self.dragging {
            thumb_border_color.darken(0.1)
        } else {
            thumb_border_color
        };
        let thumb_border_width = self.mss.border_width.unwrap_or(2.0);

        if self.vertical {
            let thumb_w = self.mss.max_width.map(|d| d.resolve(0.0)).unwrap_or(18.0);
            let thumb_h: f32 = 6.0;
            let thumb_rect = Rect::new(
                Point::new(
                    self.bounds.x() + (self.bounds.size.width - thumb_w) / 2.0,
                    thumb_pos - thumb_h / 2.0,
                ),
                Size::new(thumb_w, thumb_h),
            );
            let radii = [thumb_h / 2.0; 4];
            list.push_shadow(thumb_rect, Color::new(0.0, 0.0, 0.0, 0.15), 2.0, (0.0, 1.0), radii);
            list.push_rect_bordered(
                thumb_rect,
                thumb_color,
                radii,
                Border { width: thumb_border_width, color: thumb_border },
            );
        } else {
            let thumb_size = self.mss.max_height.map(|d| d.resolve(0.0)).unwrap_or(16.0);
            let thumb_rect = Rect::new(
                Point::new(
                    thumb_pos - thumb_size / 2.0,
                    self.bounds.y() + (self.bounds.size.height - thumb_size) / 2.0,
                ),
                Size::new(thumb_size, thumb_size),
            );
            let thumb_radius = thumb_size / 2.0;
            list.push_shadow(thumb_rect, Color::new(0.0, 0.0, 0.0, 0.15), 2.0, (0.0, 1.0), [thumb_radius; 4]);
            list.push_rect_bordered(
                thumb_rect,
                thumb_color,
                [thumb_radius; 4],
                Border { width: thumb_border_width, color: thumb_border },
            );
        }

        // Readout / инлайн-редактор значения (только horizontal + show_value).
        if self.show_value && !self.vertical {
            let vr = self.value_rect();
            let fs = self.value_font_size.or(self.mss.font_size).unwrap_or(11.0);
            let text_color = self.label_color.unwrap_or_else(|| Color::from_hex("#98A0AD"));

            if self.editing {
                let accent = self.mss.caret_color.unwrap_or(fill_base);
                list.push_rect_bordered(
                    vr,
                    Color::new(0.0, 0.0, 0.0, 0.35),
                    [4.0; 4],
                    Border { width: 1.0, color: accent },
                );
                let text_rect = Rect::new(
                    Point::new(vr.x() + 4.0, vr.y() + (vr.size.height - fs) / 2.0),
                    Size::new((vr.size.width - 8.0).max(0.0), fs + 2.0),
                );
                list.push_text(&self.edit_text, text_rect, text_color, fs);

                let blink_phase = (self.cursor_blink * CURSOR_BLINK_RATE * 2.0) % 2.0;
                if blink_phase < 1.0 {
                    let prefix_len = self.edit_text.chars().take(self.edit_cursor).count();
                    let char_w = fs * 0.6;
                    let cursor_x = text_rect.x() + prefix_len as f32 * char_w;
                    let cursor_rect = Rect::new(
                        Point::new(cursor_x, vr.y() + (vr.size.height - fs) / 2.0),
                        Size::new(1.5, fs),
                    );
                    list.push_rect(cursor_rect, accent, [0.0; 4]);
                }
            } else {
                list.push_text_centered(&self.formatted_value(), vr, text_color, fs);
            }
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled {
            return EventResult::Ignored;
        }

        match event {
            Event::MouseMove(pos) => {
                let was_hover = self.hover;
                self.hover = self.bounds.contains(*pos);
                if self.hover {
                    if self.value_zone_width() > 0.0 && self.value_rect().contains(*pos) {
                        ctx.set_cursor(CursorIcon::Text);
                    } else {
                        ctx.set_cursor(CursorIcon::Pointer);
                    }
                }

                if self.dragging {
                    ctx.set_cursor(CursorIcon::Grabbing);
                    let drag_axis = if self.vertical { pos.y } else { pos.x };
                    let new_value = self.pos_to_value(drag_axis);
                    if (new_value - self.value).abs() > 0.001 {
                        self.value = new_value.clamp(self.min, self.max);
                        self.trigger_change();
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.hover != was_hover {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hover { return EventResult::Handled; }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left {
                    if self.value_zone_width() > 0.0 && self.value_rect().contains(*position) {
                        if !self.editing {
                            self.start_editing();
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    if self.bounds.contains(*position) {
                        if self.editing {
                            self.commit_editing();
                        }
                        self.dragging = true;
                        let drag_axis = if self.vertical { position.y } else { position.x };
                        self.value = self.pos_to_value(drag_axis).clamp(self.min, self.max);
                        self.trigger_change();
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
                if *button == MouseButton::Left && self.dragging {
                    self.dragging = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            // Тачскрины: при движении пальца MouseMove не синтезируется (см.
            // app/event_handling.rs — только TouchMove), поэтому drag ведём по
            // Touch-событиям сами. Handled на TouchStart в границах слайдера
            // не даёт родительскому ScrollView начать прокрутку этим жестом;
            // значение и dragging выставит синтезированный MouseDown следом.
            Event::TouchStart { id, position } => {
                if self.bounds.contains(*position) {
                    if self.editing {
                        self.commit_editing();
                    }
                    self.touch_id = Some(*id);
                    self.dragging = true;
                    let drag_axis = if self.vertical { position.y } else { position.x };
                    self.value = self.pos_to_value(drag_axis).clamp(self.min, self.max);
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::TouchMove { id, position } => {
                if self.touch_id == Some(*id) && self.dragging {
                    let drag_axis = if self.vertical { position.y } else { position.x };
                    let new_value = self.pos_to_value(drag_axis);
                    if (new_value - self.value).abs() > 0.001 {
                        self.value = new_value.clamp(self.min, self.max);
                        self.trigger_change();
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::TouchEnd { id, .. } => {
                if self.touch_id.take() == Some(*id) {
                    self.dragging = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::CharInput(ch) if self.editing => {
                if ch.is_control() {
                    return EventResult::Ignored;
                }
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
            Event::KeyDown(Key::Down) | Event::KeyDown(Key::Left) => {
                if self.focused {
                    self.value = (self.value - self.step).max(self.min);
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::Up) | Event::KeyDown(Key::Right) => {
                if self.focused {
                    self.value = (self.value + self.step).min(self.max);
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::Home) => {
                if self.focused {
                    self.value = self.min;
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::End) => {
                if self.focused {
                    self.value = self.max;
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::FocusGained => {
                self.focused = true;
                ctx.request_paint();
                EventResult::Handled
            }
            Event::FocusLost => {
                self.focused = false;
                if self.editing {
                    self.commit_editing();
                }
                ctx.request_paint();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        if self.editing {
            self.cursor_blink += dt.as_secs_f32();
            self.mark_dirty(DirtyFlags::RENDER);
            return true;
        }
        false
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
        if self.vertical {
            let track_w = self.track_bounds.size.width;
            self.track_bounds.origin =
                Point::new(pos.x + (self.bounds.size.width - track_w) / 2.0, pos.y + 8.0);
        } else {
            self.track_bounds.origin = Point::new(pos.x + 8.0, pos.y + 10.0);
        }
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

    fn element_type_name(&self) -> &str { "Slider" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = self.mss.width { self.width = Some(w); }
        if let Some(c) = style.get("label-color").and_then(|v| v.as_color()) {
            self.label_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(v) = style.get("value-font-size").and_then(|v| v.as_px()) {
            self.value_font_size = Some(v);
        }
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
                focused: self.focused,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                value: Some(format!("{:.1}", self.value)),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for SliderElement {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn direct(s: &Slider) -> SliderElement {
        SliderElement {
            id: ElementId::new(),
            value: s.value,
            min: s.min,
            max: s.max,
            step: s.step,
            disabled: s.disabled,
            width: s.width,
            vertical: s.vertical,
            bipolar: s.bipolar,
            show_value: s.show_value,
            decimals: s.decimals,
            value_width: s.value_width,
            bounds: Rect::zero(),
            track_bounds: Rect::zero(),
            dragging: false,
            touch_id: None,
            hover: false,
            focused: false,
            editing: false,
            edit_text: String::new(),
            edit_cursor: 0,
            cursor_blink: 0.0,
            label_color: None,
            value_font_size: None,
            on_change: s.on_change.clone(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        }
    }

    #[test]
    fn vertical_slider_layout_uses_height() {
        let s = Slider::new().vertical().range(-1.0, 1.0).value(0.0);
        let mut elem = direct(&s);
        let constraints = Constraints::tight(Size::new(200.0, 100.0));
        let size = elem.layout(constraints);
        assert!(size.height > 50.0, "vertical должен занять height: got {size:?}");
        assert!(size.width <= 50.0, "vertical width — толщина карты: got {size:?}");
    }

    #[test]
    fn vertical_value_to_pos_inverted() {
        let s = Slider::new().vertical().range(0.0, 100.0).value(0.0);
        let mut elem = direct(&s);
        elem.layout(Constraints::tight(Size::new(24.0, 120.0)));
        let pos_min = elem.value_to_pos(0.0);
        let pos_max = elem.value_to_pos(100.0);
        assert!(pos_max < pos_min, "max value должен быть выше: max_y={pos_max} min_y={pos_min}");
    }

    #[test]
    fn horizontal_value_to_pos_normal() {
        let s = Slider::new().range(0.0, 100.0).value(50.0);
        let mut elem = direct(&s);
        elem.layout(Constraints::tight(Size::new(200.0, 24.0)));
        let pos_min = elem.value_to_pos(0.0);
        let pos_max = elem.value_to_pos(100.0);
        assert!(pos_min < pos_max, "min value слева: min_x={pos_min} max_x={pos_max}");
    }

    #[test]
    fn pos_to_value_roundtrip_vertical() {
        let s = Slider::new().vertical().range(-10.0, 10.0).value(0.0).step(0.0);
        let mut elem = direct(&s);
        elem.layout(Constraints::tight(Size::new(24.0, 120.0)));
        for v in [-10.0_f32, -5.0, 0.0, 5.0, 10.0] {
            let pos = elem.value_to_pos(v);
            let back = elem.pos_to_value(pos);
            assert!(
                (back - v).abs() < 0.5,
                "vertical roundtrip {v} → pos={pos} → {back}"
            );
        }
    }

    #[test]
    fn edit_commit_snaps_and_clamps() {
        let s = Slider::new().range(256.0, 1920.0).step(32.0).value(1344.0).show_value(0);
        let mut elem = direct(&s);
        elem.start_editing();
        assert_eq!(elem.edit_text, "1344");
        elem.edit_text = "1000".into();
        elem.commit_editing();
        assert_eq!(elem.value, 992.0, "1000 снапится к ближайшему кратному 32");

        elem.start_editing();
        elem.edit_text = "5000".into();
        elem.commit_editing();
        assert_eq!(elem.value, 1920.0, "ввод выше max клампится");

        elem.start_editing();
        elem.edit_text = "мусор".into();
        elem.commit_editing();
        assert_eq!(elem.value, 1920.0, "не-число — значение не меняется");
    }

    #[test]
    fn edit_cancel_keeps_value() {
        let s = Slider::new().range(0.0, 10.0).step(0.5).value(5.0).show_value(1);
        let mut elem = direct(&s);
        elem.start_editing();
        elem.edit_text = "9.5".into();
        elem.cancel_editing();
        assert_eq!(elem.value, 5.0);
        assert!(!elem.editing);
    }

    #[test]
    fn show_value_reserves_track_zone() {
        let plain = Slider::new().range(0.0, 1.0);
        let with_value = Slider::new().range(0.0, 1.0).show_value(0);
        let mut e1 = direct(&plain);
        let mut e2 = direct(&with_value);
        let c = Constraints::tight(Size::new(200.0, 24.0));
        e1.layout(c);
        e2.layout(c);
        assert!(
            e2.track_bounds.size.width < e1.track_bounds.size.width,
            "readout-зона должна укорачивать трек: {} vs {}",
            e2.track_bounds.size.width,
            e1.track_bounds.size.width
        );
        assert!(e2.value_rect().size.width > 0.0);
    }

    #[test]
    fn bipolar_requires_min_lt_zero_lt_max() {
        let s = Slider::new().range(-18.0, 18.0).value(6.0).bipolar();
        let elem = direct(&s);
        assert!(elem.bipolar);
        assert!(elem.min < 0.0 && elem.max > 0.0);
    }

    // Тачскрины: TouchStart в границах клеймит жест (Handled), движение пальца
    // ведёт drag по TouchMove (MouseMove на таче не синтезируется).
    #[test]
    fn touch_drag_moves_value() {
        let s = Slider::new().range(0.0, 1.0).step(0.01).value(1.0);
        let mut elem = direct(&s);
        elem.layout(Constraints::tight(Size::new(200.0, 24.0)));
        let mut ctx = crate::widget::context::EventContext::new(elem.id);

        let y = elem.bounds.y() + elem.bounds.size.height / 2.0;
        let start = Point::new(elem.track_bounds.x() + elem.track_bounds.size.width, y);

        // TouchStart клеймится (иначе родительский ScrollView начал бы скролл)
        // и сам начинает drag: MouseDown на таче синтезируется только для тапа.
        let r = elem.handle_event(&Event::TouchStart { id: 7, position: start }, &mut ctx);
        assert!(r.is_handled(), "TouchStart в границах должен клеймиться");
        assert!(elem.dragging, "drag начинается прямо с TouchStart");

        // Палец на середину трека → значение ~0.5.
        let mid = Point::new(elem.track_bounds.x() + elem.track_bounds.size.width / 2.0, y);
        let r = elem.handle_event(&Event::TouchMove { id: 7, position: mid }, &mut ctx);
        assert!(r.is_handled(), "TouchMove ведущего пальца должен двигать drag");
        assert!((elem.value - 0.5).abs() < 0.05, "value={}", elem.value);

        // Чужой палец drag не трогает.
        let r = elem.handle_event(&Event::TouchMove { id: 8, position: start }, &mut ctx);
        assert!(!r.is_handled());
        assert!((elem.value - 0.5).abs() < 0.05);

        // TouchEnd завершает drag сам (MouseUp при скролле не синтезируется).
        let r = elem.handle_event(&Event::TouchEnd { id: 7, position: mid }, &mut ctx);
        assert!(r.is_handled());
        assert!(!elem.dragging);
        assert!(elem.touch_id.is_none());
    }

    // Регресс step-снапа: диапазон 0..1 с дефолтным step=1.0 давал бы только
    // крайние значения — мелкий step обязан давать промежуточные.
    #[test]
    fn fine_step_yields_intermediate_values() {
        let s = Slider::new().range(0.0, 1.0).step(0.01).value(0.0);
        let mut elem = direct(&s);
        elem.layout(Constraints::tight(Size::new(200.0, 24.0)));
        let mid_x = elem.track_bounds.x() + elem.track_bounds.size.width / 2.0;
        let v = elem.pos_to_value(mid_x);
        assert!(v > 0.4 && v < 0.6, "середина трека ≈ 0.5, а не снап к 0/1: {v}");
    }
}
