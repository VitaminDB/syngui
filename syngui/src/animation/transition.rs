use std::collections::{HashMap, HashSet};

use crate::animation::Easing;
use crate::core::Color;
use crate::core::shadow::Shadows;
use crate::effects::FilterEffect;
use crate::mss::{ComputedStyle, StyleValue};

#[derive(Clone, Debug, PartialEq)]
pub enum AnimatedValue {
    Color(Color),
    Float(f32),
    Shadows(Shadows),
    FilterChain(Vec<FilterEffect>),
    None,
}

impl AnimatedValue {
    pub fn lerp(&self, other: &AnimatedValue, t: f32) -> AnimatedValue {
        match (self, other) {
            (AnimatedValue::Color(a), AnimatedValue::Color(b)) => {
                AnimatedValue::Color(a.lerp(b, t))
            }
            (AnimatedValue::Float(a), AnimatedValue::Float(b)) => {
                AnimatedValue::Float(a + (b - a) * t)
            }
            (AnimatedValue::Shadows(a), AnimatedValue::Shadows(b)) => {
                AnimatedValue::Shadows(a.lerp(b, t))
            }
            (AnimatedValue::FilterChain(a), AnimatedValue::FilterChain(b)) => {
                AnimatedValue::FilterChain(crate::effects::lerp_filter_chains(a, b, t))
            }
            _ => other.clone(),
        }
    }

    pub fn from_style_value(sv: &StyleValue, property: &str) -> AnimatedValue {
        match sv {
            StyleValue::Color(c) => AnimatedValue::Color(mss_color_to_core(*c)),
            StyleValue::Number(n) => AnimatedValue::Float(*n),
            StyleValue::Length(v, crate::mss::Unit::Px) => AnimatedValue::Float(*v),
            StyleValue::String(s) => {
                match classify_property(property) {
                    PropertyType::Color => {
                        crate::mss::MssColor::parse(s)
                            .map(|c| AnimatedValue::Color(mss_color_to_core(c)))
                            .unwrap_or(AnimatedValue::None)
                    }
                    PropertyType::Float => {
                        let s = s.trim().trim_end_matches("px");
                        s.trim_end_matches('%').parse::<f32>().ok()
                            .map(AnimatedValue::Float)
                            .unwrap_or(AnimatedValue::None)
                    }
                    PropertyType::Shadows => {
                        Shadows::parse(s)
                            .map(AnimatedValue::Shadows)
                            .unwrap_or(AnimatedValue::None)
                    }
                    PropertyType::FilterChain => {
                        let chain = crate::effects::parse_filter_chain(s);
                        if chain.is_empty() { AnimatedValue::None }
                        else { AnimatedValue::FilterChain(chain) }
                    }
                    PropertyType::NonAnimatable => AnimatedValue::None,
                }
            }
            _ => AnimatedValue::None,
        }
    }

    pub fn default_for_property(property: &str) -> AnimatedValue {
        match classify_property(property) {
            PropertyType::Color => AnimatedValue::Color(Color::TRANSPARENT),
            PropertyType::Float => {
                match property {
                    "opacity" => AnimatedValue::Float(1.0),
                    "scale" | "scale-x" | "scale-y" => AnimatedValue::Float(1.0),
                    _ => AnimatedValue::Float(0.0),
                }
            }
            PropertyType::Shadows => AnimatedValue::Shadows(Shadows::new()),
            PropertyType::FilterChain => AnimatedValue::FilterChain(Vec::new()),
            PropertyType::NonAnimatable => AnimatedValue::None,
        }
    }
}

pub type AnimatableValue = AnimatedValue;

#[derive(Clone, Copy, Debug, PartialEq)]
enum PropertyType {
    Color,
    Float,
    Shadows,
    FilterChain,
    NonAnimatable,
}

fn classify_property(name: &str) -> PropertyType {
    match name {
        "background-color" | "background" | "color" | "border-color"
        | "outline-color" | "accent-color" | "color-tint" => PropertyType::Color,

        "opacity" | "outline-width" | "outline-offset" | "border-width"
        | "font-size" | "gap" | "icon-size" | "letter-spacing"
        | "padding" | "padding-left" | "padding-right" | "padding-top" | "padding-bottom"
        | "margin" | "margin-left" | "margin-right" | "margin-top" | "margin-bottom"
        | "width" | "height" | "min-width" | "max-width" | "min-height" | "max-height"
        | "noise" | "vignette"
        | "border-radius"
        | "translate-x" | "translate-y" | "rotate" | "scale" | "scale-x" | "scale-y" => PropertyType::Float,

        "box-shadow" | "glow" => PropertyType::Shadows,
        "filter" | "backdrop-filter" => PropertyType::FilterChain,

        _ => PropertyType::NonAnimatable,
    }
}

