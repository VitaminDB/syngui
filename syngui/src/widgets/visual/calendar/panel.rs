//! Общая «панель календаря»: раскладка, отрисовка и hit-test.
//!
//! Один и тот же код рисует и виджет [`Calendar`](super::Calendar), и попап
//! [`DatePicker`](crate::widgets::input::DatePicker) — поэтому у них
//! одинаковая MSS-тема (`--cal-*`), локализация и быстрый выбор месяца/года.

use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::mss::TextAlign;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{Border, DisplayList};
use crate::widget::context::TextMeasure;
use std::sync::Arc;

use super::{CalendarLocale, Date};

// Стрелки навигации: Material Icons, если шрифт включён, иначе текстовые
// шевроны — они есть в любом основном шрифте.
#[cfg(feature = "material-icons")]
pub const ICON_PREV: &str = "\u{E5CB}";
#[cfg(feature = "material-icons")]
pub const ICON_NEXT: &str = "\u{E5CC}";
#[cfg(not(feature = "material-icons"))]
pub const ICON_PREV: &str = "\u{2039}";
#[cfg(not(feature = "material-icons"))]
pub const ICON_NEXT: &str = "\u{203A}";

/// Иконка календаря в поле ввода `DatePicker`.
#[cfg(feature = "material-icons")]
pub const ICON_CALENDAR: &str = "\u{E935}";
#[cfg(not(feature = "material-icons"))]
pub const ICON_CALENDAR: &str = "\u{25A6}";

pub(super) const PAD: f32 = 8.0;
pub(super) const CELL_GAP: f32 = 2.0;
const HEADER_H: f32 = 44.0;
const DOW_H: f32 = 28.0;
const GRID_ROWS: u32 = 6;
const NAV_SIZE: f32 = 28.0;
const DEFAULT_CELL: f32 = 36.0;
/// Наибольшая ячейка: крупный виджет рабочего стола (≈ 600×600 px).
const MAX_CELL: f32 = 160.0;

/// Масштаб шапки, строки дней недели и стрелок: от ячейки крупнее обычной
/// (36 px) — иначе большой месяц сидел под мелкой шапкой.
fn chrome_scale(cell: f32) -> f32 {
    (cell / DEFAULT_CELL).max(1.0)
}

/// Форма выделения дня (`--cal-day-shape`): выбранный, сегодня (рамка), наведение.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DayShape {
    #[default]
    Circle,
    Rounded,
    Square,
}

impl DayShape {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "circle" => Some(Self::Circle),
            "rounded" => Some(Self::Rounded),
            "square" => Some(Self::Square),
            _ => None,
        }
    }

    fn radius(self, cell: f32) -> f32 {
        match self {
            Self::Circle => cell / 2.0,
            Self::Rounded => (cell * 0.25).round(),
            Self::Square => 0.0,
        }
    }
}

/// Что показывает панель: дни месяца, быстрый выбор месяца или года.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelMode {
    #[default]
    Days,
    Months,
    Years,
}

/// Интерактивная зона панели.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelHit {
    /// Стрелка «назад»: месяц / год / страница лет — по режиму.
    Prev,
    /// Стрелка «вперёд».
    Next,
    /// Название месяца в шапке — открывает быстрый выбор месяца.
    MonthLabel,
    /// Год в шапке — открывает быстрый выбор года.
    YearLabel,
    Day(Date),
    Month(u32),
    Year(i32),
}

/// Состояние навигации панели: какой месяц показан и в каком режиме.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelState {
    pub view_year: i32,
    pub view_month: u32,
    pub mode: PanelMode,
    pub hover: Option<PanelHit>,
}

impl PanelState {
    pub fn new(year: i32, month: u32) -> Self {
        Self {
            view_year: year,
            view_month: month.clamp(1, 12),
            mode: PanelMode::Days,
            hover: None,
        }
    }

    /// Панель, открытая на текущем месяце.
    pub fn today() -> Self {
        let t = Date::today();
        Self::new(t.year, t.month)
    }

    pub fn from_date(date: Date) -> Self {
        Self::new(date.year, date.month)
    }

    /// Показать месяц указанной даты, не меняя режим.
    pub fn goto(&mut self, date: Date) {
        self.view_year = date.year;
        self.view_month = date.month.clamp(1, 12);
    }

    /// Первый год текущей страницы быстрого выбора (12 лет на страницу).
    pub fn year_page_start(&self) -> i32 {
        self.view_year.div_euclid(12) * 12
    }

