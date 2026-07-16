//! Резолвинг интервала двойного клика из системной настройки.
//!
//! winit не отдаёт готовое OS-событие «двойной клик» ни на одной платформе —
//! только сырые `Pressed`/`Released` кнопки мыши. Поэтому сам двойной клик
//! синтезирует фреймворк (см. `app/event_handling.rs`), ровно как это делают
//! GTK/Qt. «Системным» здесь является **порог**: его берём из настройки ОС/DE
//! в рантайме, а не хардкодим константой.
//!
//! Приоритет источника: env-override → настройка ОС/DE → дефолт.

use std::time::Duration;

/// Разрешить интервал двойного клика в рантайме (один раз на старте окна).
///
/// Порядок:
/// 1. env `SYNGUI_DOUBLE_CLICK_MS` — универсальный override без зависимостей;
/// 2. настройка ОС/DE:
///    - Windows: `GetDoubleClickTime()` (Панель управления → Мышь);
///    - Linux: KDE `~/.config/kdeglobals [KDE] DoubleClickInterval`, затем
///      GNOME `gsettings … double-click`;
/// 3. [`crate::input::DOUBLE_CLICK_INTERVAL`] как дефолт.
pub fn resolve_double_click_interval() -> Duration {
    if let Some(ms) = env_override_ms() {
        return Duration::from_millis(ms);
    }
    if let Some(ms) = os_double_click_ms() {
        if ms > 0 {
            return Duration::from_millis(ms);
        }
    }
    crate::input::DOUBLE_CLICK_INTERVAL
}

fn env_override_ms() -> Option<u64> {
    let v = std::env::var("SYNGUI_DOUBLE_CLICK_MS").ok()?;
    let ms = v.trim().parse::<u64>().ok()?;
    (ms > 0).then_some(ms)
}

#[cfg(target_os = "windows")]
fn os_double_click_ms() -> Option<u64> {
    // user32 линкуется всегда; значение отражает системную «скорость
    // двойного щелчка».
    extern "system" {
        fn GetDoubleClickTime() -> u32;
    }
    let ms = unsafe { GetDoubleClickTime() };
    (ms > 0).then_some(ms as u64)
}

#[cfg(target_os = "linux")]
fn os_double_click_ms() -> Option<u64> {
    kde_kdeglobals_ms().or_else(gnome_gsettings_ms)
}

/// KDE: `$XDG_CONFIG_HOME/kdeglobals` (или `~/.config/kdeglobals`),
/// секция `[KDE]`, ключ `DoubleClickInterval` (мс).
#[cfg(target_os = "linux")]
fn kde_kdeglobals_ms() -> Option<u64> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    let text = std::fs::read_to_string(base.join("kdeglobals")).ok()?;
    let mut in_kde = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_kde = line == "[KDE]";
            continue;
        }
        if in_kde {
            if let Some(v) = line.strip_prefix("DoubleClickInterval=") {
                if let Ok(ms) = v.trim().parse::<u64>() {
                    return Some(ms);
                }
            }
        }
    }
    None
}

/// GNOME хранит настройку в dconf (бинарь), поэтому читаем через `gsettings`
/// — разовый запуск на старте. Отсутствие бинаря / ошибка → `None`.
#[cfg(target_os = "linux")]
fn gnome_gsettings_ms() -> Option<u64> {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.peripherals.mouse", "double-click"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.trim().parse::<u64>().ok()
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn os_double_click_ms() -> Option<u64> {
    None
}