#[derive(Clone, Debug, Default)]
pub struct AnimatedPropertyMap {
    background_color: Option<Color>,
    color: Option<Color>,
    border_color: Option<Color>,
    outline_color: Option<Color>,
    opacity: Option<f32>,
    border_width: Option<f32>,
    outline_width: Option<f32>,
    outline_offset: Option<f32>,
    translate_x: Option<f32>,
    translate_y: Option<f32>,
    rotate: Option<f32>,
    scale: Option<f32>,
    scale_x: Option<f32>,
    scale_y: Option<f32>,
    box_shadow: Option<Shadows>,
    glow: Option<Shadows>,
    filter: Option<Vec<FilterEffect>>,
    extras: HashMap<String, AnimatedValue>,
}

pub type ResolvedProps = AnimatedPropertyMap;

impl AnimatedPropertyMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_style(style: &ComputedStyle) -> Self {
        let mut map = Self::new();
        for (name, sv) in style.properties() {
            let av = AnimatedValue::from_style_value(sv, name);
            if !matches!(av, AnimatedValue::None) {
                map.set(name, av);
            }
        }
        map
    }

    pub fn set(&mut self, property: &str, value: AnimatedValue) {
        if matches!(value, AnimatedValue::None) {
            self.remove(property);
            return;
        }
        match (property, value) {
            ("background-color" | "background", AnimatedValue::Color(c)) => self.background_color = Some(c),
            ("color", AnimatedValue::Color(c)) => self.color = Some(c),
            ("border-color", AnimatedValue::Color(c)) => self.border_color = Some(c),
            ("outline-color", AnimatedValue::Color(c)) => self.outline_color = Some(c),
            ("opacity", AnimatedValue::Float(f)) => self.opacity = Some(f),
            ("border-width", AnimatedValue::Float(f)) => self.border_width = Some(f),
            ("outline-width", AnimatedValue::Float(f)) => self.outline_width = Some(f),
            ("outline-offset", AnimatedValue::Float(f)) => self.outline_offset = Some(f),
            ("translate-x", AnimatedValue::Float(f)) => self.translate_x = Some(f),
            ("translate-y", AnimatedValue::Float(f)) => self.translate_y = Some(f),
            ("rotate", AnimatedValue::Float(f)) => self.rotate = Some(f),
            ("scale", AnimatedValue::Float(f)) => self.scale = Some(f),
            ("scale-x", AnimatedValue::Float(f)) => self.scale_x = Some(f),
            ("scale-y", AnimatedValue::Float(f)) => self.scale_y = Some(f),
            ("box-shadow", AnimatedValue::Shadows(s)) => self.box_shadow = Some(s),
            ("glow", AnimatedValue::Shadows(s)) => self.glow = Some(s),
            ("filter", AnimatedValue::FilterChain(f)) => self.filter = Some(f),
            (name, value) => {
                self.extras.insert(name.to_string(), value);
            }
        }
    }

    pub fn remove(&mut self, property: &str) {
        match property {
            "background-color" | "background" => self.background_color = None,
            "color" => self.color = None,
            "border-color" => self.border_color = None,
            "outline-color" => self.outline_color = None,
            "opacity" => self.opacity = None,
            "border-width" => self.border_width = None,
            "outline-width" => self.outline_width = None,
            "outline-offset" => self.outline_offset = None,
            "translate-x" => self.translate_x = None,
            "translate-y" => self.translate_y = None,
            "rotate" => self.rotate = None,
            "scale" => self.scale = None,
            "scale-x" => self.scale_x = None,
            "scale-y" => self.scale_y = None,
            "box-shadow" => self.box_shadow = None,
            "glow" => self.glow = None,
            "filter" => self.filter = None,
            other => { self.extras.remove(other); }
        }
    }

    pub fn get(&self, property: &str) -> AnimatedValue {
        match property {
            "background-color" | "background" => self.background_color.map(AnimatedValue::Color).unwrap_or(AnimatedValue::None),
            "color" => self.color.map(AnimatedValue::Color).unwrap_or(AnimatedValue::None),
            "border-color" => self.border_color.map(AnimatedValue::Color).unwrap_or(AnimatedValue::None),
            "outline-color" => self.outline_color.map(AnimatedValue::Color).unwrap_or(AnimatedValue::None),
            "opacity" => self.opacity.map(AnimatedValue::Float).unwrap_or(AnimatedValue::None),
            "border-width" => self.border_width.map(AnimatedValue::Float).unwrap_or(AnimatedValue::None),
            "outline-width" => self.outline_width.map(AnimatedValue::Float).unwrap_or(AnimatedValue::None),
            "outline-offset" => self.outline_offset.map(AnimatedValue::Float).unwrap_or(AnimatedValue::None),
            "translate-x" => self.translate_x.map(AnimatedValue::Float).unwrap_or(AnimatedValue::None),
            "translate-y" => self.translate_y.map(AnimatedValue::Float).unwrap_or(AnimatedValue::None),
            "rotate" => self.rotate.map(AnimatedValue::Float).unwrap_or(AnimatedValue::None),
            "scale" => self.scale.map(AnimatedValue::Float).unwrap_or(AnimatedValue::None),
            "scale-x" => self.scale_x.map(AnimatedValue::Float).unwrap_or(AnimatedValue::None),
            "scale-y" => self.scale_y.map(AnimatedValue::Float).unwrap_or(AnimatedValue::None),
            "box-shadow" => self.box_shadow.clone().map(AnimatedValue::Shadows).unwrap_or(AnimatedValue::None),
            "glow" => self.glow.clone().map(AnimatedValue::Shadows).unwrap_or(AnimatedValue::None),
            "filter" => self.filter.clone().map(AnimatedValue::FilterChain).unwrap_or(AnimatedValue::None),
            other => self.extras.get(other).cloned().unwrap_or(AnimatedValue::None),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.background_color.is_none()
            && self.color.is_none()
            && self.border_color.is_none()
            && self.outline_color.is_none()
            && self.opacity.is_none()
            && self.border_width.is_none()
            && self.outline_width.is_none()
            && self.outline_offset.is_none()
            && self.translate_x.is_none()
            && self.translate_y.is_none()
            && self.rotate.is_none()
            && self.scale.is_none()
            && self.scale_x.is_none()
            && self.scale_y.is_none()
            && self.box_shadow.is_none()
            && self.glow.is_none()
            && self.filter.is_none()
            && self.extras.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, AnimatedValue)> + '_ {
        let mut out: Vec<(&str, AnimatedValue)> = Vec::new();
        if let Some(c) = self.background_color { out.push(("background-color", AnimatedValue::Color(c))); }
        if let Some(c) = self.color { out.push(("color", AnimatedValue::Color(c))); }
        if let Some(c) = self.border_color { out.push(("border-color", AnimatedValue::Color(c))); }
        if let Some(c) = self.outline_color { out.push(("outline-color", AnimatedValue::Color(c))); }
        if let Some(f) = self.opacity { out.push(("opacity", AnimatedValue::Float(f))); }
        if let Some(f) = self.border_width { out.push(("border-width", AnimatedValue::Float(f))); }
        if let Some(f) = self.outline_width { out.push(("outline-width", AnimatedValue::Float(f))); }
        if let Some(f) = self.outline_offset { out.push(("outline-offset", AnimatedValue::Float(f))); }
        if let Some(f) = self.translate_x { out.push(("translate-x", AnimatedValue::Float(f))); }
        if let Some(f) = self.translate_y { out.push(("translate-y", AnimatedValue::Float(f))); }
        if let Some(f) = self.rotate { out.push(("rotate", AnimatedValue::Float(f))); }
        if let Some(f) = self.scale { out.push(("scale", AnimatedValue::Float(f))); }
        if let Some(f) = self.scale_x { out.push(("scale-x", AnimatedValue::Float(f))); }
        if let Some(f) = self.scale_y { out.push(("scale-y", AnimatedValue::Float(f))); }
        if let Some(ref s) = self.box_shadow { out.push(("box-shadow", AnimatedValue::Shadows(s.clone()))); }
        if let Some(ref s) = self.glow { out.push(("glow", AnimatedValue::Shadows(s.clone()))); }
        if let Some(ref f) = self.filter { out.push(("filter", AnimatedValue::FilterChain(f.clone()))); }
        out.extend(self.extras.iter().map(|(k, v)| (k.as_str(), v.clone())));
        out.into_iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> + '_ {
        self.iter().map(|(k, _)| k).collect::<Vec<_>>().into_iter()
    }

    pub fn with_color(mut self, property: &str, color: Color) -> Self {
        self.set(property, AnimatedValue::Color(color));
        self
    }

    pub fn with_float(mut self, property: &str, value: f32) -> Self {
        self.set(property, AnimatedValue::Float(value));
        self
    }

    pub fn set_color(&mut self, property: &str, color: Color) {
        self.set(property, AnimatedValue::Color(color));
    }

    pub fn set_float(&mut self, property: &str, value: f32) {
        self.set(property, AnimatedValue::Float(value));
    }

    pub fn background_color(&self) -> Option<Color> { self.background_color }
    pub fn color(&self) -> Option<Color> { self.color }
    pub fn border_color(&self) -> Option<Color> { self.border_color }
    pub fn outline_color(&self) -> Option<Color> { self.outline_color }

    pub fn opacity(&self) -> Option<f32> { self.opacity }
    pub fn outline_width(&self) -> Option<f32> { self.outline_width }
    pub fn outline_offset(&self) -> Option<f32> { self.outline_offset }
    pub fn border_width(&self) -> Option<f32> { self.border_width }

    pub fn translate_x(&self) -> Option<f32> { self.translate_x }
    pub fn translate_y(&self) -> Option<f32> { self.translate_y }
    pub fn rotate(&self) -> Option<f32> { self.rotate }
    pub fn scale(&self) -> Option<f32> { self.scale }
    pub fn scale_x(&self) -> Option<f32> { self.scale_x }
    pub fn scale_y(&self) -> Option<f32> { self.scale_y }

    pub fn filter(&self) -> Option<Vec<FilterEffect>> { self.filter.clone() }
    pub fn box_shadow(&self) -> Option<Shadows> { self.box_shadow.clone() }
    pub fn glow(&self) -> Option<Shadows> { self.glow.clone() }
}

