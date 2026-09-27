//! Сеанс syndesktop: оформление берётся из его собственного конфига.
//!
//! В сеансе syndesktop портал `Settings` обслуживает бэкенд KDE или GTK
//! (`syndesktop-portals.conf`), и он отдаёт схему и акцент из `kdeglobals`, а
//! `kwinrc` описывает рамки KWin, которых здесь нет. Источник правды — это
//! `~/.config/syndesktop/config.toml`: `[appearance]` (схема, акцент, палитра)
//! и `[decorations]` (кнопки, выравнивание заголовка, высота). Композитор
//! перечитывает файл на лету, поэтому и мы следим за ним опросом, а не
//! подпиской на портал.

use std::path::PathBuf;

use super::decorations::{
    DecorationLayout, DecorationMetrics, DecorationStyle, SyndesktopButtons, SystemDecorations,
    TitleAlignment, WindowButton,
};
use super::{ColorScheme, SystemAppearance};
use crate::core::Color;

/// Приложение запущено в сеансе syndesktop. Композитор выставляет детям
/// `SYNDESKTOP_SOCKET`, сеанс из менеджера входа — `XDG_CURRENT_DESKTOP`.
pub(crate) fn is_session() -> bool {
    let in_list = |var: &str| {
        std::env::var(var)
            .map(|v| v.split(':').any(|d| d.eq_ignore_ascii_case("syndesktop")))
            .unwrap_or(false)
    };
    in_list("XDG_CURRENT_DESKTOP")
        || std::env::var_os("SYNDESKTOP_SOCKET").is_some_and(|v| !v.is_empty())
}

/// `$SYNDESKTOP_CONFIG_DIR/config.toml`, иначе `$XDG_CONFIG_HOME/syndesktop/…`
/// — тот же порядок, что у самого syndesktop.
fn config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SYNDESKTOP_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir).join("config.toml"));
    }
    Some(super::desktop::config_dir()?.join("syndesktop/config.toml"))
}

/// Конфиг сеанса. Файла нет (syndesktop ещё не записал его) — пустой конфиг,
/// то есть умолчания syndesktop: оформление всё равно его, а не KDE.
fn read_config() -> Option<String> {
    if !is_session() {
        return None;
    }
    Some(
        config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default(),
    )
}

pub(super) fn read_appearance() -> Option<SystemAppearance> {
    let text = read_config()?;
    Some(appearance_from(&Toml::parse(&text)))
}

pub(super) fn read_decorations() -> Option<SystemDecorations> {
    let text = read_config()?;
    Some(decorations_from(&Toml::parse(&text)))
}

fn appearance_from(cfg: &Toml) -> SystemAppearance {
    let palette = Palette::from_config(cfg);
    SystemAppearance {
        color_scheme: if palette.dark {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        },
        accent: Some(palette.accent),
        high_contrast: false,
        reduced_motion: cfg
            .get("animations", "enabled")
            .is_some_and(|v| v == "false"),
    }
}

fn decorations_from(cfg: &Toml) -> SystemDecorations {
    let palette = Palette::from_config(cfg);
    let buttons = cfg
        .get("decorations", "buttons")
        .unwrap_or("icon:minimize,maximize,close");
    let (left, right) = buttons.split_once(':').unwrap_or(("", buttons));
    let parse = |s: &str| -> Vec<WindowButton> {
        s.split(',')
            .filter_map(WindowButton::from_syndesktop_name)
            .collect()
    };
    let title_height = cfg
        .get("decorations", "title_height")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(32.0)
        .clamp(16.0, 96.0);
    let centered = cfg.get("decorations", "title_align") != Some("left");

    SystemDecorations {
        layout: DecorationLayout {
            left: parse(left),
            right: parse(right),
        },
        // Как в `DecoTheme::button_rects` композитора: квадрат высотой в
        // заголовок, вплотную друг к другу, 4 px от края окна.
        metrics: DecorationMetrics {
            button_size: title_height,
            button_spacing: 0.0,
            edge_left: 4.0,
            edge_right: 4.0,
            title_alignment: if centered {
                TitleAlignment::Center
            } else {
                TitleAlignment::Left
            },
        },
        style: DecorationStyle::Syndesktop(SyndesktopButtons {
            danger: palette.danger,
        }),
    }
}

/// Нужная нам часть палитры syndesktop (`Appearance::palette`): базовые цвета
/// схемы и переопределения из `[appearance.colors]`.
struct Palette {
    dark: bool,
    accent: Color,
    danger: Color,
}

