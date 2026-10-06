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
    let mut out: Vec<&'static str> = Vec::new();
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

#[cfg(all(test, feature = "ffmpeg"))]
mod tests {
    use super::*;

    #[test]
    fn video_player_defaults_parse() {
        let sheet = with_widget_defaults(StyleSheet::new());
        assert!(sheet
            .rules()
            .iter()
            .any(|r| r.selector_str.contains("vp-controls")));
    }
}