#[derive(Clone, Debug)]
pub struct TransitionSpec {
    pub property: String,
    pub duration_secs: f32,
    pub easing: Easing,
    pub delay_secs: f32,
}

#[derive(Clone, Debug)]
struct PropertyTransition {
    property: String,
    from: AnimatedValue,
    to: AnimatedValue,
    duration: f32,
    elapsed: f32,
    delay: f32,
    easing: Easing,
}

impl PropertyTransition {
    fn progress(&self) -> f32 {
        if self.elapsed < self.delay {
            return 0.0;
        }
        let t = ((self.elapsed - self.delay) / self.duration).clamp(0.0, 1.0);
        self.easing.apply(t)
    }

    fn current_value(&self) -> AnimatedValue {
        self.from.lerp(&self.to, self.progress())
    }

    fn is_complete(&self) -> bool {
        self.elapsed >= self.delay + self.duration
    }
}

#[derive(Clone, Debug, Default)]
pub struct TransitionState {
    specs: Vec<TransitionSpec>,
    active: Vec<PropertyTransition>,
}

impl TransitionState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse_from_style(style: &ComputedStyle) -> Self {
        let mut specs = Vec::new();

        if let Some(transition_str) = style.get("transition").and_then(|v| v.as_string()) {
            for part in transition_str.split(',') {
                if let Some(spec) = parse_transition_shorthand(part.trim()) {
                    specs.push(spec);
                }
            }
        } else if let Some(dur) = style.transition_duration_ms() {
            let prop = style.transition_property().unwrap_or("all");
            let easing = style.transition_easing()
                .map(easing_from_str)
                .unwrap_or(Easing::CSS_EASE);
            specs.push(TransitionSpec {
                property: prop.to_string(),
                duration_secs: dur as f32 / 1000.0,
                easing,
                delay_secs: 0.0,
            });
        }

