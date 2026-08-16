//! Fallback для Linux без XDG-портала: читаем конфиги самого DE.
//!
//! Формат специально «дешёвый» — ini-файл KDE читается напрямую, GNOME
//! опрашивается через `gsettings` (dconf — бинарный формат), ровно как это уже
//! делает [`crate::input::resolve_double_click_interval`].

use super::{ColorScheme, SystemAppearance};
use crate::core::Color;

pub(super) fn read() -> Option<SystemAppearance> {
    kde().or_else(gnome)
}

// ─── KDE ────────────────────────────────────────────────────────────────

/// `kdeglobals`: `[General] ColorSchemeMode` (Plasma 6) либо яркость фона окна
/// из `[Colors:Window] BackgroundNormal`. Акцент — `[General] AccentColor`,
/// а если он не задан (акцент берётся из схемы) — `[Colors:Selection]
/// BackgroundNormal`.
fn kde() -> Option<SystemAppearance> {
    let text = std::fs::read_to_string(config_dir()?.join("kdeglobals")).ok()?;
    let ini = Ini::parse(&text);

    let scheme_mode = ini.get("General", "ColorSchemeMode");
    let window_bg = ini.get("Colors:Window", "BackgroundNormal").and_then(parse_rgb);
    let color_scheme = match scheme_mode.map(str::trim) {
        Some(m) if m.eq_ignore_ascii_case("dark") => ColorScheme::Dark,
        Some(m) if m.eq_ignore_ascii_case("light") => ColorScheme::Light,
        // «Follow color scheme» и Plasma 5: судим по яркости фона окна.
        _ => match window_bg {
            Some(bg) if bg.relative_luminance() < 0.18 => ColorScheme::Dark,
            Some(_) => ColorScheme::Light,
            None => return None,
        },
    };

    let accent = ini
        .get("General", "AccentColor")
        .and_then(parse_rgb)
        .or_else(|| ini.get("Colors:Selection", "BackgroundNormal").and_then(parse_rgb));

    Some(SystemAppearance {
        color_scheme,
        accent,
        high_contrast: false,
        reduced_motion: false,
    })
}

/// `R,G,B` (Plasma хранит компоненты как десятичные байты).
fn parse_rgb(value: &str) -> Option<Color> {
    let mut parts = value.split(',').map(|p| p.trim().parse::<u8>());
    let r = parts.next()?.ok()?;
    let g = parts.next()?.ok()?;
    let b = parts.next()?.ok()?;
    Some(Color::from_srgb(r, g, b, 1.0))
}

// ─── GNOME ──────────────────────────────────────────────────────────────

fn gnome() -> Option<SystemAppearance> {
    let scheme = gsettings("org.gnome.desktop.interface", "color-scheme")?;
    let color_scheme = match scheme.trim().trim_matches('\'') {
        "prefer-dark" => ColorScheme::Dark,
        "prefer-light" => ColorScheme::Light,
        _ => ColorScheme::NoPreference,
    };
    // GNOME 47+: акцент задаётся именем из фиксированной палитры.
    let accent = gsettings("org.gnome.desktop.interface", "accent-color")
        .and_then(|v| gnome_accent_hex(v.trim().trim_matches('\'')))
        .map(Color::from_hex);
    Some(SystemAppearance {
        color_scheme,
        accent,
        high_contrast: false,
        reduced_motion: false,
    })
}

fn gsettings(schema: &str, key: &str) -> Option<String> {
    let out = std::process::Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Палитра акцентов GNOME 47 (`libadwaita`).
fn gnome_accent_hex(name: &str) -> Option<&'static str> {
    Some(match name {
        "blue" => "#3584E4",
        "teal" => "#2190A4",
        "green" => "#3A944A",
        "yellow" => "#C88800",
        "orange" => "#ED5B00",
        "red" => "#E62D42",
        "pink" => "#D56199",
        "purple" => "#9141AC",
        "slate" => "#6F8396",
        _ => return None,
    })
}

// ─── Мини-парсер ini ────────────────────────────────────────────────────

pub(crate) fn config_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
}

/// Плоский разбор ini: `[секция] ключ=значение`. Достаточно для kdeglobals и
/// kwinrc — там нет ни экранирования, ни многострочных значений.
pub(crate) struct Ini<'a> {
    entries: Vec<(&'a str, &'a str, &'a str)>,
}

impl<'a> Ini<'a> {
    pub(crate) fn parse(text: &'a str) -> Self {
        let mut entries = Vec::new();
        let mut section = "";
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name;
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                entries.push((section, key.trim(), value.trim()));
            }
        }
        Self { entries }
    }

    pub(crate) fn get(&self, section: &str, key: &str) -> Option<&'a str> {
        self.entries
            .iter()
            .find(|(s, k, _)| *s == section && *k == key)
            .map(|(_, _, v)| *v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_reads_sections() {
        let ini = Ini::parse("[General]\nColorSchemeMode=dark\n\n[Colors:Window]\nBackgroundNormal=50,50,50\n");
        assert_eq!(ini.get("General", "ColorSchemeMode"), Some("dark"));
        assert_eq!(ini.get("Colors:Window", "BackgroundNormal"), Some("50,50,50"));
        assert_eq!(ini.get("General", "BackgroundNormal"), None);
    }

    #[test]
    fn rgb_parses_plasma_triples() {
        assert_eq!(parse_rgb("0, 122, 255").map(|c| c.to_hex()), Some("#007AFF".into()));
        assert!(parse_rgb("не цвет").is_none());
    }
}
