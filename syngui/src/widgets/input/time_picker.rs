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
use std::time::Duration;
use crate::core::sync::Mutex;

const REPEAT_INITIAL_DELAY: f32 = 0.4;
const REPEAT_INTERVAL: f32 = 0.08;
const CURSOR_BLINK_RATE: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Time {
    pub hour: u32,
    pub minute: u32,
}

impl Time {
    pub fn new(hour: u32, minute: u32) -> Self {
        Self {
            hour: hour.min(23),
            minute: minute.min(59),
        }
    }

    pub fn format(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

pub struct TimePicker {
    selected: Option<Time>,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(Option<Time>) + Send>>>,
    width: Option<Dimension>,
    use_24h: bool,
}

impl TimePicker {
    pub fn new() -> Self {
        Self {
            selected: None,
            placeholder: "Select time...".to_string(),
            on_change: None,
            width: None,
            use_24h: true,
        }
    }

    pub fn selected(mut self, time: Time) -> Self {
        self.selected = Some(time);
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn on_change(mut self, f: impl FnMut(Option<Time>) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }

    pub fn use_24h(mut self, v: bool) -> Self {
        self.use_24h = v;
        self
    }
}

impl Default for TimePicker {
    fn default() -> Self { Self::new() }
}

impl Widget for TimePicker {
    fn create_element(&self) -> Box<dyn Element> {
        let (h, m) = self.selected.map(|t| (t.hour, t.minute)).unwrap_or((12, 0));
        Box::new(TimePickerElement {
            id: ElementId::new(),
            selected: self.selected,
            placeholder: self.placeholder.clone(),
            on_change: self.on_change.clone(),
            width: self.width,
            use_24h: self.use_24h,
            is_open: false,
            view_hour: h,
            view_minute: m,
            hover_zone: HoverZone::None,
            opens_upward: false,
            bounds: Rect::zero(),
            repeat_zone: None,
            repeat_elapsed: 0.0,
            repeat_initial_done: false,
            editing_field: None,
            edit_text: String::new(),
            cursor_blink: 0.0,
            focus_requested: false,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

const INPUT_HEIGHT: f32 = 40.0;
const POPUP_WIDTH: f32 = 200.0;
const POPUP_HEIGHT: f32 = 180.0;
const SPINNER_WIDTH: f32 = 72.0;
const BTN_HEIGHT: f32 = 32.0;
const VALUE_HEIGHT: f32 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum HoverZone {
    None,
    HourUp,
    HourDown,
    MinuteUp,
    MinuteDown,
    Confirm,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EditField {
    Hour,
    Minute,
}

pub struct TimePickerElement {
    id: ElementId,
    selected: Option<Time>,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(Option<Time>) + Send>>>,
    width: Option<Dimension>,
    use_24h: bool,
    is_open: bool,
    view_hour: u32,
    view_minute: u32,
    hover_zone: HoverZone,
    opens_upward: bool,
    bounds: Rect,
    repeat_zone: Option<HoverZone>,
    repeat_elapsed: f32,
    repeat_initial_done: bool,
    editing_field: Option<EditField>,
    edit_text: String,
    cursor_blink: f32,
    focus_requested: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl TimePickerElement {
    fn popup_rect(&self) -> Rect {
        let y = if self.opens_upward {
            self.bounds.y() - POPUP_HEIGHT - 4.0
        } else {
            self.bounds.y() + INPUT_HEIGHT + 4.0
        };
        Rect::new(
            Point::new(self.bounds.x(), y),
            Size::new(POPUP_WIDTH, POPUP_HEIGHT),
        )
    }

    fn fire_change(&self) {
        if let Some(ref cb) = self.on_change {
            if let Ok(mut f) = cb.lock() { f(self.selected); }
        }
    }

    fn format_hour(&self, h: u32) -> String {
        if self.use_24h {
            format!("{:02}", h)
        } else {
            let h12 = if h == 0 { 12 } else if h > 12 { h - 12 } else { h };
            format!("{}", h12)
        }
    }

    fn format_display(&self, time: &Time) -> String {
        if self.use_24h {
            time.format()
        } else {
            let period = if time.hour < 12 { "AM" } else { "PM" };
            let h12 = if time.hour == 0 { 12 } else if time.hour > 12 { time.hour - 12 } else { time.hour };
            format!("{}:{:02} {}", h12, time.minute, period)
        }
    }

    fn spinner_rects(&self, popup: Rect) -> SpinnerRects {
        let padding = 16.0;
        let colon_width = 20.0;
        let hour_x = popup.x() + padding;
        let minute_x = hour_x + SPINNER_WIDTH + colon_width;
        let top_y = popup.y() + 12.0;

        SpinnerRects {
            hour_up: Rect::new(Point::new(hour_x, top_y), Size::new(SPINNER_WIDTH, BTN_HEIGHT)),
            hour_value: Rect::new(Point::new(hour_x, top_y + BTN_HEIGHT), Size::new(SPINNER_WIDTH, VALUE_HEIGHT)),
            hour_down: Rect::new(Point::new(hour_x, top_y + BTN_HEIGHT + VALUE_HEIGHT), Size::new(SPINNER_WIDTH, BTN_HEIGHT)),
            minute_up: Rect::new(Point::new(minute_x, top_y), Size::new(SPINNER_WIDTH, BTN_HEIGHT)),
            minute_value: Rect::new(Point::new(minute_x, top_y + BTN_HEIGHT), Size::new(SPINNER_WIDTH, VALUE_HEIGHT)),
            minute_down: Rect::new(Point::new(minute_x, top_y + BTN_HEIGHT + VALUE_HEIGHT), Size::new(SPINNER_WIDTH, BTN_HEIGHT)),
            confirm: Rect::new(
                Point::new(popup.x() + padding, top_y + BTN_HEIGHT * 2.0 + VALUE_HEIGHT + 16.0),
                Size::new(POPUP_WIDTH - padding * 2.0, 32.0),
            ),
        }
    }

    fn apply_spinner_step(&mut self, zone: HoverZone) {
        match zone {
            HoverZone::HourUp => {
                self.view_hour = if self.view_hour >= 23 { 0 } else { self.view_hour + 1 };
            }
            HoverZone::HourDown => {
                self.view_hour = if self.view_hour == 0 { 23 } else { self.view_hour - 1 };
            }
            HoverZone::MinuteUp => {
                self.view_minute = if self.view_minute >= 59 { 0 } else { self.view_minute + 1 };
            }
            HoverZone::MinuteDown => {
                self.view_minute = if self.view_minute == 0 { 59 } else { self.view_minute - 1 };
            }
            _ => {}
        }
    }

    fn commit_edit(&mut self) {
        if let Some(field) = self.editing_field.take() {
            if let Ok(v) = self.edit_text.parse::<u32>() {
                match field {
                    EditField::Hour => self.view_hour = v.min(23),
                    EditField::Minute => self.view_minute = v.min(59),
                }
            }
        }
    }

    fn start_edit(&mut self, field: EditField) {
        self.commit_edit();
        self.editing_field = Some(field);
        self.focus_requested = true;
        self.edit_text = match field {
            EditField::Hour => format!("{:02}", self.view_hour),
            EditField::Minute => format!("{:02}", self.view_minute),
        };
        self.cursor_blink = 0.0;
    }
}

struct SpinnerRects {
    hour_up: Rect,
    hour_value: Rect,
    hour_down: Rect,
    minute_up: Rect,
    minute_value: Rect,
    minute_down: Rect,
    confirm: Rect,
}

impl Element for TimePickerElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(tp) = widget.as_any().downcast_ref::<TimePicker>() {
            self.selected = tp.selected;
            self.placeholder = tp.placeholder.clone();
            self.on_change = tp.on_change.clone();
            self.width = tp.width;
            self.use_24h = tp.use_24h;
            if let Some(t) = tp.selected {
                self.view_hour = t.hour;
                self.view_minute = t.minute;
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let default_w = if constraints.max_width.is_finite() { constraints.max_width } else { POPUP_WIDTH };
        let w = self.width.map(|d| d.resolve(constraints.max_width)).unwrap_or(default_w).min(constraints.max_width);
        self.bounds = Rect::new(Point::zero(), Size::new(w, INPUT_HEIGHT));
        Size::new(w, INPUT_HEIGHT)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
        let border_base = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let accent = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let border_color = if self.is_open { accent } else { border_base };

        list.push_rect_bordered(
            self.bounds, bg, [8.0; 4],
            Border::new(if self.is_open { 2.0 } else { 1.0 }, border_color),
        );

        let text_rect = Rect::new(
            Point::new(self.bounds.x() + 12.0, self.bounds.y() + (INPUT_HEIGHT - 14.0) / 2.0),
            Size::new(self.bounds.size.width - 40.0, 16.0),
        );
        if let Some(t) = self.selected {
            list.push_text(&self.format_display(&t), text_rect, fg, 14.0);
        } else {
            let placeholder_color = self.mss.color.map(|c| c.with_alpha(0.5)).unwrap_or(Color::from_hex("#9CA3AF"));
            list.push_text(&self.placeholder, text_rect, placeholder_color, 14.0);
        }

        let icon_rect = Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - 28.0, self.bounds.y() + (INPUT_HEIGHT - 14.0) / 2.0),
            Size::new(16.0, 14.0),
        );
        let icon_color = self.mss.color.map(|c| c.with_alpha(0.6)).unwrap_or(Color::from_hex("#6B7280"));
        list.push_text("\u{E8B5}", icon_rect, icon_color, 14.0);

        if !self.is_open { return; }

        let popup = self.popup_rect();
        let rects = self.spinner_rects(popup);
        let popup_bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let popup_fg = self.mss.color.unwrap_or(Color::from_hex("#111827"));
        let popup_fg_secondary = self.mss.color.map(|c| c.with_alpha(0.7)).unwrap_or(Color::from_hex("#374151"));
        let hover_bg = self.mss.background_color.map(|c| c.darken(0.05)).unwrap_or(Color::from_hex("#F3F4F6"));
        let value_bg = self.mss.background_color.map(|c| c.darken(0.02)).unwrap_or(Color::from_hex("#F8FAFC"));
        let popup_border = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));

        list.begin_overlay();

        list.push_shadow(popup, Color::BLACK.with_alpha(0.15), 16.0, (0.0, 4.0), [12.0; 4]);
        list.push_rect_bordered(popup, popup_bg, [12.0; 4], Border::new(1.0, popup_border));

        let hour_up_bg = if self.hover_zone == HoverZone::HourUp { hover_bg } else { Color::TRANSPARENT };
        list.push_rect(rects.hour_up, hour_up_bg, [6.0; 4]);
        list.push_text_centered("\u{E316}", rects.hour_up, popup_fg_secondary, 14.0);

        let editing_hour = self.editing_field == Some(EditField::Hour);
        let hour_value_bg = if editing_hour { popup_bg } else { value_bg };
        let hour_border = if editing_hour { accent } else { Color::TRANSPARENT };
        list.push_rect_bordered(rects.hour_value, hour_value_bg, [8.0; 4], Border::new(if editing_hour { 2.0 } else { 0.0 }, hour_border));
        if editing_hour {
            list.push_text_centered(&self.edit_text, rects.hour_value, popup_fg, 28.0);
            let blink_phase = (self.cursor_blink * CURSOR_BLINK_RATE * 2.0) % 2.0;
            if blink_phase < 1.0 {
                let cursor_x = rects.hour_value.x() + rects.hour_value.size.width / 2.0 + self.edit_text.len() as f32 * 8.0;
                let cursor_rect = Rect::new(
                    Point::new(cursor_x, rects.hour_value.y() + (VALUE_HEIGHT - 28.0) / 2.0),
                    Size::new(1.5, 28.0),
                );
                list.push_rect(cursor_rect, accent, [0.0; 4]);
            }
        } else {
            list.push_text_centered(&self.format_hour(self.view_hour), rects.hour_value, popup_fg, 28.0);
        }

        let hour_down_bg = if self.hover_zone == HoverZone::HourDown { hover_bg } else { Color::TRANSPARENT };
        list.push_rect(rects.hour_down, hour_down_bg, [6.0; 4]);
        list.push_text_centered("\u{E313}", rects.hour_down, popup_fg_secondary, 14.0);

        let colon_rect = Rect::new(
            Point::new(rects.hour_value.x() + SPINNER_WIDTH, rects.hour_value.y()),
            Size::new(20.0, VALUE_HEIGHT),
        );
        list.push_text_centered(":", colon_rect, popup_fg, 28.0);

        let minute_up_bg = if self.hover_zone == HoverZone::MinuteUp { hover_bg } else { Color::TRANSPARENT };
        list.push_rect(rects.minute_up, minute_up_bg, [6.0; 4]);
        list.push_text_centered("\u{E316}", rects.minute_up, popup_fg_secondary, 14.0);

        let editing_minute = self.editing_field == Some(EditField::Minute);
        let minute_value_bg = if editing_minute { popup_bg } else { value_bg };
        let minute_border = if editing_minute { accent } else { Color::TRANSPARENT };
        list.push_rect_bordered(rects.minute_value, minute_value_bg, [8.0; 4], Border::new(if editing_minute { 2.0 } else { 0.0 }, minute_border));
        if editing_minute {
            list.push_text_centered(&self.edit_text, rects.minute_value, popup_fg, 28.0);
            let blink_phase = (self.cursor_blink * CURSOR_BLINK_RATE * 2.0) % 2.0;
            if blink_phase < 1.0 {
                let cursor_x = rects.minute_value.x() + rects.minute_value.size.width / 2.0 + self.edit_text.len() as f32 * 8.0;
                let cursor_rect = Rect::new(
                    Point::new(cursor_x, rects.minute_value.y() + (VALUE_HEIGHT - 28.0) / 2.0),
                    Size::new(1.5, 28.0),
                );
                list.push_rect(cursor_rect, accent, [0.0; 4]);
            }
        } else {
            list.push_text_centered(&format!("{:02}", self.view_minute), rects.minute_value, popup_fg, 28.0);
        }

        let minute_down_bg = if self.hover_zone == HoverZone::MinuteDown { hover_bg } else { Color::TRANSPARENT };
        list.push_rect(rects.minute_down, minute_down_bg, [6.0; 4]);
        list.push_text_centered("\u{E313}", rects.minute_down, popup_fg_secondary, 14.0);

        if !self.use_24h {
            let period = if self.view_hour < 12 { "AM" } else { "PM" };
            let period_rect = Rect::new(
                Point::new(rects.minute_value.x() + SPINNER_WIDTH + 4.0, rects.minute_value.y()),
                Size::new(32.0, VALUE_HEIGHT),
            );
            list.push_text_centered(period, period_rect, popup_fg_secondary, 14.0);
        }

        let confirm_bg = if self.hover_zone == HoverZone::Confirm { accent.darken(0.1) } else { accent };
        list.push_rect(rects.confirm, confirm_bg, [6.0; 4]);
        list.push_text_centered("OK", rects.confirm, Color::WHITE, 14.0);

        list.end_overlay();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Pointer);
                    return EventResult::Handled;
                }

                if self.is_open {
                    let popup = self.popup_rect();
                    if popup.contains(*pos) {
                        let rects = self.spinner_rects(popup);
                        let new_zone = if rects.hour_up.contains(*pos) {
                            HoverZone::HourUp
                        } else if rects.hour_down.contains(*pos) {
                            HoverZone::HourDown
                        } else if rects.minute_up.contains(*pos) {
                            HoverZone::MinuteUp
                        } else if rects.minute_down.contains(*pos) {
                            HoverZone::MinuteDown
                        } else if rects.confirm.contains(*pos) {
                            HoverZone::Confirm
                        } else {
                            HoverZone::None
                        };
                        if new_zone != self.hover_zone {
                            self.hover_zone = new_zone;
                            ctx.request_paint();
                        }
                        if rects.hour_value.contains(*pos) || rects.minute_value.contains(*pos) {
                            ctx.set_cursor(CursorIcon::Text);
                        } else {
                            ctx.set_cursor(CursorIcon::Pointer);
                        }
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.bounds.contains(*position) {
                    self.commit_edit();
                    self.is_open = !self.is_open;
                    if self.is_open {
                        self.opens_upward = self.bounds.y() + INPUT_HEIGHT + 4.0 + POPUP_HEIGHT > ctx.viewport_size().height
                            && self.bounds.y() >= POPUP_HEIGHT + 4.0;
                        let popup = self.popup_rect();
                        let overlay_bounds = if self.opens_upward {
                            Rect::new(
                                Point::new(self.bounds.x(), popup.y()),
                                Size::new(POPUP_WIDTH, popup.size.height + 4.0 + INPUT_HEIGHT),
                            )
                        } else {
                            Rect::new(
                                self.bounds.origin,
                                Size::new(POPUP_WIDTH, INPUT_HEIGHT + 4.0 + popup.size.height),
                            )
                        };
                        ctx.register_overlay(overlay_bounds, false);
                    } else {
                        ctx.unregister_overlay();
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.is_open {
                    let popup = self.popup_rect();
                    if popup.contains(*position) {
                        let rects = self.spinner_rects(popup);

                        if rects.hour_value.contains(*position) {
                            self.start_edit(EditField::Hour);
                            ctx.set_virtual_keyboard_visible(true);
                            ctx.set_numeric_keyboard(true);
                            ctx.set_focused_text(self.edit_text.clone());
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        if rects.minute_value.contains(*position) {
                            self.start_edit(EditField::Minute);
                            ctx.set_virtual_keyboard_visible(true);
                            ctx.set_numeric_keyboard(true);
                            ctx.set_focused_text(self.edit_text.clone());
                            ctx.request_paint();
                            return EventResult::Handled;
                        }

                        self.commit_edit();

                        let zone = if rects.hour_up.contains(*position) {
                            Some(HoverZone::HourUp)
                        } else if rects.hour_down.contains(*position) {
                            Some(HoverZone::HourDown)
                        } else if rects.minute_up.contains(*position) {
                            Some(HoverZone::MinuteUp)
                        } else if rects.minute_down.contains(*position) {
                            Some(HoverZone::MinuteDown)
                        } else {
                            None
                        };

                        if let Some(z) = zone {
                            self.apply_spinner_step(z);
                            self.repeat_zone = Some(z);
                            self.repeat_elapsed = 0.0;
                            self.repeat_initial_done = false;
                            ctx.request_paint();
                            return EventResult::Handled;
                        }

                        if rects.confirm.contains(*position) {
                            self.selected = Some(Time::new(self.view_hour, self.view_minute));
                            self.is_open = false;
                            ctx.unregister_overlay();
                            ctx.set_virtual_keyboard_visible(false);
                            self.fire_change();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        return EventResult::Handled;
                    }

                    self.commit_edit();
                    self.is_open = false;
                    ctx.unregister_overlay();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } if *button == MouseButton::Left => {
                if self.repeat_zone.is_some() {
                    self.repeat_zone = None;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::CharInput(ch) if self.editing_field.is_some() => {
                if ch.is_ascii_digit() && self.edit_text.len() < 2 {
                    self.edit_text.push(*ch);
                    self.cursor_blink = 0.0;
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::KeyDown(key) if self.editing_field.is_some() => {
                match key {
                    Key::Backspace => {
                        self.edit_text.pop();
                        self.cursor_blink = 0.0;
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Enter | Key::Tab => {
                        self.commit_edit();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Escape => {
                        self.editing_field = None;
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    _ => EventResult::Handled,
                }
            }
            Event::FocusLost => {
                self.commit_edit();
                ctx.set_virtual_keyboard_visible(false);
                ctx.request_paint();
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let dt_secs = dt.as_secs_f32();
        let mut needs_anim = false;

        if let Some(zone) = self.repeat_zone {
            self.repeat_elapsed += dt_secs;
            if !self.repeat_initial_done {
                if self.repeat_elapsed >= REPEAT_INITIAL_DELAY {
                    self.repeat_initial_done = true;
                    self.repeat_elapsed -= REPEAT_INITIAL_DELAY;
                    self.apply_spinner_step(zone);
                }
            } else {
                while self.repeat_elapsed >= REPEAT_INTERVAL {
                    self.repeat_elapsed -= REPEAT_INTERVAL;
                    self.apply_spinner_step(zone);
                }
            }
            self.mark_dirty(DirtyFlags::RENDER);
            needs_anim = true;
        }

        if self.editing_field.is_some() {
            self.cursor_blink += dt_secs;
            self.mark_dirty(DirtyFlags::RENDER);
            needs_anim = true;
        }

        needs_anim
    }

    fn needs_repaint(&self) -> bool {
        self.repeat_zone.is_some() || self.editing_field.is_some()
    }

    fn children(&self) -> &[ElementId] { &[] }

    fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_requested)
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::TextField,
            state: crate::a11y::NodeState {
                focused: self.editing_field.is_some(),
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties::default(),
        })
    }

    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "TimePicker" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = self.mss.width { self.width = Some(w); }
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
}

impl StyledElement for TimePickerElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