        Self { specs, active: Vec::new() }
    }

    pub fn has_specs(&self) -> bool {
        !self.specs.is_empty()
    }

    pub fn add_default_specs(&mut self, duration_ms: f32) {
        if self.specs.is_empty() {
            self.specs.push(TransitionSpec {
                property: "all".to_string(),
                duration_secs: duration_ms / 1000.0,
                easing: Easing::CSS_EASE,
                delay_secs: 0.0,
            });
        }
    }

    pub fn start_transition(&mut self, old_props: &AnimatedPropertyMap, new_props: &AnimatedPropertyMap) {
        let all_keys: HashSet<&str> = old_props.keys().chain(new_props.keys()).collect();

        for prop_name in all_keys {
            let spec = match self.find_spec(prop_name) {
                Some(s) => s.clone(),
                None => continue,
            };

            let old_val = old_props.get(prop_name);
            let new_val = new_props.get(prop_name);

            let default = AnimatedValue::default_for_property(prop_name);
            let old_val = if matches!(old_val, AnimatedValue::None) { default.clone() } else { old_val };
            let new_val = if matches!(new_val, AnimatedValue::None) { default } else { new_val };

            if matches!(old_val, AnimatedValue::None) || matches!(new_val, AnimatedValue::None) {
                continue;
            }
            if old_val == new_val {
                self.active.retain(|t| t.property != prop_name);
                continue;
            }

            let from = self.get_animated_value(prop_name).unwrap_or(old_val);
            self.active.retain(|t| t.property != prop_name);
            self.active.push(PropertyTransition {
                property: prop_name.to_string(),
                from,
                to: new_val,
                duration: spec.duration_secs,
                elapsed: 0.0,
                delay: spec.delay_secs,
                easing: spec.easing,
            });
        }
    }

    pub fn tick(&mut self, dt_secs: f32) -> bool {
        for t in &mut self.active {
            t.elapsed += dt_secs;
        }
        self.active.retain(|t| !t.is_complete());
        !self.active.is_empty()
    }

    pub fn is_animating(&self) -> bool {
        !self.active.is_empty()
    }

    pub fn get_animated_value(&self, property: &str) -> Option<AnimatedValue> {
        self.active
            .iter()
            .find(|t| t.property == property)
            .map(|t| t.current_value())
    }

    pub fn background_color(&self) -> Option<Color> {
        self.get_animated_color("background-color")
    }

    pub fn color(&self) -> Option<Color> {
        self.get_animated_color("color")
    }

    pub fn border_color(&self) -> Option<Color> {
        self.get_animated_color("border-color")
    }

    pub fn outline_color(&self) -> Option<Color> {
        self.get_animated_color("outline-color")
    }

    pub fn opacity(&self) -> Option<f32> {
        self.get_animated_float("opacity")
    }

    pub fn outline_width(&self) -> Option<f32> {
        self.get_animated_float("outline-width")
    }

    pub fn border_width(&self) -> Option<f32> {
        self.get_animated_float("border-width")
    }

    pub fn translate_x(&self) -> Option<f32> {
        self.get_animated_float("translate-x")
    }

    pub fn translate_y(&self) -> Option<f32> {
        self.get_animated_float("translate-y")
    }

    pub fn rotate(&self) -> Option<f32> {
        self.get_animated_float("rotate")
    }

    pub fn scale(&self) -> Option<f32> {
        self.get_animated_float("scale")
    }

    pub fn scale_x(&self) -> Option<f32> {
        self.get_animated_float("scale-x")
    }

    pub fn scale_y(&self) -> Option<f32> {
        self.get_animated_float("scale-y")
    }

    pub fn filter_chain(&self) -> Option<Vec<FilterEffect>> {
        match self.get_animated_value("filter")? {
            AnimatedValue::FilterChain(f) => Some(f),
            _ => None,
        }
    }

    pub fn box_shadow(&self) -> Option<Shadows> {
        match self.get_animated_value("box-shadow")? {
            AnimatedValue::Shadows(s) => Some(s),
            _ => None,
        }
    }

    pub fn glow(&self) -> Option<Shadows> {
        match self.get_animated_value("glow")? {
            AnimatedValue::Shadows(s) => Some(s),
            _ => None,
        }
    }

    pub fn start_filter_transition(
        &mut self,
        old_filter: &[FilterEffect],
        new_filter: &[FilterEffect],
    ) {
        let mut old = AnimatedPropertyMap::new();
        let mut new = AnimatedPropertyMap::new();
        if !old_filter.is_empty() {
            old.set("filter", AnimatedValue::FilterChain(old_filter.to_vec()));
        }
        if !new_filter.is_empty() {
            new.set("filter", AnimatedValue::FilterChain(new_filter.to_vec()));
        }
        self.start_transition(&old, &new);
    }

    pub fn start_shadow_transition(
        &mut self,
        property: &str,
        old_shadow: &Shadows,
        new_shadow: &Shadows,
    ) {
        let mut old = AnimatedPropertyMap::new();
        let mut new = AnimatedPropertyMap::new();
        if !old_shadow.is_empty() {
            old.set(property, AnimatedValue::Shadows(old_shadow.clone()));
        }
        if !new_shadow.is_empty() {
            new.set(property, AnimatedValue::Shadows(new_shadow.clone()));
        }
        self.start_transition(&old, &new);
    }

    fn get_animated_color(&self, property: &str) -> Option<Color> {
        match self.get_animated_value(property)? {
            AnimatedValue::Color(c) => Some(c),
            _ => None,
        }
    }

    fn get_animated_float(&self, property: &str) -> Option<f32> {
        match self.get_animated_value(property)? {
            AnimatedValue::Float(f) => Some(f),
            _ => None,
        }
    }

    fn find_spec(&self, property: &str) -> Option<&TransitionSpec> {
        if let Some(spec) = self.specs.iter().find(|s| s.property == property) {
            return Some(spec);
        }
        let alias = match property {
            "background-color" => Some("background"),
            "background" => Some("background-color"),
            "translate-x" | "translate-y" | "rotate"
            | "scale" | "scale-x" | "scale-y" => Some("transform"),
            _ => None,
        };
        if let Some(alias) = alias {
            if let Some(spec) = self.specs.iter().find(|s| s.property == alias) {
                return Some(spec);
            }
        }
        self.specs.iter().find(|s| s.property == "all")
    }
}

