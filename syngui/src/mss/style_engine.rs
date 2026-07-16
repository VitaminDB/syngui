use super::{stylesheet::*, value::*};
use crate::core::{EdgeInsets, Shadows};
use crate::input::CursorIcon;
use std::collections::HashMap;
use std::ops::BitOr;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextAlign(u8);

impl TextAlign {
    pub const LEFT:    TextAlign = TextAlign(0b0000);
    pub const HCENTER: TextAlign = TextAlign(0b0001);
    pub const RIGHT:   TextAlign = TextAlign(0b0010);
    pub const TOP:     TextAlign = TextAlign(0b0000);
    pub const VCENTER: TextAlign = TextAlign(0b0100);
    pub const BOTTOM:  TextAlign = TextAlign(0b1000);
    pub const CENTER:  TextAlign = TextAlign(0b0101);
    pub const DEFAULT: TextAlign = TextAlign(0b0100);

    #[inline]
    pub fn horizontal(self) -> TextAlign { TextAlign(self.0 & 0b0011) }
    #[inline]
    pub fn vertical(self) -> TextAlign { TextAlign(self.0 & 0b1100) }
    #[inline]
    pub fn is_left(self) -> bool { self.0 & 0b0011 == 0 }
    #[inline]
    pub fn is_hcenter(self) -> bool { self.0 & 0b0011 == 1 }
    #[inline]
    pub fn is_right(self) -> bool { self.0 & 0b0011 == 2 }
    #[inline]
    pub fn is_top(self) -> bool { self.0 & 0b1100 == 0 }
    #[inline]
    pub fn is_vcenter(self) -> bool { self.0 & 0b1100 == 4 }
    #[inline]
    pub fn is_bottom(self) -> bool { self.0 & 0b1100 == 8 }
}

impl Default for TextAlign {
    fn default() -> Self { Self::DEFAULT }
}

impl BitOr for TextAlign {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { TextAlign(self.0 | rhs.0) }
}

impl std::fmt::Debug for TextAlign {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let h = match self.0 & 0b0011 {
            1 => "HCENTER",
            2 => "RIGHT",
            _ => "LEFT",
        };
        let v = match self.0 & 0b1100 {
            4 => "VCENTER",
            8 => "BOTTOM",
            _ => "TOP",
        };
        write!(f, "TextAlign({} | {})", h, v)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    LineThrough,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
}

#[derive(Debug, Clone)]
pub struct StyleEngine {
    stylesheet: StyleSheet,
    cache: HashMap<StyleCacheKey, ComputedStyle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct StyleCacheKey {
    classes: Vec<String>,
}

impl StyleEngine {
    pub fn new(stylesheet: StyleSheet) -> Self {
        Self {
            stylesheet,
            cache: HashMap::new(),
        }
    }

    pub fn empty() -> Self {
        Self {
            stylesheet: StyleSheet::new(),
            cache: HashMap::new(),
        }
    }

    pub fn compute_style(&mut self, ctx: &StyleContext) -> ComputedStyle {

        let key = StyleCacheKey {
            classes: ctx.classes.clone(),
        };

        if let Some(style) = self.cache.get(&key) {
            return style.clone();
        }

        let mut computed = ComputedStyle::default();

        for class in &ctx.classes {
            if let Some(rule) = self.stylesheet.find_class_styles(class) {
                self.apply_rule(&mut computed, rule);
            }
        }

        self.cache.insert(key, computed.clone());
        
        computed
    }

    pub fn compute_style_with_state(
        &mut self,
        ctx: &StyleContext,
        state: ElementState
    ) -> ComputedStyle {
        let mut computed = self.compute_style(ctx);

        let pseudo = match state {
            ElementState::Hover => Some("hover"),
            ElementState::Active => Some("active"),
            ElementState::Focus => Some("focus"),
            ElementState::Selected => Some("selected"),
            ElementState::Checked => Some("checked"),
            ElementState::Disabled => Some("disabled"),
            ElementState::Normal => None,
        };

        if let Some(pseudo) = pseudo {
            for class in &ctx.classes {
                if let Some(rule) = self.stylesheet.find_class_pseudo_styles(class, pseudo) {
                    self.apply_rule(&mut computed, rule);
                }
            }
            if !ctx.element_type.is_empty() {
                if let Some(rule) = self.stylesheet.find_element_pseudo_styles(&ctx.element_type, pseudo) {
                    self.apply_rule(&mut computed, rule);
                }
            }
        }

        if ctx.window_flags != 0 {
            for &(flag, pseudo_name) in WINDOW_PSEUDOS {
                if ctx.window_flags & flag == 0 {
                    continue;
                }
                for class in &ctx.classes {
                    if let Some(rule) = self.stylesheet.find_class_pseudo_styles(class, pseudo_name) {
                        self.apply_rule(&mut computed, rule);
                    }
                }
                if !ctx.element_type.is_empty() {
                    if let Some(rule) = self.stylesheet.find_element_pseudo_styles(&ctx.element_type, pseudo_name) {
                        self.apply_rule(&mut computed, rule);
                    }
                }
            }
        }

        computed
    }