impl Palette {
    fn from_config(cfg: &Toml) -> Self {
        let dark = cfg.get("appearance", "color_scheme") != Some("light");
        let accent = cfg
            .get("appearance", "accent")
            .and_then(parse_hex)
            .unwrap_or_else(|| Color::from_hex("#3D8BFD"));
        let danger = cfg
            .get("appearance.colors", "danger")
            .and_then(parse_hex)
            .unwrap_or_else(|| Color::from_hex(if dark { "#E5484D" } else { "#D13438" }));
        Self {
            dark,
            accent,
            danger,
        }
    }
}

/// `#rgb`, `#rrggbb`, `#rrggbbaa` — те же формы, что принимает syndesktop.
fn parse_hex(s: &str) -> Option<Color> {
    let h = s.trim().strip_prefix('#')?;
    if !matches!(h.len(), 3 | 6 | 8) || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    if h.len() == 3 {
        let long: String = h.chars().flat_map(|c| [c, c]).collect();
        return Some(Color::from_hex(&long));
    }
    Some(Color::from_hex(h))
}

// ─── Мини-разбор TOML ───────────────────────────────────────────────────

/// Плоский разбор TOML: `[секция]` и `ключ = значение` со скалярами. Массивы
/// таблиц (`[[panel]]`) и многострочные значения нам не нужны — строки внутри
/// них просто не совпадут ни с одним запрашиваемым ключом.
struct Toml {
    entries: Vec<(String, String, String)>,
}

impl Toml {
    fn parse(text: &str) -> Self {
        let mut entries = Vec::new();
        let mut section = String::new();
        for line in text.lines() {
            let line = strip_comment(line).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with("[[") {
                // Элемент массива таблиц — его ключи ни к чему не относим.
                section = "[[]]".into();
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_string();
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim().trim_matches('"').to_string();
                let value = unquote(value.trim()).to_string();
                entries.push((section.clone(), key, value));
            }
        }
        Self { entries }
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(s, k, _)| s == section && k == key)
            .map(|(_, _, v)| v.as_str())
    }
}

/// Отрезает `# комментарий`, не трогая `#` внутри строки (`accent = "#3d8bfd"`).
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '#' => return &line[..i],
                _ => {}
            },
        }
    }
    line
}

fn unquote(v: &str) -> &str {
    for q in ['"', '\''] {
        if let Some(inner) = v.strip_prefix(q).and_then(|v| v.strip_suffix(q)) {
            return inner;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
[appearance]
color_scheme = "light"          # dark | light
accent = "#ff8800"

[appearance.colors]
danger = "#aa0000"

[decorations]
title_height = 28
title_align = "left"         # left | center
buttons = "close,minimize:maximize"

[[panel]]
buttons = "игнор"
"##;

    #[test]
    fn toml_reads_sections_and_strips_comments() {
        let t = Toml::parse(SAMPLE);
        assert_eq!(t.get("appearance", "color_scheme"), Some("light"));
        assert_eq!(t.get("appearance", "accent"), Some("#ff8800"));
        assert_eq!(t.get("appearance.colors", "danger"), Some("#aa0000"));
        assert_eq!(t.get("decorations", "title_height"), Some("28"));
        assert_eq!(t.get("decorations", "buttons"), Some("close,minimize:maximize"));
    }

    #[test]
    fn appearance_follows_config() {
        let a = appearance_from(&Toml::parse(SAMPLE));
        assert_eq!(a.color_scheme, ColorScheme::Light);
        assert_eq!(a.accent.map(|c| c.to_hex()), Some("#FF8800".into()));
        // Пустой конфиг — умолчания syndesktop: тёмная схема и его акцент.
        let d = appearance_from(&Toml::parse(""));
        assert_eq!(d.color_scheme, ColorScheme::Dark);
        assert_eq!(d.accent.map(|c| c.to_hex()), Some("#3D8BFD".into()));
    }

    #[test]
    fn decorations_follow_config() {
        let d = decorations_from(&Toml::parse(SAMPLE));
        assert_eq!(d.layout.left, vec![WindowButton::Close, WindowButton::Minimize]);
        assert_eq!(d.layout.right, vec![WindowButton::Maximize]);
        assert_eq!(d.metrics.button_size, 28.0);
        assert_eq!(d.metrics.title_alignment, TitleAlignment::Left);
        assert_eq!(
            d.style,
            DecorationStyle::Syndesktop(SyndesktopButtons {
                danger: Color::from_hex("#AA0000")
            })
        );

        let d = decorations_from(&Toml::parse(""));
        assert_eq!(d.layout.left, vec![WindowButton::Menu]);
        assert_eq!(
            d.layout.right,
            vec![WindowButton::Minimize, WindowButton::Maximize, WindowButton::Close]
        );
        assert_eq!(d.metrics.title_alignment, TitleAlignment::Center);
    }
}