pub fn mss_color_to_core(c: crate::mss::MssColor) -> Color {
    Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0)
}

pub fn easing_from_str(s: &str) -> Easing {
    match s.trim() {
        "linear" => Easing::Linear,
        "ease" => Easing::CSS_EASE,
        "ease-in" => Easing::CSS_EASE_IN,
        "ease-out" => Easing::CSS_EASE_OUT,
        "ease-in-out" => Easing::CSS_EASE_IN_OUT,
        "ease-in-sine" => Easing::EaseInSine,
        "ease-out-sine" => Easing::EaseOutSine,
        "ease-in-out-sine" => Easing::EaseInOutSine,
        "ease-in-quad" => Easing::EaseInQuad,
        "ease-out-quad" => Easing::EaseOutQuad,
        "ease-in-out-quad" => Easing::EaseInOutQuad,
        "ease-in-cubic" => Easing::EaseInCubic,
        "ease-out-cubic" => Easing::EaseOutCubic,
        "ease-in-out-cubic" => Easing::EaseInOutCubic,
        "ease-in-back" => Easing::EaseInBack,
        "ease-out-back" => Easing::EaseOutBack,
        "ease-in-out-back" => Easing::EaseInOutBack,
        "ease-in-bounce" => Easing::EaseInBounce,
        "ease-out-bounce" => Easing::EaseOutBounce,
        "ease-in-out-bounce" => Easing::EaseInOutBounce,
        "ease-in-elastic" => Easing::EaseInElastic,
        "ease-out-elastic" => Easing::EaseOutElastic,
        "ease-in-out-elastic" => Easing::EaseInOutElastic,
        _ => Easing::CSS_EASE,
    }
}

