pub use crate::render::display_list::{Effect, BlendModeType};
use crate::core::Color;

pub fn blur(radius: f32) -> Effect {
    Effect::Blur { radius }
}

pub fn backdrop_blur(radius: f32) -> Effect {
    Effect::BackdropBlur { radius }
}

pub fn shadow(color: crate::core::Color, blur_radius: f32, offset_x: f32, offset_y: f32) -> Effect {
    Effect::Shadow { color, blur_radius, offset_x, offset_y }
}

pub fn opacity(value: f32) -> Effect {
    Effect::Opacity { value }
}

pub fn grayscale(amount: f32) -> Effect {
    Effect::Grayscale { amount }
}

pub fn sepia(amount: f32) -> Effect {
    Effect::Sepia { amount }
}

pub fn invert(amount: f32) -> Effect {
    Effect::Invert { amount }
}

pub fn brightness(amount: f32) -> Effect {
    Effect::Brightness { amount }
}

pub fn contrast(amount: f32) -> Effect {
    Effect::Contrast { amount }
}

pub fn pixelate(block_size: f32) -> Effect {
    Effect::Pixelate { block_size }
}

pub fn edge_detection(threshold: f32) -> Effect {
    Effect::EdgeDetection { threshold }
}

pub fn chromatic_aberration(offset: f32) -> Effect {
    Effect::ChromaticAberration { offset }
}

pub fn displacement(amplitude: f32, frequency: f32) -> Effect {
    Effect::Displacement { amplitude, frequency }
}

pub fn scanlines(density: f32, opacity: f32) -> Effect {
    Effect::Scanlines { density, opacity }
}

pub fn vignette(radius: f32, softness: f32) -> Effect {
    Effect::Vignette { radius, softness }
}

pub fn noise(intensity: f32) -> Effect {
    Effect::Noise { intensity }
}

pub fn hsb_adjust(hue: f32, saturation: f32, brightness: f32) -> Effect {
    Effect::HsbAdjust { hue, saturation, brightness }
}

pub fn glow(radius: f32, intensity: f32) -> Effect {
    Effect::Glow { radius, intensity }
}

pub fn glitch(intensity: f32, block_size: f32) -> Effect {
    Effect::Glitch { intensity, block_size }
}

pub fn dissolve(threshold: f32) -> Effect {
    Effect::Dissolve { threshold }
}

pub fn swirl(angle: f32, radius: f32) -> Effect {
    Effect::Swirl { angle, radius }
}

pub fn bulge(strength: f32, radius: f32) -> Effect {
    Effect::Bulge { strength, radius }
}

pub fn gradient_map(dark: Color, light: Color) -> Effect {
    Effect::GradientMap { dark, light }
}

pub fn duotone(shadow: Color, highlight: Color) -> Effect {
    Effect::Duotone { shadow, highlight }
}

pub fn silhouette(color: Color) -> Effect {
    Effect::Silhouette { color }
}

pub fn heat_haze(amplitude: f32, speed: f32) -> Effect {
    Effect::HeatHaze { amplitude, speed }
}

pub fn directional_blur(angle: f32, radius: f32) -> Effect {
    Effect::DirectionalBlur { angle, radius }
}

pub fn radial_blur(intensity: f32) -> Effect {
    Effect::RadialBlur { intensity }
}

pub fn color_grade(lift: f32, gamma: f32, gain: f32) -> Effect {
    Effect::ColorGrade { lift, gamma, gain }
}

pub fn hologram(color: Color, intensity: f32) -> Effect {
    Effect::Hologram { color, intensity }
}

pub fn refraction(distortion: f32, ior: f32) -> Effect {
    Effect::Refraction { distortion, ior }
}

pub fn lens_flare(threshold: f32, intensity: f32) -> Effect {
    Effect::LensFlare { threshold, intensity }
}

pub fn mask_reveal(progress: f32, direction: f32) -> Effect {
    Effect::MaskReveal { progress, direction }
}

pub fn chain(effects: Vec<Effect>) -> Effect {
    Effect::Chain(effects)
}