    pub fn prev(&mut self) {
        match self.mode {
            PanelMode::Days => {
                if self.view_month == 1 {
                    self.view_month = 12;
                    self.view_year -= 1;
                } else {
                    self.view_month -= 1;
                }
            }
            PanelMode::Months => self.view_year -= 1,
            PanelMode::Years => self.view_year -= 12,
        }
    }

    pub fn next(&mut self) {
        match self.mode {
            PanelMode::Days => {
                if self.view_month == 12 {
                    self.view_month = 1;
                    self.view_year += 1;
                } else {
                    self.view_month += 1;
                }
            }
            PanelMode::Months => self.view_year += 1,
            PanelMode::Years => self.view_year += 12,
        }
    }

    /// Применяет клик по зоне панели. Возвращает дату, если кликнули по дню.
    pub fn apply(&mut self, hit: PanelHit) -> Option<Date> {
        match hit {
            PanelHit::Prev => {
                self.prev();
                None
            }
            PanelHit::Next => {
                self.next();
                None
            }
            PanelHit::MonthLabel => {
                self.mode = if self.mode == PanelMode::Months {
                    PanelMode::Days
                } else {
                    PanelMode::Months
                };
                None
            }
            PanelHit::YearLabel => {
                self.mode = if self.mode == PanelMode::Years {
                    PanelMode::Days
                } else {
                    PanelMode::Years
                };
                None
            }
            PanelHit::Month(m) => {
                self.view_month = m.clamp(1, 12);
                self.mode = PanelMode::Days;
                None
            }
            PanelHit::Year(y) => {
                self.view_year = y;
                self.mode = PanelMode::Months;
                None
            }
            PanelHit::Day(date) => {
                self.goto(date);
                self.mode = PanelMode::Days;
                Some(date)
            }
        }
    }
}

impl Default for PanelState {
    fn default() -> Self {
        Self::today()
    }
}

/// Цвета и размеры панели: MSS-поля виджета + переменные `--cal-*`.
#[derive(Clone, Debug, PartialEq)]
pub struct CalendarTheme {
    pub background: Color,
    pub border: Color,
    pub text: Color,
    pub accent: Color,
    /// Заголовки дней недели и номера недель.
    pub muted: Color,
    /// Суббота/воскресенье.
    pub weekend: Color,
    /// Обводка сегодняшнего дня.
    pub today: Color,
    /// Текст на выбранном дне.
    pub selected_text: Color,
    pub hover: Color,
    /// Дни вне диапазона min/max.
    pub disabled: Color,
    /// Дни соседних месяцев.
    pub outside: Color,
    pub radius: f32,
    pub font_size: f32,
    /// Размер шрифта задан явно (`--cal-font-size` или свой `font-size`):
    /// тогда он не растёт с ячейкой у `fit`/`fill_width`.
    pub font_size_explicit: bool,
    /// Семейство шрифта всего текста панели (MSS `font-family`).
    pub font_family: Option<String>,
    /// Насыщенность чисел дней и подписей (MSS `font-weight`); выбранный день и
    /// сегодня — жирнее.
    pub font_weight: u16,
    pub day_shape: DayShape,
    pub cell: f32,
}

/// Переменные `--cal-*` из MSS. Читаются и `Calendar`, и `DatePicker`,
/// поэтому одно правило стиля задаёт тему обоим.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CalendarVars {
    pub panel_bg: Option<Color>,
    pub panel_border: Option<Color>,
    pub muted: Option<Color>,
    pub weekend: Option<Color>,
    pub today: Option<Color>,
    pub selected_text: Option<Color>,
    pub hover: Option<Color>,
    pub disabled: Option<Color>,
    pub outside: Option<Color>,
    pub cell_size: Option<f32>,
    pub radius: Option<f32>,
    pub font_size: Option<f32>,
    /// Свой (не унаследованный) `font-size` элемента.
    pub own_font_size: Option<f32>,
    pub day_shape: Option<DayShape>,
}

impl CalendarVars {
    /// Достаёт `--cal-*` из вычисленного стиля элемента.
    pub fn read(style: &ComputedStyle) -> Self {
        let color = |name: &str| {
            style
                .get(name)
                .and_then(|v| v.as_color())
                .map(crate::animation::transition::mss_color_to_core)
        };
        let px = |name: &str| style.get(name).and_then(|v| v.as_px());
        Self {
            panel_bg: color("--cal-panel-bg"),
            panel_border: color("--cal-panel-border"),
            muted: color("--cal-muted-color"),
            weekend: color("--cal-weekend-color"),
            today: color("--cal-today-color"),
            selected_text: color("--cal-selected-color"),
            hover: color("--cal-hover-bg"),
            disabled: color("--cal-disabled-color"),
            outside: color("--cal-outside-color"),
            cell_size: px("--cal-cell-size"),
            radius: px("--cal-radius"),
            font_size: px("--cal-font-size"),
            own_font_size: px(crate::mss::fields::ICON_FONT_SIZE),
            day_shape: style.get("--cal-day-shape").and_then(|v| v.as_string()).and_then(DayShape::parse),
        }
    }
}