fn parse_transition_shorthand(s: &str) -> Option<TransitionSpec> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let property = parts[0].to_string();
    let mut duration_secs = 0.2;
    let mut easing = Easing::CSS_EASE;
    let mut delay_secs = 0.0;

    if parts.len() > 1 {
        duration_secs = parse_duration_secs(parts[1]).unwrap_or(0.2);
    }
    if parts.len() > 2 {
        easing = easing_from_str(parts[2]);
    }
    if parts.len() > 3 {
        delay_secs = parse_duration_secs(parts[3]).unwrap_or(0.0);
    }

    Some(TransitionSpec {
        property,
        duration_secs,
        easing,
        delay_secs,
    })
}

fn parse_duration_secs(s: &str) -> Option<f32> {
    if s.ends_with("ms") {
        s.trim_end_matches("ms").parse::<f32>().ok().map(|v| v / 1000.0)
    } else if s.ends_with('s') {
        s.trim_end_matches('s').parse::<f32>().ok()
    } else {
        s.parse::<f32>().ok().map(|v| v / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ts() -> TransitionState {
        TransitionState {
            specs: vec![TransitionSpec {
                property: "all".to_string(),
                duration_secs: 0.1,
                easing: Easing::Linear,
                delay_secs: 0.0,
            }],
            active: Vec::new(),
        }
    }

    #[test]
    fn animated_color_lerp() {
        let a = AnimatedValue::Color(Color::BLACK);
        let b = AnimatedValue::Color(Color::WHITE);
        if let AnimatedValue::Color(c) = a.lerp(&b, 0.5) {
            assert!((c.r - 0.5).abs() < 1e-5);
            assert!((c.g - 0.5).abs() < 1e-5);
            assert!((c.b - 0.5).abs() < 1e-5);
        } else {
            panic!("expected Color");
        }
    }

    #[test]
    fn animated_color_lerp_zero() {
        let a = AnimatedValue::Color(Color::RED);
        let b = AnimatedValue::Color(Color::BLUE);
        if let AnimatedValue::Color(c) = a.lerp(&b, 0.0) {
            assert_eq!(c, Color::RED);
        } else {
            panic!("expected Color");
        }
    }

    #[test]
    fn animated_color_lerp_one() {
        let a = AnimatedValue::Color(Color::RED);
        let b = AnimatedValue::Color(Color::BLUE);
        if let AnimatedValue::Color(c) = a.lerp(&b, 1.0) {
            assert_eq!(c, Color::BLUE);
        } else {
            panic!("expected Color");
        }
    }

    #[test]
    fn animated_float_lerp() {
        let a = AnimatedValue::Float(0.0);
        let b = AnimatedValue::Float(1.0);
        if let AnimatedValue::Float(f) = a.lerp(&b, 0.5) {
            assert!((f - 0.5).abs() < 1e-5);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn animated_float_lerp_endpoints() {
        let a = AnimatedValue::Float(10.0);
        let b = AnimatedValue::Float(20.0);
        assert_eq!(a.lerp(&b, 0.0), AnimatedValue::Float(10.0));
        assert_eq!(a.lerp(&b, 1.0), AnimatedValue::Float(20.0));
    }

    #[test]
    fn animated_mismatched_types_returns_other() {
        let a = AnimatedValue::Color(Color::RED);
        let b = AnimatedValue::Float(1.0);
        assert_eq!(a.lerp(&b, 0.5), AnimatedValue::Float(1.0));
    }

    #[test]
    fn animated_none_lerp() {
        let a = AnimatedValue::None;
        let b = AnimatedValue::Float(1.0);
        assert_eq!(a.lerp(&b, 0.5), AnimatedValue::Float(1.0));
    }

    #[test]
    fn property_map_get_background_color() {
        let props = AnimatedPropertyMap::new()
            .with_color("background-color", Color::RED);
        assert_eq!(props.get("background-color"), AnimatedValue::Color(Color::RED));
        assert_eq!(props.background_color(), Some(Color::RED));
    }

    #[test]
    fn property_map_get_color() {
        let props = AnimatedPropertyMap::new()
            .with_color("color", Color::BLUE);
        assert_eq!(props.get("color"), AnimatedValue::Color(Color::BLUE));
    }

    #[test]
    fn property_map_get_border_color() {
        let props = AnimatedPropertyMap::new()
            .with_color("border-color", Color::GREEN);
        assert_eq!(props.get("border-color"), AnimatedValue::Color(Color::GREEN));
    }

    #[test]
    fn property_map_get_opacity() {
        let props = AnimatedPropertyMap::new()
            .with_float("opacity", 0.5);
        assert_eq!(props.get("opacity"), AnimatedValue::Float(0.5));
    }

    #[test]
    fn property_map_get_none_when_missing() {
        let props = AnimatedPropertyMap::new();
        assert_eq!(props.get("background-color"), AnimatedValue::None);
        assert_eq!(props.get("color"), AnimatedValue::None);
        assert_eq!(props.get("unknown"), AnimatedValue::None);
    }

    #[test]
    fn easing_from_str_common() {
        assert_eq!(easing_from_str("linear"), Easing::Linear);
        assert_eq!(easing_from_str("ease"), Easing::CSS_EASE);
        assert_eq!(easing_from_str("ease-in"), Easing::CSS_EASE_IN);
        assert_eq!(easing_from_str("ease-out"), Easing::CSS_EASE_OUT);
        assert_eq!(easing_from_str("ease-in-out"), Easing::CSS_EASE_IN_OUT);
    }

    #[test]
    fn easing_from_str_sine() {
        assert_eq!(easing_from_str("ease-in-sine"), Easing::EaseInSine);
        assert_eq!(easing_from_str("ease-out-sine"), Easing::EaseOutSine);
        assert_eq!(easing_from_str("ease-in-out-sine"), Easing::EaseInOutSine);
    }

    #[test]
    fn easing_from_str_quad() {
        assert_eq!(easing_from_str("ease-in-quad"), Easing::EaseInQuad);
        assert_eq!(easing_from_str("ease-out-quad"), Easing::EaseOutQuad);
        assert_eq!(easing_from_str("ease-in-out-quad"), Easing::EaseInOutQuad);
    }

    #[test]
    fn easing_from_str_cubic() {
        assert_eq!(easing_from_str("ease-in-cubic"), Easing::EaseInCubic);
        assert_eq!(easing_from_str("ease-out-cubic"), Easing::EaseOutCubic);
        assert_eq!(easing_from_str("ease-in-out-cubic"), Easing::EaseInOutCubic);
    }

    #[test]
    fn easing_from_str_bounce() {
        assert_eq!(easing_from_str("ease-in-bounce"), Easing::EaseInBounce);
        assert_eq!(easing_from_str("ease-out-bounce"), Easing::EaseOutBounce);
        assert_eq!(easing_from_str("ease-in-out-bounce"), Easing::EaseInOutBounce);
    }

    #[test]
    fn easing_from_str_elastic() {
        assert_eq!(easing_from_str("ease-in-elastic"), Easing::EaseInElastic);
        assert_eq!(easing_from_str("ease-out-elastic"), Easing::EaseOutElastic);
        assert_eq!(easing_from_str("ease-in-out-elastic"), Easing::EaseInOutElastic);
    }

    #[test]
    fn easing_from_str_back() {
        assert_eq!(easing_from_str("ease-in-back"), Easing::EaseInBack);
        assert_eq!(easing_from_str("ease-out-back"), Easing::EaseOutBack);
        assert_eq!(easing_from_str("ease-in-out-back"), Easing::EaseInOutBack);
    }

    #[test]
    fn easing_from_str_unknown_defaults() {
        assert_eq!(easing_from_str("unknown"), Easing::CSS_EASE);
        assert_eq!(easing_from_str(""), Easing::CSS_EASE);
    }

    #[test]
    fn easing_from_str_trims_whitespace() {
        assert_eq!(easing_from_str("  linear  "), Easing::Linear);
    }

    #[test]
    fn parse_shorthand_basic() {
        let spec = parse_transition_shorthand("background-color 200ms ease").unwrap();
        assert_eq!(spec.property, "background-color");
        assert!((spec.duration_secs - 0.2).abs() < 1e-5);
        assert_eq!(spec.easing, Easing::CSS_EASE);
        assert_eq!(spec.delay_secs, 0.0);
    }

    #[test]
    fn parse_shorthand_with_delay() {
        let spec = parse_transition_shorthand("opacity 300ms linear 100ms").unwrap();
        assert_eq!(spec.property, "opacity");
        assert!((spec.duration_secs - 0.3).abs() < 1e-5);
        assert_eq!(spec.easing, Easing::Linear);
        assert!((spec.delay_secs - 0.1).abs() < 1e-5);
    }

    #[test]
    fn parse_shorthand_seconds() {
        let spec = parse_transition_shorthand("all 0.5s ease-out").unwrap();
        assert_eq!(spec.property, "all");
        assert!((spec.duration_secs - 0.5).abs() < 1e-5);
    }

    #[test]
    fn parse_shorthand_property_only() {
        let spec = parse_transition_shorthand("color").unwrap();
        assert_eq!(spec.property, "color");
        assert!((spec.duration_secs - 0.2).abs() < 1e-5);
    }

    #[test]
    fn parse_shorthand_empty() {
        assert!(parse_transition_shorthand("").is_none());
    }

    #[test]
    fn parse_duration_ms() {
        assert!((parse_duration_secs("200ms").unwrap() - 0.2).abs() < 1e-5);
        assert!((parse_duration_secs("1000ms").unwrap() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn parse_duration_s() {
        assert!((parse_duration_secs("0.5s").unwrap() - 0.5).abs() < 1e-5);
        assert!((parse_duration_secs("2s").unwrap() - 2.0).abs() < 1e-5);
    }

    #[test]
    fn parse_duration_bare_number() {
        assert!((parse_duration_secs("300").unwrap() - 0.3).abs() < 1e-5);
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(parse_duration_secs("abc").is_none());
    }

    #[test]
    fn transition_state_new_empty() {
        let ts = TransitionState::new();
        assert!(!ts.has_specs());
        assert!(!ts.is_animating());
    }

    #[test]
    fn transition_state_no_active_accessors() {
        let ts = TransitionState::new();
        assert!(ts.background_color().is_none());
        assert!(ts.color().is_none());
        assert!(ts.border_color().is_none());
        assert!(ts.opacity().is_none());
    }

    #[test]
    fn transition_state_start_and_tick() {
        let mut ts = make_ts();
        let old = AnimatedPropertyMap::new()
            .with_color("background-color", Color::BLACK)
            .with_float("opacity", 0.0);
        let new = AnimatedPropertyMap::new()
            .with_color("background-color", Color::WHITE)
            .with_float("opacity", 1.0);

        ts.start_transition(&old, &new);
        assert!(ts.is_animating());

        ts.tick(0.05);
        let bg = ts.background_color().unwrap();
        assert!(bg.r > 0.3 && bg.r < 0.7, "bg should be mid-transition, r={}", bg.r);

        let opacity = ts.opacity().unwrap();
        assert!(opacity > 0.3 && opacity < 0.7, "opacity should be mid: {}", opacity);
    }

    #[test]
    fn transition_state_completes() {
        let mut ts = make_ts();
        let old = AnimatedPropertyMap::new()
            .with_color("background-color", Color::BLACK);
        let new = AnimatedPropertyMap::new()
            .with_color("background-color", Color::WHITE);

        ts.start_transition(&old, &new);
        let still_animating = ts.tick(0.2);
        assert!(!still_animating);
        assert!(!ts.is_animating());
        assert!(ts.background_color().is_none());
    }

    #[test]
    fn transition_same_values_no_animation() {
        let mut ts = make_ts();
        let props = AnimatedPropertyMap::new()
            .with_color("background-color", Color::RED);
        ts.start_transition(&props, &props);
        assert!(!ts.is_animating());
    }

    #[test]
    fn transition_find_spec_all() {
        let ts = make_ts();
        assert!(ts.has_specs());
    }

    #[test]
    fn transition_smooth_interruption() {
        let mut ts = make_ts();
        let black = AnimatedPropertyMap::new()
            .with_color("background-color", Color::BLACK);
        let white = AnimatedPropertyMap::new()
            .with_color("background-color", Color::WHITE);

        ts.start_transition(&black, &white);
        ts.tick(0.05);

        let mid = ts.background_color().unwrap();
        assert!(mid.r > 0.3, "should be mid-transition");

        ts.start_transition(&white, &black);
        let start = ts.background_color().unwrap();
        assert!(start.r > 0.3, "interruption should start from current value, r={}", start.r);
    }

    #[test]
    fn transition_any_float_property_works() {
        let mut ts = make_ts();
        let old = AnimatedPropertyMap::new().with_float("gap", 0.0);
        let new = AnimatedPropertyMap::new().with_float("gap", 20.0);

        ts.start_transition(&old, &new);
        assert!(ts.is_animating());

        ts.tick(0.05);
        if let Some(AnimatedValue::Float(v)) = ts.get_animated_value("gap") {
            assert!(v > 5.0 && v < 15.0, "gap should be mid-transition: {}", v);
        } else {
            panic!("expected Float for gap");
        }
    }
}
