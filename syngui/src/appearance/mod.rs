//! Системное оформление рабочего стола: светлая/тёмная схема и акцентный цвет.
//!
//! winit сообщает тему только на Windows/macOS/web — на Linux `Window::theme()`
//! жёстко возвращает `None` под X11 и лишь ту тему CSD, которую фреймворк сам
//! же и выставил, под Wayland. Поэтому на Linux источник правды —
//! XDG Desktop Portal (`org.freedesktop.portal.Settings`, namespace
//! `org.freedesktop.appearance`), а при его отсутствии — конфиги DE
//! (`kdeglobals` у KDE, `gsettings` у GNOME).
//!
//! Приложение обычно не вызывает этот модуль напрямую: `AppBuilder`
//! [`with_system_appearance`](crate::app::AppBuilder::with_system_appearance)
//! читает состояние до первого кадра и обновляет сигнал при каждом изменении.
//!
//! Отладочные override'ы (в стиле `SYNGUI_DOUBLE_CLICK_MS`):
//! `SYNGUI_COLOR_SCHEME=dark|light|no-preference`, `SYNGUI_ACCENT_COLOR=#RRGGBB`.

pub mod decorations;
#[cfg(target_os = "linux")]
mod desktop;
#[cfg(all(target_os = "linux", feature = "system-theme"))]
mod portal;

use crate::core::Color;

/// Предпочтения пользователя по светлоте оформления.
///
/// Совпадает по смыслу с `color-scheme` из `org.freedesktop.appearance`:
/// 0 — нет предпочтения, 1 — тёмная, 2 — светлая.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorScheme {
    /// Пользователь не выразил предпочтения — приложение решает само.
    #[default]
    NoPreference,
    Dark,
    Light,
}

impl ColorScheme {
    pub fn from_portal_u32(v: u32) -> Self {
        match v {
            1 => Self::Dark,
            2 => Self::Light,
            _ => Self::NoPreference,
        }
    }

    /// `true`, только если тёмная выбрана явно.
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }
}

/// Снимок системного оформления.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SystemAppearance {
    pub color_scheme: ColorScheme,
    /// Акцент DE. `None` — система его не сообщает (X11 без портала, Windows,
    /// macOS), тогда приложение оставляет акцент своей темы.
    pub accent: Option<Color>,
    /// `contrast` из портала: пользователь просит повышенный контраст.
    pub high_contrast: bool,
    /// `reduced-motion` из портала: анимации стоит сократить.
    pub reduced_motion: bool,
}

impl SystemAppearance {
    pub fn is_dark(&self) -> bool {
        self.color_scheme.is_dark()
    }
}

/// Синхронно читает текущее оформление. Дешёвая операция (один D-Bus-вызов
/// либо чтение файла), рассчитана на вызов при старте — до первого кадра.
pub fn read_system_appearance() -> SystemAppearance {
    if let Some(a) = env_override() {
        return a;
    }
    read_platform().unwrap_or_default()
}

#[cfg(all(target_os = "linux", feature = "system-theme"))]
fn read_platform() -> Option<SystemAppearance> {
    portal::read().or_else(desktop::read)
}

#[cfg(all(target_os = "linux", not(feature = "system-theme")))]
fn read_platform() -> Option<SystemAppearance> {
    desktop::read()
}

// Windows/macOS: light/dark приходит из winit (`WindowEvent::ThemeChanged` и
// `Window::theme()` после создания окна) — здесь читать нечего.
#[cfg(not(target_os = "linux"))]
fn read_platform() -> Option<SystemAppearance> {
    None
}

/// Ручка слежения: пока жива, `on_change` вызывается при каждом изменении
/// системного оформления. Колбэк приходит **не** из главного потока —
/// прокидывайте его через [`crate::async_runtime::run_on_main_thread`].
pub struct AppearanceWatcher {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for AppearanceWatcher {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Интервал fallback-опроса, когда портала нет и остаётся только перечитывать
/// конфиги DE.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);

/// Запускает слежение за системным оформлением в фоновом потоке.
///
/// На Linux с порталом это подписка на сигнал `SettingChanged` (реакция
/// мгновенная, поток спит на сокете). Без портала — опрос конфигов DE раз в
/// [`POLL_INTERVAL`]. Если оформление зафиксировано переменной окружения,
/// слежение не запускается вовсе.
pub fn watch_system_appearance<F>(on_change: F) -> AppearanceWatcher
where
    F: Fn(SystemAppearance) + Send + 'static,
{
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    if env_override().is_some() {
        return AppearanceWatcher { stop };
    }

    let thread_stop = stop.clone();
    let spawned = std::thread::Builder::new()
        .name("syngui-appearance".into())
        .spawn(move || {
            #[cfg(all(target_os = "linux", feature = "system-theme"))]
            {
                // Возвращает `None`, если портала нет или соединение оборвалось —
                // тогда доигрываем опросом.
                if portal::watch(&on_change, &thread_stop).is_some() {
                    return;
                }
            }
            poll_loop(&on_change, &thread_stop);
        });

    if let Err(e) = spawned {
        log::warn!("[syngui] appearance watcher not started: {e}");
    }
    AppearanceWatcher { stop }
}

fn poll_loop<F>(on_change: &F, stop: &std::sync::atomic::AtomicBool)
where
    F: Fn(SystemAppearance),
{
    let mut last = read_system_appearance();
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        std::thread::sleep(POLL_INTERVAL);
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let current = read_system_appearance();
        if current != last {
            last = current;
            on_change(current);
        }
    }
}

fn env_override() -> Option<SystemAppearance> {
    let scheme = std::env::var("SYNGUI_COLOR_SCHEME").ok();
    let accent = std::env::var("SYNGUI_ACCENT_COLOR").ok();
    if scheme.is_none() && accent.is_none() {
        return None;
    }
    let color_scheme = match scheme.as_deref().map(str::trim) {
        Some("dark") => ColorScheme::Dark,
        Some("light") => ColorScheme::Light,
        _ => ColorScheme::NoPreference,
    };
    let accent = accent
        .as_deref()
        .map(str::trim)
        .filter(|s| s.len() >= 6)
        .map(Color::from_hex);
    Some(SystemAppearance { color_scheme, accent, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_scheme_codes() {
        assert_eq!(ColorScheme::from_portal_u32(0), ColorScheme::NoPreference);
        assert_eq!(ColorScheme::from_portal_u32(1), ColorScheme::Dark);
        assert_eq!(ColorScheme::from_portal_u32(2), ColorScheme::Light);
        assert_eq!(ColorScheme::from_portal_u32(42), ColorScheme::NoPreference);
    }

    /// Диагностика на живой системе: `cargo test -p syngui --features system-theme
    /// -- --ignored --nocapture appearance::tests::dump`.
    #[test]
    #[ignore = "зависит от окружения рабочего стола"]
    fn dump() {
        println!("{:#?}", read_system_appearance());
    }

    #[test]
    fn accent_roundtrips_through_srgb() {
        // Значения KDE Breeze-акцента из портала (sRGB 0..1).
        let c = Color::from_srgb_f32(0.627451, 0.705882, 0.972549);
        assert_eq!(c.to_hex(), "#A0B4F8");
    }
}