impl CalendarTheme {
    /// Собирает тему: базовые MSS-поля виджета + переопределения `--cal-*`.
    pub fn resolve(mss: &MssFields, vars: &CalendarVars) -> Self {
        let background = vars
            .panel_bg
            .or(mss.background_color)
            .unwrap_or_else(crate::theme_fallback::fallback_surface);
        let text = mss.color.unwrap_or_else(crate::theme_fallback::fallback_fg);
        let accent = mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        Self {
            background,
            border: vars
                .panel_border
                .or(mss.border_color)
                .unwrap_or_else(crate::theme_fallback::fallback_divider),
            text,
            accent,
            muted: vars.muted.unwrap_or_else(|| text.with_alpha(0.45)),
            weekend: vars.weekend.unwrap_or_else(|| text.with_alpha(0.7)),
            today: vars.today.unwrap_or(accent),
            selected_text: vars.selected_text.unwrap_or_else(|| accent.readable_on()),
            hover: vars.hover.unwrap_or_else(|| accent.with_alpha(0.14)),
            disabled: vars.disabled.unwrap_or_else(|| text.with_alpha(0.25)),
            outside: vars.outside.unwrap_or_else(|| text.with_alpha(0.32)),
            radius: vars.radius.unwrap_or(12.0),
            font_size: vars.font_size.or(mss.font_size).unwrap_or(13.0),
            font_size_explicit: vars.font_size.is_some() || vars.own_font_size.is_some(),
            font_family: mss.font_family.clone(),
            font_weight: mss.font_weight.unwrap_or(400),
            day_shape: vars.day_shape.unwrap_or_default(),
            cell: vars.cell_size.unwrap_or(DEFAULT_CELL),
        }
    }
}

impl Default for CalendarTheme {
    fn default() -> Self {
        Self::resolve(&MssFields::new(), &CalendarVars::default())
    }
}

/// Размеры панели: считаются из размера ячейки, поэтому `--cal-cell-size`
/// масштабирует календарь целиком.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelMetrics {
    pub cell: f32,
    pub week_col: f32,
    pub width: f32,
    pub height: f32,
    /// Высота шапки, строки дней недели и размер стрелок — от ячейки.
    pub header_h: f32,
    pub dow_h: f32,
    pub nav: f32,
}

impl PanelMetrics {
    pub fn new(cell: f32, show_week_numbers: bool) -> Self {
        let cell = cell.clamp(24.0, MAX_CELL);
        let k = chrome_scale(cell);
        let (header_h, dow_h, nav) = ((HEADER_H * k).round(), (DOW_H * k).round(), (NAV_SIZE * k).round());
        let week_col = if show_week_numbers {
            (cell * 0.8).round()
        } else {
            0.0
        };
        let grid_w = 7.0 * cell + 6.0 * CELL_GAP;
        Self {
            cell,
            week_col,
            width: PAD * 2.0 + week_col + grid_w,
            height: header_h + dow_h + GRID_ROWS as f32 * cell + PAD,
            header_h,
            dow_h,
            nav,
        }
    }