    fn apply_rule(&self, computed: &mut ComputedStyle, rule: &StyleRule) {
        for (property, value) in &rule.declarations {
            let resolved = self.resolve_value(value);

            computed.set(property, resolved);
        }
    }

    fn resolve_value(&self, value: &StyleValue) -> StyleValue {
        match value {
            StyleValue::Var(name) => {
                self.stylesheet.get_variable(name)
                    .cloned()
                    .map(|v| self.resolve_value(&v))
                    .unwrap_or(StyleValue::None)
            }
            StyleValue::VarWithFallback(name, fallback) => {
                self.stylesheet.get_variable(name)
                    .cloned()
                    .map(|v| self.resolve_value(&v))
                    .unwrap_or_else(|| self.resolve_value(fallback))
            }
            StyleValue::String(s) if s.contains("var(--") => {
                let mut result = s.clone();
                while let Some(start) = result.find("var(--") {
                    let var_start = start + 4;
                    if let Some(end) = result[var_start..].find(')') {
                        let var_name = &result[var_start..var_start + end];
                        let replacement = self.stylesheet.get_variable(var_name)
                            .cloned()
                            .map(|v| self.resolve_value(&v))
                            .map(|v| match v {
                                StyleValue::Color(c) => format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a),
                                StyleValue::String(s) => s,
                                StyleValue::Number(n) => format!("{}", n),
                                StyleValue::Length(n, unit) => {
                                    let u = match unit {
                                        crate::mss::Unit::Px => "px",
                                        crate::mss::Unit::Percent => "%",
                                        crate::mss::Unit::Em => "em",
                                        crate::mss::Unit::Rem => "rem",
                                        crate::mss::Unit::Vw => "vw",
                                        crate::mss::Unit::Vh => "vh",
                                        crate::mss::Unit::Auto
                                        | crate::mss::Unit::FitContent
                                        | crate::mss::Unit::MaxContent
                                        | crate::mss::Unit::MinContent => {
                                            return match unit {
                                                crate::mss::Unit::Auto => "auto".into(),
                                                crate::mss::Unit::FitContent => "fit-content".into(),
                                                crate::mss::Unit::MaxContent => "max-content".into(),
                                                _ => "min-content".into(),
                                            };
                                        }
                                    };
                                    format!("{}{}", n, u)
                                }
                                _ => String::new(),
                            })
                            .unwrap_or_default();
                        result = format!("{}{}{}", &result[..start], replacement, &result[var_start + end + 1..]);
                    } else {
                        break;
                    }
                }
                if let Some(gradient) = crate::mss::parser::gradient::parse_gradient(&result) {
                    gradient
                } else if let Some(color) = crate::mss::value::Color::parse_color_function(&result) {
                    StyleValue::Color(color)
                } else if let Some(color) = crate::mss::value::Color::parse(&result) {
                    StyleValue::Color(color)
                } else {
                    StyleValue::String(result)
                }
            }
            _ => value.clone(),
        }
    }

    pub fn stylesheet(&self) -> &StyleSheet {
        &self.stylesheet
    }

    pub fn resolve_variable(&self, value: &StyleValue) -> StyleValue {
        self.resolve_value(value)
    }

    pub fn load_stylesheet(&mut self, stylesheet: StyleSheet) {
        self.stylesheet = stylesheet;
        self.cache.clear();
    }

    pub fn load_additional_stylesheet(&mut self, additional: StyleSheet) {
        self.stylesheet.merge(&additional);
        self.cache.clear();
    }
}

