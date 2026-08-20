//! Локализация календаря: названия месяцев/дней, первый день недели,
//! выходные, формат даты.
//!
//! Язык по умолчанию — русский. Меняется глобально
//! ([`set_default_locale`]) или точечно на виджете (`.locale(...)`).
//!
//! ```no_run
//! use syngui::widgets::visual::calendar::{CalendarLocale, set_default_locale};
//!
//! set_default_locale(CalendarLocale::english());     // весь UI на английском
//! let mut custom = CalendarLocale::russian();        // или свой вариант
//! custom.first_weekday = 6;                          // неделя с воскресенья
//! ```

use super::Date;
use crate::core::sync::Mutex;
use std::borrow::Cow;
use std::sync::OnceLock;

/// Строка локали: статическая для пресетов, `String` для пользовательских.
pub type LocaleStr = Cow<'static, str>;

/// Порядок компонентов в числовом формате даты.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateOrder {
    /// 20.08.2026
    DayMonthYear,
    /// 08/20/2026
    MonthDayYear,
    /// 2026-08-20
    YearMonthDay,
}

/// Набор строк и правил календаря для одного языка.
#[derive(Clone, Debug, PartialEq)]
pub struct CalendarLocale {
    /// Код языка: `"ru"`, `"en"`, …
    pub id: LocaleStr,
    /// Именительный падеж: «Август» — для шапки календаря.
    pub months: [LocaleStr; 12],
    /// Родительный падеж: «августа» — для даты словами.
    pub months_genitive: [LocaleStr; 12],
    /// Сокращения для быстрого выбора месяца: «авг».
    pub months_short: [LocaleStr; 12],
    /// Дни недели, всегда в порядке Пн…Вс независимо от [`Self::first_weekday`].
    pub weekdays_short: [LocaleStr; 7],
    /// С какого дня начинается неделя: 0 = понедельник … 6 = воскресенье.
    pub first_weekday: u32,
    /// Выходные дни, индексы Пн…Вс.
    pub weekend: [bool; 7],
    /// Порядок компонентов в числовой дате.
    pub date_order: DateOrder,
    /// Разделитель числовой даты: `"."`, `"/"`, `"-"`.
    pub date_separator: LocaleStr,
    /// Подсказка формата ввода: «дд.мм.гггг».
    pub placeholder: LocaleStr,
    /// Заголовок колонки номеров недель: «Нед».
    pub week_abbr: LocaleStr,
    /// Подпись кнопки «сегодня».
    pub today_label: LocaleStr,
    /// Шаблон даты словами: `{d}`, `{month}`, `{y}`.
    pub long_pattern: LocaleStr,
}

impl CalendarLocale {
    /// Русский (по умолчанию): неделя с понедельника, `дд.мм.гггг`.
    pub fn russian() -> Self {
        Self {
            id: Cow::Borrowed("ru"),
            months: strs12([
                "Январь", "Февраль", "Март", "Апрель", "Май", "Июнь",
                "Июль", "Август", "Сентябрь", "Октябрь", "Ноябрь", "Декабрь",
            ]),
            months_genitive: strs12([
                "января", "февраля", "марта", "апреля", "мая", "июня",
                "июля", "августа", "сентября", "октября", "ноября", "декабря",
            ]),
            months_short: strs12([
                "янв", "фев", "мар", "апр", "май", "июн",
                "июл", "авг", "сен", "окт", "ноя", "дек",
            ]),
            weekdays_short: strs7(["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"]),
            first_weekday: 0,
            weekend: [false, false, false, false, false, true, true],
            date_order: DateOrder::DayMonthYear,
            date_separator: Cow::Borrowed("."),
            placeholder: Cow::Borrowed("дд.мм.гггг"),
            week_abbr: Cow::Borrowed("Нед"),
            today_label: Cow::Borrowed("Сегодня"),
            long_pattern: Cow::Borrowed("{d} {month} {y}"),
        }
    }

