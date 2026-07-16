use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widgets::input::date_picker::Date;
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

pub struct Calendar {
    selected: Option<Date>,
    on_select: Option<Arc<Mutex<dyn FnMut(Date) + Send>>>,
    show_week_numbers: bool,
    min_date: Option<Date>,
    max_date: Option<Date>,
}

impl Calendar {
    pub fn new() -> Self {
        Self {
            selected: None,
            on_select: None,
            show_week_numbers: false,
            min_date: None,
            max_date: None,
        }
    }

    pub fn selected(mut self, date: Date) -> Self {
        self.selected = Some(date);
        self
    }

    pub fn on_select(mut self, f: impl FnMut(Date) + Send + 'static) -> Self {
        self.on_select = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn show_week_numbers(mut self, v: bool) -> Self {
        self.show_week_numbers = v;
        self
    }

    pub fn min_date(mut self, date: Date) -> Self {
        self.min_date = Some(date);
        self
    }

    pub fn max_date(mut self, date: Date) -> Self {
        self.max_date = Some(date);
        self
    }
}

impl Default for Calendar {
    fn default() -> Self { Self::new() }
}

impl Widget for Calendar {
    fn create_element(&self) -> Box<dyn Element> {
        let view_year = self.selected.map(|d| d.year).unwrap_or(2026);
        let view_month = self.selected.map(|d| d.month).unwrap_or(3);
        Box::new(CalendarElement {
            id: ElementId::new(),
            selected: self.selected,
            on_select: self.on_select.clone(),
            show_week_numbers: self.show_week_numbers,
            min_date: self.min_date,
            max_date: self.max_date,
            view_year,
            view_month,
            hover_day: None,
            hover_zone: HoverZone::None,
            bounds: Rect::zero(),
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

const CALENDAR_WIDTH: f32 = 280.0;
const HEADER_HEIGHT: f32 = 40.0;
const DAY_LABEL_HEIGHT: f32 = 28.0;
const CELL_SIZE: f32 = 36.0;
const WEEK_NUM_WIDTH: f32 = 32.0;

const MONTH_NAMES: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];

const DAY_NAMES: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

#[derive(Clone, Copy, Debug, PartialEq)]
enum HoverZone {
    None,
    PrevMonth,
    NextMonth,
}

pub struct CalendarElement {
    id: ElementId,
    selected: Option<Date>,
    on_select: Option<Arc<Mutex<dyn FnMut(Date) + Send>>>,
    show_week_numbers: bool,
    min_date: Option<Date>,
    max_date: Option<Date>,
    view_year: i32,
    view_month: u32,
    hover_day: Option<u32>,
    hover_zone: HoverZone,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl CalendarElement {
    fn days_in_month(year: i32, month: u32) -> u32 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 { 29 } else { 28 }
            }
            _ => 30,
        }
    }

    fn first_day_of_week(year: i32, month: u32) -> u32 {
        let (y, m) = if month <= 2 { (year - 1, month + 12) } else { (year, month) };
        let k = y % 100;
        let j = y / 100;
        let h = (1 + (13 * (m as i32 + 1)) / 5 + k + k / 4 + j / 4 - 2 * j) % 7;
        ((h + 5) % 7) as u32
    }

    fn row_count(&self) -> u32 {
        let first_dow = Self::first_day_of_week(self.view_year, self.view_month);
        let days = Self::days_in_month(self.view_year, self.view_month);
        (first_dow + days + 6) / 7
    }

    fn calendar_height(&self) -> f32 {
        HEADER_HEIGHT + DAY_LABEL_HEIGHT + self.row_count() as f32 * CELL_SIZE + 8.0
    }

    fn grid_offset_x(&self) -> f32 {
        if self.show_week_numbers { WEEK_NUM_WIDTH } else { 0.0 }
    }

    fn day_rect(&self, day: u32) -> Rect {
        let first_dow = Self::first_day_of_week(self.view_year, self.view_month);
        let idx = first_dow + day - 1;
        let col = idx % 7;
        let row = idx / 7;
        let offset_x = self.grid_offset_x();
        let x = self.bounds.x() + offset_x + 6.0 + col as f32 * (CELL_SIZE + 2.0);
        let y = self.bounds.y() + HEADER_HEIGHT + DAY_LABEL_HEIGHT + row as f32 * CELL_SIZE;
        Rect::new(Point::new(x, y), Size::new(CELL_SIZE, CELL_SIZE))
    }

    fn is_date_enabled(&self, date: &Date) -> bool {
        if let Some(min) = &self.min_date {
            if date.year < min.year || (date.year == min.year && date.month < min.month)
                || (date.year == min.year && date.month == min.month && date.day < min.day) {
                return false;
            }
        }
        if let Some(max) = &self.max_date {
            if date.year > max.year || (date.year == max.year && date.month > max.month)
                || (date.year == max.year && date.month == max.month && date.day > max.day) {
                return false;
            }
        }
        true
    }

    fn is_today(&self, day: u32) -> bool {
        self.view_year == 2026 && self.view_month == 3 && day == 3
    }

    fn fire_select(&self, date: Date) {
        if let Some(ref cb) = self.on_select {
            if let Ok(mut f) = cb.lock() { f(date); }
        }
    }

    fn prev_month(&mut self) {
        if self.view_month == 1 { self.view_month = 12; self.view_year -= 1; }
        else { self.view_month -= 1; }
    }

    fn next_month(&mut self) {
        if self.view_month == 12 { self.view_month = 1; self.view_year += 1; }
        else { self.view_month += 1; }
    }

    fn week_number(year: i32, month: u32, day: u32) -> u32 {
        let mut ordinal = day;
        for m in 1..month {
            ordinal += Self::days_in_month(year, m);
        }
        let first_dow = Self::first_day_of_week(year, 1);
        ((ordinal + first_dow + 5) / 7).max(1)
    }
}

impl Element for CalendarElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(cal) = widget.as_any().downcast_ref::<Calendar>() {
            self.selected = cal.selected;
            self.on_select = cal.on_select.clone();
            self.show_week_numbers = cal.show_week_numbers;
            self.min_date = cal.min_date;
            self.max_date = cal.max_date;
            if let Some(d) = cal.selected {
                self.view_year = d.year;
                self.view_month = d.month;
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let total_w = CALENDAR_WIDTH + self.grid_offset_x();
        let w = total_w.min(constraints.max_width);
        let h = self.calendar_height().min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
        let border = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let gray100 = bg.darken(0.05);
        let gray400 = fg.with_alpha(0.45);
        let gray700 = fg.with_alpha(0.7);
        let gray900 = fg;
        let offset_x = self.grid_offset_x();

        list.push_rect_bordered(
            self.bounds, bg, [12.0; 4],
            Border::new(1.0, border),
        );

        let header_y = self.bounds.y() + 8.0;
        let prev_rect = Rect::new(Point::new(self.bounds.x() + 8.0, header_y), Size::new(28.0, 28.0));
        let prev_bg = if self.hover_zone == HoverZone::PrevMonth { gray100 } else { Color::TRANSPARENT };
        list.push_rect(prev_rect, prev_bg, [6.0; 4]);
        list.push_text_centered("\u{25C0}", prev_rect, gray700, 12.0);

        let next_rect = Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - 36.0, header_y),
            Size::new(28.0, 28.0),
        );
        let next_bg = if self.hover_zone == HoverZone::NextMonth { gray100 } else { Color::TRANSPARENT };
        list.push_rect(next_rect, next_bg, [6.0; 4]);
        list.push_text_centered("\u{25B6}", next_rect, gray700, 12.0);