pub mod window_flags {
    pub const MAXIMIZED:  u8 = 0b0000_0001;
    pub const FULLSCREEN: u8 = 0b0000_0010;
    pub const FOCUSED:    u8 = 0b0000_0100;
}

const WINDOW_PSEUDOS: &[(u8, &str)] = &[
    (window_flags::MAXIMIZED,  "window-maximized"),
    (window_flags::FULLSCREEN, "window-fullscreen"),
    (window_flags::FOCUSED,    "window-focused"),
];

#[derive(Debug, Clone, Default)]
pub struct StyleContext {
    pub classes: Vec<String>,
    pub id: Option<String>,
    pub element_type: String,
    pub window_flags: u8,
}

impl StyleContext {
    pub fn with_class(class: impl Into<String>) -> Self {
        Self {
            classes: vec![class.into()],
            id: None,
            element_type: String::new(),
            window_flags: 0,
        }
    }

    pub fn add_class(&mut self, class: impl Into<String>) {
        let class = class.into();
        if !self.classes.contains(&class) {
            self.classes.push(class);
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_element_type(mut self, element_type: impl Into<String>) -> Self {
        self.element_type = element_type.into();
        self
    }

    pub fn with_window_flags(mut self, flags: u8) -> Self {
        self.window_flags = flags;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedStyle {
    properties: HashMap<String, StyleValue>,
}

impl ComputedStyle {
    pub fn new() -> Self {
        Self { properties: HashMap::new() }
    }

    pub fn with_color(color: crate::core::Color) -> Self {
        let mut s = Self::new();
        s.set("color", StyleValue::from(color));
        s
    }

    pub fn properties(&self) -> impl Iterator<Item = (&str, &StyleValue)> {
        self.properties.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn set(&mut self, property: &str, value: StyleValue) {
        self.properties.insert(property.to_string(), value);
    }

    pub fn get(&self, property: &str) -> Option<&StyleValue> {
        self.properties.get(property)
    }

    pub fn background_color(&self) -> Option<Color> {
        self.get("background")
            .or_else(|| self.get("background-color"))
            .and_then(|v| v.as_color())
    }

    pub fn background_gradient(&self) -> Option<&crate::core::Gradient> {
        self.get("background-gradient")
            .or_else(|| self.get("background"))
            .or_else(|| self.get("background-color"))
            .and_then(|v| v.as_gradient())
    }

    pub fn color(&self) -> Option<Color> {
        self.get("color").and_then(|v| v.as_color())
    }

    pub fn padding(&self) -> f32 {
        self.get("padding-top")
            .and_then(|v| v.as_px())
            .unwrap_or(0.0)
    }

    pub fn padding_sides(&self) -> Option<[f32; 4]> {
        let t = self.get("padding-top").and_then(|v| v.as_px());
        let r = self.get("padding-right").and_then(|v| v.as_px());
        let b = self.get("padding-bottom").and_then(|v| v.as_px());
        let l = self.get("padding-left").and_then(|v| v.as_px());
        if t.is_none() && r.is_none() && b.is_none() && l.is_none() {
            return None;
        }
        Some([
            t.unwrap_or(0.0),
            r.unwrap_or(0.0),
            b.unwrap_or(0.0),
            l.unwrap_or(0.0),
        ])
    }

    pub fn border_radius(&self) -> f32 {
        self.get("border-top-left-radius")
            .and_then(|v| v.as_px())
            .unwrap_or(0.0)
    }

    pub fn border_radius_corners(&self) -> Option<[f32; 4]> {
        self.border_radius_dimensions().map(|dims| {
            dims.map(|d| match d {
                super::value::Dimension::Px(v) => v,
                super::value::Dimension::Percent(v) => v,
                super::value::Dimension::Auto
                | super::value::Dimension::FitContent
                | super::value::Dimension::MaxContent
                | super::value::Dimension::MinContent => 0.0,
            })
        })
    }

    pub fn border_radius_dimensions(&self) -> Option<[super::value::Dimension; 4]> {
        use super::value::Dimension;
        let corners = [
            "border-top-left-radius",
            "border-top-right-radius",
            "border-bottom-right-radius",
            "border-bottom-left-radius",
        ];
        let mut out = [Dimension::Px(0.0); 4];
        let mut any = false;
        for (i, name) in corners.iter().enumerate() {
            if let Some(v) = self.get(name).and_then(|v| v.as_dimension()) {
                out[i] = v;
                any = true;
            }
        }
        if any { Some(out) } else { None }
    }

    pub fn font_size(&self) -> f32 {
        self.get("font-size")
            .and_then(|v| v.as_px())
            .unwrap_or(16.0)
    }

    pub fn width(&self) -> Option<super::value::Dimension> {
        self.get("width").and_then(|v| v.as_dimension())
    }

    pub fn height(&self) -> Option<super::value::Dimension> {
        self.get("height").and_then(|v| v.as_dimension())
    }

    pub fn min_width(&self) -> Option<super::value::Dimension> {
        self.get("min-width").and_then(|v| v.as_dimension())
    }

    pub fn max_width(&self) -> Option<super::value::Dimension> {
        self.get("max-width").and_then(|v| v.as_dimension())
    }

    pub fn min_height(&self) -> Option<super::value::Dimension> {
        self.get("min-height").and_then(|v| v.as_dimension())
    }

    pub fn max_height(&self) -> Option<super::value::Dimension> {
        self.get("max-height").and_then(|v| v.as_dimension())
    }

    pub fn box_shadow(&self) -> Option<Shadows> {
        self.get("box-shadow")
            .and_then(|v| v.as_string())
            .and_then(|s| Shadows::parse(s))
    }

    pub fn animation_name(&self) -> Option<&str> {
        self.get("animation-name").and_then(|v| v.as_string())
    }

    pub fn animation_duration_ms(&self) -> Option<u32> {
        self.get("animation-duration").and_then(|v| match v {
            StyleValue::String(s) => {
                if s.ends_with("ms") {
                    s.trim_end_matches("ms").parse::<u32>().ok()
                } else if s.ends_with('s') {
                    s.trim_end_matches('s').parse::<f32>().ok().map(|v| (v * 1000.0) as u32)
                } else {
                    s.parse::<u32>().ok()
                }
            }
            StyleValue::Number(n) => Some(*n as u32),
            _ => None,
        })
    }

    pub fn animation_easing(&self) -> Option<&str> {
        self.get("animation-timing-function").and_then(|v| v.as_string())
    }

    pub fn animation_repeat(&self) -> Option<&str> {
        self.get("animation-iteration-count").and_then(|v| v.as_string())
    }

    pub fn animation_delay_ms(&self) -> Option<u32> {
        self.get("animation-delay").and_then(|v| match v {
            StyleValue::String(s) => {
                if s.ends_with("ms") {
                    s.trim_end_matches("ms").parse::<u32>().ok()
                } else if s.ends_with('s') {
                    s.trim_end_matches('s').parse::<f32>().ok().map(|v| (v * 1000.0) as u32)
                } else {
                    s.parse::<u32>().ok()
                }
            }
            StyleValue::Number(n) => Some(*n as u32),
            _ => None,
        })
    }

    pub fn animation_direction(&self) -> Option<&str> {
        self.get("animation-direction").and_then(|v| v.as_string())
    }

    pub fn animation_fill_mode(&self) -> Option<&str> {
        self.get("animation-fill-mode").and_then(|v| v.as_string())
    }

    pub fn animation_play_state(&self) -> Option<&str> {
        self.get("animation-play-state").and_then(|v| v.as_string())
    }

    pub fn opacity(&self) -> Option<f32> {
        self.get("opacity").and_then(|v| match v {
            StyleValue::Number(n) => Some(n.clamp(0.0, 1.0)),
            StyleValue::String(s) => s.parse::<f32>().ok().map(|n| n.clamp(0.0, 1.0)),
            _ => None,
        })
    }

    pub fn cursor(&self) -> Option<CursorIcon> {
        self.get("cursor").and_then(|v| v.as_string()).and_then(|s| match s {
            "pointer" => Some(CursorIcon::Pointer),
            "text" => Some(CursorIcon::Text),
            "move" => Some(CursorIcon::Move),
            "grab" => Some(CursorIcon::Grab),
            "grabbing" => Some(CursorIcon::Grabbing),
            "not-allowed" => Some(CursorIcon::NotAllowed),
            "crosshair" => Some(CursorIcon::Crosshair),
            "col-resize" => Some(CursorIcon::ColResize),
            "row-resize" => Some(CursorIcon::RowResize),
            "default" | "auto" => Some(CursorIcon::Default),
            _ => None,
        })
    }

    pub fn border_width(&self) -> f32 {
        self.get("border-top-width")
            .and_then(|v| v.as_px())
            .unwrap_or(0.0)
    }

    pub fn accent_color(&self) -> Option<Color> {
        self.get("accent-color").and_then(|v| v.as_color())
    }

    pub fn border_color(&self) -> Option<Color> {
        self.get("border-color").and_then(|v| v.as_color())
    }

    pub fn text_align(&self) -> Option<TextAlign> {
        let h = self.get("text-align").and_then(|v| v.as_string()).and_then(|s| match s {
            "left" => Some(TextAlign::LEFT),
            "center" => Some(TextAlign::HCENTER),
            "right" => Some(TextAlign::RIGHT),
            _ => None,
        });
        let v = self.get("text-vertical-align").and_then(|v| v.as_string()).and_then(|s| match s {
            "top" => Some(TextAlign::TOP),
            "center" => Some(TextAlign::VCENTER),
            "bottom" => Some(TextAlign::BOTTOM),
            _ => None,
        });
        match (h, v) {
            (Some(h), Some(v)) => Some(h | v),
            (Some(h), None) => Some(h | TextAlign::VCENTER),
            (None, Some(v)) => Some(TextAlign::LEFT | v),
            (None, None) => None,
        }
    }

    pub fn text_decoration(&self) -> Option<TextDecoration> {
        self.get("text-decoration").and_then(|v| v.as_string()).and_then(|s| match s {
            "none" => Some(TextDecoration::None),
            "underline" => Some(TextDecoration::Underline),
            "line-through" => Some(TextDecoration::LineThrough),
            _ => None,
        })
    }

    pub fn overflow(&self) -> Option<Overflow> {
        self.get("overflow").and_then(|v| v.as_string()).and_then(|s| match s {
            "hidden" => Some(Overflow::Hidden),
            "scroll" => Some(Overflow::Scroll),
            "visible" => Some(Overflow::Visible),
            _ => None,
        })
    }

    pub fn has_margin(&self) -> bool {
        self.get("margin-left").is_some()
            || self.get("margin-top").is_some()
            || self.get("margin-right").is_some()
            || self.get("margin-bottom").is_some()
    }

    pub fn margin(&self) -> EdgeInsets {
        EdgeInsets::new(
            self.get("margin-left").and_then(|v| v.as_px()).unwrap_or(0.0),
            self.get("margin-top").and_then(|v| v.as_px()).unwrap_or(0.0),
            self.get("margin-right").and_then(|v| v.as_px()).unwrap_or(0.0),
            self.get("margin-bottom").and_then(|v| v.as_px()).unwrap_or(0.0),
        )
    }

    pub fn flex_grow(&self) -> Option<f32> {
        self.get("flex-grow").and_then(|v| v.as_px()).map(|v| v.max(0.0))
    }

    pub fn font_weight(&self) -> Option<u16> {
        self.get("font-weight").and_then(|v| match v {
            StyleValue::Number(n) => Some(*n as u16),
            StyleValue::String(s) => match s.as_str() {
                "normal" => Some(400),
                "bold" => Some(700),
                "lighter" => Some(300),
                "bolder" => Some(800),
                _ => s.parse::<u16>().ok(),
            },
            _ => None,
        })
    }

    pub fn font_family(&self) -> Option<&str> {
        self.get("font-family").and_then(|v| v.as_string()).map(|s| {
            s.trim_matches('"').trim_matches('\'')
        })
    }

    pub fn transition_property(&self) -> Option<&str> {
        self.get("transition-property").and_then(|v| v.as_string())
            .or_else(|| {
                self.get("transition").and_then(|v| v.as_string()).and_then(|s| {
                    s.split_whitespace().next()
                })
            })
    }

    pub fn transition_duration_ms(&self) -> Option<u32> {
        self.get("transition-duration").and_then(|v| match v {
            StyleValue::String(s) => Self::parse_duration(s),
            StyleValue::Number(n) => Some(*n as u32),
            _ => None,
        }).or_else(|| {
            self.get("transition").and_then(|v| v.as_string()).and_then(|s| {
                s.split_whitespace().nth(1).and_then(Self::parse_duration)
            })
        })
    }

    pub fn transition_easing(&self) -> Option<&str> {
        self.get("transition-timing-function").and_then(|v| v.as_string())
            .or_else(|| {
                self.get("transition").and_then(|v| v.as_string()).and_then(|s| {
                    s.split_whitespace().nth(2)
                })
            })
    }

    fn parse_duration(s: &str) -> Option<u32> {
        if s.ends_with("ms") {
            s.trim_end_matches("ms").parse::<u32>().ok()
        } else if s.ends_with('s') {
            s.trim_end_matches('s').parse::<f32>().ok().map(|v| (v * 1000.0) as u32)
        } else {
            s.parse::<u32>().ok()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementState {
    Normal,
    Hover,
    Active,
    Focus,
    Selected,
    Checked,
    Disabled,
}

impl Default for ElementState {
    fn default() -> Self {
        ElementState::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mss::parser::MssParser;

    fn engine_from(src: &str) -> StyleEngine {
        let (sheet, _) = MssParser::new(src).parse().expect("parse");
        StyleEngine::new(sheet)
    }

    #[test]
    fn window_maximized_pseudo_overrides_base_when_flag_set() {
        let mut eng = engine_from(
            ".shell { padding: 30px; border-radius: 12px; } \
             .shell:window-maximized { padding: 0; border-radius: 0; }",
        );

        let ctx = StyleContext::with_class("shell");
        let s = eng.compute_style_with_state(&ctx, ElementState::Normal);
        assert_eq!(s.padding(), 30.0);
        assert_eq!(s.border_radius(), 12.0);

        let ctx = StyleContext::with_class("shell").with_window_flags(window_flags::MAXIMIZED);
        let s = eng.compute_style_with_state(&ctx, ElementState::Normal);
        assert_eq!(s.padding(), 0.0);
        assert_eq!(s.border_radius(), 0.0);

        let ctx = StyleContext::with_class("shell").with_window_flags(window_flags::FULLSCREEN);
        let s = eng.compute_style_with_state(&ctx, ElementState::Normal);
        assert_eq!(s.padding(), 30.0);
    }

    #[test]
    fn window_pseudo_applied_after_state_pseudo() {
        let mut eng = engine_from(
            ".btn { padding: 10px; } \
             .btn:hover { padding: 20px; } \
             .btn:window-maximized { padding: 0; }",
        );
        let ctx = StyleContext::with_class("btn").with_window_flags(window_flags::MAXIMIZED);
        let s = eng.compute_style_with_state(&ctx, ElementState::Hover);
        assert_eq!(s.padding(), 0.0);
    }

    #[test]
    fn parser_creates_class_pseudo_for_window_maximized() {
        use crate::mss::Selector;
        let (sheet, _) = MssParser::new(
            ".shell { border-radius: 20px; } \
             .shell:window-maximized { border-radius: 0; }",
        ).parse().expect("parse");
        let rules = sheet.rules();
        assert_eq!(rules.len(), 2);
        match &rules[0].selector {
            Selector::Class(c) => assert_eq!(c, "shell"),
            other => panic!("expected Class(\"shell\"), got {other:?}"),
        }
        match &rules[1].selector {
            Selector::ClassPseudo(c, p) => {
                assert_eq!(c, "shell");
                assert_eq!(p, "window-maximized");
            }
            other => panic!("expected ClassPseudo(\"shell\", \"window-maximized\"), got {other:?}"),
        }
    }

    #[test]
    fn window_focused_and_fullscreen_independent() {
        let mut eng = engine_from(
            ".x { color: #000; } \
             .x:window-focused { color: #f00; } \
             .x:window-fullscreen { color: #0f0; }",
        );
        let ctx = StyleContext::with_class("x").with_window_flags(window_flags::FOCUSED);
        let s = eng.compute_style_with_state(&ctx, ElementState::Normal);
        let c = s.color().expect("color");
        assert_eq!((c.r, c.g, c.b), (255, 0, 0));

        let ctx = StyleContext::with_class("x").with_window_flags(window_flags::FULLSCREEN);
        let s = eng.compute_style_with_state(&ctx, ElementState::Normal);
        let c = s.color().expect("color");
        assert_eq!((c.r, c.g, c.b), (0, 255, 0));
    }
}