    /// Наибольшая ячейка, при которой панель помещается в высоту `h`
    /// (шапка растёт вместе с ячейкой крупнее обычной).
    pub fn cell_for_height(h: f32) -> f32 {
        let small = (h - HEADER_H - DOW_H - PAD) / GRID_ROWS as f32;
        if small <= DEFAULT_CELL {
            return small;
        }
        // cell ≥ 36: шапка и дни недели — (44 + 28) · cell/36
        let mut c = (h - PAD) / (GRID_ROWS as f32 + (HEADER_H + DOW_H) / DEFAULT_CELL);
        // округление высот шапки — не вылезти за `h`
        while c > DEFAULT_CELL && Self::new(c.floor(), false).height > h {
            c -= 1.0;
        }
        c.max(DEFAULT_CELL)
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    fn grid_x(&self, rect: Rect) -> f32 {
        rect.x() + PAD + self.week_col
    }

    fn body_y(&self, rect: Rect) -> f32 {
        rect.y() + self.header_h
    }

    fn grid_y(&self, rect: Rect) -> f32 {
        self.body_y(rect) + self.dow_h
    }

    fn body_height(&self) -> f32 {
        self.dow_h + GRID_ROWS as f32 * self.cell
    }

    /// Ячейка сетки дней по колонке/строке.
    fn day_cell(&self, rect: Rect, col: u32, row: u32) -> Rect {
        Rect::new(
            Point::new(
                self.grid_x(rect) + col as f32 * (self.cell + CELL_GAP),
                self.grid_y(rect) + row as f32 * self.cell,
            ),
            Size::new(self.cell, self.cell),
        )
    }

    fn grid_width(&self) -> f32 {
        7.0 * self.cell + 6.0 * CELL_GAP
    }

    /// Ячейка сетки быстрого выбора (3×4 месяца или 4×3 года).
    fn quick_cell(&self, rect: Rect, cols: u32, rows: u32, index: u32) -> Rect {
        let cw = self.grid_width() / cols as f32;
        let ch = (self.body_height() / rows as f32).min(64.0 * chrome_scale(self.cell));
        let top = self.body_y(rect) + (self.body_height() - ch * rows as f32) / 2.0;
        let (col, row) = (index % cols, index / cols);
        Rect::new(
            Point::new(self.grid_x(rect) + col as f32 * cw, top + row as f32 * ch),
            Size::new(cw, ch),
        )
    }
}

/// Всё, что нужно панели для отрисовки и hit-test.
pub struct PanelInput<'a> {
    pub state: PanelState,
    pub theme: &'a CalendarTheme,
    pub locale: &'a CalendarLocale,
    pub selected: Option<Date>,
    pub today: Date,
    pub min: Option<Date>,
    pub max: Option<Date>,
    pub show_week_numbers: bool,
    pub measure: Option<&'a Arc<dyn TextMeasure>>,
}

impl<'a> PanelInput<'a> {
    pub fn metrics(&self) -> PanelMetrics {
        PanelMetrics::new(self.theme.cell, self.show_week_numbers)
    }

    fn is_enabled(&self, date: Date) -> bool {
        self.min.map(|min| date >= min).unwrap_or(true)
            && self.max.map(|max| date <= max).unwrap_or(true)
    }

    fn text_width(&self, text: &str, font_size: f32) -> f32 {
        match self.measure {
            Some(tm) => tm.measure_text_width_styled(
                text,
                font_size,
                text.chars().count(),
                true,
                self.theme.font_family.as_deref(),
            ),
            // Кириллица/латиница в среднем ~0.55em; запас нужен только для
            // hit-зоны, поэтому грубой оценки достаточно.
            None => text.chars().count() as f32 * font_size * 0.58,
        }
    }

    /// Подписи шапки: месяц и год (в режиме лет — диапазон страницы).
    fn header_labels(&self) -> (Option<String>, String) {
        match self.state.mode {
            PanelMode::Days => (
                Some(self.locale.month_title(self.state.view_month)),
                self.state.view_year.to_string(),
            ),
            PanelMode::Months => (None, self.state.view_year.to_string()),
            PanelMode::Years => {
                let start = self.state.year_page_start();
                (None, format!("{}–{}", start, start + 11))
            }
        }
    }
}

/// Прямоугольники шапки: стрелки и кликабельные подписи месяца/года.
struct HeaderRects {
    prev: Rect,
    next: Rect,
    month: Option<Rect>,
    year: Rect,
}

fn header_rects(rect: Rect, input: &PanelInput) -> HeaderRects {
    let m = input.metrics();
    let nav = m.nav;
    let k = chrome_scale(m.cell);
    let y = rect.y() + (m.header_h - nav) / 2.0;
    let prev = Rect::new(Point::new(rect.x() + PAD, y), Size::new(nav, nav));
    let next = Rect::new(
        Point::new(rect.x() + m.width - PAD - nav, y),
        Size::new(nav, nav),
    );

    let fs = header_font(input.theme);
    let (month_label, year_label) = input.header_labels();
    let pad_x = 10.0 * k;
    let month_w = month_label
        .as_ref()
        .map(|l| input.text_width(l, fs) + pad_x * 2.0);
    let year_w = input.text_width(&year_label, fs) + pad_x * 2.0;
    let gap = if month_w.is_some() { 4.0 } else { 0.0 };
    let total = month_w.unwrap_or(0.0) + gap + year_w;

    let avail_left = prev.right() + 4.0;
    let avail_right = next.x() - 4.0;
    let mut x = (avail_left + avail_right - total) / 2.0;
    x = x.max(avail_left);

    let month = month_w.map(|w| Rect::new(Point::new(x, y), Size::new(w, nav)));
    let year_x = month.map(|r| r.right() + gap).unwrap_or(x);
    let year = Rect::new(
        Point::new(year_x, y),
        Size::new(year_w.min((avail_right - year_x).max(24.0)), nav),
    );
    HeaderRects {
        prev,
        next,
        month,
        year,
    }
}