#[derive(Clone, Debug, PartialEq)]
pub enum FilterEffect {
    Blur(f32),
    Grayscale(f32),
    Sepia(f32),
    Invert(f32),
    Brightness(f32),
    Contrast(f32),
    HueRotate(f32),
    Saturate(f32),
    Pixelate(f32),
    EdgeDetect(f32),
    ChromaticAberration(f32),
    Wave(f32, f32),
    Crt(f32),
    Vignette(f32),
    Noise(f32),
    Glitch(f32),
    Dissolve(f32),
    Swirl(f32, f32),
    Bulge(f32),
    GradientMap([f32; 3], [f32; 3]),
    Duotone([f32; 3], [f32; 3]),
    Silhouette([f32; 4]),
    HeatHaze(f32, f32),
    DirectionalBlur(f32, f32),
    MotionBlur(f32, f32),
    RadialBlur(f32),
    ColorGrade(f32, f32, f32),
    Hologram([f32; 3], f32),
    Refraction(f32, f32),
    LensFlare(f32, f32),
    MaskReveal(f32, f32),
}

impl FilterEffect {
    pub fn identity(&self) -> Self {
        match self {
            FilterEffect::Blur(_) => FilterEffect::Blur(0.0),
            FilterEffect::Grayscale(_) => FilterEffect::Grayscale(0.0),
            FilterEffect::Sepia(_) => FilterEffect::Sepia(0.0),
            FilterEffect::Invert(_) => FilterEffect::Invert(0.0),
            FilterEffect::Brightness(_) => FilterEffect::Brightness(1.0),
            FilterEffect::Contrast(_) => FilterEffect::Contrast(1.0),
            FilterEffect::HueRotate(_) => FilterEffect::HueRotate(0.0),
            FilterEffect::Saturate(_) => FilterEffect::Saturate(1.0),
            FilterEffect::Pixelate(_) => FilterEffect::Pixelate(1.0),
            FilterEffect::EdgeDetect(_) => FilterEffect::EdgeDetect(0.0),
            FilterEffect::ChromaticAberration(_) => FilterEffect::ChromaticAberration(0.0),
            FilterEffect::Wave(_, _) => FilterEffect::Wave(0.0, 0.0),
            FilterEffect::Crt(_) => FilterEffect::Crt(0.0),
            FilterEffect::Vignette(_) => FilterEffect::Vignette(0.0),
            FilterEffect::Noise(_) => FilterEffect::Noise(0.0),
            FilterEffect::Glitch(_) => FilterEffect::Glitch(0.0),
            FilterEffect::Dissolve(_) => FilterEffect::Dissolve(0.0),
            FilterEffect::Swirl(_, _) => FilterEffect::Swirl(0.0, 0.0),
            FilterEffect::Bulge(_) => FilterEffect::Bulge(0.0),
            FilterEffect::GradientMap(_, _) => FilterEffect::GradientMap([0.0; 3], [1.0; 3]),
            FilterEffect::Duotone(_, _) => FilterEffect::Duotone([0.0; 3], [1.0; 3]),
            FilterEffect::Silhouette(_) => FilterEffect::Silhouette([0.0, 0.0, 0.0, 1.0]),
            FilterEffect::HeatHaze(_, _) => FilterEffect::HeatHaze(0.0, 0.0),
            FilterEffect::DirectionalBlur(_, _) => FilterEffect::DirectionalBlur(0.0, 0.0),
            FilterEffect::MotionBlur(_, _) => FilterEffect::MotionBlur(0.0, 0.0),
            FilterEffect::RadialBlur(_) => FilterEffect::RadialBlur(0.0),
            FilterEffect::ColorGrade(_, _, _) => FilterEffect::ColorGrade(0.0, 1.0, 1.0),
            FilterEffect::Hologram(_, _) => FilterEffect::Hologram([0.0; 3], 0.0),
            FilterEffect::Refraction(_, _) => FilterEffect::Refraction(0.0, 1.0),
            FilterEffect::LensFlare(_, _) => FilterEffect::LensFlare(0.8, 0.0),
            FilterEffect::MaskReveal(_, _) => FilterEffect::MaskReveal(0.0, 0.0),
        }
    }

