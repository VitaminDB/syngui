//! Цвет MSS: sRGB-байты и разбор `#hex`, `rgb()/rgba()`, имён,
//! `darken()/lighten()` — тот же, что применяет syngui
//! (`syngui::mss::MssColor` — реэкспорт этого типа).

/// Цвета по имени, которые понимает MSS (других имён CSS нет).
pub const NAMED_COLORS: &[(&str, Color)] = &[
    ("black", Color::rgb(0, 0, 0)),
    ("white", Color::rgb(255, 255, 255)),
    ("red", Color::rgb(255, 0, 0)),
    ("green", Color::rgb(0, 128, 0)),
    ("blue", Color::rgb(0, 0, 255)),
    ("transparent", Color::rgba(0, 0, 0, 0)),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();

        if s.starts_with('#') {
            return Self::parse_hex(s);
        }

        if s.starts_with("rgb(") || s.starts_with("rgba(") {
            return Self::parse_rgb(s);
        }

        Self::parse_named(s)
    }

    fn parse_hex(s: &str) -> Option<Self> {
        let hex = &s[1..];

        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                Some(Self::rgb(r, g, b))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self::rgb(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self::rgba(r, g, b, a))
            }
            _ => None,
        }
    }

    fn parse_rgb(s: &str) -> Option<Self> {
        let inner = s
            .trim_start_matches("rgb(")
            .trim_start_matches("rgba(")
            .trim_end_matches(')');

        let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();

        if parts.len() < 3 {
            return None;
        }

        let r = parts[0].parse::<u8>().ok()?;
        let g = parts[1].parse::<u8>().ok()?;
        let b = parts[2].parse::<u8>().ok()?;
        let a = if parts.len() > 3 {
            (parts[3].parse::<f32>().ok()? * 255.0) as u8
        } else {
            255
        };

        Some(Self::rgba(r, g, b, a))
    }

    pub fn darken(self, factor: f32) -> Self {
        let f = 1.0 - factor.clamp(0.0, 1.0);
        Self::rgba(
            (self.r as f32 * f) as u8,
            (self.g as f32 * f) as u8,
            (self.b as f32 * f) as u8,
            self.a,
        )
    }

    pub fn lighten(self, factor: f32) -> Self {
        let f = factor.clamp(0.0, 1.0);
        Self::rgba(
            (self.r as f32 + (255.0 - self.r as f32) * f) as u8,
            (self.g as f32 + (255.0 - self.g as f32) * f) as u8,
            (self.b as f32 + (255.0 - self.b as f32) * f) as u8,
            self.a,
        )
    }

    pub fn parse_color_function(s: &str) -> Option<Self> {
        let s = s.trim();
        let (func, inner) = if s.starts_with("darken(") && s.ends_with(')') {
            ("darken", &s[7..s.len() - 1])
        } else if s.starts_with("lighten(") && s.ends_with(')') {
            ("lighten", &s[8..s.len() - 1])
        } else {
            return None;
        };

        let mut parts = Vec::new();
        let mut depth = 0;
        let mut start = 0;
        for (i, ch) in inner.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    parts.push(inner[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
        parts.push(inner[start..].trim());

        let color = Self::parse(parts[0])?;
        let amount = if parts.len() > 1 {
            parts[1].parse::<f32>().ok()?
        } else {
            0.1
        };

        match func {
            "darken" => Some(color.darken(amount)),
            "lighten" => Some(color.lighten(amount)),
            _ => None,
        }
    }

    fn parse_named(s: &str) -> Option<Self> {
        let s = s.to_lowercase();
        NAMED_COLORS
            .iter()
            .find(|(name, _)| *name == s)
            .map(|(_, c)| *c)
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::rgb(0, 0, 0)
    }
}

impl Color {
    /// `#rrggbb` или `#rrggbbaa` (если альфа не 255).
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_before() {
        assert_eq!(Color::parse("#abc"), Some(Color::rgb(0xaa, 0xbb, 0xcc)));
        assert_eq!(
            Color::parse("#11223344"),
            Some(Color::rgba(0x11, 0x22, 0x33, 0x44))
        );
        assert_eq!(
            Color::parse("rgba(1, 2, 3, 0.5)"),
            Some(Color::rgba(1, 2, 3, 127))
        );
        assert_eq!(Color::parse("Red"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(Color::parse("orange"), None);
        assert_eq!(
            Color::parse_color_function("darken(#ffffff, 0.5)"),
            Some(Color::rgb(127, 127, 127))
        );
        assert_eq!(Color::rgba(1, 2, 3, 4).to_hex(), "#01020304");
    }
}
