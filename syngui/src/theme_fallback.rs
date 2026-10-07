//! Запасные цвета виджетов, зависящие от текущей темы.
//!
//! Виджеты рисуют цвета из MSS (`background`, `color`, `border-color`, `accent-color`). Когда приложение
//! свойство не задало, нужен запасной цвет — раньше он был захардкожен «светлым» (белый фон, рамка
//! `#D1D5DB`, текст `#374151`), и в тёмных темах, где задают только `color` и `accent-color`, а фон —
//! на контейнерах, виджеты выглядели белыми чужеродными пятнами.
//!
//! Этот модуль — единый источник таких запасных цветов. Признак «тёмная тема» выставляет [`App`](crate::app):
//! при старте из `with_theme_styles` (сигнал темы), иначе по яркости `.background(...)`, иначе из
//! системного оформления ([`crate::appearance::read_system_appearance`]); при смене темы — заново.
//! Приложение может выставить его и само через [`set_dark_theme`].
//!
//! Семантика функций:
//! - [`fallback_surface`] — непрозрачная поверхность всплывашек, меню, диалогов, полей, карточек;
//! - [`fallback_bg`] — фон окна/подложки;
//! - [`fallback_fg`] / [`fallback_fg_strong`] — основной текст / заголовки;
//! - [`fallback_muted`] — второстепенный текст;
//! - [`fallback_border`] / [`fallback_divider`] — рамка поля / тонкий разделитель;
//! - [`fallback_hover`] — полупрозрачная заливка при наведении и нейтральных «плашек» (поверх любого фона);
//! - [`fallback_track`] — полупрозрачная дорожка индикаторов (прогресс, слайдер, выключенный тумблер).
//!
//! Акцент (`#3B82F6`) от темы не зависит и здесь не задаётся.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::Color;

static DARK: AtomicBool = AtomicBool::new(false);

/// Выставить признак тёмной темы. Вызывает `App`; приложение может вызвать само,
/// если управляет темой в обход `with_theme_styles`.
pub fn set_dark_theme(dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
}

/// Текущий признак тёмной темы.
pub fn is_dark_theme() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Тёмный ли цвет фона — по относительной яркости (порог как у `SystemPalette::is_dark`).
pub fn color_is_dark(bg: Color) -> bool {
    bg.relative_luminance() < 0.18
}

/// Выставить признак тёмной темы по яркости фона окна (запасной признак,
/// когда сигнала темы нет).
pub fn set_dark_theme_from_background(bg: Color) {
    set_dark_theme(color_is_dark(bg));
}

#[inline]
fn pick(light: &str, dark: &str) -> Color {
    Color::from_hex(if is_dark_theme() { dark } else { light })
}

/// Непрозрачная поверхность: всплывающие списки, меню, диалоги, подсказки, карточки, поля ввода.
pub fn fallback_surface() -> Color {
    pick("#FFFFFF", "#262A33")
}

/// Фон окна/подложки.
pub fn fallback_bg() -> Color {
    pick("#F9FAFB", "#1E2128")
}

/// Основной текст.
pub fn fallback_fg() -> Color {
    pick("#374151", "#E8EAEF")
}

/// Заголовки и акцентированный текст.
pub fn fallback_fg_strong() -> Color {
    pick("#111827", "#F5F6F8")
}

/// Второстепенный текст (подписи, подсказки-плейсхолдеры, неактивные элементы).
pub fn fallback_muted() -> Color {
    pick("#6B7280", "#9CA3AF")
}

/// Рамка поля/контрола.
pub fn fallback_border() -> Color {
    pick("#D1D5DB", "#3A3F4B")
}

/// Тонкий разделитель, рамка панели/карточки.
pub fn fallback_divider() -> Color {
    pick("#E5E7EB", "#353A46")
}

/// Заливка при наведении и нейтральная «плашка» (кнопка без акцента, неактивная вкладка):
/// полупрозрачный цвет текста, годится поверх любого фона темы.
pub fn fallback_hover() -> Color {
    fallback_fg().with_alpha(0.08)
}

/// Дорожка индикатора (прогресс, слайдер, выключенный тумблер): полупрозрачный цвет текста,
/// заметна и на светлом, и на тёмном фоне.
pub fn fallback_track() -> Color {
    fallback_fg().with_alpha(0.15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_detection_by_background() {
        assert!(color_is_dark(Color::from_hex("#1E2128")));
        assert!(!color_is_dark(Color::from_hex("#F9FAFB")));
    }
}