        let month_label = format!("{} {}", MONTH_NAMES[(self.view_month - 1) as usize], self.view_year);
        let label_rect = Rect::new(
            Point::new(self.bounds.x() + 40.0, header_y + 4.0),
            Size::new(self.bounds.size.width - 80.0, 20.0),
        );
        list.push_text_centered(&month_label, label_rect, gray900, 14.0);

        if self.show_week_numbers {
            let wn_rect = Rect::new(
                Point::new(self.bounds.x() + 2.0, self.bounds.y() + HEADER_HEIGHT),
                Size::new(WEEK_NUM_WIDTH, DAY_LABEL_HEIGHT),
            );
            list.push_text_centered("Wk", wn_rect, gray400, 10.0);
        }

        let dow_y = self.bounds.y() + HEADER_HEIGHT;
        for (i, name) in DAY_NAMES.iter().enumerate() {
            let x = self.bounds.x() + offset_x + 6.0 + i as f32 * (CELL_SIZE + 2.0);
            let r = Rect::new(Point::new(x, dow_y), Size::new(CELL_SIZE, DAY_LABEL_HEIGHT));
            list.push_text_centered(name, r, gray400, 11.0);
        }

        if self.show_week_numbers {
            let first_dow = Self::first_day_of_week(self.view_year, self.view_month);
            let rows = self.row_count();
            for row in 0..rows {
                let day_in_row = (row * 7 + 1).saturating_sub(first_dow).max(1);
                let day_clamped = day_in_row.min(Self::days_in_month(self.view_year, self.view_month));
                let wn = Self::week_number(self.view_year, self.view_month, day_clamped);
                let y = self.bounds.y() + HEADER_HEIGHT + DAY_LABEL_HEIGHT + row as f32 * CELL_SIZE;
                let r = Rect::new(Point::new(self.bounds.x() + 2.0, y), Size::new(WEEK_NUM_WIDTH, CELL_SIZE));
                list.push_text_centered(&wn.to_string(), r, gray400, 10.0);
            }
        }

