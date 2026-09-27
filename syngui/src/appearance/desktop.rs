//! Fallback для Linux без XDG-портала: читаем конфиги самого DE.
//!
//! Формат специально «дешёвый» — ini-файл KDE читается напрямую, GNOME
//! опрашивается через `gsettings` (dconf — бинарный формат), ровно как это уже
//! делает [`crate::input::resolve_double_click_interval`].

use super::{ColorScheme, SystemAppearance, SystemPalette};
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
    let text = read_kdeglobals()?;
    let ini = Ini::parse(&text);

    let scheme_mode = ini.get("General", "ColorSchemeMode");
    let window_bg = ini
        .get("Colors:Window", "BackgroundNormal")
        .and_then(parse_rgb);
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
        .or_else(|| {
            ini.get("Colors:Selection", "BackgroundNormal")
                .and_then(parse_rgb)
        });

    Some(SystemAppearance {
        color_scheme,
        accent,
        high_contrast: false,
        reduced_motion: false,
        palette: kde_palette(&ini),
    })
}

fn read_kdeglobals() -> Option<String> {
    std::fs::read_to_string(config_dir()?.join("kdeglobals")).ok()
}

/// Сеанс KDE Plasma (по `XDG_CURRENT_DESKTOP`).
fn is_kde_session() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.split(':').any(|d| d.eq_ignore_ascii_case("KDE")))
        .unwrap_or(false)
}

/// Портал сообщает только схему и акцент — палитру у KDE добираем из
/// `kdeglobals`. В чужих сеансах файл может остаться от давнего запуска
/// Plasma и не иметь к текущей теме отношения, поэтому только в KDE.
pub(super) fn with_kde_palette(mut appearance: SystemAppearance) -> SystemAppearance {
    if appearance.palette.is_none() && is_kde_session() {
        appearance.palette = kde_palette_file();
    }
    appearance
}

/// Палитра из `kdeglobals` на диске.
pub(super) fn kde_palette_file() -> Option<SystemPalette> {
    kde_palette(&Ini::parse(&read_kdeglobals()?))
}

/// Палитра из `kdeglobals`, если его цвета записал syndesktop (схема
/// `Syndesktop`). Иначе там цвета, к теме syndesktop не относящиеся.
pub(super) fn syndesktop_palette() -> Option<SystemPalette> {
    let text = read_kdeglobals()?;
    let ini = Ini::parse(&text);
    if ini.get("General", "ColorScheme") != Some("Syndesktop") {
        return None;
    }
    kde_palette(&ini)
}

/// Роли палитры из групп `[Colors:*]`. Без `[Colors:Window]` и
/// `[Colors:View]` палитры нет; остальное достраивается от них.
fn kde_palette(ini: &Ini) -> Option<SystemPalette> {
    let get = |group: &str, key: &str| ini.get(group, key).and_then(parse_rgb);
    let window = get("Colors:Window", "BackgroundNormal")?;
    let view = get("Colors:View", "BackgroundNormal")?;
    let fg = get("Colors:Window", "ForegroundNormal")
        .or_else(|| get("Colors:View", "ForegroundNormal"))?;
    let muted = get("Colors:Window", "ForegroundInactive").unwrap_or_else(|| mix(fg, window, 0.4));
    let accent = ini
        .get("General", "AccentColor")
        .and_then(parse_rgb)
        .or_else(|| get("Colors:Selection", "BackgroundNormal"))
        .or_else(|| get("Colors:Window", "DecorationFocus"))
        .unwrap_or_else(|| Color::from_hex("#3D8BFD"));
    Some(SystemPalette {
        window,
        view,
        view_alt: get("Colors:View", "BackgroundAlternate").unwrap_or_else(|| mix(view, fg, 0.04)),
        button: get("Colors:Button", "BackgroundNormal").unwrap_or(window),
        header: get("Colors:Header", "BackgroundNormal").unwrap_or(window),
        tooltip: get("Colors:Tooltip", "BackgroundNormal").unwrap_or(window),
        fg,
        muted,
        // Отдельного цвета рамок у KDE нет: Breeze смешивает текст с фоном.
        border: mix(window, fg, 0.2),
        accent,
        accent_fg: get("Colors:Selection", "ForegroundNormal").unwrap_or_else(|| accent.readable_on()),
        link: get("Colors:View", "ForegroundLink").unwrap_or(accent),
        danger: get("Colors:View", "ForegroundNegative").unwrap_or_else(|| Color::from_hex("#DA4453")),
        success: get("Colors:View", "ForegroundPositive").unwrap_or_else(|| Color::from_hex("#27AE60")),
        warning: get("Colors:View", "ForegroundNeutral").unwrap_or_else(|| Color::from_hex("#F67400")),
    })
}

/// Смесь в sRGB, как у тем KDE: `t = 0` — `a`, `t = 1` — `b`.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let [ar, ag, ab] = a.to_srgb_u8();
    let [br, bg, bb] = b.to_srgb_u8();
    let m = |x: u8, y: u8| ((x as f32 + (y as f32 - x as f32) * t) / 255.0).clamp(0.0, 1.0);
    Color::from_srgb_f32(m(ar, br), m(ag, bg), m(ab, bb))
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
        palette: None,
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
        let ini = Ini::parse(
            "[General]\nColorSchemeMode=dark\n\n[Colors:Window]\nBackgroundNormal=50,50,50\n",
        );
        assert_eq!(ini.get("General", "ColorSchemeMode"), Some("dark"));
        assert_eq!(
            ini.get("Colors:Window", "BackgroundNormal"),
            Some("50,50,50")
        );
        assert_eq!(ini.get("General", "BackgroundNormal"), None);
    }

    #[test]
    fn palette_from_color_groups() {
        let ini = Ini::parse(
            "[General]\nColorScheme=Syndesktop\n\n[Colors:Selection]\nBackgroundNormal=94,234,212\nForegroundNormal=0,0,0\n\n[Colors:View]\nBackgroundNormal=10,20,30\nForegroundNegative=251,113,133\n\n[Colors:Window]\nBackgroundNormal=15,32,51\nForegroundNormal=234,246,255\nForegroundInactive=167,192,214\n",
        );
        let p = kde_palette(&ini).expect("палитра");
        assert!(p.is_dark());
        assert_eq!(p.window.to_hex(), "#0F2033");
        assert_eq!(p.view.to_hex(), "#0A141E");
        assert_eq!(p.accent.to_hex(), "#5EEAD4");
        assert_eq!(p.accent_fg.to_hex(), "#000000");
        assert_eq!(p.muted.to_hex(), "#A7C0D6");
        assert_eq!(p.danger.to_hex(), "#FB7185");
        // Без фона содержимого палитры нет.
        assert!(kde_palette(&Ini::parse("[Colors:Window]\nBackgroundNormal=1,2,3\n")).is_none());
    }

    #[test]
    fn rgb_parses_plasma_triples() {
        assert_eq!(
            parse_rgb("0, 122, 255").map(|c| c.to_hex()),
            Some("#007AFF".into())
        );
        assert!(parse_rgb("не цвет").is_none());
    }
}
