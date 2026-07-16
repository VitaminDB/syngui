use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension, MssFields};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt, TextMeasure};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

type LabelFn = Arc<dyn Fn(f32) -> String + Send + Sync>;
type ChangeFn = Arc<Mutex<dyn FnMut(f32) + Send>>;

const THUMB_W: f32 = 20.0;
const THUMB_H: f32 = 6.0;
const TRACK_THICKNESS: f32 = 6.0;
const TICK_THICKNESS: f32 = 2.0;

fn estimate_text_width(text: &str, font_size: f32, tm: Option<&Arc<dyn TextMeasure>>) -> f32 {
    tm.map(|tm| tm.measure_text_width(text, font_size, text.chars().count()))
        .unwrap_or_else(|| text.chars().count() as f32 * font_size * 0.6)
}

pub struct TickSlider {
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    disabled: bool,
    vertical: bool,
    ticks: Vec<f32>,
    tick_count: usize,
    tick_labels: Option<LabelFn>,
    snap_to_ticks: bool,
    show_value_label: bool,
    value_formatter: Option<LabelFn>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    on_change: Option<ChangeFn>,
    classes: Vec<String>,
}

impl TickSlider {
    pub fn new() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            disabled: false,
            vertical: true,
            ticks: Vec::new(),
            tick_count: 0,
            tick_labels: None,
            snap_to_ticks: false,
            show_value_label: true,
            value_formatter: None,
            width: None,
            height: None,
            on_change: None,
            classes: Vec::new(),
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

    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    pub fn horizontal(mut self) -> Self {
        self.vertical = false;
        self
    }

    pub fn ticks(mut self, ticks: Vec<f32>) -> Self {
        self.ticks = ticks;
        self
    }

    pub fn tick_count(mut self, count: usize) -> Self {
        self.tick_count = count;
        self
    }

    pub fn tick_labels(mut self, f: impl Fn(f32) -> String + Send + Sync + 'static) -> Self {
        self.tick_labels = Some(Arc::new(f));
        self
    }

    pub fn snap_to_ticks(mut self, snap: bool) -> Self {
        self.snap_to_ticks = snap;
        self
    }

    pub fn show_value_label(mut self, show: bool) -> Self {
        self.show_value_label = show;
        self
    }