    pub fn lerp(&self, other: &FilterEffect, t: f32) -> Option<FilterEffect> {
        fn mix(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }
        fn mix3(a: &[f32; 3], b: &[f32; 3], t: f32) -> [f32; 3] {
            [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)]
        }
        fn mix4(a: &[f32; 4], b: &[f32; 4], t: f32) -> [f32; 4] {
            [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t), mix(a[3], b[3], t)]
        }

        match (self, other) {
            (FilterEffect::Blur(a), FilterEffect::Blur(b)) => Some(FilterEffect::Blur(mix(*a, *b, t))),
            (FilterEffect::Grayscale(a), FilterEffect::Grayscale(b)) => Some(FilterEffect::Grayscale(mix(*a, *b, t))),
            (FilterEffect::Sepia(a), FilterEffect::Sepia(b)) => Some(FilterEffect::Sepia(mix(*a, *b, t))),
            (FilterEffect::Invert(a), FilterEffect::Invert(b)) => Some(FilterEffect::Invert(mix(*a, *b, t))),
            (FilterEffect::Brightness(a), FilterEffect::Brightness(b)) => Some(FilterEffect::Brightness(mix(*a, *b, t))),
            (FilterEffect::Contrast(a), FilterEffect::Contrast(b)) => Some(FilterEffect::Contrast(mix(*a, *b, t))),
            (FilterEffect::HueRotate(a), FilterEffect::HueRotate(b)) => Some(FilterEffect::HueRotate(mix(*a, *b, t))),
            (FilterEffect::Saturate(a), FilterEffect::Saturate(b)) => Some(FilterEffect::Saturate(mix(*a, *b, t))),
            (FilterEffect::Pixelate(a), FilterEffect::Pixelate(b)) => Some(FilterEffect::Pixelate(mix(*a, *b, t))),
            (FilterEffect::EdgeDetect(a), FilterEffect::EdgeDetect(b)) => Some(FilterEffect::EdgeDetect(mix(*a, *b, t))),
            (FilterEffect::ChromaticAberration(a), FilterEffect::ChromaticAberration(b)) => Some(FilterEffect::ChromaticAberration(mix(*a, *b, t))),
            (FilterEffect::Wave(a1, a2), FilterEffect::Wave(b1, b2)) => Some(FilterEffect::Wave(mix(*a1, *b1, t), mix(*a2, *b2, t))),
            (FilterEffect::Crt(a), FilterEffect::Crt(b)) => Some(FilterEffect::Crt(mix(*a, *b, t))),
            (FilterEffect::Vignette(a), FilterEffect::Vignette(b)) => Some(FilterEffect::Vignette(mix(*a, *b, t))),
            (FilterEffect::Noise(a), FilterEffect::Noise(b)) => Some(FilterEffect::Noise(mix(*a, *b, t))),
            (FilterEffect::Glitch(a), FilterEffect::Glitch(b)) => Some(FilterEffect::Glitch(mix(*a, *b, t))),
            (FilterEffect::Dissolve(a), FilterEffect::Dissolve(b)) => Some(FilterEffect::Dissolve(mix(*a, *b, t))),
            (FilterEffect::Swirl(a1, a2), FilterEffect::Swirl(b1, b2)) => Some(FilterEffect::Swirl(mix(*a1, *b1, t), mix(*a2, *b2, t))),
            (FilterEffect::Bulge(a), FilterEffect::Bulge(b)) => Some(FilterEffect::Bulge(mix(*a, *b, t))),
            (FilterEffect::GradientMap(ad, al), FilterEffect::GradientMap(bd, bl)) => Some(FilterEffect::GradientMap(mix3(ad, bd, t), mix3(al, bl, t))),
            (FilterEffect::Duotone(as_, ah), FilterEffect::Duotone(bs, bh)) => Some(FilterEffect::Duotone(mix3(as_, bs, t), mix3(ah, bh, t))),
            (FilterEffect::Silhouette(a), FilterEffect::Silhouette(b)) => Some(FilterEffect::Silhouette(mix4(a, b, t))),
            (FilterEffect::HeatHaze(a1, a2), FilterEffect::HeatHaze(b1, b2)) => Some(FilterEffect::HeatHaze(mix(*a1, *b1, t), mix(*a2, *b2, t))),
            (FilterEffect::DirectionalBlur(a1, a2), FilterEffect::DirectionalBlur(b1, b2)) => Some(FilterEffect::DirectionalBlur(mix(*a1, *b1, t), mix(*a2, *b2, t))),
            (FilterEffect::MotionBlur(a1, a2), FilterEffect::MotionBlur(b1, b2)) => Some(FilterEffect::MotionBlur(mix(*a1, *b1, t), mix(*a2, *b2, t))),
            (FilterEffect::RadialBlur(a), FilterEffect::RadialBlur(b)) => Some(FilterEffect::RadialBlur(mix(*a, *b, t))),
            (FilterEffect::ColorGrade(a1, a2, a3), FilterEffect::ColorGrade(b1, b2, b3)) => Some(FilterEffect::ColorGrade(mix(*a1, *b1, t), mix(*a2, *b2, t), mix(*a3, *b3, t))),
            (FilterEffect::Hologram(ac, ai), FilterEffect::Hologram(bc, bi)) => Some(FilterEffect::Hologram(mix3(ac, bc, t), mix(*ai, *bi, t))),
            (FilterEffect::Refraction(a1, a2), FilterEffect::Refraction(b1, b2)) => Some(FilterEffect::Refraction(mix(*a1, *b1, t), mix(*a2, *b2, t))),
            (FilterEffect::LensFlare(a1, a2), FilterEffect::LensFlare(b1, b2)) => Some(FilterEffect::LensFlare(mix(*a1, *b1, t), mix(*a2, *b2, t))),
            (FilterEffect::MaskReveal(a1, a2), FilterEffect::MaskReveal(b1, b2)) => Some(FilterEffect::MaskReveal(mix(*a1, *b1, t), mix(*a2, *b2, t))),
            _ => None,
        }
    }

    pub fn to_effect(&self) -> Effect {
        match self {
            FilterEffect::Blur(r) => Effect::Blur { radius: *r },
            FilterEffect::Grayscale(a) => Effect::Grayscale { amount: *a },
            FilterEffect::Sepia(a) => Effect::Sepia { amount: *a },
            FilterEffect::Invert(a) => Effect::Invert { amount: *a },
            FilterEffect::Brightness(a) => Effect::Brightness { amount: *a },
            FilterEffect::Contrast(a) => Effect::Contrast { amount: *a },
            FilterEffect::HueRotate(deg) => Effect::HsbAdjust { hue: *deg / 360.0, saturation: 1.0, brightness: 1.0 },
            FilterEffect::Saturate(a) => Effect::HsbAdjust { hue: 0.0, saturation: *a, brightness: 1.0 },
            FilterEffect::Pixelate(s) => Effect::Pixelate { block_size: *s },
            FilterEffect::EdgeDetect(t) => Effect::EdgeDetection { threshold: *t },
            FilterEffect::ChromaticAberration(o) => Effect::ChromaticAberration { offset: *o },
            FilterEffect::Wave(amp, freq) => Effect::Displacement { amplitude: *amp, frequency: *freq },
            FilterEffect::Crt(o) => Effect::Scanlines { density: 2.0, opacity: *o },
            FilterEffect::Vignette(r) => Effect::Vignette { radius: *r, softness: 0.3 },
            FilterEffect::Noise(i) => Effect::Noise { intensity: *i },
            FilterEffect::Glitch(i) => Effect::Glitch { intensity: *i, block_size: 8.0 },
            FilterEffect::Dissolve(t) => Effect::Dissolve { threshold: *t },
            FilterEffect::Swirl(a, r) => Effect::Swirl { angle: *a, radius: *r },
            FilterEffect::Bulge(s) => Effect::Bulge { strength: *s, radius: 0.5 },
            FilterEffect::GradientMap(d, l) => Effect::GradientMap {
                dark: Color::new(d[0], d[1], d[2], 1.0),
                light: Color::new(l[0], l[1], l[2], 1.0),
            },
            FilterEffect::Duotone(s, h) => Effect::Duotone {
                shadow: Color::new(s[0], s[1], s[2], 1.0),
                highlight: Color::new(h[0], h[1], h[2], 1.0),
            },
            FilterEffect::Silhouette(c) => Effect::Silhouette {
                color: Color::new(c[0], c[1], c[2], c[3]),
            },
            FilterEffect::HeatHaze(a, s) => Effect::HeatHaze { amplitude: *a, speed: *s },
            FilterEffect::DirectionalBlur(angle, radius) => Effect::DirectionalBlur { angle: *angle, radius: *radius },
            FilterEffect::MotionBlur(angle, radius) => Effect::DirectionalBlur { angle: *angle, radius: *radius },
            FilterEffect::RadialBlur(i) => Effect::RadialBlur { intensity: *i },
            FilterEffect::ColorGrade(l, g, ga) => Effect::ColorGrade { lift: *l, gamma: *g, gain: *ga },
            FilterEffect::Hologram(c, i) => Effect::Hologram {
                color: Color::new(c[0], c[1], c[2], 1.0),
                intensity: *i,
            },
            FilterEffect::Refraction(d, ior) => Effect::Refraction { distortion: *d, ior: *ior },
            FilterEffect::LensFlare(t, i) => Effect::LensFlare { threshold: *t, intensity: *i },
            FilterEffect::MaskReveal(p, d) => Effect::MaskReveal { progress: *p, direction: *d },
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let paren = s.find('(')?;
        let name = s[..paren].trim();
        let args = s[paren+1..].trim_end_matches(')').trim();

        let parse_value = |v: &str| -> Option<f32> {
            let v = v.trim();
            if let Some(pct) = v.strip_suffix('%') {
                pct.trim().parse::<f32>().ok().map(|p| p / 100.0)
            } else if let Some(px) = v.strip_suffix("px") {
                px.trim().parse::<f32>().ok()
            } else if let Some(deg) = v.strip_suffix("deg") {
                deg.trim().parse::<f32>().ok()
            } else {
                v.parse::<f32>().ok()
            }
        };

        match name {
            "blur" => parse_value(args).map(FilterEffect::Blur),
            "grayscale" => parse_value(args).map(FilterEffect::Grayscale),
            "sepia" => parse_value(args).map(FilterEffect::Sepia),
            "invert" => parse_value(args).map(FilterEffect::Invert),
            "brightness" => parse_value(args).map(FilterEffect::Brightness),
            "contrast" => parse_value(args).map(FilterEffect::Contrast),
            "hue-rotate" => parse_value(args).map(FilterEffect::HueRotate),
            "saturate" => parse_value(args).map(FilterEffect::Saturate),
            "pixelate" => parse_value(args).map(FilterEffect::Pixelate),
            "edge-detect" => parse_value(args).map(FilterEffect::EdgeDetect),
            "chromatic-aberration" => parse_value(args).map(FilterEffect::ChromaticAberration),
            "vignette" => parse_value(args).map(FilterEffect::Vignette),
            "noise" => parse_value(args).map(FilterEffect::Noise),
            "crt" => parse_value(args).map(FilterEffect::Crt),
            "wave" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let amp = parse_value(parts[0])?;
                    let freq = parse_value(parts[1])?;
                    Some(FilterEffect::Wave(amp, freq))
                } else {
                    None
                }
            }
            "glitch" => parse_value(args).map(FilterEffect::Glitch),
            "dissolve" => parse_value(args).map(FilterEffect::Dissolve),
            "swirl" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let angle = parse_value(parts[0])?;
                    let radius = parse_value(parts[1])?;
                    Some(FilterEffect::Swirl(angle, radius))
                } else {
                    parse_value(args).map(|a| FilterEffect::Swirl(a, 0.5))
                }
            }
            "bulge" => parse_value(args).map(FilterEffect::Bulge),
            "pinch" => parse_value(args).map(|v| FilterEffect::Bulge(-v)),
            "gradient-map" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let dark = parse_hex_rgb(parts[0])?;
                    let light = parse_hex_rgb(parts[1])?;
                    Some(FilterEffect::GradientMap(dark, light))
                } else {
                    None
                }
            }
            "duotone" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let shadow = parse_hex_rgb(parts[0])?;
                    let highlight = parse_hex_rgb(parts[1])?;
                    Some(FilterEffect::Duotone(shadow, highlight))
                } else {
                    None
                }
            }
            "silhouette" => {
                let rgb = parse_hex_rgb(args)?;
                Some(FilterEffect::Silhouette([rgb[0], rgb[1], rgb[2], 1.0]))
            }
            "heat-haze" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let amp = parse_value(parts[0])?;
                    let speed = parse_value(parts[1])?;
                    Some(FilterEffect::HeatHaze(amp, speed))
                } else {
                    parse_value(args).map(|a| FilterEffect::HeatHaze(a, 1.0))
                }
            }
            "directional-blur" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let angle = parse_value(parts[0])?;
                    let radius = parse_value(parts[1])?;
                    Some(FilterEffect::DirectionalBlur(angle.to_radians(), radius))
                } else {
                    None
                }
            }
            "motion-blur" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let angle = parse_value(parts[0])?;
                    let radius = parse_value(parts[1])?;
                    Some(FilterEffect::MotionBlur(angle.to_radians(), radius))
                } else {
                    None
                }
            }
            "radial-blur" | "zoom-blur" => parse_value(args).map(FilterEffect::RadialBlur),
            "color-grade" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 3 {
                    let lift = parse_value(parts[0])?;
                    let gamma = parse_value(parts[1])?;
                    let gain = parse_value(parts[2])?;
                    Some(FilterEffect::ColorGrade(lift, gamma, gain))
                } else {
                    None
                }
            }
            "hologram" | "x-ray" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let rgb = parse_hex_rgb(parts[0])?;
                    let intensity = parse_value(parts[1])?;
                    Some(FilterEffect::Hologram(rgb, intensity))
                } else if let Some(rgb) = parse_hex_rgb(args) {
                    Some(FilterEffect::Hologram(rgb, 1.0))
                } else {
                    parse_value(args).map(|i| FilterEffect::Hologram([0.0, 1.0, 0.5], i))
                }
            }
            "refraction" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let dist = parse_value(parts[0])?;
                    let ior = parse_value(parts[1])?;
                    Some(FilterEffect::Refraction(dist, ior))
                } else {
                    parse_value(args).map(|d| FilterEffect::Refraction(d, 1.33))
                }
            }
            "lens-flare" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let threshold = parse_value(parts[0])?;
                    let intensity = parse_value(parts[1])?;
                    Some(FilterEffect::LensFlare(threshold, intensity))
                } else {
                    parse_value(args).map(|t| FilterEffect::LensFlare(t, 1.0))
                }
            }
            "mask-reveal" => {
                let parts: Vec<&str> = args.split(',').collect();
                if parts.len() >= 2 {
                    let progress = parse_value(parts[0])?;
                    let dir = parse_value(parts[1])?;
                    Some(FilterEffect::MaskReveal(progress, dir.to_radians()))
                } else {
                    parse_value(args).map(|p| FilterEffect::MaskReveal(p, 0.0))
                }
            }
            _ => None,
        }
    }
}

