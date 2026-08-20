//! Поле ввода даты с всплывающим календарём.
//!
//! Попап рисуется той же панелью, что и виджет
//! [`Calendar`](crate::widgets::Calendar): общие локализация, быстрый выбор
//! месяца/года и MSS-переменные `--cal-*`.

use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt, TextMeasure};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widgets::visual::calendar::panel::{self, PanelMetrics};
use crate::widgets::visual::calendar::{default_locale, CalendarLocale, CalendarTheme, CalendarVars, PanelState};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

pub use crate::widgets::visual::calendar::Date;

/// Поле с датой и календарём по клику.
pub struct DatePicker {
    selected: Option<Date>,
    placeholder: Option<String>,
    on_change: Option<Arc<Mutex<dyn FnMut(Option<Date>) + Send>>>,
    width: Option<Dimension>,
    show_week_numbers: bool,
    min_date: Option<Date>,
    max_date: Option<Date>,
    locale: Option<CalendarLocale>,
}

impl DatePicker {
    pub fn new() -> Self {
        Self {
            selected: None,
            placeholder: None,
            on_change: None,
            width: None,
            show_week_numbers: false,
            min_date: None,
            max_date: None,
            locale: None,
        }
    }

    pub fn selected(mut self, date: Date) -> Self {
        self.selected = Some(date);
        self
    }

    /// Открыть с уже выбранной сегодняшней датой.
    pub fn today(self) -> Self {
        self.selected(Date::today())
    }

    /// Подсказка в пустом поле. По умолчанию — формат из локали
    /// (`дд.мм.гггг` для русской).
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
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

    /// Язык календаря и формат даты в поле. По умолчанию — [`default_locale`].
    pub fn locale(mut self, locale: CalendarLocale) -> Self {
        self.locale = Some(locale);
        self
    }
}

impl Default for DatePicker {
    fn default() -> Self { Self::new() }
}