    pub fn value_formatter(mut self, f: impl Fn(f32) -> String + Send + Sync + 'static) -> Self {
        self.value_formatter = Some(Arc::new(f));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(Dimension::Px(h));
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(f32) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Default for TickSlider {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TickSlider {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(TickSliderElement {
            id: ElementId::new(),
            value: self.value,
            min: self.min,
            max: self.max,
            step: self.step,
            disabled: self.disabled,
            vertical: self.vertical,
            ticks: self.ticks.clone(),
            tick_count: self.tick_count,
            tick_labels: self.tick_labels.clone(),
            snap_to_ticks: self.snap_to_ticks,
            show_value_label: self.show_value_label,
            value_formatter: self.value_formatter.clone(),
            width: self.width,
            height: self.height,
            on_change: self.on_change.clone(),
            classes: self.classes.clone(),
            bounds: Rect::zero(),
            track_bounds: Rect::zero(),
            value_label_h: 0.0,
            tick_len: 8.0,
            tick_label_w: 0.0,
            value_font: 16.0,
            tick_font: 11.0,
            dragging: false,
            hover: false,
            focused: false,
            text_measure: None,
            mss: MssFields::new(),
            mss_tick_color: None,
            mss_tick_label_color: None,
            mss_value_color: None,
            mss_tick_length: None,
            mss_tick_font_size: None,
            mss_value_font_size: None,
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

pub struct TickSliderElement {
    id: ElementId,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    disabled: bool,
    vertical: bool,
    ticks: Vec<f32>,
    tick_count: usize,
    tick_labels: Option<LabelFn>,
    snap_to_ticks: bool,
    show_value_label: bool,
    value_formatter: Option<LabelFn>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    on_change: Option<ChangeFn>,
    classes: Vec<String>,
    bounds: Rect,
    track_bounds: Rect,
    value_label_h: f32,
    tick_len: f32,
    tick_label_w: f32,
    value_font: f32,
    tick_font: f32,
    dragging: bool,
    hover: bool,
    focused: bool,
    text_measure: Option<Arc<dyn TextMeasure>>,
    mss: MssFields,
    mss_tick_color: Option<Color>,
    mss_tick_label_color: Option<Color>,
    mss_value_color: Option<Color>,
    mss_tick_length: Option<f32>,
    mss_tick_font_size: Option<f32>,
    mss_value_font_size: Option<f32>,
    dirty_flags: DirtyFlags,
}

impl TickSliderElement {
    fn effective_ticks(&self) -> Vec<f32> {
        if !self.ticks.is_empty() {
            return self.ticks.clone();
        }
        if self.tick_count >= 2 {
            let range = self.max - self.min;
            (0..self.tick_count)
                .map(|i| self.min + range * i as f32 / (self.tick_count - 1) as f32)
                .collect()
        } else if self.tick_count == 1 {
            vec![self.min]
        } else {
            Vec::new()
        }
    }

    fn snap(&self, raw: f32) -> f32 {
        if self.snap_to_ticks {
            let ticks = self.effective_ticks();
            if !ticks.is_empty() {
                let mut best = ticks[0];
                let mut best_d = (raw - best).abs();
                for &t in &ticks[1..] {
                    let d = (raw - t).abs();
                    if d < best_d {
                        best_d = d;
                        best = t;
                    }
                }
                return best;
            }
        }
        if self.step > 0.0 {
            (raw / self.step).round() * self.step
        } else {
            raw
        }
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
        let raw = if self.vertical {
            if range <= 0.0 || self.track_bounds.size.height <= 0.0 {
                return self.min;
            }
            let percent = ((self.track_bounds.y() + self.track_bounds.size.height - pos)
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
        self.snap(raw)
    }

    fn trigger_change(&mut self) {
        if let Some(ref callback) = self.on_change {
            if let Ok(mut cb) = callback.lock() {
                cb(self.value);
            }
        }
    }
}

impl Element for TickSliderElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<TickSlider>() {
            self.value = w.value;
            self.min = w.min;
            self.max = w.max;
            self.step = w.step;
            self.disabled = w.disabled;
            self.vertical = w.vertical;
            self.ticks = w.ticks.clone();
            self.tick_count = w.tick_count;
            self.tick_labels = w.tick_labels.clone();
            self.snap_to_ticks = w.snap_to_ticks;
            self.show_value_label = w.show_value_label;
            self.value_formatter = w.value_formatter.clone();
            self.width = w.width;
            self.height = w.height;
            self.on_change = w.on_change.clone();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        self.value_font = self.mss_value_font_size.or(self.mss.font_size).unwrap_or(16.0);
        self.tick_font = self.mss_tick_font_size.unwrap_or(11.0);
        self.tick_len = self.mss_tick_length.unwrap_or(8.0);

        let has_value = self.show_value_label && self.value_formatter.is_some();
        self.value_label_h = if has_value { self.value_font * 1.4 + 8.0 } else { 0.0 };

        let ticks = self.effective_ticks();
        self.tick_label_w = if let Some(ref f) = self.tick_labels {
            ticks
                .iter()
                .map(|&tv| estimate_text_width(&f(tv), self.tick_font, self.text_measure.as_ref()))
                .fold(0.0_f32, f32::max)
                + 6.0
        } else {
            0.0
        };

        let track_w = self
            .mss
            .min_width
            .map(|d| d.resolve(THUMB_W))
            .unwrap_or(TRACK_THICKNESS);

        if self.vertical {
            let column_w = THUMB_W + self.tick_len + 2.0 + self.tick_label_w;
            let width = self
                .width
                .or(self.mss.width)
                .map(|d| d.resolve(constraints.max_width))
                .unwrap_or_else(|| column_w.max(48.0))
                .min(constraints.max_width);
            let height = self
                .height
                .or(self.mss.height)
                .map(|d| d.resolve(constraints.max_height))
                .unwrap_or(280.0)
                .min(constraints.max_height);

            self.bounds = Rect::new(Point::zero(), Size::new(width, height));

            let col_x = ((width - column_w) * 0.5).max(0.0);
            let track_center_x = col_x + THUMB_W * 0.5;
            let track_top = self.value_label_h + 6.0;
            let track_h = (height - track_top - 8.0).max(0.0);
            self.track_bounds = Rect::new(
                Point::new(track_center_x - track_w * 0.5, track_top),
                Size::new(track_w, track_h),
            );

            Size::new(width, height)
        } else {
            let height = self
                .height
                .or(self.mss.height)
                .map(|d| d.resolve(constraints.max_height))
                .unwrap_or(self.value_label_h + 28.0)
                .min(constraints.max_height);
            let width = self
                .width
                .or(self.mss.width)
                .map(|d| d.resolve(constraints.max_width))
                .unwrap_or(constraints.max_width)
                .min(constraints.max_width);

            self.bounds = Rect::new(Point::zero(), Size::new(width, height));

            let track_top = self.value_label_h + (height - self.value_label_h - track_w) * 0.5;
            self.track_bounds = Rect::new(
                Point::new(8.0, track_top),
                Size::new(width - 16.0, track_w),
            );

            Size::new(width, height)
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let track_base = self.mss.background_color.unwrap_or(Color::from_hex("#3F4147"));
        let fill_base = self.mss.color.unwrap_or(Color::from_hex("#00B4D8"));
        let track_color = if self.disabled { track_base.darken(0.1) } else { track_base };
        let fill_color = if self.disabled { track_base.darken(0.2) } else { fill_base };
        let tick_color = self.mss_tick_color.unwrap_or(Color::from_hex("#6B7280"));
        let tick_label_color = self
            .mss_tick_label_color
            .unwrap_or_else(|| self.mss.color.unwrap_or(Color::from_hex("#B5BAC1")));
        let value_color = self
            .mss_value_color
            .unwrap_or_else(|| self.mss.color.unwrap_or(Color::from_hex("#F2F3F5")));

        let radius_basis = if self.vertical {
            self.track_bounds.size.width
        } else {
            self.track_bounds.size.height
        };
        let track_radius = [radius_basis * 0.5; 4];

        list.push_rect(self.track_bounds, track_color, track_radius);

        let thumb_pos = self.value_to_pos(self.value);

        if self.vertical {
            let bottom = self.track_bounds.y() + self.track_bounds.size.height;
            let h = bottom - thumb_pos;
            if h > 0.0 {
                let fill_rect = Rect::new(
                    Point::new(self.track_bounds.x(), thumb_pos),
                    Size::new(self.track_bounds.size.width, h),
                );
                list.push_rect(fill_rect, fill_color, track_radius);
            }
        } else {
            let fill_w = thumb_pos - self.track_bounds.x();
            if fill_w > 0.0 {
                let fill_rect = Rect::new(
                    self.track_bounds.origin,
                    Size::new(fill_w, self.track_bounds.size.height),
                );
                list.push_rect(fill_rect, fill_color, track_radius);
            }
        }

        let ticks = self.effective_ticks();
        if self.vertical {
            let tick_x0 = self.track_bounds.x() + self.track_bounds.size.width + 2.0;
            for tv in &ticks {
                let y = self.value_to_pos(*tv);
                let tick_rect = Rect::new(
                    Point::new(tick_x0, y - TICK_THICKNESS * 0.5),
                    Size::new(self.tick_len, TICK_THICKNESS),
                );
                list.push_rect(tick_rect, tick_color, [TICK_THICKNESS * 0.5; 4]);

                if let Some(ref f) = self.tick_labels {
                    let label = f(*tv);
                    let lx = tick_x0 + self.tick_len + 4.0;
                    let lr = Rect::new(
                        Point::new(lx, y - self.tick_font * 0.7),
                        Size::new(self.tick_label_w, self.tick_font + 2.0),
                    );
                    list.push_text(&label, lr, tick_label_color, self.tick_font);
                }
            }
        } else {
            let tick_y0 = self.track_bounds.y() + self.track_bounds.size.height + 2.0;
            for tv in &ticks {
                let x = self.value_to_pos(*tv);
                let tick_rect = Rect::new(
                    Point::new(x - TICK_THICKNESS * 0.5, tick_y0),
                    Size::new(TICK_THICKNESS, self.tick_len),
                );
                list.push_rect(tick_rect, tick_color, [TICK_THICKNESS * 0.5; 4]);
            }
        }

        let thumb_border_color = self.mss.border_color.unwrap_or(fill_color);
        let thumb_border = if self.disabled {
            track_color
        } else if self.hover || self.dragging {
            thumb_border_color.darken(0.1)
        } else {
            thumb_border_color
        };
        let thumb_color = if self.disabled {
            track_base.darken(0.2)
        } else {
            self.mss.accent_color.unwrap_or(Color::WHITE)
        };
        let thumb_border_width = self.mss.border_width.unwrap_or(2.0);

        if self.vertical {
            let track_center_x = self.track_bounds.x() + self.track_bounds.size.width * 0.5;
            let thumb_rect = Rect::new(
                Point::new(track_center_x - THUMB_W * 0.5, thumb_pos - THUMB_H * 0.5),
                Size::new(THUMB_W, THUMB_H),
            );
            let radii = [THUMB_H * 0.5; 4];
            list.push_shadow(thumb_rect, Color::new(0.0, 0.0, 0.0, 0.18), 3.0, (0.0, 1.0), radii);
            list.push_rect_bordered(
                thumb_rect,
                thumb_color,
                radii,
                Border { width: thumb_border_width, color: thumb_border },
            );
        } else {
            let track_center_y = self.track_bounds.y() + self.track_bounds.size.height * 0.5;
            let thumb_size = 16.0;
            let thumb_rect = Rect::new(
                Point::new(thumb_pos - thumb_size * 0.5, track_center_y - thumb_size * 0.5),
                Size::new(thumb_size, thumb_size),
            );
            let radii = [thumb_size * 0.5; 4];
            list.push_shadow(thumb_rect, Color::new(0.0, 0.0, 0.0, 0.18), 3.0, (0.0, 1.0), radii);
            list.push_rect_bordered(
                thumb_rect,
                thumb_color,
                radii,
                Border { width: thumb_border_width, color: thumb_border },
            );
        }

        if self.show_value_label {
            if let Some(ref f) = self.value_formatter {
                let label = f(self.value);
                let vr = Rect::new(
                    Point::new(self.bounds.x(), self.bounds.y() + 2.0),
                    Size::new(self.bounds.size.width, self.value_label_h),
                );
                list.push_text_centered(&label, vr, value_color, self.value_font);
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
                    ctx.set_cursor(CursorIcon::Pointer);
                }

                if self.dragging {
                    ctx.set_cursor(CursorIcon::Grabbing);
                    let axis = if self.vertical { pos.y } else { pos.x };
                    let new_value = self.pos_to_value(axis).clamp(self.min, self.max);
                    if (new_value - self.value).abs() > f32::EPSILON {
                        self.value = new_value;
                        self.trigger_change();
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.hover != was_hover {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hover {
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    self.dragging = true;
                    let axis = if self.vertical { position.y } else { position.x };
                    self.value = self.pos_to_value(axis).clamp(self.min, self.max);
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
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
            Event::KeyDown(Key::Down) | Event::KeyDown(Key::Left) => {
                if self.focused {
                    self.value = self.step_value(-1);
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::Up) | Event::KeyDown(Key::Right) => {
                if self.focused {
                    self.value = self.step_value(1);
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
                ctx.request_paint();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        let delta = Point::new(pos.x - self.bounds.origin.x, pos.y - self.bounds.origin.y);
        self.bounds.origin = pos;
        self.track_bounds.origin =
            Point::new(self.track_bounds.origin.x + delta.x, self.track_bounds.origin.y + delta.y);
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

    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

    fn element_type_name(&self) -> &str {
        "TickSlider"
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = self.mss.width {
            self.width = Some(w);
        }
        if let Some(h) = self.mss.height {
            self.height = Some(h);
        }
        if let Some(c) = style.get("tick-color").and_then(|v| v.as_color()) {
            self.mss_tick_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(c) = style.get("tick-label-color").and_then(|v| v.as_color()) {
            self.mss_tick_label_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(c) = style.get("value-color").and_then(|v| v.as_color()) {
            self.mss_value_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(v) = style.get("tick-length").and_then(|v| v.as_px()) {
            self.mss_tick_length = Some(v);
        }
        if let Some(v) = style.get("tick-font-size").and_then(|v| v.as_px()) {
            self.mss_tick_font_size = Some(v);
        }
        if let Some(v) = style.get("value-font-size").and_then(|v| v.as_px()) {
            self.mss_value_font_size = Some(v);
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

impl TickSliderElement {
    fn step_value(&self, dir: i32) -> f32 {
        if self.snap_to_ticks {
            let ticks = self.effective_ticks();
            if !ticks.is_empty() {
                let cur = ticks
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        (**a - self.value)
                            .abs()
                            .partial_cmp(&(**b - self.value).abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i as i32)
                    .unwrap_or(0);
                let next = (cur + dir).clamp(0, ticks.len() as i32 - 1) as usize;
                return ticks[next];
            }
        }
        (self.value + dir as f32 * self.step).clamp(self.min, self.max)
    }
}

impl StyledElement for TickSliderElement {
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

    fn direct(s: &TickSlider) -> TickSliderElement {
        TickSliderElement {
            id: ElementId::new(),
            value: s.value,
            min: s.min,
            max: s.max,
            step: s.step,
            disabled: s.disabled,
            vertical: s.vertical,
            ticks: s.ticks.clone(),
            tick_count: s.tick_count,
            tick_labels: s.tick_labels.clone(),
            snap_to_ticks: s.snap_to_ticks,
            show_value_label: s.show_value_label,
            value_formatter: s.value_formatter.clone(),
            width: s.width,
            height: s.height,
            on_change: s.on_change.clone(),
            classes: s.classes.clone(),
            bounds: Rect::zero(),
            track_bounds: Rect::zero(),
            value_label_h: 0.0,
            tick_len: 8.0,
            tick_label_w: 0.0,
            value_font: 16.0,
            tick_font: 11.0,
            dragging: false,
            hover: false,
            focused: false,
            text_measure: None,
            mss: MssFields::new(),
            mss_tick_color: None,
            mss_tick_label_color: None,
            mss_value_color: None,
            mss_tick_length: None,
            mss_tick_font_size: None,
            mss_value_font_size: None,
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
        }
    }

    #[test]
    fn vertical_layout_reserves_value_label_space() {
        let s = TickSlider::new()
            .vertical()
            .range(0.0, 10.0)
            .tick_count(5)
            .value_formatter(|v| format!("{v:.0}"));
        let mut e = direct(&s);
        let size = e.layout(Constraints::tight(Size::new(150.0, 300.0)));
        assert!(size.height > 100.0, "vertical занимает высоту: {size:?}");
        assert!(e.value_label_h > 0.0, "зона значения зарезервирована");
        assert!(
            e.track_bounds.y() >= e.value_label_h,
            "трек ниже зоны значения"
        );
    }

    #[test]
    fn vertical_value_to_pos_inverted() {
        let s = TickSlider::new().vertical().range(0.0, 100.0);
        let mut e = direct(&s);
        e.layout(Constraints::tight(Size::new(120.0, 280.0)));
        let pos_min = e.value_to_pos(0.0);
        let pos_max = e.value_to_pos(100.0);
        assert!(pos_max < pos_min, "max выше min: max={pos_max} min={pos_min}");
    }

    #[test]
    fn effective_ticks_evenly_spaced() {
        let s = TickSlider::new().range(0.0, 8.0).tick_count(5);
        let e = direct(&s);
        let ticks = e.effective_ticks();
        assert_eq!(ticks.len(), 5);
        assert!((ticks[0] - 0.0).abs() < 1e-4);
        assert!((ticks[4] - 8.0).abs() < 1e-4);
        assert!((ticks[2] - 4.0).abs() < 1e-4);
    }

    #[test]
    fn snap_picks_nearest_tick() {
        let s = TickSlider::new()
            .range(0.0, 10.0)
            .ticks(vec![0.0, 2.5, 5.0, 7.5, 10.0])
            .snap_to_ticks(true);
        let e = direct(&s);
        assert!((e.snap(2.4) - 2.5).abs() < 1e-4);
        assert!((e.snap(6.0) - 5.0).abs() < 1e-4);
        assert!((e.snap(9.9) - 10.0).abs() < 1e-4);
    }
}
