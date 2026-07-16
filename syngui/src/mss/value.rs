use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub enum StyleValue {
    Color(Color),
    Length(f32, Unit),
    String(String),
    Number(f32),
    Var(String),
    VarWithFallback(String, Box<StyleValue>),
    List(Vec<StyleValue>),
    Gradient(crate::core::Gradient),
    Inherit,
    Initial,
    Unset,
    None,
}

impl From<crate::core::Color> for StyleValue {
    fn from(c: crate::core::Color) -> Self {
        StyleValue::Color(Color::rgba(
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8,
            (c.a * 255.0) as u8,
        ))
    }
}

impl From<f32> for StyleValue {
    fn from(v: f32) -> Self {
        StyleValue::Length(v, Unit::Px)
    }
}

impl StyleValue {
    pub fn px(v: f32) -> Self {
        StyleValue::Length(v, Unit::Px)
    }

    pub fn percent(v: f32) -> Self {
        StyleValue::Length(v, Unit::Percent)
    }

    pub fn as_color(&self) -> Option<Color> {
        match self {
            StyleValue::Color(c) => Some(*c),
            _ => None,
        }
    }

    pub fn as_px(&self) -> Option<f32> {
        match self {
            StyleValue::Length(v, Unit::Px) => Some(*v),
            StyleValue::Number(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_dimension(&self) -> Option<Dimension> {
        match self {
            StyleValue::Length(v, Unit::Px) => Some(Dimension::Px(*v)),
            StyleValue::Length(v, Unit::Percent) => Some(Dimension::Percent(*v)),
            StyleValue::Length(_, Unit::Auto) => Some(Dimension::Auto),
            StyleValue::Length(_, Unit::FitContent) => Some(Dimension::FitContent),
            StyleValue::Length(_, Unit::MaxContent) => Some(Dimension::MaxContent),
            StyleValue::Length(_, Unit::MinContent) => Some(Dimension::MinContent),
            StyleValue::Number(v) => Some(Dimension::Px(*v)),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            StyleValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_gradient(&self) -> Option<&crate::core::Gradient> {
        match self {
            StyleValue::Gradient(g) => Some(g),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Px,
    Percent,
    Em,
    Rem,
    Vw,
    Vh,
    Auto,
    FitContent,
    MaxContent,
    MinContent,
}

impl FromStr for Unit {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "px" => Ok(Unit::Px),
            "%" => Ok(Unit::Percent),
            "em" => Ok(Unit::Em),
            "rem" => Ok(Unit::Rem),
            "vw" => Ok(Unit::Vw),
            "vh" => Ok(Unit::Vh),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    Px(f32),
    Percent(f32),
    Auto,
    FitContent,
    MaxContent,
    MinContent,
}

impl Dimension {
    pub fn resolve(self, parent: f32) -> f32 {
        match self {
            Dimension::Px(v) => v,
            Dimension::Percent(p) => {
                if parent.is_finite() {
                    parent * p / 100.0
                } else {
                    0.0
                }
            }
            Dimension::Auto
            | Dimension::FitContent
            | Dimension::MaxContent
            | Dimension::MinContent => {
                if parent.is_finite() { parent } else { 0.0 }
            }
        }
    }

    pub fn resolve_opt(self, parent: f32) -> Option<f32> {
        match self {
            Dimension::Px(v) => Some(v),
            Dimension::Percent(p) => {
                if parent.is_finite() {
                    Some(parent * p / 100.0)
                } else {
                    None
                }
            }
            Dimension::Auto
            | Dimension::FitContent
            | Dimension::MaxContent
            | Dimension::MinContent => None,
        }
    }

    pub fn is_intrinsic(self) -> bool {
        matches!(
            self,
            Dimension::FitContent | Dimension::MaxContent | Dimension::MinContent
        )
    }

    pub fn is_auto(self) -> bool {
        matches!(self, Dimension::Auto)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
        let inner = s.trim_start_matches("rgb(")
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
        match s.to_lowercase().as_str() {
            "black" => Some(Self::rgb(0, 0, 0)),
            "white" => Some(Self::rgb(255, 255, 255)),
            "red" => Some(Self::rgb(255, 0, 0)),
            "green" => Some(Self::rgb(0, 128, 0)),
            "blue" => Some(Self::rgb(0, 0, 255)),
            "transparent" => Some(Self::rgba(0, 0, 0, 0)),
            _ => None,
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::rgb(0, 0, 0)
    }
}