/// Шрифт шапки (месяц, год): чуть крупнее чисел.
fn header_font(t: &CalendarTheme) -> f32 {
    (t.font_size + 1.0).max(13.0)
}

/// Насыщенность выделенного: выбранный день, сегодня, подписи шапки.
fn strong_weight(t: &CalendarTheme) -> u16 {
    (t.font_weight + 200).clamp(600, 900)
}

/// Подпись панели по центру прямоугольника — семейством и насыщенностью темы.
fn label(list: &mut DisplayList, t: &CalendarTheme, text: &str, r: Rect, color: Color, size: f32, weight: u16) {
    list.push_text_full(
        text,
        r,
        color,
        size,
        TextAlign::CENTER,
        Default::default(),
        weight,
        t.font_family.clone(),
        0.0,
        None,
        false,
    );
}

/// Дата в ячейке сетки дней (включая дни соседних месяцев).
fn grid_date(state: PanelState, locale: &CalendarLocale, index: u32) -> Date {
    let first = Date::new(state.view_year, state.view_month, 1);
    let lead = locale.column_of(first.weekday()) as i64;
    first.add_days(index as i64 - lead)
}

/// Отрисовка панели целиком: фон, шапка и тело по текущему режиму.
pub fn draw(list: &mut DisplayList, rect: Rect, input: &PanelInput) {
    let m = input.metrics();
    let t = input.theme;
    let panel = Rect::new(rect.origin, m.size());

    list.push_rect_bordered(
        panel,
        t.background,
        [t.radius; 4],
        Border::new(1.0, t.border),
    );
    draw_header(list, panel, input);

    match input.state.mode {
        PanelMode::Days => draw_days(list, panel, input, &m),
        PanelMode::Months => draw_months(list, panel, input, &m),
        PanelMode::Years => draw_years(list, panel, input, &m),
    }
}

fn draw_header(list: &mut DisplayList, rect: Rect, input: &PanelInput) {
    let t = input.theme;
    let h = header_rects(rect, input);
    let fs = header_font(t);
    let k = chrome_scale(input.metrics().cell);
    let hr = 8.0 * k;
    let hovered = |zone: PanelHit| input.state.hover == Some(zone);

    for (r, icon, zone) in [
        (h.prev, ICON_PREV, PanelHit::Prev),
        (h.next, ICON_NEXT, PanelHit::Next),
    ] {
        if hovered(zone) {
            list.push_rect(r, t.hover, [hr; 4]);
        }
        list.push_text_centered(icon, r, t.text, (18.0 * k).round());
    }

    let (month_label, year_label) = input.header_labels();
    if let (Some(label), Some(r)) = (month_label, h.month) {
        if hovered(PanelHit::MonthLabel) {
            list.push_rect(r, t.hover, [hr; 4]);
        }
        let color = if input.state.mode == PanelMode::Months {
            t.accent
        } else {
            t.text
        };
        self::label(list, t, &label, r, color, fs, strong_weight(t));
    }
    if hovered(PanelHit::YearLabel) {
        list.push_rect(h.year, t.hover, [hr; 4]);
    }
    let year_color = if input.state.mode == PanelMode::Years {
        t.accent
    } else {
        t.text
    };
    label(list, t, &year_label, h.year, year_color, fs, strong_weight(t));
}

