//! Встроенные стили виджетов фреймворка — то, что у браузера называют
//! «user agent stylesheet». Правила лежат под таблицей приложения (её правила
//! идут позже и при равной специфичности побеждают), поэтому составной виджет
//! с классами (видеоплеер) выглядит прилично без стилей приложения, а
//! приложение переопределяет что хочет теми же селекторами.
//!
//! Цвета приложения берутся через `var(--имя, запасное)`: тема задала
//! переменную — виджет в её цветах, нет — запасной цвет.

use super::{parse_stylesheet_str, StyleSheet};

/// Исходники встроенных таблиц (по фичам).
fn sources() -> Vec<&'static str> {
    #[allow(unused_mut)]
    let mut out: Vec<&'static str> = vec![include_str!("../../styles/split_view.mss")];
    #[cfg(feature = "ffmpeg")]
    out.push(include_str!("../../styles/video_player.mss"));
    out
}

thread_local! {
    static DEFAULTS: StyleSheet = parse_defaults();
}

fn parse_defaults() -> StyleSheet {
    let mut sheet = StyleSheet::new();
    for src in sources() {
        match parse_stylesheet_str(src) {
            Ok(s) => sheet.merge(&s),
            Err(e) => log::warn!("[mss] встроенные стили виджетов не разобрались: {e:?}"),
        }
    }
    sheet
}

/// Таблица `app` поверх встроенных стилей виджетов.
pub(crate) fn with_widget_defaults(app: StyleSheet) -> StyleSheet {
    DEFAULTS.with(|base| {
        if base.rules().is_empty() {
            return app;
        }
        let mut sheet = base.clone();
        sheet.merge(&app);
        sheet
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mss::{StyleEngine, StyleValue};

    /// Цвет свойства правила `SplitView { … }` после подстановки переменных
    /// темы `vars` (в дереве так же: правило типа + `resolve_variable`).
    fn split_color(vars: &str, prop: &str) -> Option<(u8, u8, u8)> {
        let engine = StyleEngine::new(parse_stylesheet_str(vars).unwrap());
        let rule = engine.stylesheet().find_element_styles("SplitView")?;
        let v = rule
            .declarations
            .iter()
            .find(|(p, _)| p.as_str() == prop)?
            .1
            .clone();
        match engine.resolve_variable(&v) {
            StyleValue::Color(c) => Some((c.r, c.g, c.b)),
            _ => None,
        }
    }

    /// Разделитель SplitView берёт цвета темы, если она задала переменные,
    /// и остаётся без цвета (запасной `theme_fallback`), если нет.
    #[test]
    fn split_view_uses_theme_vars() {
        let vars = ":root { --border: #112233; --accent: #445566; }";
        assert_eq!(split_color(vars, "border-color"), Some((0x11, 0x22, 0x33)));
        assert_eq!(split_color(vars, "accent-color"), Some((0x44, 0x55, 0x66)));
        let vars = ":root { --divider: #010203; --border: #112233; --primary: #0a0b0c; }";
        assert_eq!(split_color(vars, "border-color"), Some((1, 2, 3)));
        assert_eq!(split_color(vars, "accent-color"), Some((10, 11, 12)));
        assert_eq!(split_color(":root { --x: #000000; }", "border-color"), None);
        assert_eq!(split_color(":root { --x: #000000; }", "accent-color"), None);
    }

    #[cfg(feature = "ffmpeg")]
    #[test]
    fn video_player_defaults_parse() {
        let sheet = with_widget_defaults(StyleSheet::new());
        assert!(sheet
            .rules()
            .iter()
            .any(|r| r.selector_str.contains("vp-controls")));
    }
}
