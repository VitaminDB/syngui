//! Гражданская дата без часового пояса + получение «сегодня» от ОС.
//!
//! Алгоритмы `days_from_civil`/`civil_from_days` — классика Говарда Хиннанта
//! (proleptic gregorian, эпоха 1970-01-01). Локальная дата берётся у платформы:
//! `localtime_r` на unix, `GetLocalTime` на Windows, `js_sys::Date` на wasm.
//! Если платформа неизвестна — падаем на UTC.

/// Календарная дата: год, месяц (1..=12), день (1..=31).
///
/// Порядок полей задаёт хронологический `Ord` — сравнивать даты можно
/// напрямую (`min_date..=max_date`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }

    /// Сегодняшняя дата в локальной зоне пользователя.
    pub fn today() -> Self {
        let (y, m, d) = platform::local_ymd();
        Self::new(y, m, d)
    }

    /// Сегодняшняя дата в UTC (без обращения к часовому поясу ОС).
    pub fn today_utc() -> Self {
        let (y, m, d) = platform::utc_ymd();
        Self::new(y, m, d)
    }

    /// ISO-8601: `2026-08-20`. Формат, независимый от локали.
    pub fn format(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Дата в порядке и с разделителем локали: `20.08.2026`.
    pub fn format_localized(&self, locale: &super::CalendarLocale) -> String {
        locale.format_date(self)
    }

    /// Дата словами: `20 августа 2026`.
    pub fn format_long(&self, locale: &super::CalendarLocale) -> String {
        locale.format_long(self)
    }

    /// День недели, 0 = понедельник … 6 = воскресенье.
    pub fn weekday(&self) -> u32 {
        ((days_from_civil(self.year, self.month, self.day) + 3).rem_euclid(7)) as u32
    }

    /// Порядковый день в году, 1..=366.
    pub fn day_of_year(&self) -> u32 {
        let mut n = self.day;
        for m in 1..self.month {
            n += Self::days_in_month(self.year, m);
        }
        n
    }

    /// Номер недели по ISO-8601 (неделя с понедельника, 1..=53).
    pub fn iso_week(&self) -> u32 {
        let ord = self.day_of_year() as i32;
        let wd = self.weekday() as i32 + 1; // 1 = Пн … 7 = Вс
        let week = (ord - wd + 10) / 7;
        if week < 1 {
            iso_weeks_in_year(self.year - 1)
        } else if week > iso_weeks_in_year(self.year) as i32 {
            1
        } else {
            week as u32
        }
    }

    /// Дата, сдвинутая на `days` дней (может быть отрицательным).
    pub fn add_days(&self, days: i64) -> Self {
        let (y, m, d) = civil_from_days(days_from_civil(self.year, self.month, self.day) + days);
        Self::new(y, m, d)
    }

    /// Первое число того же месяца.
    pub fn first_of_month(&self) -> Self {
        Self::new(self.year, self.month, 1)
    }

    /// Дней в месяце с учётом високосного года.
    pub fn days_in_month(year: i32, month: u32) -> u32 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => if Self::is_leap_year(year) { 29 } else { 28 },
            _ => 30,
        }
    }

    pub fn is_leap_year(year: i32) -> bool {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }

    /// День недели первого числа месяца, 0 = понедельник.
    pub fn first_weekday_of_month(year: i32, month: u32) -> u32 {
        Self::new(year, month, 1).weekday()
    }

    /// Валидна ли дата (день не выходит за границы месяца).
    pub fn is_valid(&self) -> bool {
        (1..=12).contains(&self.month)
            && self.day >= 1
            && self.day <= Self::days_in_month(self.year, self.month)
    }

    /// Подрезает день до последнего дня месяца — нужно при смене месяца/года
    /// в быстром выборе (31 марта → 30 апреля).
    pub fn clamp_day(year: i32, month: u32, day: u32) -> Self {
        let max = Self::days_in_month(year, month);
        Self::new(year, month, day.min(max))
    }
}

impl Default for Date {
    fn default() -> Self { Self::today() }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format())
    }
}

fn iso_weeks_in_year(year: i32) -> u32 {
    let p = |y: i32| (y + y / 4 - y / 100 + y / 400).rem_euclid(7);
    if p(year) == 4 || p(year - 1) == 3 { 53 } else { 52 }
}

/// Дней от 1970-01-01 (может быть отрицательным).
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let m = m.clamp(1, 12) as i64;
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Обратное к [`days_from_civil`].
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

mod platform {
    use super::civil_from_days;

    /// Год/месяц/день по UTC — работает везде, используется как fallback.
    pub fn utc_ymd() -> (i32, u32, u32) {
        let secs = web_time::SystemTime::now()
            .duration_since(web_time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
        (y, m, d)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn local_ymd() -> (i32, u32, u32) {
        let d = js_sys::Date::new_0();
        let (y, m, day) = (d.get_full_year() as i32, d.get_month() as u32 + 1, d.get_date() as u32);
        if (1..=12).contains(&m) && day >= 1 { (y, m, day) } else { utc_ymd() }
    }

    #[cfg(all(not(target_arch = "wasm32"), unix))]
    pub fn local_ymd() -> (i32, u32, u32) {
        unsafe {
            let t = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&t, &mut tm).is_null() {
                return utc_ymd();
            }
            (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32)
        }
    }

    // kernel32 линкуется стандартной библиотекой — отдельный крейт не нужен.
    #[cfg(all(not(target_arch = "wasm32"), windows))]
    pub fn local_ymd() -> (i32, u32, u32) {
        #[repr(C)]
        struct SystemTime {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetLocalTime(lp: *mut SystemTime);
        }
        unsafe {
            let mut st: SystemTime = std::mem::zeroed();
            GetLocalTime(&mut st);
            if st.month == 0 || st.day == 0 {
                return utc_ymd();
            }
            (st.year as i32, st.month as u32, st.day as u32)
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), not(unix), not(windows)))]
    pub fn local_ymd() -> (i32, u32, u32) {
        utc_ymd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_civil_days() {
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2026, 8, 20), (1899, 12, 31)] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
    }

    #[test]
    fn weekday_is_monday_based() {
        // 2026-08-20 — четверг, 1970-01-01 — четверг, 2026-03-01 — воскресенье.
        assert_eq!(Date::new(2026, 8, 20).weekday(), 3);
        assert_eq!(Date::new(1970, 1, 1).weekday(), 3);
        assert_eq!(Date::new(2026, 3, 1).weekday(), 6);
    }

    #[test]
    fn iso_weeks() {
        assert_eq!(Date::new(2026, 1, 1).iso_week(), 1);
        assert_eq!(Date::new(2026, 12, 31).iso_week(), 53);
        assert_eq!(Date::new(2027, 1, 1).iso_week(), 53);
        assert_eq!(Date::new(2026, 8, 20).iso_week(), 34);
    }

    #[test]
    fn month_lengths() {
        assert_eq!(Date::days_in_month(2024, 2), 29);
        assert_eq!(Date::days_in_month(2026, 2), 28);
        assert_eq!(Date::days_in_month(2000, 2), 29);
        assert_eq!(Date::days_in_month(1900, 2), 28);
    }

    #[test]
    fn add_days_crosses_month() {
        assert_eq!(Date::new(2026, 1, 31).add_days(1), Date::new(2026, 2, 1));
        assert_eq!(Date::new(2026, 1, 1).add_days(-1), Date::new(2025, 12, 31));
    }

    #[test]
    fn today_is_sane() {
        let t = Date::today();
        assert!(t.is_valid(), "today() вернул невалидную дату: {t:?}");
        assert!(t.year >= 2020 && t.year < 2200);
    }
}