fn draw_days(list: &mut DisplayList, rect: Rect, input: &PanelInput, m: &PanelMetrics) {
    let t = input.theme;
    let locale = input.locale;
    let state = input.state;

    if input.show_week_numbers {
        let r = Rect::new(
            Point::new(rect.x() + PAD, m.body_y(rect)),
            Size::new(m.week_col, m.dow_h),
        );
        label(list, t, locale.week_abbr.as_ref(), r, t.muted, week_font(t), t.font_weight);
    }

    for col in 0..7u32 {
        let weekday = locale.weekday_at_column(col);
        let r = Rect::new(
            Point::new(
                m.grid_x(rect) + col as f32 * (m.cell + CELL_GAP),
                m.body_y(rect),
            ),
            Size::new(m.cell, m.dow_h),
        );
        let color = if locale.is_weekend(weekday) {
            t.weekend.with_alpha(0.6)
        } else {
            t.muted
        };
        label(list, t, locale.weekday_short(weekday), r, color, dow_font(t), t.font_weight);
    }

    for index in 0..GRID_ROWS * 7 {
        let (col, row) = (index % 7, index / 7);
        let date = grid_date(state, locale, index);
        let cell = m.day_cell(rect, col, row);
        let outside = date.month != state.view_month || date.year != state.view_year;
        let enabled = input.is_enabled(date);
        let selected = input.selected == Some(date);
        let hovered = input.state.hover == Some(PanelHit::Day(date)) && enabled;
        let today = date == input.today;
        // `--cal-outside-color: transparent` — соседних дней нет совсем: ни числа,
        // ни выделения, ни наведения, ни рамки «сегодня».
        if outside && hides_outside(t) {
            continue;
        }
        let radius = t.day_shape.radius(m.cell);

        if selected {
            list.push_rect(cell, t.accent, [radius; 4]);
        } else if hovered {
            list.push_rect(cell, t.hover, [radius; 4]);
        }
        if today && !selected {
            list.push_rect_bordered(
                cell,
                Color::TRANSPARENT,
                [radius; 4],
                Border::new(1.5 * chrome_scale(m.cell), t.today),
            );
        }

        let color = if selected {
            t.selected_text
        } else if !enabled {
            t.disabled
        } else if outside {
            t.outside
        } else if today {
            t.today
        } else if locale.is_weekend(date.weekday()) {
            t.weekend
        } else {
            t.text
        };
        let weight = if today || selected { strong_weight(t) } else { t.font_weight };
        label(list, t, &date.day.to_string(), cell, color, t.font_size, weight);
    }

    if input.show_week_numbers {
        for row in 0..GRID_ROWS {
            // Номер недели считаем по любому дню строки — берём первый.
            let date = grid_date(state, locale, row * 7);
            let r = Rect::new(
                Point::new(rect.x() + PAD, m.grid_y(rect) + row as f32 * m.cell),
                Size::new(m.week_col, m.cell),
            );
            label(list, t, &date.iso_week().to_string(), r, t.muted, week_font(t), t.font_weight);
        }
    }
}

/// Дни недели — мельче чисел.
fn dow_font(t: &CalendarTheme) -> f32 {
    (t.font_size * 0.85).max(10.0)
}

/// Номера недель — ещё мельче.
fn week_font(t: &CalendarTheme) -> f32 {
    (t.font_size * 0.77).max(9.0)
}

/// Дни соседних месяцев спрятаны (`--cal-outside-color` прозрачный).
fn hides_outside(t: &CalendarTheme) -> bool {
    t.outside.a < 0.01
}

fn draw_months(list: &mut DisplayList, rect: Rect, input: &PanelInput, m: &PanelMetrics) {
    let t = input.theme;
    for i in 0..12u32 {
        let month = i + 1;
        let cell = m.quick_cell(rect, 3, 4, i);
        let inner = cell.inflate(-4.0, -4.0);
        let qr = 10.0 * chrome_scale(m.cell);
        let selected = input
            .selected
            .map(|d| d.year == input.state.view_year && d.month == month)
            .unwrap_or(false)
            || (input.selected.is_none() && month == input.state.view_month);
        let current = input.today.year == input.state.view_year && input.today.month == month;
        let hovered = input.state.hover == Some(PanelHit::Month(month));

        if selected {
            list.push_rect(inner, t.accent, [qr; 4]);
        } else if hovered {
            list.push_rect(inner, t.hover, [qr; 4]);
        }
        if current && !selected {
            list.push_rect_bordered(
                inner,
                Color::TRANSPARENT,
                [qr; 4],
                Border::new(1.5, t.today),
            );
        }
        let color = if selected {
            t.selected_text
        } else if current {
            t.today
        } else {
            t.text
        };
        let weight = if selected || current { strong_weight(t) } else { t.font_weight };
        label(list, t, &input.locale.month_short(month), inner, color, t.font_size + 1.0, weight);
    }
}

