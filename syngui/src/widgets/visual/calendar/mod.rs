//! Календарь: сетка месяца с локализацией и быстрым выбором месяца/года.
//!
//! ```no_run
//! use syngui::prelude::*;
//! use syngui::widgets::{Calendar, Date};
//!
//! // По умолчанию — русская локаль, текущий месяц, сегодняшняя дата выбрана.
//! Calendar::new().on_select(|d: Date| println!("{}", d.format()));
//! ```
//!
//! Тема задаётся через MSS (`Calendar { ... }` + переменные `--cal-*`) и
//! общая с попапом `DatePicker` — оба рисуются одной и той же панелью.

mod date;
mod locale;
pub mod panel;

pub use date::{civil_from_days, days_from_civil, Date};
pub use locale::{default_locale, set_default_locale, CalendarLocale, DateOrder, LocaleStr};
pub use panel::{CalendarTheme, CalendarVars, PanelHit, PanelMetrics, PanelMode, PanelState};

use crate::core::{Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt, TextMeasure};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::core::sync::Mutex;
use std::any::Any;
use std::sync::Arc;

/// Месячная сетка с выбором даты.
///
/// Без `.selected(...)` открывается на текущем месяце с выбранной сегодняшней
/// датой; сегодняшний день всегда обведён рамкой `--cal-today-color`.
pub struct Calendar {
    selected: Option<Date>,
    on_select: Option<Arc<Mutex<dyn FnMut(Date) + Send>>>,
    show_week_numbers: bool,
    min_date: Option<Date>,
    max_date: Option<Date>,
    locale: Option<CalendarLocale>,
}

impl Calendar {
    pub fn new() -> Self {
        Self {
            selected: Some(Date::today()),
            on_select: None,
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

    /// Открыть календарь без выделенной даты (сегодня всё равно отмечено).
    pub fn no_selection(mut self) -> Self {
        self.selected = None;
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

    /// Язык календаря. Без вызова берётся [`default_locale`] (русский).
    pub fn locale(mut self, locale: CalendarLocale) -> Self {
        self.locale = Some(locale);
        self
    }
}

impl Default for Calendar {
    fn default() -> Self { Self::new() }
}

impl Widget for Calendar {
    fn create_element(&self) -> Box<dyn Element> {
        let state = self.selected.map(PanelState::from_date).unwrap_or_else(PanelState::today);
        Box::new(CalendarElement {
            id: ElementId::new(),
            selected: self.selected,
            widget_selected: self.selected,
            on_select: self.on_select.clone(),
            show_week_numbers: self.show_week_numbers,
            min_date: self.min_date,
            max_date: self.max_date,
            locale: self.locale.clone().unwrap_or_else(default_locale),
            state,
            today: Date::today(),
            vars: CalendarVars::default(),
            text_measure: None,
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

pub struct CalendarElement {
    id: ElementId,
    selected: Option<Date>,
    /// Последнее значение, пришедшее из виджета: нужно, чтобы перестройка
    /// дерева не сбрасывала навигацию пользователя.
    widget_selected: Option<Date>,
    on_select: Option<Arc<Mutex<dyn FnMut(Date) + Send>>>,
    show_week_numbers: bool,
    min_date: Option<Date>,
    max_date: Option<Date>,
    locale: CalendarLocale,
    state: PanelState,
    today: Date,
    vars: CalendarVars,
    text_measure: Option<Arc<dyn TextMeasure>>,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl CalendarElement {
    fn theme(&self) -> CalendarTheme {
        CalendarTheme::resolve(&self.mss, &self.vars)
    }

    fn fire_select(&self, date: Date) {
        if let Some(ref cb) = self.on_select {
            if let Ok(mut f) = cb.lock() { f(date); }
        }
    }
}

impl Element for CalendarElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(cal) = widget.as_any().downcast_ref::<Calendar>() {
            self.on_select = cal.on_select.clone();
            self.show_week_numbers = cal.show_week_numbers;
            self.min_date = cal.min_date;
            self.max_date = cal.max_date;
            self.locale = cal.locale.clone().unwrap_or_else(default_locale);
            self.today = Date::today();
            if cal.selected != self.widget_selected {
                self.widget_selected = cal.selected;
                self.selected = cal.selected;
                if let Some(d) = cal.selected {
                    self.state.goto(d);
                }
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let metrics = PanelMetrics::new(self.theme().cell, self.show_week_numbers);
        let w = metrics.width.min(constraints.max_width);
        let h = metrics.height.min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
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
        panel::draw(list, self.bounds, &input);
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
                if !self.bounds.contains(*pos) {
                    if self.state.hover.take().is_some() {
                        ctx.request_paint();
                    }
                    return EventResult::Ignored;
                }
                let hit = panel::hit_test(self.bounds, &input, *pos);
                if hit != self.state.hover {
                    self.state.hover = hit;
                    ctx.request_paint();
                }
                ctx.set_cursor(if hit.is_some() { CursorIcon::Pointer } else { CursorIcon::Default });
                EventResult::Handled
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if !self.bounds.contains(*position) { return EventResult::Ignored; }
                if let Some(hit) = panel::hit_test(self.bounds, &input, *position) {
                    if panel::is_hit_enabled(&input, hit) {
                        if let Some(date) = self.state.apply(hit) {
                            self.selected = Some(date);
                            self.fire_select(date);
                        }
                        self.state.hover = Some(hit);
                        ctx.request_paint();
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
    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

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
        self.vars = CalendarVars::read(style);
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

impl StyledElement for CalendarElement {
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