    /// Английский (US): неделя с воскресенья, `mm/dd/yyyy`.
    pub fn english() -> Self {
        Self {
            id: Cow::Borrowed("en"),
            months: strs12([
                "January", "February", "March", "April", "May", "June",
                "July", "August", "September", "October", "November", "December",
            ]),
            months_genitive: strs12([
                "January", "February", "March", "April", "May", "June",
                "July", "August", "September", "October", "November", "December",
            ]),
            months_short: strs12([
                "Jan", "Feb", "Mar", "Apr", "May", "Jun",
                "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ]),
            weekdays_short: strs7(["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]),
            first_weekday: 6,
            weekend: [false, false, false, false, false, true, true],
            date_order: DateOrder::MonthDayYear,
            date_separator: Cow::Borrowed("/"),
            placeholder: Cow::Borrowed("mm/dd/yyyy"),
            week_abbr: Cow::Borrowed("Wk"),
            today_label: Cow::Borrowed("Today"),
            long_pattern: Cow::Borrowed("{month} {d}, {y}"),
        }
    }

    /// Немецкий: неделя с понедельника, `tt.mm.jjjj`.
    pub fn german() -> Self {
        Self {
            id: Cow::Borrowed("de"),
            months: strs12([
                "Januar", "Februar", "März", "April", "Mai", "Juni",
                "Juli", "August", "September", "Oktober", "November", "Dezember",
            ]),
            months_genitive: strs12([
                "Januar", "Februar", "März", "April", "Mai", "Juni",
                "Juli", "August", "September", "Oktober", "November", "Dezember",
            ]),
            months_short: strs12([
                "Jan", "Feb", "Mär", "Apr", "Mai", "Jun",
                "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
            ]),
            weekdays_short: strs7(["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"]),
            first_weekday: 0,
            weekend: [false, false, false, false, false, true, true],
            date_order: DateOrder::DayMonthYear,
            date_separator: Cow::Borrowed("."),
            placeholder: Cow::Borrowed("tt.mm.jjjj"),
            week_abbr: Cow::Borrowed("KW"),
            today_label: Cow::Borrowed("Heute"),
            long_pattern: Cow::Borrowed("{d}. {month} {y}"),
        }
    }

    /// Французский: неделя с понедельника, `jj/mm/aaaa`.
    pub fn french() -> Self {
        Self {
            id: Cow::Borrowed("fr"),
            months: strs12([
                "janvier", "février", "mars", "avril", "mai", "juin",
                "juillet", "août", "septembre", "octobre", "novembre", "décembre",
            ]),
            months_genitive: strs12([
                "janvier", "février", "mars", "avril", "mai", "juin",
                "juillet", "août", "septembre", "octobre", "novembre", "décembre",
            ]),
            months_short: strs12([
                "janv", "févr", "mars", "avr", "mai", "juin",
                "juil", "août", "sept", "oct", "nov", "déc",
            ]),
            weekdays_short: strs7(["lun", "mar", "mer", "jeu", "ven", "sam", "dim"]),
            first_weekday: 0,
            weekend: [false, false, false, false, false, true, true],
            date_order: DateOrder::DayMonthYear,
            date_separator: Cow::Borrowed("/"),
            placeholder: Cow::Borrowed("jj/mm/aaaa"),
            week_abbr: Cow::Borrowed("sem"),
            today_label: Cow::Borrowed("Aujourd'hui"),
            long_pattern: Cow::Borrowed("{d} {month} {y}"),
        }
    }

    /// Испанский: неделя с понедельника, `dd/mm/aaaa`.
    pub fn spanish() -> Self {
        Self {
            id: Cow::Borrowed("es"),
            months: strs12([
                "enero", "febrero", "marzo", "abril", "mayo", "junio",
                "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre",
            ]),
            months_genitive: strs12([
                "enero", "febrero", "marzo", "abril", "mayo", "junio",
                "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre",
            ]),
            months_short: strs12([
                "ene", "feb", "mar", "abr", "may", "jun",
                "jul", "ago", "sep", "oct", "nov", "dic",
            ]),
            weekdays_short: strs7(["lun", "mar", "mié", "jue", "vie", "sáb", "dom"]),
            first_weekday: 0,
            weekend: [false, false, false, false, false, true, true],
            date_order: DateOrder::DayMonthYear,
            date_separator: Cow::Borrowed("/"),
            placeholder: Cow::Borrowed("dd/mm/aaaa"),
            week_abbr: Cow::Borrowed("sem"),
            today_label: Cow::Borrowed("Hoy"),
            long_pattern: Cow::Borrowed("{d} de {month} de {y}"),
        }
    }

    /// Пресет по коду языка: `"ru"`, `"en"`, `"de"`, `"fr"`, `"es"`.
    /// Регистр и региональный суффикс (`ru_RU.UTF-8`) игнорируются.
    pub fn from_id(id: &str) -> Option<Self> {
        let lang = id
            .split(['_', '-', '.'])
            .next()
            .unwrap_or(id)
            .to_ascii_lowercase();
        match lang.as_str() {
            "ru" => Some(Self::russian()),
            "en" => Some(Self::english()),
            "de" => Some(Self::german()),
            "fr" => Some(Self::french()),
            "es" => Some(Self::spanish()),
            _ => None,
        }
    }

    /// Язык системы из `LC_ALL`/`LC_TIME`/`LANG`. `None`, если не распознан.
    pub fn detect() -> Option<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            for var in ["LC_ALL", "LC_TIME", "LANG", "LANGUAGE"] {
                if let Ok(v) = std::env::var(var) {
                    if let Some(l) = Self::from_id(&v) {
                        return Some(l);
                    }
                }
            }
        }
        None
    }

    /// Название месяца (1..=12) с заглавной буквы — для шапки календаря.
    pub fn month_title(&self, month: u32) -> String {
        capitalize(self.months[month_idx(month)].as_ref())
    }

    /// Название месяца как есть.
    pub fn month_name(&self, month: u32) -> &str {
        self.months[month_idx(month)].as_ref()
    }

    /// Сокращение месяца с заглавной буквы — для сетки быстрого выбора.
    pub fn month_short(&self, month: u32) -> String {
        capitalize(self.months_short[month_idx(month)].as_ref())
    }

    /// День недели по индексу Пн…Вс (0..=6).
    pub fn weekday_short(&self, weekday: u32) -> &str {
        self.weekdays_short[(weekday % 7) as usize].as_ref()
    }

    /// Выходной ли день недели (индекс Пн…Вс).
    pub fn is_weekend(&self, weekday: u32) -> bool {
        self.weekend[(weekday % 7) as usize]
    }

    /// Порядковый номер колонки для дня недели с учётом первого дня недели.
    pub fn column_of(&self, weekday: u32) -> u32 {
        (weekday + 7 - self.first_weekday % 7) % 7
    }

    /// День недели, стоящий в колонке `column`.
    pub fn weekday_at_column(&self, column: u32) -> u32 {
        (self.first_weekday % 7 + column) % 7
    }

    /// Числовая дата в порядке локали: `20.08.2026`.
    pub fn format_date(&self, date: &Date) -> String {
        let sep = self.date_separator.as_ref();
        let (d, m, y) = (date.day, date.month, date.year);
        match self.date_order {
            DateOrder::DayMonthYear => format!("{d:02}{sep}{m:02}{sep}{y:04}"),
            DateOrder::MonthDayYear => format!("{m:02}{sep}{d:02}{sep}{y:04}"),
            DateOrder::YearMonthDay => format!("{y:04}{sep}{m:02}{sep}{d:02}"),
        }
    }

    /// Дата словами по шаблону локали: `20 августа 2026`.
    pub fn format_long(&self, date: &Date) -> String {
        self.long_pattern
            .replace("{d}", &date.day.to_string())
            .replace("{month}", self.months_genitive[month_idx(date.month)].as_ref())
            .replace("{y}", &date.year.to_string())
    }

    /// Шапка календаря в режиме дней: `Август 2026`.
    pub fn month_year_title(&self, year: i32, month: u32) -> String {
        format!("{} {}", self.month_title(month), year)
    }
}

impl Default for CalendarLocale {
    fn default() -> Self { default_locale() }
}

fn month_idx(month: u32) -> usize {
    (month.clamp(1, 12) - 1) as usize
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn strs12(v: [&'static str; 12]) -> [LocaleStr; 12] {
    v.map(Cow::Borrowed)
}

fn strs7(v: [&'static str; 7]) -> [LocaleStr; 7] {
    v.map(Cow::Borrowed)
}

static DEFAULT_LOCALE: OnceLock<Mutex<CalendarLocale>> = OnceLock::new();

fn slot() -> &'static Mutex<CalendarLocale> {
    DEFAULT_LOCALE.get_or_init(|| Mutex::new(CalendarLocale::russian()))
}

/// Локаль, которую берут `Calendar`/`DatePicker` без явного `.locale(...)`.
/// Изначально русская.
pub fn default_locale() -> CalendarLocale {
    slot()
        .lock()
        .map(|l| l.clone())
        .unwrap_or_else(|_| CalendarLocale::russian())
}

/// Меняет язык всех календарей приложения. Уже созданные элементы подхватят
/// её при следующей перестройке дерева.
pub fn set_default_locale(locale: CalendarLocale) {
    if let Ok(mut slot) = slot().lock() {
        *slot = locale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ru_is_default() {
        assert_eq!(default_locale().id, "ru");
        assert_eq!(default_locale().month_title(8), "Август");
    }

    #[test]
    fn formats_follow_locale() {
        let d = Date::new(2026, 8, 20);
        assert_eq!(CalendarLocale::russian().format_date(&d), "20.08.2026");
        assert_eq!(CalendarLocale::english().format_date(&d), "08/20/2026");
        assert_eq!(CalendarLocale::russian().format_long(&d), "20 августа 2026");
        assert_eq!(CalendarLocale::english().format_long(&d), "August 20, 2026");
        assert_eq!(CalendarLocale::german().format_long(&d), "20. August 2026");
        assert_eq!(CalendarLocale::spanish().format_long(&d), "20 de agosto de 2026");
    }

    #[test]
    fn columns_respect_first_weekday() {
        let ru = CalendarLocale::russian();
        assert_eq!(ru.column_of(0), 0); // Пн — первая колонка
        assert_eq!(ru.column_of(6), 6); // Вс — последняя
        let en = CalendarLocale::english();
        assert_eq!(en.column_of(6), 0); // Вс — первая колонка
        assert_eq!(en.column_of(0), 1);
        assert_eq!(en.weekday_at_column(0), 6);
    }

    #[test]
    fn from_id_handles_posix_locales() {
        assert_eq!(CalendarLocale::from_id("ru_RU.UTF-8").unwrap().id, "ru");
        assert_eq!(CalendarLocale::from_id("en-GB").unwrap().id, "en");
        assert!(CalendarLocale::from_id("ja").is_none());
    }
}