fn draw_years(list: &mut DisplayList, rect: Rect, input: &PanelInput, m: &PanelMetrics) {
    let t = input.theme;
    let start = input.state.year_page_start();
    for i in 0..12u32 {
        let year = start + i as i32;
        let cell = m.quick_cell(rect, 4, 3, i);
        let inner = cell.inflate(-4.0, -4.0);
        let qr = 10.0 * chrome_scale(m.cell);
        let selected = year == input.state.view_year;
        let current = year == input.today.year;
        let hovered = input.state.hover == Some(PanelHit::Year(year));

        if selected {
            list.push_rect(inner, t.accent, [qr; 4]);
        } else if hovered {
            list.push_rect(inner, t.hover, [qr; 4]);
        }
        if current && !selected {
            list.push_rect_bordered(
                inner,
                Color::TRANSPARENT,
                [qr; 4],
                Border::new(1.5, t.today),
            );
        }
        let color = if selected {
            t.selected_text
        } else if current {
            t.today
        } else {
            t.text
        };
        let weight = if selected || current { strong_weight(t) } else { t.font_weight };
        label(list, t, &year.to_string(), inner, color, t.font_size + 1.0, weight);
    }
}

/// Зона панели под курсором. `rect` — левый верхний угол панели.
pub fn hit_test(rect: Rect, input: &PanelInput, pos: Point) -> Option<PanelHit> {
    let m = input.metrics();
    let panel = Rect::new(rect.origin, m.size());
    if !panel.contains(pos) {
        return None;
    }

    let h = header_rects(panel, input);
    if h.prev.contains(pos) {
        return Some(PanelHit::Prev);
    }
    if h.next.contains(pos) {
        return Some(PanelHit::Next);
    }
    if let Some(r) = h.month {
        if r.contains(pos) {
            return Some(PanelHit::MonthLabel);
        }
    }
    if h.year.contains(pos) {
        return Some(PanelHit::YearLabel);
    }

    match input.state.mode {
        PanelMode::Days => {
            for index in 0..GRID_ROWS * 7 {
                let cell = m.day_cell(panel, index % 7, index / 7);
                if cell.contains(pos) {
                    let date = grid_date(input.state, input.locale, index);
                    let outside = date.month != input.state.view_month || date.year != input.state.view_year;
                    // спрятанный соседний день — пустое место
                    if outside && hides_outside(input.theme) {
                        return None;
                    }
                    return Some(PanelHit::Day(date));
                }
            }
            None
        }
        PanelMode::Months => (0..12u32)
            .find(|i| m.quick_cell(panel, 3, 4, *i).contains(pos))
            .map(|i| PanelHit::Month(i + 1)),
        PanelMode::Years => (0..12u32)
            .find(|i| m.quick_cell(panel, 4, 3, *i).contains(pos))
            .map(|i| PanelHit::Year(input.state.year_page_start() + i as i32)),
    }
}

