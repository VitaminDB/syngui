use crate::animation::transition::{AnimatedPropertyMap, mss_color_to_core, ResolvedProps, TransitionState};
use crate::core::{Color, Gradient, Shadows};
use crate::input::CursorIcon;
use crate::mss::parser::transform::TransformOrigin;
use crate::mss::style_engine::{ComputedStyle, Overflow, TextAlign, TextDecoration};
use crate::mss::value::{Dimension, StyleValue, Unit};
use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
pub const KNOWN_PROPERTIES_FOR_TESTS: &[&str] = KNOWN_PROPERTIES;

const KNOWN_PROPERTIES: &[&str] = &[
    "background", "background-color", "color", "border-color", "accent-color",
    "border-radius", "border-width", "border", "border-left-width", "border-top-width",
    "border-right-width", "border-bottom-width",
    "border-style", "border-top-style", "border-right-style",
    "border-bottom-style", "border-left-style",
    "border-left-color", "border-top-color",
    "border-right-color", "border-bottom-color",
    "border-top-left-radius", "border-top-right-radius",
    "border-bottom-right-radius", "border-bottom-left-radius",
    "width", "height", "min-width", "max-width", "min-height", "max-height",
    "padding", "padding-left", "padding-right", "padding-top", "padding-bottom",
    "margin", "margin-left", "margin-top", "margin-right", "margin-bottom",
    "font-size", "font-weight", "font-family", "icon-size",
    "icon-color", "icon-color-selected", "icon-color-hover", "icon-color-disabled",
    "icon-opacity",
    "selection-color",
    "caret-color",
    "line-height",
    "transform", "transform-origin",
    "translate-x", "translate-y", "rotate", "scale", "scale-x", "scale-y",
    "opacity", "cursor", "box-shadow", "overflow",
    "text-align", "text-vertical-align", "text-decoration",
    "letter-spacing", "text-transform", "text-shadow", "line-clamp",
    "gap",
    "transition", "transition-property", "transition-duration", "transition-timing-function",
    "animation",
    "animation-name", "animation-duration", "animation-timing-function", "animation-iteration-count",
    "animation-delay", "animation-direction", "animation-fill-mode", "animation-play-state",
    "filter", "backdrop-filter", "mix-blend-mode",
    "outline", "outline-width", "outline-color", "outline-offset",
    "glow", "color-tint", "noise", "vignette",
    "flex-grow",
    "grid-color", "axis-color", "axis-font-size",
    "title-font-size", "legend-font-size",
    "tooltip-background", "tooltip-border-color",
    "label-color", "label-font-size", "value-font-size",
    "track-color", "needle-color", "point-size",
    "divider-thickness",
    "scrollbar-width", "scrollbar-color", "scrollbar-thumb-hover-color",
    "scrollbar-track-color", "scrollbar-radius",
    "scrollbar-policy", "scrollbar-fade-delay",
    "editor-bg", "editor-fg", "editor-gutter-bg", "editor-gutter-fg",
    "editor-cursor", "editor-selection", "editor-current-line",
    "editor-bracket-match", "editor-whitespace",
    "editor-find-match", "editor-find-current",
    "token-keyword", "token-keyword-control",
    "token-type", "token-type-builtin",
    "token-function", "token-function-macro",
    "token-constant", "token-constant-builtin",
    "token-string", "token-string-special",
    "token-number", "token-comment", "token-operator", "token-punctuation",
    "token-variable", "token-property", "token-attribute",
    "token-tag", "token-namespace",
    "header-bg", "header-color", "header-font-size", "header-padding",
    "row-hover-bg", "row-selected-bg", "row-striped-bg",
    "row-padding", "row-padding-left", "row-padding-top",
    "row-padding-right", "row-padding-bottom",
    "cell-padding", "cell-font-size", "cell-min-width", "cell-max-width",
    "grid-alpha",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineHeight {
    Multiplier(f32),
    Px(f32),
}

impl LineHeight {
    pub fn resolve(&self, font_size: f32) -> f32 {
        match self {
            LineHeight::Multiplier(m) => font_size * *m,
            LineHeight::Px(v) => *v,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub color: Color,
}

impl TextShadow {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let mut nums = Vec::new();
        let mut rest = s;
        for _ in 0..3 {
            rest = rest.trim_start();
            let end = rest.find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
                .unwrap_or(rest.len());
            if end == 0 { break; }
            if let Ok(n) = rest[..end].parse::<f32>() {
                nums.push(n);
                rest = &rest[end..];
            } else {
                break;
            }
        }
        if nums.len() < 2 { return None; }
        let offset_x = nums[0];
        let offset_y = nums[1];
        let blur_radius = nums.get(2).copied().unwrap_or(0.0);
        let color_str = rest.trim();
        let color = if color_str.is_empty() {
            Color::new(0.0, 0.0, 0.0, 0.5)
        } else {
            parse_color_string(color_str)?
        };
        Some(TextShadow { offset_x, offset_y, blur_radius, color })
    }
}

fn parse_color_string(s: &str) -> Option<Color> {
    use crate::mss::value::Color as MssColor;
    MssColor::parse(s).map(|c| Color::new(
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Normal,
    Hover,
    Selected,
    Disabled,
}

#[derive(Clone, Debug)]
pub struct MssFields {
    pub background_color: Option<Color>,
    pub background_gradient: Option<Gradient>,
    pub color: Option<Color>,
    pub border_color: Option<Color>,
    pub accent_color: Option<Color>,

    pub border_radius: Option<[Dimension; 4]>,
    pub border_width: Option<f32>,
    pub border_widths: Option<[f32; 4]>,
    pub border_side_colors: [Option<Color>; 4],

    pub padding_left: Option<f32>,
    pub padding_right: Option<f32>,
    pub padding_top: Option<f32>,
    pub padding_bottom: Option<f32>,

    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub min_width: Option<Dimension>,
    pub max_width: Option<Dimension>,
    pub min_height: Option<Dimension>,
    pub max_height: Option<Dimension>,

    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub font_family: Option<String>,

    pub icon_size: Option<f32>,
    pub icon_color: Option<Color>,
    pub icon_color_selected: Option<Color>,
    pub icon_color_hover: Option<Color>,
    pub icon_color_disabled: Option<Color>,
    pub icon_opacity: Option<f32>,

    pub selection_color: Option<Color>,

    pub caret_color: Option<Color>,

    /// `clipboard-hint: on|off` — показывать при фокусе текстового поля
    /// всплывашку с текстом из буфера обмена (тап по ней вставляет текст).
    pub clipboard_hint: Option<bool>,

    pub line_height: Option<LineHeight>,

    pub transform_origin: Option<TransformOrigin>,

    pub opacity: Option<f32>,
    pub cursor: Option<CursorIcon>,
    pub box_shadow: Option<Shadows>,
    pub overflow: Option<Overflow>,

    pub filter: Option<Vec<crate::effects::FilterEffect>>,
    pub backdrop_filter: Option<Vec<crate::effects::FilterEffect>>,
    pub outline_width: Option<f32>,
    pub outline_color: Option<Color>,
    pub outline_offset: Option<f32>,
    pub glow: Option<Shadows>,
    pub color_tint: Option<Color>,
    pub gutter_color: Option<Color>,
    pub noise: Option<f32>,
    pub vignette: Option<f32>,
    pub blend_mode: Option<crate::render::display_list::BlendModeType>,

    pub text_align: Option<TextAlign>,
    pub text_decoration: Option<TextDecoration>,
    pub letter_spacing: Option<f32>,
    pub text_transform: Option<TextTransform>,
    pub text_shadow: Option<TextShadow>,

    pub gap: Option<f32>,
    pub flex_grow: Option<f32>,

    pub scrollbar_width: Option<f32>,
    pub scrollbar_color: Option<Color>,
    pub scrollbar_thumb_hover_color: Option<Color>,
    pub scrollbar_track_color: Option<Color>,
    pub scrollbar_radius: Option<f32>,
    pub scrollbar_policy: Option<crate::widgets::scroll::ScrollbarPolicy>,
    pub scrollbar_fade_delay: Option<f32>,

    pub divider_thickness: Option<f32>,

    pub transition: TransitionState,
    pub style_normal: Option<ResolvedProps>,
    pub style_hover: Option<ResolvedProps>,
    pub style_active: Option<ResolvedProps>,
    pub style_focus: Option<ResolvedProps>,
    pub style_selected: Option<ResolvedProps>,
    pub has_mss_styles: bool,
    pub filter_normal: Option<Vec<crate::effects::FilterEffect>>,
    pub filter_hover: Option<Vec<crate::effects::FilterEffect>>,
    pub shadow_normal: Option<crate::core::shadow::Shadows>,
    pub shadow_hover: Option<crate::core::shadow::Shadows>,
    pub glow_normal: Option<crate::core::shadow::Shadows>,
    pub glow_hover: Option<crate::core::shadow::Shadows>,
    pub keyframe_animation: Option<crate::animation::KeyframeAnimation>,
    pub animation_name: Option<String>,
    pub current_target: Option<ResolvedProps>,
}

impl Default for MssFields {
    fn default() -> Self {
        Self::new()
    }
}

impl MssFields {
    pub fn new() -> Self {
        Self {
            background_color: None,
            background_gradient: None,
            color: None,
            border_color: None,
            accent_color: None,
            border_radius: None,
            border_width: None,
            border_widths: None,
            border_side_colors: [None; 4],
            padding_left: None,
            padding_right: None,
            padding_top: None,
            padding_bottom: None,
            width: None,
            height: None,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            font_size: None,
            font_weight: None,
            font_family: None,
            icon_size: None,
            icon_color: None,
            icon_color_selected: None,
            icon_color_hover: None,
            icon_color_disabled: None,
            icon_opacity: None,
            selection_color: None,
            caret_color: None,
            clipboard_hint: None,
            line_height: None,
            transform_origin: None,
            opacity: None,
            cursor: None,
            box_shadow: None,
            overflow: None,
            filter: None,
            backdrop_filter: None,
            outline_width: None,
            outline_color: None,
            outline_offset: None,
            gutter_color: None,
            glow: None,
            color_tint: None,
            noise: None,
            vignette: None,
            blend_mode: None,
            text_align: None,
            text_decoration: None,
            letter_spacing: None,
            text_transform: None,
            text_shadow: None,
            gap: None,
            flex_grow: None,
            divider_thickness: None,
            scrollbar_width: None,
            scrollbar_color: None,
            scrollbar_thumb_hover_color: None,
            scrollbar_track_color: None,
            scrollbar_radius: None,
            scrollbar_policy: None,
            scrollbar_fade_delay: None,
            transition: TransitionState::new(),
            style_normal: None,
            style_hover: None,
            style_active: None,
            style_focus: None,
            style_selected: None,
            has_mss_styles: false,
            filter_normal: None,
            filter_hover: None,
            shadow_normal: None,
            shadow_hover: None,
            glow_normal: None,
            glow_hover: None,
            keyframe_animation: None,
            animation_name: None,
            current_target: None,
        }
    }

    pub fn reset(&mut self) {
        self.background_color = None;
        self.background_gradient = None;
        self.color = None;
        self.border_color = None;
        self.accent_color = None;
        self.border_radius = None;
        self.border_width = None;
        self.border_widths = None;
        self.border_side_colors = [None; 4];
        self.padding_left = None;
        self.padding_right = None;
        self.padding_top = None;
        self.padding_bottom = None;
        self.width = None;
        self.height = None;
        self.min_width = None;
        self.max_width = None;
        self.min_height = None;
        self.max_height = None;
        self.font_size = None;
        self.font_weight = None;
        self.font_family = None;
        self.icon_size = None;
        self.icon_color = None;
        self.icon_color_selected = None;
        self.icon_color_hover = None;
        self.icon_color_disabled = None;
        self.icon_opacity = None;
        self.selection_color = None;
        self.caret_color = None;
        self.line_height = None;
        self.transform_origin = None;
        self.opacity = None;
        self.cursor = None;
        self.box_shadow = None;
        self.overflow = None;
        self.filter = None;
        self.backdrop_filter = None;
        self.outline_width = None;
        self.outline_color = None;
        self.outline_offset = None;
        self.gutter_color = None;
        self.glow = None;
        self.color_tint = None;
        self.noise = None;
        self.vignette = None;
        self.blend_mode = None;
        self.text_align = None;
        self.text_decoration = None;
        self.letter_spacing = None;
        self.text_transform = None;
        self.text_shadow = None;
        self.gap = None;
        self.flex_grow = None;
        self.divider_thickness = None;
        self.scrollbar_width = None;
        self.scrollbar_color = None;
        self.scrollbar_thumb_hover_color = None;
        self.scrollbar_track_color = None;
        self.scrollbar_radius = None;
        self.scrollbar_policy = None;
        self.scrollbar_fade_delay = None;
        self.has_mss_styles = false;
        self.filter_normal = None;
        self.filter_hover = None;
        self.shadow_normal = None;
        self.shadow_hover = None;
        self.glow_normal = None;
        self.glow_hover = None;
        self.keyframe_animation = None;
        self.animation_name = None;
    }

    pub fn apply(&mut self, style: &ComputedStyle) {
        if let Some(grad) = style.background_gradient() {
            self.background_gradient = Some(grad.clone());
            self.background_color = None;
        } else if let Some(bg) = style.background_color() {
            self.background_color = Some(mss_color_to_core(bg));
            self.background_gradient = None;
        }
        if let Some(fg) = style.color() {
            self.color = Some(mss_color_to_core(fg));
        }
        if let Some(bc) = style.border_color() {
            self.border_color = Some(mss_color_to_core(bc));
        }
        if let Some(ac) = style.accent_color() {
            self.accent_color = Some(mss_color_to_core(ac));
        }

        if let Some(dims) = style.border_radius_dimensions() {
            self.border_radius = Some(dims);
        }
        let hidden_style = |value: Option<&str>| matches!(value, Some("none") | Some("hidden"));
        let all_hidden = hidden_style(style.get("border-style").and_then(|v| v.as_string()));
        let side_width = |width_prop: &str, style_prop: &str| {
            let w = style.get(width_prop).and_then(|v| v.as_px())?;
            let off = all_hidden || hidden_style(style.get(style_prop).and_then(|v| v.as_string()));
            Some(if off { 0.0 } else { w })
        };
        let bl = side_width("border-left-width", "border-left-style");
        let bt = side_width("border-top-width", "border-top-style");
        let br = side_width("border-right-width", "border-right-style");
        let bb = side_width("border-bottom-width", "border-bottom-style");
        if bl.is_some() || bt.is_some() || br.is_some() || bb.is_some() {
            self.border_widths = Some([
                bl.unwrap_or(0.0),
                bt.unwrap_or(0.0),
                br.unwrap_or(0.0),
                bb.unwrap_or(0.0),
            ]);
            if let (Some(a), Some(b), Some(c), Some(d)) = (bl, bt, br, bb) {
                if a == b && b == c && c == d {
                    self.border_width = Some(a);
                }
            }
        }
        for (i, prop) in [
            "border-left-color",
            "border-top-color",
            "border-right-color",
            "border-bottom-color",
        ]
        .iter()
        .enumerate()
        {
            if let Some(c) = style.get(prop).and_then(|v| v.as_color()) {
                self.border_side_colors[i] = Some(mss_color_to_core(c));
            }
        }

        if let Some(d) = style.width() { self.width = Some(d); }
        if let Some(d) = style.height() { self.height = Some(d); }
        if let Some(d) = style.min_width() { self.min_width = Some(d); }
        if let Some(d) = style.max_width() { self.max_width = Some(d); }
        if let Some(d) = style.min_height() { self.min_height = Some(d); }
        if let Some(d) = style.max_height() { self.max_height = Some(d); }

        self.padding_left = style.get("padding-left").and_then(|v| v.as_px());
        self.padding_right = style.get("padding-right").and_then(|v| v.as_px());
        self.padding_top = style.get("padding-top").and_then(|v| v.as_px());
        self.padding_bottom = style.get("padding-bottom").and_then(|v| v.as_px());

        if let Some(v) = style.get("font-size").and_then(|v| v.as_px()) {
            self.font_size = Some(v);
        }
        if let Some(w) = style.font_weight() {
            self.font_weight = Some(w);
        }
        if let Some(f) = style.font_family() {
            self.font_family = Some(f.to_string());
        }
        if let Some(v) = style.get("icon-size").and_then(|v| v.as_px()) {
            self.icon_size = Some(v);
        }
        if let Some(c) = style.get("icon-color").and_then(|v| v.as_color()) {
            self.icon_color = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(c) = style.get("icon-color-selected").and_then(|v| v.as_color()) {
            self.icon_color_selected = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(c) = style.get("icon-color-hover").and_then(|v| v.as_color()) {
            self.icon_color_hover = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(c) = style.get("icon-color-disabled").and_then(|v| v.as_color()) {
            self.icon_color_disabled = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(v) = style.get("icon-opacity").and_then(|v| v.as_px()) {
            self.icon_opacity = Some(v.clamp(0.0, 1.0));
        }
        if let Some(c) = style.get("selection-color").and_then(|v| v.as_color()) {
            self.selection_color = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(c) = style.get("caret-color").and_then(|v| v.as_color()) {
            self.caret_color = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(s) = style.get("clipboard-hint").and_then(|v| v.as_string()) {
            self.clipboard_hint = match s {
                "on" | "true" | "show" => Some(true),
                "off" | "false" | "none" | "hidden" => Some(false),
                _ => self.clipboard_hint,
            };
        }
        if let Some(v) = style.get("line-height") {
            self.line_height = match v {
                StyleValue::Number(m) => Some(LineHeight::Multiplier(*m)),
                StyleValue::Length(px, Unit::Px) => Some(LineHeight::Px(*px)),
                _ => None,
            };
        }
        if let Some(s) = style.get("transform-origin").and_then(|v| v.as_string()) {
            if let Some(origin) = TransformOrigin::parse(s) {
                self.transform_origin = Some(origin);
            }
        }

        if let Some(o) = style.opacity() { self.opacity = Some(o); }
        if let Some(c) = style.cursor() { self.cursor = Some(c); }
        if let Some(s) = style.box_shadow() { self.box_shadow = Some(s); }
        if let Some(o) = style.overflow() { self.overflow = Some(o); }

        if let Some(s) = style.get("filter").and_then(|v| v.as_string()) {
            let effects = crate::effects::parse_filter_chain(s);
            if !effects.is_empty() { self.filter = Some(effects); }
        }
        if let Some(s) = style.get("backdrop-filter").and_then(|v| v.as_string()) {
            let effects = crate::effects::parse_filter_chain(s);
            if !effects.is_empty() { self.backdrop_filter = Some(effects); }
        }
        if let Some(v) = style.get("outline-width").and_then(|v| v.as_px()) {
            self.outline_width = Some(v);
        }
        if let Some(c) = style.get("outline-color").and_then(|v| v.as_color()) {
            self.outline_color = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(v) = style.get("outline-offset").and_then(|v| v.as_px()) {
            self.outline_offset = Some(v);
        }
        if let Some(c) = style.get("gutter-color").and_then(|v| v.as_color()) {
            self.gutter_color = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(s) = style.get("glow").and_then(|v| v.as_string()) {
            let mut shadows = Vec::new();
            for part in s.split(',') {
                if let Some(shadow) = crate::core::Shadow::parse(part.trim()) {
                    shadows.push(shadow);
                }
            }
            if !shadows.is_empty() { self.glow = Some(crate::core::Shadows(shadows)); }
        }
        if let Some(c) = style.get("color-tint").and_then(|v| v.as_color()) {
            self.color_tint = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(v) = style.get("noise").and_then(|v| v.as_px()) {
            self.noise = Some(v.clamp(0.0, 1.0));
        }
        if let Some(v) = style.get("vignette").and_then(|v| v.as_px()) {
            self.vignette = Some(v.clamp(0.0, 1.0));
        }
        if let Some(s) = style.get("mix-blend-mode").and_then(|v| v.as_string()) {
            self.blend_mode = match s {
                "multiply" => Some(crate::render::display_list::BlendModeType::Multiply),
                "screen" => Some(crate::render::display_list::BlendModeType::Screen),
                "overlay" => Some(crate::render::display_list::BlendModeType::Overlay),
                "darken" => Some(crate::render::display_list::BlendModeType::Darken),
                "lighten" => Some(crate::render::display_list::BlendModeType::Lighten),
                "color-dodge" => Some(crate::render::display_list::BlendModeType::ColorDodge),
                "color-burn" => Some(crate::render::display_list::BlendModeType::ColorBurn),
                "hard-light" => Some(crate::render::display_list::BlendModeType::HardLight),
                "soft-light" => Some(crate::render::display_list::BlendModeType::SoftLight),
                "difference" => Some(crate::render::display_list::BlendModeType::Difference),
                "exclusion" => Some(crate::render::display_list::BlendModeType::Exclusion),
                _ => None,
            };
        }

        if let Some(a) = style.text_align() { self.text_align = Some(a); }
        if let Some(d) = style.text_decoration() { self.text_decoration = Some(d); }

        if let Some(v) = style.get("letter-spacing").and_then(|v| v.as_px()) {
            self.letter_spacing = Some(v);
        }

        if let Some(s) = style.get("text-transform").and_then(|v| v.as_string()) {
            self.text_transform = match s {
                "uppercase" => Some(TextTransform::Uppercase),
                "lowercase" => Some(TextTransform::Lowercase),
                "capitalize" => Some(TextTransform::Capitalize),
                "none" => Some(TextTransform::None),
                _ => None,
            };
        }

        if let Some(s) = style.get("text-shadow").and_then(|v| v.as_string()) {
            self.text_shadow = TextShadow::parse(s);
        }

        if let Some(g) = style.get("gap").and_then(|v| v.as_px()) {
            self.gap = Some(g);
        }
        if let Some(v) = style.get("flex-grow").and_then(|v| v.as_px()) {
            self.flex_grow = Some(v.max(0.0));
        }

        if let Some(v) = style.get("divider-thickness").and_then(|v| v.as_px()) {
            self.divider_thickness = Some(v.max(0.0));
        }

        if let Some(v) = style.get("scrollbar-width").and_then(|v| v.as_px()) {
            self.scrollbar_width = Some(v.max(0.0));
        }
        if let Some(c) = style.get("scrollbar-color").and_then(|v| v.as_color()) {
            self.scrollbar_color = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(c) = style.get("scrollbar-thumb-hover-color").and_then(|v| v.as_color()) {
            self.scrollbar_thumb_hover_color = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(c) = style.get("scrollbar-track-color").and_then(|v| v.as_color()) {
            self.scrollbar_track_color = Some(Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0));
        }
        if let Some(v) = style.get("scrollbar-radius").and_then(|v| v.as_px()) {
            self.scrollbar_radius = Some(v.max(0.0));
        }
        if let Some(s) = style.get("scrollbar-policy").and_then(|v| v.as_string()) {
            use crate::widgets::scroll::ScrollbarPolicy as P;
            self.scrollbar_policy = match s {
                "auto" => Some(P::Auto),
                "always" => Some(P::Always),
                "never" | "none" | "hidden" => Some(P::Never),
                _ => None,
            };
        }
        if let Some(v) = style.get("scrollbar-fade-delay") {
            self.scrollbar_fade_delay = match v {
                StyleValue::Number(n) => Some(n.max(0.0)),
                StyleValue::Length(px, Unit::Px) => Some(px.max(0.0)),
                _ => None,
            };
        }

        if let Some(name) = style.animation_name() {
            self.animation_name = Some(name.to_string());
        }

        static WARNED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
        let warned = WARNED.get_or_init(|| Mutex::new(HashSet::new()));
        for (prop, _) in style.properties() {
            if !prop.starts_with("--") && !KNOWN_PROPERTIES.contains(&prop) {
                if let Ok(mut set) = warned.lock() {
                    if set.insert(prop.to_string()) {
                        log::warn!("[MSS] Свойство '{}' не поддерживается и будет проигнорировано", prop);
                    }
                }
            }
        }
    }

    pub fn apply_transitions(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
    ) {
        let prev_normal = self.style_normal.clone();

        self.transition = TransitionState::parse_from_style(base);
        let new_normal = ResolvedProps::from_style(base);

        if self.transition.has_specs() {
            if let Some(ref old) = prev_normal {
                self.transition.start_transition(old, &new_normal);
            }
        }

        self.current_target = Some(new_normal.clone());
        self.style_normal = Some(new_normal);
        self.style_hover = hover.map(ResolvedProps::from_style);
        self.style_active = active.map(ResolvedProps::from_style);
        self.style_focus = focus.map(ResolvedProps::from_style);
        self.style_selected = selected.map(ResolvedProps::from_style);
        let has_paint = |s: &ResolvedProps| {
            s.background_color().is_some()
                || s.color().is_some()
                || s.border_color().is_some()
        };
        self.has_mss_styles =
            self.style_normal.as_ref().map(has_paint).unwrap_or(false)
                || self.style_hover.as_ref().map(has_paint).unwrap_or(false)
                || self.style_active.as_ref().map(has_paint).unwrap_or(false)
                || self.style_focus.as_ref().map(has_paint).unwrap_or(false)
                || self.style_selected.as_ref().map(has_paint).unwrap_or(false);

        self.filter_normal = Self::extract_filter(base);
        self.filter_hover = hover.and_then(Self::extract_filter);

        self.shadow_normal = base.box_shadow();
        self.shadow_hover = hover.and_then(|h| h.box_shadow());
        self.glow_normal = Self::extract_glow(base);
        self.glow_hover = hover.and_then(Self::extract_glow);

        if self.filter_hover.is_some() && self.filter_hover != self.filter_normal {
            self.has_mss_styles = true;
        }
        if self.shadow_hover.is_some() && self.shadow_hover != self.shadow_normal {
            self.has_mss_styles = true;
        }
        if self.glow_hover.is_some() && self.glow_hover != self.glow_normal {
            self.has_mss_styles = true;
        }
    }

    pub fn setup_keyframe_animation(
        &mut self,
        style: &ComputedStyle,
        stylesheet: &crate::mss::StyleSheet,
    ) {
        if let Some(ref name) = self.animation_name {
            if let Some(ref existing) = self.keyframe_animation {
                if existing.keyframes.name == *name && existing.is_running() {
                    return;
                }
            }

            if let Some(anim) = crate::animation::KeyframeAnimation::from_style(style, stylesheet) {
                self.keyframe_animation = Some(anim);
            }
        }
    }

    fn extract_filter(style: &ComputedStyle) -> Option<Vec<crate::effects::FilterEffect>> {
        style.get("filter")
            .and_then(|v| v.as_string())
            .map(|s| crate::effects::parse_filter_chain(s))
            .filter(|v| !v.is_empty())
    }

    fn extract_glow(style: &ComputedStyle) -> Option<crate::core::shadow::Shadows> {
        style.get("glow")
            .and_then(|v| v.as_string())
            .and_then(|s| crate::core::shadow::Shadows::parse(s))
    }

    pub fn padding_ltrb(&self, defaults: [f32; 4]) -> [f32; 4] {
        [
            self.padding_left.unwrap_or(defaults[0]),
            self.padding_top.unwrap_or(defaults[1]),
            self.padding_right.unwrap_or(defaults[2]),
            self.padding_bottom.unwrap_or(defaults[3]),
        ]
    }

    pub fn border_radius_resolved(&self, reference_size: f32, default: f32) -> [f32; 4] {
        match self.border_radius {
            Some(dims) => dims.map(|d| d.resolve(reference_size)),
            None => [default; 4],
        }
    }

    pub fn border_width_or(&self, default: f32) -> f32 {
        self.border_width.unwrap_or(default)
    }

    pub fn resolved_corner_radii(&self, bounds: crate::core::Rect) -> [f32; 4] {
        let reference = bounds.size.width.min(bounds.size.height);
        self.border_radius_resolved(reference, 0.0)
    }

    pub fn paint_background(&self, list: &mut crate::render::DisplayList, bounds: crate::core::Rect) {
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return;
        }
        let radii = self.resolved_corner_radii(bounds);
        if let Some(ref gradient) = self.background_gradient {
            list.push_gradient_rect(bounds, gradient.clone(), radii);
        } else if let Some(bg) = self.background_color {
            if bg.a > 0.0 {
                list.push_rect(bounds, bg, radii);
            }
        }
        if let Some(tint) = self.color_tint {
            list.push_rect(bounds, tint, radii);
        }
    }

    fn resolved_border_sides(&self) -> [Option<(f32, Color)>; 4] {
        let mut sides = [None; 4];
        let widths = match self.border_widths {
            Some(w) => w,
            None => match (self.border_width, self.border_color.or(self.side_color(0))) {
                (Some(w), Some(_)) if w > 0.0 => [w; 4],
                _ => return sides,
            },
        };
        for (i, w) in widths.iter().enumerate() {
            if *w > 0.0 {
                if let Some(color) = self.side_color(i) {
                    sides[i] = Some((*w, color));
                }
            }
        }
        sides
    }

    fn side_color(&self, index: usize) -> Option<Color> {
        self.border_side_colors[index].or(self.border_color)
    }

    pub fn paint_border(&self, list: &mut crate::render::DisplayList, bounds: crate::core::Rect) {
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return;
        }
        let sides = self.resolved_border_sides();
        if sides.iter().all(|s| s.is_none()) {
            return;
        }
        let radii = self.resolved_corner_radii(bounds);

        let uniform = sides.iter().all(|s| s.is_some()) && {
            let (w0, c0) = sides[0].unwrap();
            sides.iter().all(|s| s.map(|(w, c)| w == w0 && c == c0).unwrap_or(false))
        };
        if uniform {
            let (w, c) = sides[0].unwrap();
            let fill = Color::new(c.r, c.g, c.b, 0.0);
            list.push_rect_bordered(bounds, fill, radii, crate::render::Border::new(w, c));
            return;
        }

        let mut groups: Vec<(Color, [f32; 4])> = Vec::new();
        for (i, side) in sides.iter().enumerate() {
            if let Some((w, c)) = side {
                if let Some(g) = groups.iter_mut().find(|(gc, _)| gc == c) {
                    g.1[i] = *w;
                } else {
                    let mut widths = [0.0f32; 4];
                    widths[i] = *w;
                    groups.push((*c, widths));
                }
            }
        }
        for (color, widths) in &groups {
            list.push_rect_per_side_border(
                bounds,
                Color::TRANSPARENT,
                radii,
                None,
                crate::render::PerSideBorder { widths: *widths, color: *color },
            );
        }
    }

    pub fn border_radius_uniform(&self, reference_size: f32, default: f32) -> f32 {
        match self.border_radius {
            Some(dims) => dims[0].resolve(reference_size),
            None => default,
        }
    }

    pub fn target_props(&self, hovered: bool, pressed: bool, focused: bool, selected: bool) -> &ResolvedProps {
        if pressed {
            if let Some(ref p) = self.style_active { return p; }
        }
        if selected {
            if let Some(ref p) = self.style_selected { return p; }
        }
        if focused {
            if let Some(ref p) = self.style_focus { return p; }
        }
        if hovered {
            if let Some(ref p) = self.style_hover { return p; }
        }
        if let Some(ref p) = self.style_normal { return p; }
        static EMPTY: std::sync::OnceLock<ResolvedProps> = std::sync::OnceLock::new();
        EMPTY.get_or_init(ResolvedProps::new)
    }

    pub fn effective_bg(&self, target: &ResolvedProps, fallback: Color) -> Color {
        self.transition
            .background_color()
            .or(target.background_color())
            .or(self.style_normal.as_ref().and_then(|n| n.background_color()))
            .unwrap_or(fallback)
    }

    pub fn effective_fg(&self, target: &ResolvedProps, fallback: Color) -> Color {
        self.transition
            .color()
            .or(target.color())
            .or(self.style_normal.as_ref().and_then(|n| n.color()))
            .unwrap_or(fallback)
    }

    pub fn effective_border_color(&self, target: &ResolvedProps, fallback: Color) -> Color {
        self.transition
            .border_color()
            .or(target.border_color())
            .or(self.style_normal.as_ref().and_then(|n| n.border_color()))
            .unwrap_or(fallback)
    }

    pub fn effective_opacity(&self, target: &ResolvedProps) -> f32 {
        self.transition
            .opacity()
            .or(target.opacity())
            .or(self.style_normal.as_ref().and_then(|n| n.opacity()))
            .unwrap_or(1.0)
    }

    pub fn start_transition_to(&mut self, hovered: bool, pressed: bool, focused: bool, selected: bool) {
        if !self.has_mss_styles || !self.transition.has_specs() {
            return;
        }
        let normal = self.style_normal.clone().unwrap_or_default();
        let mut from = AnimatedPropertyMap::new();
        for (prop, val) in normal.iter() {
            let current = self.transition.get_animated_value(prop).unwrap_or(val);
            from.set(prop, current);
        }
        let target = self.target_props(hovered, pressed, focused, selected).clone();
        self.current_target = Some(target.clone());
        self.transition.start_transition(&from, &target);

        let old_filter = self.filter_normal.as_deref().unwrap_or(&[]);
        let new_filter = if hovered {
            self.filter_hover.as_deref()
                .unwrap_or(self.filter_normal.as_deref().unwrap_or(&[]))
        } else {
            self.filter_normal.as_deref().unwrap_or(&[])
        };
        self.transition.start_filter_transition(old_filter, new_filter);

        let empty_shadows = crate::core::shadow::Shadows::new();
        let old_shadow = self.transition.box_shadow()
            .or_else(|| self.shadow_normal.clone())
            .unwrap_or_else(|| empty_shadows.clone());
        let new_shadow = if hovered {
            self.shadow_hover.as_ref()
                .or(self.shadow_normal.as_ref())
                .cloned()
                .unwrap_or_else(|| empty_shadows.clone())
        } else {
            self.shadow_normal.as_ref().cloned().unwrap_or_else(|| empty_shadows.clone())
        };
        self.transition.start_shadow_transition("box-shadow", &old_shadow, &new_shadow);

        let old_glow = self.transition.glow()
            .or_else(|| self.glow_normal.clone())
            .unwrap_or_else(|| empty_shadows.clone());
        let new_glow = if hovered {
            self.glow_hover.as_ref()
                .or(self.glow_normal.as_ref())
                .cloned()
                .unwrap_or(empty_shadows)
        } else {
            self.glow_normal.as_ref().cloned().unwrap_or(empty_shadows)
        };
        self.transition.start_shadow_transition("glow", &old_glow, &new_glow);
    }

    pub fn font_size_or(&self, default: f32) -> f32 {
        self.font_size.unwrap_or(default)
    }

    pub fn icon_color(&self, state: IconState, fallback: Color) -> Color {
        let (specific, base_fallback) = match state {
            IconState::Selected => (self.icon_color_selected, self.accent_color),
            IconState::Hover => (self.icon_color_hover, None),
            IconState::Disabled => (self.icon_color_disabled, None),
            IconState::Normal => (None, None),
        };

        let specific_was_set = specific.is_some();
        let mut color = specific
            .or(base_fallback)
            .or(self.icon_color)
            .or(self.color)
            .unwrap_or(fallback);

        if state == IconState::Disabled && !specific_was_set {
            color = color.with_alpha(color.a * 0.38);
        }

        if let Some(op) = self.icon_opacity {
            color = color.with_alpha(color.a * op.clamp(0.0, 1.0));
        }

        color
    }

    pub fn font_weight_or(&self, default: u16) -> u16 {
        self.font_weight.unwrap_or(default)
    }

    pub fn selection_color_or_default(&self) -> Color {
        self.selection_color
            .unwrap_or(Color::new(0.231, 0.510, 0.965, 0.30))
    }

    pub fn letter_spacing_or(&self, default: f32) -> f32 {
        self.letter_spacing.unwrap_or(default)
    }

    pub fn caret_color_or(&self, fallback: Color) -> Color {
        self.caret_color
            .or(self.accent_color)
            .or(self.color)
            .unwrap_or(fallback)
    }

    pub fn line_height_or(&self, font_size: f32, default_multiplier: f32) -> f32 {
        self.line_height
            .map(|lh| lh.resolve(font_size))
            .unwrap_or(font_size * default_multiplier)
    }

    pub fn scrollbar_style(&self, fg: Color) -> crate::widgets::scroll::ScrollbarStyle {
        let mut style = crate::widgets::scroll::ScrollbarStyle::with_foreground(fg);
        if let Some(w) = self.scrollbar_width { style.width = w; }
        if let Some(c) = self.scrollbar_color { style.thumb_color = c; }
        if let Some(c) = self.scrollbar_thumb_hover_color {
            style.thumb_hover_color = c;
        } else if self.scrollbar_color.is_some() {
            style.thumb_hover_color = style.thumb_color.with_alpha((style.thumb_color.a * 1.7).min(1.0));
        }
        if let Some(c) = self.scrollbar_track_color { style.track_color = c; }
        if let Some(r) = self.scrollbar_radius {
            style.corner_radius = r;
        } else {
            style.corner_radius = style.width / 2.0;
        }
        if let Some(p) = self.scrollbar_policy { style.policy = p; }
        if let Some(d) = self.scrollbar_fade_delay { style.fade_delay = d; }
        style
    }

    pub fn transform_origin_or_center(&self, size: crate::core::Size) -> crate::core::Point {
        let o = self.transform_origin.unwrap_or(TransformOrigin::CENTER);
        crate::core::Point::new(o.x.resolve(size.width), o.y.resolve(size.height))
    }

    pub fn compute_active_transform(&self, bounds: crate::core::Rect) -> Option<crate::core::Transform> {
        let resolve = |kf: Option<f32>, tr: Option<f32>, tg: Option<f32>| -> Option<f32> {
            kf.or(tr).or(tg)
        };

        let kf_vals = self.keyframe_animation.as_ref()
            .filter(|a| a.is_running())
            .map(|a| a.current_values());

        let tg = self.current_target.as_ref();
        let tx = resolve(
            kf_vals.as_ref().and_then(|v| v.translate_x()),
            self.transition.translate_x(),
            tg.and_then(|t| t.translate_x()),
        ).unwrap_or(0.0);
        let ty = resolve(
            kf_vals.as_ref().and_then(|v| v.translate_y()),
            self.transition.translate_y(),
            tg.and_then(|t| t.translate_y()),
        ).unwrap_or(0.0);
        let rot_deg = resolve(
            kf_vals.as_ref().and_then(|v| v.rotate()),
            self.transition.rotate(),
            tg.and_then(|t| t.rotate()),
        ).unwrap_or(0.0);
        let su = resolve(
            kf_vals.as_ref().and_then(|v| v.scale()),
            self.transition.scale(),
            tg.and_then(|t| t.scale()),
        ).unwrap_or(1.0);
        let sxi = resolve(
            kf_vals.as_ref().and_then(|v| v.scale_x()),
            self.transition.scale_x(),
            tg.and_then(|t| t.scale_x()),
        ).unwrap_or(1.0);
        let syi = resolve(
            kf_vals.as_ref().and_then(|v| v.scale_y()),
            self.transition.scale_y(),
            tg.and_then(|t| t.scale_y()),
        ).unwrap_or(1.0);
        let sx = su * sxi;
        let sy = su * syi;

        let has_transform = tx != 0.0 || ty != 0.0 || rot_deg != 0.0
            || (sx - 1.0).abs() > f32::EPSILON || (sy - 1.0).abs() > f32::EPSILON;
        if !has_transform {
            return None;
        }

        let origin = self.transform_origin_or_center(bounds.size);
        let cx = bounds.origin.x + origin.x;
        let cy = bounds.origin.y + origin.y;
        let needs_origin = sx != 1.0 || sy != 1.0 || rot_deg != 0.0;

        let mut t = crate::core::Transform::identity();
        if needs_origin {
            t = t.then(&crate::core::Transform::translation(-cx, -cy));
        }
        if sx != 1.0 || sy != 1.0 {
            t = t.then(&crate::core::Transform::new(sx, 0.0, 0.0, sy, 0.0, 0.0));
        }
        if rot_deg != 0.0 {
            let radians = rot_deg * std::f32::consts::PI / 180.0;
            t = t.then_rotate(euclid::Angle::radians(radians));
        }
        if needs_origin {
            t = t.then_translate(euclid::Vector2D::new(cx + tx, cy + ty));
        } else {
            t = t.then_translate(euclid::Vector2D::new(tx, ty));
        }
        Some(t)
    }

    pub fn transform_text<'a>(&self, text: &'a str) -> Cow<'a, str> {
        match self.text_transform {
            Some(TextTransform::Uppercase) => Cow::Owned(text.to_uppercase()),
            Some(TextTransform::Lowercase) => Cow::Owned(text.to_lowercase()),
            Some(TextTransform::Capitalize) => {
                let mut result = String::with_capacity(text.len());
                let mut capitalize_next = true;
                for c in text.chars() {
                    if capitalize_next && c.is_alphabetic() {
                        for uc in c.to_uppercase() {
                            result.push(uc);
                        }
                        capitalize_next = false;
                    } else {
                        result.push(c);
                        if c.is_whitespace() {
                            capitalize_next = true;
                        }
                    }
                }
                Cow::Owned(result)
            }
            _ => Cow::Borrowed(text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_defaults() {
        let f = MssFields::new();
        assert!(f.background_color.is_none());
        assert!(f.color.is_none());
        assert_eq!(f.border_width, None);
        assert!(f.padding_left.is_none());
        assert!(f.width.is_none());
        assert!(f.font_size.is_none());
        assert!(!f.has_mss_styles);
    }

    #[test]
    fn test_padding_ltrb_defaults() {
        let f = MssFields::new();
        let p = f.padding_ltrb([8.0, 4.0, 8.0, 4.0]);
        assert_eq!(p, [8.0, 4.0, 8.0, 4.0]);
    }

    #[test]
    fn test_padding_ltrb_overrides() {
        let mut f = MssFields::new();
        f.padding_left = Some(16.0);
        f.padding_top = Some(12.0);
        let p = f.padding_ltrb([8.0, 4.0, 8.0, 4.0]);
        assert_eq!(p, [16.0, 12.0, 8.0, 4.0]);
    }

    #[test]
    fn test_border_radius_resolved() {
        let f = MssFields::new();
        let r = f.border_radius_resolved(100.0, 6.0);
        assert_eq!(r, [6.0; 4]);
    }

    #[test]
    fn test_border_radius_px() {
        let mut f = MssFields::new();
        f.border_radius = Some([Dimension::Px(10.0); 4]);
        let r = f.border_radius_resolved(100.0, 0.0);
        assert_eq!(r, [10.0; 4]);
    }

    #[test]
    fn test_border_radius_percent() {
        let mut f = MssFields::new();
        f.border_radius = Some([Dimension::Percent(50.0); 4]);
        let r = f.border_radius_resolved(100.0, 0.0);
        assert_eq!(r, [50.0; 4]);
    }

    #[test]
    fn test_effective_bg_fallback() {
        let f = MssFields::new();
        let target = ResolvedProps::new();
        let c = f.effective_bg(&target, Color::WHITE);
        assert_eq!(c, Color::WHITE);
    }

    #[test]
    fn test_effective_bg_target() {
        let f = MssFields::new();
        let target = ResolvedProps::new()
            .with_color("background-color", Color::RED);
        let c = f.effective_bg(&target, Color::WHITE);
        assert_eq!(c, Color::RED);
    }

    #[test]
    fn test_target_props_priority() {
        let mut f = MssFields::new();
        let normal = ResolvedProps::new().with_color("background-color", Color::WHITE);
        let hover = ResolvedProps::new().with_color("background-color", Color::BLUE);
        let active = ResolvedProps::new().with_color("background-color", Color::RED);
        f.style_normal = Some(normal);
        f.style_hover = Some(hover);
        f.style_active = Some(active);

        let t = f.target_props(false, false, false, false);
        assert_eq!(t.background_color(), Some(Color::WHITE));
        let t = f.target_props(true, false, false, false);
        assert_eq!(t.background_color(), Some(Color::BLUE));
        let t = f.target_props(true, true, false, false);
        assert_eq!(t.background_color(), Some(Color::RED));
    }

    #[test]
    fn test_flex_grow_from_number() {
        let mut style = ComputedStyle::new();
        style.set("flex-grow", crate::mss::StyleValue::Number(2.5));
        let mut f = MssFields::new();
        f.apply(&style);
        assert_eq!(f.flex_grow, Some(2.5));
    }

    #[test]
    fn test_flex_grow_absent_is_none() {
        let style = ComputedStyle::new();
        let mut f = MssFields::new();
        f.apply(&style);
        assert!(f.flex_grow.is_none());
    }

    #[test]
    fn test_flex_grow_negative_clamped_to_zero() {
        let mut style = ComputedStyle::new();
        style.set("flex-grow", crate::mss::StyleValue::Number(-1.0));
        let mut f = MssFields::new();
        f.apply(&style);
        assert_eq!(f.flex_grow, Some(0.0));
    }

    #[test]
    fn test_font_size_or() {
        let mut f = MssFields::new();
        assert_eq!(f.font_size_or(14.0), 14.0);
        f.font_size = Some(18.0);
        assert_eq!(f.font_size_or(14.0), 18.0);
    }

    #[test]
    fn test_icon_color_normal_falls_back_to_color() {
        let mut f = MssFields::new();
        f.color = Some(Color::RED);
        let c = f.icon_color(IconState::Normal, Color::WHITE);
        assert_eq!(c, Color::RED);
    }

    #[test]
    fn test_icon_color_selected_falls_back_to_accent() {
        let mut f = MssFields::new();
        f.color = Some(Color::RED);
        f.accent_color = Some(Color::BLUE);
        let c = f.icon_color(IconState::Selected, Color::WHITE);
        assert_eq!(c, Color::BLUE);
    }

    #[test]
    fn test_icon_color_specific_override_beats_accent() {
        let mut f = MssFields::new();
        f.accent_color = Some(Color::BLUE);
        f.icon_color_selected = Some(Color::GREEN);
        let c = f.icon_color(IconState::Selected, Color::WHITE);
        assert_eq!(c, Color::GREEN);
    }

    #[test]
    fn test_icon_opacity_multiplier() {
        let mut f = MssFields::new();
        f.color = Some(Color::WHITE);
        f.icon_opacity = Some(0.5);
        let c = f.icon_color(IconState::Normal, Color::BLACK);
        assert!((c.a - 0.5).abs() < 1e-6, "opacity multiplier ignored: a = {}", c.a);
    }

    #[test]
    fn test_icon_color_disabled_default_alpha_038() {
        let mut f = MssFields::new();
        f.color = Some(Color::WHITE);
        let c = f.icon_color(IconState::Disabled, Color::BLACK);
        assert!((c.a - 0.38).abs() < 1e-6, "expected 0.38 alpha for disabled, got {}", c.a);
        f.icon_color_disabled = Some(Color::RED);
        let c = f.icon_color(IconState::Disabled, Color::BLACK);
        assert!((c.a - 1.0).abs() < 1e-6, "explicit disabled color should keep its alpha, got {}", c.a);
    }

    #[test]
    fn test_apply_icon_color_from_style() {
        let mut style = ComputedStyle::new();
        style.set("icon-color", crate::core::Color::new(0.2, 0.4, 0.6, 1.0).into());
        style.set("icon-opacity", crate::mss::StyleValue::Number(0.5));
        let mut f = MssFields::new();
        f.apply(&style);
        assert!(f.icon_color.is_some(), "icon-color не извлечён из ComputedStyle");
        assert_eq!(f.icon_opacity, Some(0.5));
    }

    #[test]
    fn test_single_side_border_does_not_leak_to_other_sides() {
        let mut style = ComputedStyle::new();
        style.set(
            "border-bottom-width",
            crate::mss::StyleValue::Length(1.0, crate::mss::Unit::Px),
        );
        let mut f = MssFields::new();
        f.apply(&style);
        let bw = f.border_widths.expect("per-side set when any side set");
        assert_eq!(bw, [0.0, 0.0, 0.0, 1.0]);
        assert!(f.border_width.is_none());
    }

    #[test]
    fn test_uniform_border_width_sets_scalar() {
        let mut style = ComputedStyle::new();
        for side in [
            "border-top-width",
            "border-right-width",
            "border-bottom-width",
            "border-left-width",
        ] {
            style.set(side, crate::mss::StyleValue::Length(2.0, crate::mss::Unit::Px));
        }
        let mut f = MssFields::new();
        f.apply(&style);
        assert_eq!(f.border_widths, Some([2.0, 2.0, 2.0, 2.0]));
        assert_eq!(f.border_width, Some(2.0));
    }

    #[test]
    fn test_per_side_border_color_extracted() {
        let mut style = ComputedStyle::new();
        style.set(
            "border-bottom-width",
            crate::mss::StyleValue::Length(1.0, crate::mss::Unit::Px),
        );
        style.set(
            "border-bottom-color",
            StyleValue::Color(crate::mss::MssColor::rgb(255, 0, 0)),
        );
        let mut f = MssFields::new();
        f.apply(&style);
        let c = f.border_side_colors[3].expect("border-bottom-color не извлечён");
        assert!((c.r - 1.0).abs() < 1e-3);
        assert!(f.border_side_colors[1].is_none());
    }

    #[test]
    fn test_border_style_none_zeroes_side_width() {
        let mut style = ComputedStyle::new();
        style.set(
            "border-bottom-width",
            crate::mss::StyleValue::Length(3.0, crate::mss::Unit::Px),
        );
        style.set(
            "border-bottom-style",
            StyleValue::String("none".to_string()),
        );
        let mut f = MssFields::new();
        f.apply(&style);
        assert_eq!(f.border_widths, Some([0.0, 0.0, 0.0, 0.0]));
    }

    #[test]
    fn test_uniform_border_style_none_zeroes_all_sides() {
        let mut style = ComputedStyle::new();
        for side in [
            "border-top-width",
            "border-right-width",
            "border-bottom-width",
            "border-left-width",
        ] {
            style.set(side, crate::mss::StyleValue::Length(2.0, crate::mss::Unit::Px));
        }
        style.set("border-style", StyleValue::String("none".to_string()));
        let mut f = MssFields::new();
        f.apply(&style);
        assert_eq!(f.border_widths, Some([0.0, 0.0, 0.0, 0.0]));
        assert_eq!(f.border_width, Some(0.0));
    }

    #[test]
    fn caret_color_extracted_from_style() {
        let mut style = ComputedStyle::default();
        style.set(
            "caret-color",
            StyleValue::Color(crate::mss::MssColor::rgb(255, 0, 0)),
        );
        let mut f = MssFields::new();
        f.apply(&style);
        assert!(f.caret_color.is_some());
        let c = f.caret_color.unwrap();
        assert!((c.r - 1.0).abs() < 1e-3);
        assert!(c.g.abs() < 1e-3);
        assert!(c.b.abs() < 1e-3);
    }

    #[test]
    fn caret_color_or_chains_through_accent_color() {
        let mut f = MssFields::new();
        f.accent_color = Some(Color::new(0.0, 1.0, 0.0, 1.0));
        let c = f.caret_color_or(Color::WHITE);
        assert_eq!(c, Color::new(0.0, 1.0, 0.0, 1.0));
    }

    #[test]
    fn caret_color_or_caret_overrides_accent() {
        let mut f = MssFields::new();
        f.accent_color = Some(Color::new(0.0, 1.0, 0.0, 1.0));
        f.caret_color = Some(Color::new(1.0, 0.0, 1.0, 1.0));
        let c = f.caret_color_or(Color::WHITE);
        assert_eq!(c, Color::new(1.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn line_height_multiplier_extracted() {
        let mut style = ComputedStyle::default();
        style.set("line-height", StyleValue::Number(1.5));
        let mut f = MssFields::new();
        f.apply(&style);
        assert_eq!(f.line_height, Some(LineHeight::Multiplier(1.5)));
        assert_eq!(f.line_height_or(16.0, 1.3), 24.0);
    }

    #[test]
    fn line_height_px_extracted() {
        let mut style = ComputedStyle::default();
        style.set("line-height", StyleValue::Length(20.0, Unit::Px));
        let mut f = MssFields::new();
        f.apply(&style);
        assert_eq!(f.line_height, Some(LineHeight::Px(20.0)));
        assert_eq!(f.line_height_or(16.0, 1.3), 20.0);
    }

    #[test]
    fn line_height_default_uses_font_size_multiplier() {
        let f = MssFields::new();
        assert_eq!(f.line_height_or(16.0, 1.3), 16.0 * 1.3);
    }

    #[test]
    fn transform_origin_default_is_center() {
        let f = MssFields::new();
        let p = f.transform_origin_or_center(crate::core::Size::new(100.0, 50.0));
        assert_eq!(p.x, 50.0);
        assert_eq!(p.y, 25.0);
    }

    #[test]
    fn transform_origin_extracted_from_style() {
        let mut style = ComputedStyle::default();
        style.set("transform-origin", StyleValue::String("top left".to_string()));
        let mut f = MssFields::new();
        f.apply(&style);
        let origin = f.transform_origin.expect("parsed");
        let p_size = crate::core::Size::new(100.0, 100.0);
        let p = f.transform_origin_or_center(p_size);
        assert_eq!(p.x, 0.0);
        assert_eq!(p.y, 0.0);
        let _ = origin;
    }
}