fn parse_hex_rgb(s: &str) -> Option<[f32; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() >= 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
    } else {
        None
    }
}

pub fn lerp_filter_chains(from: &[FilterEffect], to: &[FilterEffect], t: f32) -> Vec<FilterEffect> {
    let max_len = from.len().max(to.len());
    let mut result = Vec::with_capacity(max_len);

    for i in 0..max_len {
        let f = from.get(i);
        let t_eff = to.get(i);

        match (f, t_eff) {
            (Some(a), Some(b)) => {
                if let Some(interp) = a.lerp(b, t) {
                    result.push(interp);
                } else {
                    result.push(if t < 0.5 { a.clone() } else { b.clone() });
                }
            }
            (Some(a), None) => {
                let id = a.identity();
                if let Some(interp) = a.lerp(&id, t) {
                    result.push(interp);
                }
            }
            (None, Some(b)) => {
                let id = b.identity();
                if let Some(interp) = id.lerp(b, t) {
                    result.push(interp);
                }
            }
            (None, None) => unreachable!(),
        }
    }

    result
}

pub fn parse_filter_chain(s: &str) -> Vec<FilterEffect> {
    let s = s.trim();
    let mut results = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in s.chars() {
        match ch {
            '(' => { depth += 1; current.push(ch); }
            ')' => {
                depth -= 1;
                current.push(ch);
                if depth == 0 {
                    if let Some(effect) = FilterEffect::parse(current.trim()) {
                        results.push(effect);
                    }
                    current.clear();
                }
            }
            ' ' | '\t' if depth == 0 => {
                if !current.trim().is_empty() && !current.contains('(') {
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    results
}
