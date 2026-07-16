use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }

    pub fn format(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

pub struct DatePicker {
    selected: Option<Date>,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(Option<Date>) + Send>>>,
    width: Option<Dimension>,
}

impl DatePicker {
    pub fn new() -> Self {
        Self {
            selected: None,
            placeholder: "Select date...".to_string(),
            on_change: None,
            width: None,
        }
    }

    pub fn selected(mut self, date: Date) -> Self {
        self.selected = Some(date);
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn on_change(mut self, f: impl FnMut(Option<Date>) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }
}

impl Default for DatePicker {
    fn default() -> Self { Self::new() }
}

impl Widget for DatePicker {
    fn create_element(&self) -> Box<dyn Element> {
        let view_year = self.selected.map(|d| d.year).unwrap_or(2026);
        let view_month = self.selected.map(|d| d.month).unwrap_or(3);
        Box::new(DatePickerElement {
            id: ElementId::new(),
            selected: self.selected,
            placeholder: self.placeholder.clone(),
            on_change: self.on_change.clone(),
            width: self.width,
            is_open: false,
            view_year,
            view_month,
            hover_day: None,
            hover_zone: HoverZone::None,
            opens_upward: false,
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

const INPUT_HEIGHT: f32 = 40.0;
const CALENDAR_WIDTH: f32 = 280.0;
const HEADER_HEIGHT: f32 = 40.0;
const DAY_LABEL_HEIGHT: f32 = 28.0;
const CELL_SIZE: f32 = 36.0;

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

pub struct DatePickerElement {
    id: ElementId,
    selected: Option<Date>,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(Option<Date>) + Send>>>,
    width: Option<Dimension>,
    is_open: bool,
    view_year: i32,
    view_month: u32,
    hover_day: Option<u32>,
    hover_zone: HoverZone,
    opens_upward: bool,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl DatePickerElement {
    fn days_in_month(year: i32, month: u32) -> u32 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    }

    fn first_day_of_week(year: i32, month: u32) -> u32 {
        let (y, m) = if month <= 2 {
            (year - 1, month + 12)
        } else {
            (year, month)
        };
        let q = 1;
        let k = y % 100;
        let j = y / 100;
        let h = (q + (13 * (m as i32 + 1)) / 5 + k + k / 4 + j / 4 - 2 * j) % 7;
        ((h + 5) % 7) as u32
    }

    fn calendar_rect(&self) -> Rect {
        let rows = {
            let first_dow = Self::first_day_of_week(self.view_year, self.view_month);
            let days = Self::days_in_month(self.view_year, self.view_month);
            ((first_dow + days + 6) / 7) as f32
        };
        let h = HEADER_HEIGHT + DAY_LABEL_HEIGHT + rows * CELL_SIZE + 8.0;
        let y = if self.opens_upward {
            self.bounds.y() - h - 4.0
        } else {
            self.bounds.y() + INPUT_HEIGHT + 4.0
        };
        Rect::new(
            Point::new(self.bounds.x(), y),
            Size::new(CALENDAR_WIDTH, h),
        )
    }

    fn fire_change(&self) {
        if let Some(ref cb) = self.on_change {
            if let Ok(mut f) = cb.lock() { f(self.selected); }
        }
    }

    fn prev_month(&mut self) {
        if self.view_month == 1 {
            self.view_month = 12;
            self.view_year -= 1;
        } else {
            self.view_month -= 1;
        }
    }

    fn next_month(&mut self) {
        if self.view_month == 12 {
            self.view_month = 1;
            self.view_year += 1;
        } else {
            self.view_month += 1;
        }
    }

    fn day_rect(&self, cal: Rect, day: u32) -> Rect {
        let first_dow = Self::first_day_of_week(self.view_year, self.view_month);
        let idx = first_dow + day - 1;
        let col = idx % 7;
        let row = idx / 7;
        let x = cal.x() + 6.0 + col as f32 * (CELL_SIZE + 2.0);
        let y = cal.y() + HEADER_HEIGHT + DAY_LABEL_HEIGHT + row as f32 * CELL_SIZE;
        Rect::new(Point::new(x, y), Size::new(CELL_SIZE, CELL_SIZE))
    }
}

impl Element for DatePickerElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(dp) = widget.as_any().downcast_ref::<DatePicker>() {
            self.selected = dp.selected;
            self.placeholder = dp.placeholder.clone();
            self.on_change = dp.on_change.clone();
            self.width = dp.width;
            if let Some(d) = dp.selected {
                self.view_year = d.year;
                self.view_month = d.month;
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let default_w = if constraints.max_width.is_finite() { constraints.max_width } else { CALENDAR_WIDTH };
        let w = self.width.map(|d| d.resolve(constraints.max_width)).unwrap_or(default_w).min(constraints.max_width);
        self.bounds = Rect::new(Point::zero(), Size::new(w, INPUT_HEIGHT));
        Size::new(w, INPUT_HEIGHT)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg_color = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg_color = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
        let base_border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let focused_border = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let placeholder_color = self.mss.color.map(|c| c.with_alpha(0.5)).unwrap_or(Color::from_hex("#9CA3AF"));
        let border_color = if self.is_open { focused_border } else { base_border };

        list.push_rect_bordered(
            self.bounds, bg_color, [8.0; 4],
            Border::new(if self.is_open { 2.0 } else { 1.0 }, border_color),
        );

        let text_rect = Rect::new(
            Point::new(self.bounds.x() + 12.0, self.bounds.y() + (INPUT_HEIGHT - 14.0) / 2.0),
            Size::new(self.bounds.size.width - 40.0, 16.0),
        );
        if let Some(d) = self.selected {
            list.push_text(&d.format(), text_rect, fg_color, 14.0);
        } else {
            list.push_text(&self.placeholder, text_rect, placeholder_color, 14.0);
        }

        let icon_rect = Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - 28.0, self.bounds.y() + (INPUT_HEIGHT - 14.0) / 2.0),
            Size::new(16.0, 14.0),
        );
        list.push_text("\u{E935}", icon_rect, self.mss.color.map(|c| c.with_alpha(0.6)).unwrap_or(Color::from_hex("#6B7280")), 14.0);

        if !self.is_open { return; }

        let cal = self.calendar_rect();
        let popup_bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let popup_border = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
        let nav_text_color = self.mss.color.unwrap_or(Color::from_hex("#374151"));
        let dim_text_color = self.mss.color.map(|c| c.with_alpha(0.5)).unwrap_or(Color::from_hex("#9CA3AF"));

        list.begin_overlay();
        list.push_shadow(cal, Color::BLACK.with_alpha(0.15), 16.0, (0.0, 4.0), [12.0; 4]);
        list.push_rect_bordered(cal, popup_bg, [12.0; 4], Border::new(1.0, popup_border));

        let header_y = cal.y() + 8.0;

        let hover_bg = self.mss.background_color.map(|c| c.darken(0.05)).unwrap_or(Color::from_hex("#F3F4F6"));
        let prev_rect = Rect::new(Point::new(cal.x() + 8.0, header_y), Size::new(28.0, 28.0));
        let prev_bg = if self.hover_zone == HoverZone::PrevMonth { hover_bg } else { Color::TRANSPARENT };
        list.push_rect(prev_rect, prev_bg, [6.0; 4]);
        list.push_text_centered("\u{E5CB}", prev_rect, nav_text_color, 18.0);

        let next_rect = Rect::new(Point::new(cal.x() + cal.size.width - 36.0, header_y), Size::new(28.0, 28.0));
        let next_bg = if self.hover_zone == HoverZone::NextMonth { hover_bg } else { Color::TRANSPARENT };
        list.push_rect(next_rect, next_bg, [6.0; 4]);
        list.push_text_centered("\u{E5CC}", next_rect, nav_text_color, 18.0);

        let month_label = format!("{} {}", MONTH_NAMES[(self.view_month - 1) as usize], self.view_year);
        let label_rect = Rect::new(
            Point::new(cal.x() + 40.0, header_y + 4.0),
            Size::new(cal.size.width - 80.0, 20.0),
        );
        list.push_text_centered(&month_label, label_rect, fg_color, 14.0);

        let dow_y = cal.y() + HEADER_HEIGHT;
        for (i, name) in DAY_NAMES.iter().enumerate() {
            let x = cal.x() + 6.0 + i as f32 * (CELL_SIZE + 2.0);
            let r = Rect::new(Point::new(x, dow_y), Size::new(CELL_SIZE, DAY_LABEL_HEIGHT));
            list.push_text_centered(name, r, dim_text_color, 11.0);
        }

        let days = Self::days_in_month(self.view_year, self.view_month);
        for day in 1..=days {
            let r = self.day_rect(cal, day);

            let is_selected = self.selected.map(|d| {
                d.year == self.view_year && d.month == self.view_month && d.day == day
            }).unwrap_or(false);
            let is_hovered = self.hover_day == Some(day);

            let accent = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
            let bg = if is_selected {
                accent
            } else if is_hovered {
                accent.with_alpha(0.1)
            } else {
                Color::TRANSPARENT
            };

            if bg != Color::TRANSPARENT {
                list.push_rect(r, bg, [CELL_SIZE / 2.0; 4]);
            }

            let text_color = if is_selected {
                Color::WHITE
            } else {
                fg_color
            };
            let label = day.to_string();
            list.push_text_centered(&label, r, text_color, 13.0);
        }

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
                    let cal = self.calendar_rect();
                    if cal.contains(*pos) {
                        let header_y = cal.y() + 8.0;
                        let prev_rect = Rect::new(Point::new(cal.x() + 8.0, header_y), Size::new(28.0, 28.0));
                        let next_rect = Rect::new(Point::new(cal.x() + cal.size.width - 36.0, header_y), Size::new(28.0, 28.0));

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
                            if self.day_rect(cal, day).contains(*pos) {
                                new_hover = Some(day);
                                break;
                            }
                        }
                        if new_hover != self.hover_day {
                            self.hover_day = new_hover;
                            ctx.request_paint();
                        }

                        ctx.set_cursor(CursorIcon::Pointer);
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.bounds.contains(*position) {
                    self.is_open = !self.is_open;
                    if self.is_open {
                        let rows = {
                            let first_dow = Self::first_day_of_week(self.view_year, self.view_month);
                            let days = Self::days_in_month(self.view_year, self.view_month);
                            ((first_dow + days + 6) / 7) as f32
                        };
                        let cal_h = HEADER_HEIGHT + DAY_LABEL_HEIGHT + rows * CELL_SIZE + 8.0;
                        self.opens_upward = self.bounds.y() + INPUT_HEIGHT + 4.0 + cal_h > ctx.viewport_size().height
                            && self.bounds.y() >= cal_h + 4.0;
                        let cal = self.calendar_rect();
                        let overlay_bounds = if self.opens_upward {
                            Rect::new(
                                Point::new(self.bounds.x(), cal.y()),
                                Size::new(CALENDAR_WIDTH, cal.size.height + 4.0 + INPUT_HEIGHT),
                            )
                        } else {
                            Rect::new(
                                self.bounds.origin,
                                Size::new(CALENDAR_WIDTH, INPUT_HEIGHT + 4.0 + cal.size.height),
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
                    let cal = self.calendar_rect();
                    if cal.contains(*position) {
                        let header_y = cal.y() + 8.0;
                        let prev_rect = Rect::new(Point::new(cal.x() + 8.0, header_y), Size::new(28.0, 28.0));
                        let next_rect = Rect::new(Point::new(cal.x() + cal.size.width - 36.0, header_y), Size::new(28.0, 28.0));

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
                            if self.day_rect(cal, day).contains(*position) {
                                self.selected = Some(Date::new(self.view_year, self.view_month, day));
                                self.is_open = false;
                                ctx.unregister_overlay();
                                self.fire_change();
                                ctx.request_paint();
                                return EventResult::Handled;
                            }
                        }
                        return EventResult::Handled;
                    }

                    self.is_open = false;
                    ctx.unregister_overlay();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
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

    fn element_type_name(&self) -> &str { "DatePicker" }

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

impl StyledElement for DatePickerElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
