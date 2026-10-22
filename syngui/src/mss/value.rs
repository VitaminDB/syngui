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
    /// `core::Color` хранит линейные компоненты, MSS — sRGB-байты, которые
    /// линеаризуются при применении. Линейные значения байтами как есть
    /// проходили гамму дважды: `.style("background-color",
    /// Color::from_hex("#243149"))` рисовался почти чёрным.
    fn from(c: crate::core::Color) -> Self {
        let [r, g, b] = c.to_srgb_u8();
        StyleValue::Color(Color::rgba(
            r,
            g,
            b,
            (c.a.clamp(0.0, 1.0) * 255.0).round() as u8,
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
                if parent.is_finite() {
                    parent
                } else {
                    0.0
                }
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

/// Цвет MSS — общий с инструментами тип из `mss-lang`.
pub use mss_lang::color::Color;

#[cfg(test)]
mod core_color_tests {
    use super::*;

    #[test]
    fn core_color_keeps_its_hex_in_inline_style() {
        let v = StyleValue::from(crate::core::Color::from_hex("#243149"));
        assert_eq!(v, StyleValue::Color(Color::rgba(0x24, 0x31, 0x49, 255)));
        let v = StyleValue::from(crate::core::Color::from_hex("#EE5E4833"));
        assert_eq!(v, StyleValue::Color(Color::rgba(0xEE, 0x5E, 0x48, 0x33)));
    }
}
