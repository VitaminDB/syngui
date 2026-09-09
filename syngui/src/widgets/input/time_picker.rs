//! Выбор времени: поле со значением и попап из колонок с прокруткой —
//! часы и минуты (в 12-часовом режиме ещё AM/PM). Колонка листается
//! колесом и стрелками, клик по строке применяет значение (клик по
//! минуте закрывает попап), набор цифр вводит время с клавиатуры:
//! «0930» → 09:30.
//!
//! Шаг колонки минут задаётся [`TimePicker::minute_step`] — календарь
//! ставит туда свой слот, чтобы список совпадал с сеткой.

use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::MssFields;
use crate::mss::{ComputedStyle, Dimension, TextAlign};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;

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

    pub fn minutes(&self) -> u32 {
        self.hour * 60 + self.minute
    }
}

pub struct TimePicker {
    selected: Option<Time>,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(Option<Time>) + Send>>>,
    width: Option<Dimension>,
    use_24h: bool,
    minute_step: u32,
}

fn period_label(hour: u32) -> String {
    if hour < 12 {
        crate::i18n::builtin("time_picker.am", "AM")
    } else {
        crate::i18n::builtin("time_picker.pm", "PM")
    }
}

impl TimePicker {
    pub fn new() -> Self {
        Self {
            selected: None,
            placeholder: "Select time...".to_string(),
            on_change: None,
            width: None,
            use_24h: true,
            minute_step: DEFAULT_MINUTE_STEP,
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

    /// Шаг колонки минут в минутах (1..=60); значение вне диапазона —
    /// умолчание.
    pub fn minute_step(mut self, m: u32) -> Self {
        self.minute_step = if (1..=60).contains(&m) { m } else { DEFAULT_MINUTE_STEP };
        self
    }
}

impl Default for TimePicker {
    fn default() -> Self {
        Self::new()
    }
}

impl TimePicker {
    fn element(&self) -> TimePickerElement {
        let (h, m) = self.selected.map(|t| (t.hour, t.minute)).unwrap_or((12, 0));
        TimePickerElement {
            id: ElementId::new(),
            selected: self.selected,
            placeholder: self.placeholder.clone(),
            on_change: self.on_change.clone(),
            width: self.width,
            use_24h: self.use_24h,
            minute_step: self.minute_step,
            is_open: false,
            view_hour: h,
            view_minute: m,
            scroll: [0.0; 3],
            hover: None,
            active_col: Col::Hour,
            digits: String::new(),
            opens_upward: false,
            bounds: Rect::zero(),
            focus_requested: false,
            popup_bg: None,
            popup_fg: None,
            popup_border: None,
            popup_selected_bg: None,
            popup_hover_bg: None,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        }
    }
}

impl Widget for TimePicker {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(self.element())
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

const DEFAULT_INPUT_HEIGHT: f32 = 40.0;
const DEFAULT_FONT_SIZE: f32 = 14.0;
const DEFAULT_MINUTE_STEP: u32 = 5;
/// Высота строки колонки и сколько их видно разом.
const ROW_H: f32 = 30.0;
const VISIBLE_ROWS: f32 = 5.0;
const COL_W: f32 = 62.0;
const PERIOD_COL_W: f32 = 46.0;
const POPUP_PAD: f32 = 6.0;
const COL_GAP: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Col {
    Hour,
    Minute,
    Period,
}

impl Col {
    fn index(self) -> usize {
        match self {
            Col::Hour => 0,
            Col::Minute => 1,
            Col::Period => 2,
        }
    }
}

pub struct TimePickerElement {
    id: ElementId,
    selected: Option<Time>,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(Option<Time>) + Send>>>,
    width: Option<Dimension>,
    use_24h: bool,
    minute_step: u32,
    is_open: bool,
    view_hour: u32,
    view_minute: u32,
    /// Прокрутка колонок в пикселях: часы, минуты, AM/PM.
    scroll: [f32; 3],
    hover: Option<(Col, usize)>,
    active_col: Col,
    /// Набор времени цифрами: «09» → час, «0930» → час и минута.
    digits: String,
    opens_upward: bool,
    bounds: Rect,
    focus_requested: bool,
    popup_bg: Option<Color>,
    popup_fg: Option<Color>,
    popup_border: Option<Color>,
    popup_selected_bg: Option<Color>,
    popup_hover_bg: Option<Color>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl TimePickerElement {
    fn input_height(&self) -> f32 {
        self.mss.height.map(|d| d.resolve(f32::INFINITY)).unwrap_or(DEFAULT_INPUT_HEIGHT)
    }

    fn font_size(&self) -> f32 {
        self.mss.font_size_or(DEFAULT_FONT_SIZE)
    }

    fn popup_size(&self) -> Size {
        let cols = COL_W * 2.0 + COL_GAP + if self.use_24h { 0.0 } else { COL_GAP + PERIOD_COL_W };
        Size::new(cols + POPUP_PAD * 2.0, ROW_H * VISIBLE_ROWS + POPUP_PAD * 2.0)
    }

    fn popup_rect(&self) -> Rect {
        let size = self.popup_size();
        let y = if self.opens_upward {
            self.bounds.y() - size.height - 4.0
        } else {
            self.bounds.y() + self.input_height() + 4.0
        };
        Rect::new(Point::new(self.bounds.x(), y), size)
    }

    fn col_rect(&self, col: Col) -> Rect {
        let popup = self.popup_rect();
        let x = popup.x() + POPUP_PAD + match col {
            Col::Hour => 0.0,
            Col::Minute => COL_W + COL_GAP,
            Col::Period => (COL_W + COL_GAP) * 2.0,
        };
        let w = if col == Col::Period { PERIOD_COL_W } else { COL_W };
        Rect::new(Point::new(x, popup.y() + POPUP_PAD), Size::new(w, ROW_H * VISIBLE_ROWS))
    }

    /// Часы колонки: 0..23 в 24-часовом режиме, 12,1..11 в 12-часовом.
    fn hour_values(&self) -> Vec<u32> {
        if self.use_24h {
            (0..24).collect()
        } else {
            std::iter::once(12).chain(1..12).collect()
        }
    }

    fn minute_values(&self) -> Vec<u32> {
        (0..60).step_by(self.minute_step.max(1) as usize).collect()
    }

    fn rows(&self, col: Col) -> usize {
        match col {
            Col::Hour => self.hour_values().len(),
            Col::Minute => self.minute_values().len(),
            Col::Period => 2,
        }
    }

    /// Выбранная строка колонки: минуты между делениями подсвечивают
    /// ближайшее сверху деление.
    fn selected_row(&self, col: Col) -> usize {
        match col {
            Col::Hour => {
                if self.use_24h {
                    self.view_hour as usize
                } else {
                    (self.view_hour % 12) as usize
                }
            }
            Col::Minute => (self.view_minute / self.minute_step.max(1)) as usize,
            Col::Period => usize::from(self.view_hour >= 12),
        }
    }

    fn max_scroll(&self, col: Col) -> f32 {
        (self.rows(col) as f32 * ROW_H - ROW_H * VISIBLE_ROWS).max(0.0)
    }

    /// Подвести выбранную строку к середине колонки.
    fn center_scroll(&mut self, col: Col) {
        let target = self.selected_row(col) as f32 * ROW_H - (ROW_H * VISIBLE_ROWS - ROW_H) / 2.0;
        self.scroll[col.index()] = target.clamp(0.0, self.max_scroll(col));
    }

    fn center_all(&mut self) {
        self.center_scroll(Col::Hour);
        self.center_scroll(Col::Minute);
        if !self.use_24h {
            self.center_scroll(Col::Period);
        }
    }

    fn cols(&self) -> Vec<Col> {
        if self.use_24h {
            vec![Col::Hour, Col::Minute]
        } else {
            vec![Col::Hour, Col::Minute, Col::Period]
        }
    }

    fn col_at(&self, p: Point) -> Option<Col> {
        self.cols().into_iter().find(|&c| self.col_rect(c).contains(p))
    }

    fn row_at(&self, col: Col, p: Point) -> Option<usize> {
        let r = self.col_rect(col);
        let i = ((p.y - r.y() + self.scroll[col.index()]) / ROW_H).floor();
        (i >= 0.0 && (i as usize) < self.rows(col)).then_some(i as usize)
    }

    fn row_label(&self, col: Col, row: usize) -> String {
        match col {
            Col::Hour => {
                let h = self.hour_values()[row];
                if self.use_24h {
                    format!("{h:02}")
                } else {
                    format!("{h}")
                }
            }
            Col::Minute => format!("{:02}", self.minute_values()[row]),
            Col::Period => period_label(if row == 0 { 0 } else { 12 }),
        }
    }

    /// Применить строку колонки к черновику времени.
    fn pick(&mut self, col: Col, row: usize) {
        match col {
            Col::Hour => {
                let h = self.hour_values()[row];
                self.view_hour = if self.use_24h {
                    h
                } else if self.view_hour >= 12 {
                    h % 12 + 12
                } else {
                    h % 12
                };
            }
            Col::Minute => self.view_minute = self.minute_values()[row],
            Col::Period => {
                self.view_hour = if row == 0 { self.view_hour % 12 } else { self.view_hour % 12 + 12 };
            }
        }
        self.digits.clear();
        self.apply();
    }

    fn apply(&mut self) {
        let t = Time::new(self.view_hour, self.view_minute);
        if self.selected != Some(t) {
            self.selected = Some(t);
            self.fire_change();
        }
    }

    fn close(&mut self, ctx: &mut EventContext) {
        if self.is_open {
            self.is_open = false;
            self.digits.clear();
            ctx.unregister_overlay();
        }
    }

    fn open(&mut self, ctx: &mut EventContext) {
        let size = self.popup_size();
        let input_h = self.input_height();
        self.opens_upward = self.bounds.y() + input_h + 4.0 + size.height > ctx.viewport_size().height && self.bounds.y() >= size.height + 4.0;
        self.is_open = true;
        self.digits.clear();
        self.active_col = Col::Hour;
        self.center_all();
        let popup = self.popup_rect();
        let overlay = if self.opens_upward {
            Rect::new(Point::new(self.bounds.x(), popup.y()), Size::new(size.width.max(self.bounds.size.width), popup.size.height + 4.0 + input_h))
        } else {
            Rect::new(self.bounds.origin, Size::new(size.width.max(self.bounds.size.width), input_h + 4.0 + popup.size.height))
        };
        ctx.register_overlay(overlay, false);
        self.focus_requested = true;
    }

    /// Шаг по активной колонке стрелками.
    fn step(&mut self, delta: i32) {
        let col = self.active_col;
        let n = self.rows(col) as i32;
        let row = (self.selected_row(col) as i32 + delta).rem_euclid(n) as usize;
        self.pick(col, row);
        self.center_scroll(col);
    }

    /// Цифра набора: две первые — часы, следующие две — минуты.
    fn push_digit(&mut self, ch: char) {
        if self.digits.len() >= 4 {
            self.digits.clear();
        }
        self.digits.push(ch);
        let d = self.digits.clone();
        if d.len() >= 2 {
            if let Ok(h) = d[0..2].parse::<u32>() {
                self.view_hour = h.min(23);
            }
        }
        if d.len() == 4 {
            if let Ok(m) = d[2..4].parse::<u32>() {
                self.view_minute = m.min(59);
            }
        }
        self.apply();
        self.center_all();
    }

    fn fire_change(&self) {
        if let Some(ref cb) = self.on_change {
            if let Ok(mut f) = cb.lock() {
                f(self.selected);
            }
        }
    }

    fn format_display(&self, time: &Time) -> String {
        if self.use_24h {
            time.format()
        } else {
            let period = period_label(time.hour);
            let h12 = if time.hour == 0 { 12 } else if time.hour > 12 { time.hour - 12 } else { time.hour };
            format!("{}:{:02} {}", h12, time.minute, period)
        }
    }

    /// Текст поля: во время набора цифр — то, что уже введено.
    fn input_text(&self) -> Option<String> {
        if self.is_open && !self.digits.is_empty() {
            let d = &self.digits;
            return Some(match d.len() {
                1 => format!("{d}_:__"),
                2 => format!("{d}:__"),
                3 => format!("{}:{}_", &d[0..2], &d[2..3]),
                _ => format!("{}:{}", &d[0..2], &d[2..4]),
            });
        }
        self.selected.map(|t| self.format_display(&t))
    }
}

impl Element for TimePickerElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(tp) = widget.as_any().downcast_ref::<TimePicker>() {
            self.selected = tp.selected;
            self.placeholder = tp.placeholder.clone();
            self.on_change = tp.on_change.clone();
            self.width = tp.width;
            self.use_24h = tp.use_24h;
            self.minute_step = tp.minute_step;
            if let Some(t) = tp.selected {
                self.view_hour = t.hour;
                self.view_minute = t.minute;
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let default_w = if constraints.max_width.is_finite() { constraints.max_width } else { self.popup_size().width };
        let w = self.width.map(|d| d.resolve(constraints.max_width)).unwrap_or(default_w).min(constraints.max_width);
        let h = self.input_height();
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
        let border_base = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let accent = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let border_color = if self.is_open { accent } else { border_base };
        let font = self.font_size();
        let input_h = self.input_height();
        let radius = self.mss.border_radius_uniform(input_h, 8.0);

        list.push_rect_bordered(self.bounds, bg, [radius; 4], Border::new(if self.is_open { 2.0 } else { 1.0 }, border_color));

        let text_rect = Rect::new(
            Point::new(self.bounds.x() + 10.0, self.bounds.y()),
            Size::new((self.bounds.size.width - 34.0).max(4.0), input_h),
        );
        match self.input_text() {
            Some(text) => list.push_text_singleline(&text, text_rect, fg, font, TextAlign::DEFAULT, 400),
            None => {
                let ph = self.mss.color.map(|c| c.with_alpha(0.5)).unwrap_or(Color::from_hex("#9CA3AF"));
                list.push_text_singleline(&self.placeholder, text_rect, ph, font, TextAlign::DEFAULT, 400);
            }
        }

        let icon_rect = Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - 22.0, self.bounds.y()),
            Size::new(14.0, input_h),
        );
        let icon_color = self.mss.color.map(|c| c.with_alpha(0.6)).unwrap_or(Color::from_hex("#6B7280"));
        list.push_text("\u{E8B5}", icon_rect, icon_color, font);

        if !self.is_open {
            return;
        }

        let popup = self.popup_rect();
        let popup_bg = self.popup_bg.or(self.mss.background_color).unwrap_or(Color::WHITE);
        let popup_fg = self.popup_fg.or(self.mss.color).unwrap_or(Color::from_hex("#111827"));
        let popup_border = self.popup_border.or(self.mss.border_color).unwrap_or(Color::from_hex("#E5E7EB"));
        let selected_bg = self.popup_selected_bg.unwrap_or(accent.with_alpha(0.22));
        let hover_bg = self.popup_hover_bg.unwrap_or(popup_fg.with_alpha(0.08));

        list.begin_overlay();
        list.push_shadow(popup, Color::BLACK.with_alpha(0.15), 16.0, (0.0, 4.0), [12.0; 4]);
        list.push_rect_bordered(popup, popup_bg, [12.0; 4], Border::new(1.0, popup_border));

        for col in self.cols() {
            let r = self.col_rect(col);
            let scroll = self.scroll[col.index()];
            let sel = self.selected_row(col);
            let first = (scroll / ROW_H).floor().max(0.0) as usize;
            let last = (((scroll + r.size.height) / ROW_H).ceil() as usize).min(self.rows(col));
            list.push_clip(r);
            for row in first..last {
                let y = r.y() + row as f32 * ROW_H - scroll;
                let row_rect = Rect::new(Point::new(r.x(), y), Size::new(r.size.width, ROW_H));
                let fill = Rect::new(Point::new(row_rect.x() + 1.0, row_rect.y() + 2.0), Size::new((row_rect.size.width - 2.0).max(0.0), ROW_H - 4.0));
                if row == sel {
                    list.push_rect(fill, selected_bg, [6.0; 4]);
                } else if self.hover == Some((col, row)) {
                    list.push_rect(fill, hover_bg, [6.0; 4]);
                }
                list.push_text_singleline(
                    &self.row_label(col, row),
                    row_rect,
                    if row == sel { accent } else { popup_fg },
                    font + 1.0,
                    TextAlign::CENTER,
                    if row == sel { 700 } else { 400 },
                );
            }
            list.pop_clip();
        }

        list.end_overlay();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if self.is_open {
                    let hover = self.col_at(*pos).and_then(|c| self.row_at(c, *pos).map(|r| (c, r)));
                    if hover != self.hover {
                        self.hover = hover;
                        ctx.request_paint();
                    }
                    if hover.is_some() {
                        ctx.set_cursor(CursorIcon::Pointer);
                        return EventResult::Handled;
                    }
                    if self.popup_rect().contains(*pos) {
                        return EventResult::Handled;
                    }
                }
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Pointer);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.bounds.contains(*position) {
                    if self.is_open {
                        self.close(ctx);
                    } else {
                        self.open(ctx);
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.is_open {
                    if let Some(col) = self.col_at(*position) {
                        if let Some(row) = self.row_at(col, *position) {
                            self.pick(col, row);
                            self.active_col = col;
                            // Минуты завершают выбор: время готово.
                            if col == Col::Minute {
                                self.close(ctx);
                            }
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                    if self.popup_rect().contains(*position) {
                        return EventResult::Handled;
                    }
                    self.close(ctx);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseWheel { delta, position, .. } => {
                if !self.is_open {
                    return EventResult::Ignored;
                }
                let col = self.col_at(*position).unwrap_or(self.active_col);
                if !self.popup_rect().contains(*position) {
                    return EventResult::Ignored;
                }
                let i = col.index();
                let next = (self.scroll[i] - delta).clamp(0.0, self.max_scroll(col));
                if next != self.scroll[i] {
                    self.scroll[i] = next;
                    self.hover = self.row_at(col, *position).map(|r| (col, r));
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::CharInput(ch) if self.is_open && ch.is_ascii_digit() => {
                self.push_digit(*ch);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(key) if self.is_open => match key {
                Key::Escape => {
                    self.close(ctx);
                    ctx.request_paint();
                    EventResult::Handled
                }
                Key::Enter | Key::Tab => {
                    self.apply();
                    self.close(ctx);
                    ctx.request_paint();
                    EventResult::Handled
                }
                Key::Up => {
                    self.step(-1);
                    ctx.request_paint();
                    EventResult::Handled
                }
                Key::Down => {
                    self.step(1);
                    ctx.request_paint();
                    EventResult::Handled
                }
                Key::Left | Key::Right => {
                    let cols = self.cols();
                    let i = cols.iter().position(|&c| c == self.active_col).unwrap_or(0) as i32;
                    let d = if *key == Key::Left { -1 } else { 1 };
                    self.active_col = cols[(i + d).rem_euclid(cols.len() as i32) as usize];
                    ctx.request_paint();
                    EventResult::Handled
                }
                Key::Backspace => {
                    self.digits.pop();
                    ctx.request_paint();
                    EventResult::Handled
                }
                _ => EventResult::Handled,
            },
            Event::FocusLost => {
                if self.is_open {
                    self.close(ctx);
                    ctx.request_paint();
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_requested)
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::TextField,
            state: crate::a11y::NodeState {
                focused: self.is_open,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties::default(),
        })
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

    fn element_type_name(&self) -> &str {
        "TimePicker"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = self.mss.width {
            self.width = Some(w);
        }
        // Попап красится своими переменными — как у Dropdown, иначе в
        // тёмной теме список падал на светлые умолчания виджета.
        let color = |name: &str| style.get(name).and_then(|v| v.as_color()).map(crate::animation::transition::mss_color_to_core);
        if let Some(c) = color("--popup-background") {
            self.popup_bg = Some(c);
        }
        if let Some(c) = color("--popup-color") {
            self.popup_fg = Some(c);
        }
        if let Some(c) = color("--popup-border") {
            self.popup_border = Some(c);
        }
        if let Some(c) = color("--popup-selected-background") {
            self.popup_selected_bg = Some(c);
        }
        if let Some(c) = color("--popup-hover-background") {
            self.popup_hover_bg = Some(c);
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
}

impl StyledElement for TimePickerElement {
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

    fn open_element(picker: TimePicker) -> TimePickerElement {
        let mut e = picker.element();
        e.layout(Constraints::tight(Size::new(120.0, 40.0)));
        e.is_open = true;
        e.center_all();
        e
    }

    /// Клик по строке колонки — выбор значения: попап листает часы и
    /// минуты списком, а не крутит спиннером.
    #[test]
    fn columns_pick_hour_and_minute() {
        let mut e = open_element(TimePicker::new().selected(Time::new(9, 30)).minute_step(15));
        let hours = e.col_rect(Col::Hour);
        // Строка выбранного часа — по центру колонки.
        let sel_y = hours.y() + e.selected_row(Col::Hour) as f32 * ROW_H - e.scroll[0] + ROW_H / 2.0;
        assert_eq!(e.row_at(Col::Hour, Point::new(hours.x() + 10.0, sel_y)), Some(9));
        // Строкой ниже — 10 часов.
        e.pick(Col::Hour, e.row_at(Col::Hour, Point::new(hours.x() + 10.0, sel_y + ROW_H)).unwrap());
        assert_eq!(e.selected, Some(Time::new(10, 30)));
        // Колонка минут идёт шагом 15: третья строка — 45 минут.
        e.pick(Col::Minute, 3);
        assert_eq!(e.selected, Some(Time::new(10, 45)));
        assert_eq!(e.minute_values(), vec![0, 15, 30, 45]);
    }

    /// Время можно набрать цифрами: «0930» → 09:30.
    #[test]
    fn typing_digits_sets_the_time() {
        let mut e = open_element(TimePicker::new().selected(Time::new(0, 0)));
        for ch in "0930".chars() {
            e.push_digit(ch);
        }
        assert_eq!(e.selected, Some(Time::new(9, 30)));
        assert_eq!(e.input_text().as_deref(), Some("09:30"));
    }

    /// 12-часовой режим: третья колонка переключает AM/PM, не трогая
    /// стрелку часов.
    #[test]
    fn period_column_switches_am_pm() {
        let mut e = open_element(TimePicker::new().selected(Time::new(9, 0)).use_24h(false));
        assert_eq!(e.cols().len(), 3);
        e.pick(Col::Period, 1);
        assert_eq!(e.selected, Some(Time::new(21, 0)));
        e.pick(Col::Period, 0);
        assert_eq!(e.selected, Some(Time::new(9, 0)));
    }
}