impl Widget for DatePicker {
    fn create_element(&self) -> Box<dyn Element> {
        let state = self.selected.map(PanelState::from_date).unwrap_or_else(PanelState::today);
        Box::new(DatePickerElement {
            id: ElementId::new(),
            selected: self.selected,
            widget_selected: self.selected,
            placeholder: self.placeholder.clone(),
            on_change: self.on_change.clone(),
            width: self.width,
            show_week_numbers: self.show_week_numbers,
            min_date: self.min_date,
            max_date: self.max_date,
            locale: self.locale.clone().unwrap_or_else(default_locale),
            state,
            today: Date::today(),
            vars: CalendarVars::default(),
            text_measure: None,
            is_open: false,
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
const POPUP_GAP: f32 = 4.0;

pub struct DatePickerElement {
    id: ElementId,
    selected: Option<Date>,
    widget_selected: Option<Date>,
    placeholder: Option<String>,
    on_change: Option<Arc<Mutex<dyn FnMut(Option<Date>) + Send>>>,
    width: Option<Dimension>,
    show_week_numbers: bool,
    min_date: Option<Date>,
    max_date: Option<Date>,
    locale: CalendarLocale,
    state: PanelState,
    today: Date,
    vars: CalendarVars,
    text_measure: Option<Arc<dyn TextMeasure>>,
    is_open: bool,
    opens_upward: bool,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl DatePickerElement {
    fn theme(&self) -> CalendarTheme {
        CalendarTheme::resolve(&self.mss, &self.vars)
    }

    fn metrics(&self) -> PanelMetrics {
        PanelMetrics::new(self.theme().cell, self.show_week_numbers)
    }

    /// Прямоугольник попапа: под полем или над ним, если снизу не помещается.
    fn calendar_rect(&self) -> Rect {
        let m = self.metrics();
        let y = if self.opens_upward {
            self.bounds.y() - m.height - POPUP_GAP
        } else {
            self.bounds.y() + INPUT_HEIGHT + POPUP_GAP
        };
        Rect::new(Point::new(self.bounds.x(), y), m.size())
    }

    fn fire_change(&self) {
        if let Some(ref cb) = self.on_change {
            if let Ok(mut f) = cb.lock() { f(self.selected); }
        }
    }

    fn placeholder_text(&self) -> &str {
        self.placeholder
            .as_deref()
            .unwrap_or_else(|| self.locale.placeholder.as_ref())
    }

    fn close(&mut self, ctx: &mut EventContext) {
        self.is_open = false;
        self.state.mode = crate::widgets::visual::calendar::PanelMode::Days;
        self.state.hover = None;
        ctx.unregister_overlay();
    }
}

impl Element for DatePickerElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(dp) = widget.as_any().downcast_ref::<DatePicker>() {
            self.placeholder = dp.placeholder.clone();
            self.on_change = dp.on_change.clone();
            self.width = dp.width;
            self.show_week_numbers = dp.show_week_numbers;
            self.min_date = dp.min_date;
            self.max_date = dp.max_date;
            self.locale = dp.locale.clone().unwrap_or_else(default_locale);
            self.today = Date::today();
            if dp.selected != self.widget_selected {
                self.widget_selected = dp.selected;
                self.selected = dp.selected;
                if let Some(d) = dp.selected {
                    self.state.goto(d);
                }
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let default_w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            self.metrics().width
        };
        let w = self.width.map(|d| d.resolve(constraints.max_width)).unwrap_or(default_w).min(constraints.max_width);
        self.bounds = Rect::new(Point::zero(), Size::new(w, INPUT_HEIGHT));
        Size::new(w, INPUT_HEIGHT)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let theme = self.theme();
        let bg_color = self.mss.background_color.unwrap_or(Color::WHITE);
        let base_border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let border_color = if self.is_open { theme.accent } else { base_border };
        let placeholder_color = theme.muted;

        list.push_rect_bordered(
            self.bounds,
            bg_color,
            [8.0; 4],
            Border::new(if self.is_open { 2.0 } else { 1.0 }, border_color),
        );

        let font_size = self.mss.font_size.unwrap_or(14.0);
        let text_rect = Rect::new(
            Point::new(self.bounds.x() + 12.0, self.bounds.y() + (INPUT_HEIGHT - font_size) / 2.0),
            Size::new(self.bounds.size.width - 40.0, font_size + 2.0),
        );
        match self.selected {
            Some(d) => list.push_text(&self.locale.format_date(&d), text_rect, theme.text, font_size),
            None => list.push_text(self.placeholder_text(), text_rect, placeholder_color, font_size),
        }

        let icon_rect = Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - 28.0, self.bounds.y() + (INPUT_HEIGHT - 14.0) / 2.0),
            Size::new(16.0, 14.0),
        );
        list.push_text(
            panel::ICON_CALENDAR,
            icon_rect,
            if self.is_open { theme.accent } else { theme.muted },
            14.0,
        );

        if !self.is_open { return; }

        let cal = self.calendar_rect();
        let input = panel::PanelInput {
            state: self.state,
            theme: &theme,
            locale: &self.locale,
            selected: self.selected,
            today: self.today,
            min: self.min_date,
            max: self.max_date,
            show_week_numbers: self.show_week_numbers,
            measure: self.text_measure.as_ref(),
        };

        list.begin_overlay();
        list.push_shadow(cal, Color::BLACK.with_alpha(0.15), 16.0, (0.0, 4.0), [theme.radius; 4]);
        panel::draw(list, cal, &input);
        list.end_overlay();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let theme = self.theme();
        let input = panel::PanelInput {
            state: self.state,
            theme: &theme,
            locale: &self.locale,
            selected: self.selected,
            today: self.today,
            min: self.min_date,
            max: self.max_date,
            show_week_numbers: self.show_week_numbers,
            measure: self.text_measure.as_ref(),
        };

        match event {
            Event::MouseMove(pos) => {
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Pointer);
                    return EventResult::Handled;
                }
                if self.is_open {
                    let cal = self.calendar_rect();
                    let hit = panel::hit_test(cal, &input, *pos);
                    if hit != self.state.hover {
                        self.state.hover = hit;
                        ctx.request_paint();
                    }
                    if hit.is_some() || cal.contains(*pos) {
                        ctx.set_cursor(if hit.is_some() { CursorIcon::Pointer } else { CursorIcon::Default });
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.bounds.contains(*position) {
                    if self.is_open {
                        self.close(ctx);
                    } else {
                        self.is_open = true;
                        if let Some(d) = self.selected {
                            self.state.goto(d);
                        }
                        let m = self.metrics();
                        self.opens_upward = self.bounds.y() + INPUT_HEIGHT + POPUP_GAP + m.height
                            > ctx.viewport_size().height
                            && self.bounds.y() >= m.height + POPUP_GAP;
                        let cal = self.calendar_rect();
                        let overlay_bounds = if self.opens_upward {
                            Rect::new(
                                Point::new(self.bounds.x(), cal.y()),
                                Size::new(m.width.max(self.bounds.size.width), m.height + POPUP_GAP + INPUT_HEIGHT),
                            )
                        } else {
                            Rect::new(
                                self.bounds.origin,
                                Size::new(m.width.max(self.bounds.size.width), INPUT_HEIGHT + POPUP_GAP + m.height),
                            )
                        };
                        ctx.register_overlay(overlay_bounds, false);
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.is_open {
                    let cal = self.calendar_rect();
                    if let Some(hit) = panel::hit_test(cal, &input, *position) {
                        if panel::is_hit_enabled(&input, hit) {
                            if let Some(date) = self.state.apply(hit) {
                                self.selected = Some(date);
                                self.close(ctx);
                                self.fire_change();
                            }
                            ctx.request_paint();
                        }
                        return EventResult::Handled;
                    }
                    if cal.contains(*position) {
                        return EventResult::Handled;
                    }
                    self.close(ctx);
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
    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

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
        self.vars = CalendarVars::read(style);
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
    fn apply_style(&mut self, style: &ComputedStyle) {
        self.vars = CalendarVars::read(style);
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