/// Кликабельна ли зона: дни вне min/max игнорируются.
pub fn is_hit_enabled(input: &PanelInput, hit: PanelHit) -> bool {
    match hit {
        PanelHit::Day(date) => input.is_enabled(date),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(
        state: PanelState,
        locale: &'a CalendarLocale,
        theme: &'a CalendarTheme,
    ) -> PanelInput<'a> {
        PanelInput {
            state,
            theme,
            locale,
            selected: None,
            today: Date::new(2026, 8, 20),
            min: None,
            max: None,
            show_week_numbers: false,
            measure: None,
        }
    }

    #[test]
    fn grid_starts_on_locale_first_weekday() {
        let ru = CalendarLocale::russian();
        // 1 марта 2026 — воскресенье: при неделе с понедельника это 7-я колонка,
        // значит сетка начинается с 23 февраля.
        let state = PanelState::new(2026, 3);
        assert_eq!(grid_date(state, &ru, 0), Date::new(2026, 2, 23));
        assert_eq!(grid_date(state, &ru, 6), Date::new(2026, 3, 1));

        let en = CalendarLocale::english();
        assert_eq!(grid_date(state, &en, 0), Date::new(2026, 3, 1));
    }

    #[test]
    fn month_and_year_labels_are_separate_zones() {
        let locale = CalendarLocale::russian();
        let theme = CalendarTheme::default();
        let inp = input(PanelState::new(2026, 8), &locale, &theme);
        let m = inp.metrics();
        let rect = Rect::new(Point::zero(), m.size());
        let h = header_rects(rect, &inp);
        let month = h.month.expect("в режиме дней есть подпись месяца");
        assert!(month.right() <= h.year.x());
        assert_eq!(
            hit_test(rect, &inp, month.center()),
            Some(PanelHit::MonthLabel)
        );
        assert_eq!(
            hit_test(rect, &inp, h.year.center()),
            Some(PanelHit::YearLabel)
        );
        assert_eq!(hit_test(rect, &inp, h.prev.center()), Some(PanelHit::Prev));
    }

    #[test]
    fn clicking_day_returns_date() {
        let locale = CalendarLocale::russian();
        let theme = CalendarTheme::default();
        let mut state = PanelState::new(2026, 8);
        let inp = input(state, &locale, &theme);
        let m = inp.metrics();
        let rect = Rect::new(Point::zero(), m.size());
        // 20 августа 2026 — четверг: колонка 3, третья строка сетки.
        let cell = m.day_cell(rect, 3, 3);
        let hit = hit_test(rect, &inp, cell.center()).unwrap();
        assert_eq!(hit, PanelHit::Day(Date::new(2026, 8, 20)));
        assert_eq!(state.apply(hit), Some(Date::new(2026, 8, 20)));
    }

    #[test]
    fn quick_pick_navigation() {
        let mut state = PanelState::new(2026, 8);
        assert_eq!(state.apply(PanelHit::MonthLabel), None);
        assert_eq!(state.mode, PanelMode::Months);
        state.apply(PanelHit::Next);
        assert_eq!(state.view_year, 2027);
        assert_eq!(state.apply(PanelHit::Month(2)), None);
        assert_eq!(
            (state.view_year, state.view_month, state.mode),
            (2027, 2, PanelMode::Days)
        );

        state.apply(PanelHit::YearLabel);
        assert_eq!(state.mode, PanelMode::Years);
        assert_eq!(state.year_page_start(), 2016);
        state.apply(PanelHit::Prev);
        assert_eq!(state.view_year, 2015);
        state.apply(PanelHit::Year(2020));
        assert_eq!((state.view_year, state.mode), (2020, PanelMode::Months));
    }

    #[test]
    fn transparent_outside_days_are_not_hit() {
        let locale = CalendarLocale::russian();
        let mut theme = CalendarTheme::default();
        let state = PanelState::new(2026, 3);
        let rect = |inp: &PanelInput| Rect::new(Point::zero(), inp.metrics().size());
        // первая клетка — 23 февраля, соседний месяц
        let inp = input(state, &locale, &theme);
        let p = inp.metrics().day_cell(rect(&inp), 0, 0).center();
        assert_eq!(hit_test(rect(&inp), &inp, p), Some(PanelHit::Day(Date::new(2026, 2, 23))));
        theme.outside = Color::TRANSPARENT;
        let inp = input(state, &locale, &theme);
        assert_eq!(hit_test(rect(&inp), &inp, p), None, "спрятанный день не наводится и не выбирается");
        // а свой месяц — как прежде
        let p = inp.metrics().day_cell(rect(&inp), 6, 0).center();
        assert_eq!(hit_test(rect(&inp), &inp, p), Some(PanelHit::Day(Date::new(2026, 3, 1))));
    }

    #[test]
    fn big_cells_scale_header_and_fit_height() {
        let m = PanelMetrics::new(36.0, false);
        assert_eq!((m.header_h, m.dow_h, m.nav), (HEADER_H, DOW_H, NAV_SIZE), "обычный размер не меняется");
        for h in [300.0f32, 380.0, 600.0, 900.0, 1400.0] {
            let c = PanelMetrics::cell_for_height(h).floor();
            let m = PanelMetrics::new(c, false);
            assert!(m.height <= h + 0.01, "h={h}: {}", m.height);
            assert!(PanelMetrics::new(c + 2.0, false).height > h || c + 2.0 > MAX_CELL, "h={h}: ячейка {c} не наибольшая");
        }
        let big = PanelMetrics::new(80.0, false);
        assert!(big.header_h > HEADER_H * 2.0 && big.nav > NAV_SIZE * 2.0);
        assert_eq!(PanelMetrics::new(500.0, false).cell, MAX_CELL);
        assert_eq!(DayShape::parse("rounded").unwrap().radius(80.0), 20.0);
        assert_eq!(DayShape::parse("square").unwrap().radius(80.0), 0.0);
        assert_eq!(DayShape::default().radius(80.0), 40.0);
    }

    #[test]
    fn min_max_disable_days() {
        let locale = CalendarLocale::russian();
        let theme = CalendarTheme::default();
        let mut inp = input(PanelState::new(2026, 8), &locale, &theme);
        inp.min = Some(Date::new(2026, 8, 10));
        inp.max = Some(Date::new(2026, 8, 20));
        assert!(!is_hit_enabled(&inp, PanelHit::Day(Date::new(2026, 8, 9))));
        assert!(is_hit_enabled(&inp, PanelHit::Day(Date::new(2026, 8, 10))));
        assert!(is_hit_enabled(&inp, PanelHit::Day(Date::new(2026, 8, 20))));
        assert!(!is_hit_enabled(&inp, PanelHit::Day(Date::new(2026, 8, 21))));
    }
}