        let days = Self::days_in_month(self.view_year, self.view_month);
        for day in 1..=days {
            let r = self.day_rect(day);
            let date = Date::new(self.view_year, self.view_month, day);
            let enabled = self.is_date_enabled(&date);
            let is_selected = self.selected.map(|d| d == date).unwrap_or(false);
            let is_hovered = self.hover_day == Some(day) && enabled;
            let is_today = self.is_today(day);

            let day_bg = if is_selected {
                primary
            } else if is_hovered {
                bg.darken(0.05)
            } else {
                Color::TRANSPARENT
            };

            if day_bg != Color::TRANSPARENT {
                list.push_rect(r, day_bg, [CELL_SIZE / 2.0; 4]);
            }

            if is_today && !is_selected {
                list.push_rect_bordered(r, Color::TRANSPARENT, [CELL_SIZE / 2.0; 4], Border::new(2.0, primary));
            }

            let text_color = if is_selected {
                Color::WHITE
            } else if !enabled {
                fg.with_alpha(0.25)
            } else {
                gray900
            };
            list.push_text_centered(&day.to_string(), r, text_color, 13.0);
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if !self.bounds.contains(*pos) { return EventResult::Ignored; }

                let header_y = self.bounds.y() + 8.0;
                let prev_rect = Rect::new(Point::new(self.bounds.x() + 8.0, header_y), Size::new(28.0, 28.0));
                let next_rect = Rect::new(
                    Point::new(self.bounds.x() + self.bounds.size.width - 36.0, header_y),
                    Size::new(28.0, 28.0),
                );

                let new_zone = if prev_rect.contains(*pos) {
                    HoverZone::PrevMonth
                } else if next_rect.contains(*pos) {
                    HoverZone::NextMonth
                } else {
                    HoverZone::None
                };

                if new_zone != self.hover_zone {
                    self.hover_zone = new_zone;
                    ctx.request_paint();
                }

                let days = Self::days_in_month(self.view_year, self.view_month);
                let mut new_hover = None;
                for day in 1..=days {
                    if self.day_rect(day).contains(*pos) {
                        new_hover = Some(day);
                        break;
                    }
                }
                if new_hover != self.hover_day {
                    self.hover_day = new_hover;
                    ctx.request_paint();
                }

                ctx.set_cursor(CursorIcon::Pointer);
                EventResult::Handled
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if !self.bounds.contains(*position) { return EventResult::Ignored; }

                let header_y = self.bounds.y() + 8.0;
                let prev_rect = Rect::new(Point::new(self.bounds.x() + 8.0, header_y), Size::new(28.0, 28.0));
                let next_rect = Rect::new(
                    Point::new(self.bounds.x() + self.bounds.size.width - 36.0, header_y),
                    Size::new(28.0, 28.0),
                );

                if prev_rect.contains(*position) {
                    self.prev_month();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if next_rect.contains(*position) {
                    self.next_month();
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                let days = Self::days_in_month(self.view_year, self.view_month);
                for day in 1..=days {
                    if self.day_rect(day).contains(*position) {
                        let date = Date::new(self.view_year, self.view_month, day);
                        if self.is_date_enabled(&date) {
                            self.selected = Some(date);
                            self.fire_select(date);
                            ctx.request_paint();
                        }
                        return EventResult::Handled;
                    }
                }
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &[] }
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

    fn element_type_name(&self) -> &str { "Calendar" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER);
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

impl StyledElement for CalendarElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
