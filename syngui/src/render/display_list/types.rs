use compact_str::CompactString;
use crate::core::Color;
use crate::mss::{TextAlign, TextDecoration};
use crate::render::{ClipRect, TextureId};
use crate::widget::RenderHandle;

#[derive(Clone, Copy, Debug)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}

impl Border {
    pub fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PerSideBorder {
    pub widths: [f32; 4],
    pub color: Color,
}

#[derive(Clone, Debug)]
pub enum DrawCommand {
    Rect {
        rect: crate::core::Rect,
        color: Color,
        corner_radius: [f32; 4],
        border: Option<Border>,
        per_side_border: Option<PerSideBorder>,
        clip_rect: ClipRect,
        z_index: u32,
    },
    Text {
        text: CompactString,
        rect: crate::core::Rect,
        color: Color,
        font_size: f32,
        font_weight: u16,
        text_align: TextAlign,
        decoration: TextDecoration,
        font_family: Option<CompactString>,
        letter_spacing: f32,
        text_shadow: Option<crate::mss::fields::TextShadow>,
        bbox_sample: Option<CompactString>,
        clip_rect: ClipRect,
        z_index: u32,
        /// Не переносить текст: рисуется одной строкой, лишнее обрезается
        /// клипом. Нужно для ячеек таблиц и прочих однострочных подписей.
        no_wrap: bool,
    },
    Image {
        rect: crate::core::Rect,
        texture_id: TextureId,
        uv_rect: crate::core::Rect,
        color: Color,
        clip_rect: ClipRect,
        z_index: u32,
    },
    Custom {
        rect: crate::core::Rect,
        shader_id: ShaderId,
        uniforms: Vec<u8>,
        input_textures: Vec<TextureId>,
        clip_rect: ClipRect,
        z_index: u32,
    },
    PushClip { rect: crate::core::Rect },
    PopClip,
    Cached(RenderHandle),
    Shadow {
        rect: crate::core::Rect,
        color: Color,
        blur_radius: f32,
        offset: (f32, f32),
        corner_radius: [f32; 4],
        inset: bool,
        clip_rect: ClipRect,
        z_index: u32,
    },
    GlowShadow {
        rect: crate::core::Rect,
        color: Color,
        blur_radius: f32,
        offset: (f32, f32),
        corner_radius: [f32; 4],
        clip_rect: ClipRect,
        z_index: u32,
    },
    Outline {
        rect: crate::core::Rect,
        color: Color,
        ring_width: f32,
        corner_radius: [f32; 4],
        clip_rect: ClipRect,
        z_index: u32,
    },
    BeginEffectLayer { effect: Effect, bounds: crate::core::Rect },
    EndEffectLayer { texture_id: TextureId },
    PushTransform(crate::core::Transform),
    PopTransform,
    PushOpacity(f32),
    ZBarrier,
    TextCursor {
        text: CompactString,
        cursor_pos: usize,
        base_x: f32,
        y: f32,
        height: f32,
        font_size: f32,
        font_weight: u16,
        color: Color,
        font_family: Option<CompactString>,
        clip_rect: ClipRect,
        z_index: u32,
    },
    TextSelection {
        text: CompactString,
        sel_start: usize,
        sel_end: usize,
        base_x: f32,
        y: f32,
        height: f32,
        font_size: f32,
        color: Color,
        font_family: Option<CompactString>,
        clip_rect: ClipRect,
        z_index: u32,
    },
    PopOpacity,
    GradientRect {
        rect: crate::core::Rect,
        gradient: crate::core::Gradient,
        corner_radius: [f32; 4],
        border: Option<Border>,
        per_side_border: Option<PerSideBorder>,
        clip_rect: ClipRect,
        z_index: u32,
    },
    Canvas {
        vertices: Vec<crate::render::Vertex>,
        indices: Vec<u32>,
        clip_rect: ClipRect,
        z_index: u32,
    },
    LineStrip {
        points: Vec<[f32; 2]>,
        color: Color,
        width: f32,
        clip_rect: ClipRect,
        z_index: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ShaderId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendModeType {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
}

#[derive(Clone, Debug)]
pub enum Effect {
    None,
    Blur { radius: f32 },
    BackdropBlur { radius: f32 },
    Shadow {
        color: Color,
        blur_radius: f32,
        offset_x: f32,
        offset_y: f32,
    },
    Opacity { value: f32 },
    Grayscale { amount: f32 },
    Sepia { amount: f32 },
    Invert { amount: f32 },
    HsbAdjust { hue: f32, saturation: f32, brightness: f32 },
    Brightness { amount: f32 },
    Contrast { amount: f32 },
    Pixelate { block_size: f32 },
    EdgeDetection { threshold: f32 },
    ChromaticAberration { offset: f32 },
    Displacement { amplitude: f32, frequency: f32 },
    Scanlines { density: f32, opacity: f32 },
    Vignette { radius: f32, softness: f32 },
    Noise { intensity: f32 },
    Glitch { intensity: f32, block_size: f32 },
    Dissolve { threshold: f32 },
    Swirl { angle: f32, radius: f32 },
    Bulge { strength: f32, radius: f32 },
    GradientMap { dark: Color, light: Color },
    Duotone { shadow: Color, highlight: Color },
    Silhouette { color: Color },
    HeatHaze { amplitude: f32, speed: f32 },
    DirectionalBlur { angle: f32, radius: f32 },
    RadialBlur { intensity: f32 },
    ColorGrade { lift: f32, gamma: f32, gain: f32 },
    Hologram { color: Color, intensity: f32 },
    Refraction { distortion: f32, ior: f32 },
    LensFlare { threshold: f32, intensity: f32 },
    MaskReveal { progress: f32, direction: f32 },
    Glow { radius: f32, intensity: f32 },
    BlendMode { mode: BlendModeType },
    Chain(Vec<Effect>),
}

impl Effect {
    pub fn blur(radius: f32) -> Self {
        Effect::Blur { radius }
    }

    pub fn backdrop_blur(radius: f32) -> Self {
        Effect::BackdropBlur { radius }
    }

    pub fn shadow(color: Color, blur_radius: f32, offset_x: f32, offset_y: f32) -> Self {
        Effect::Shadow { color, blur_radius, offset_x, offset_y }
    }

    pub fn opacity(value: f32) -> Self {
        Effect::Opacity { value }
    }

    pub fn grayscale(amount: f32) -> Self {
        Effect::Grayscale { amount }
    }

    pub fn sepia(amount: f32) -> Self {
        Effect::Sepia { amount }
    }

    pub fn invert(amount: f32) -> Self {
        Effect::Invert { amount }
    }

    pub fn brightness(amount: f32) -> Self {
        Effect::Brightness { amount }
    }

    pub fn contrast(amount: f32) -> Self {
        Effect::Contrast { amount }
    }

    pub fn pixelate(block_size: f32) -> Self {
        Effect::Pixelate { block_size }
    }

    pub fn vignette(radius: f32, softness: f32) -> Self {
        Effect::Vignette { radius, softness }
    }

    pub fn noise(intensity: f32) -> Self {
        Effect::Noise { intensity }
    }

    pub fn glitch(intensity: f32, block_size: f32) -> Self {
        Effect::Glitch { intensity, block_size }
    }

    pub fn dissolve(threshold: f32) -> Self {
        Effect::Dissolve { threshold }
    }

    pub fn swirl(angle: f32, radius: f32) -> Self {
        Effect::Swirl { angle, radius }
    }

    pub fn bulge(strength: f32, radius: f32) -> Self {
        Effect::Bulge { strength, radius }
    }

    pub fn gradient_map(dark: Color, light: Color) -> Self {
        Effect::GradientMap { dark, light }
    }

    pub fn duotone(shadow: Color, highlight: Color) -> Self {
        Effect::Duotone { shadow, highlight }
    }

    pub fn silhouette(color: Color) -> Self {
        Effect::Silhouette { color }
    }

    pub fn heat_haze(amplitude: f32, speed: f32) -> Self {
        Effect::HeatHaze { amplitude, speed }
    }

    pub fn directional_blur(angle: f32, radius: f32) -> Self {
        Effect::DirectionalBlur { angle, radius }
    }

    pub fn radial_blur(intensity: f32) -> Self {
        Effect::RadialBlur { intensity }
    }

    pub fn color_grade(lift: f32, gamma: f32, gain: f32) -> Self {
        Effect::ColorGrade { lift, gamma, gain }
    }

    pub fn hologram(color: Color, intensity: f32) -> Self {
        Effect::Hologram { color, intensity }
    }

    pub fn refraction(distortion: f32, ior: f32) -> Self {
        Effect::Refraction { distortion, ior }
    }

    pub fn lens_flare(threshold: f32, intensity: f32) -> Self {
        Effect::LensFlare { threshold, intensity }
    }

    pub fn mask_reveal(progress: f32, direction: f32) -> Self {
        Effect::MaskReveal { progress, direction }
    }

    pub fn glow(radius: f32, intensity: f32) -> Self {
        Effect::Glow { radius, intensity }
    }

    pub fn is_identity(&self) -> bool {
        match self {
            Effect::None => true,
            Effect::Blur { radius } => *radius <= 0.0,
            Effect::BackdropBlur { radius } => *radius <= 0.0,
            Effect::Glow { radius, .. } => *radius <= 0.0,
            Effect::Opacity { value } => (*value - 1.0).abs() < 0.001,
            Effect::Grayscale { amount } | Effect::Sepia { amount } | Effect::Invert { amount } => *amount <= 0.0,
            Effect::Brightness { amount } => (*amount - 1.0).abs() < 0.001,
            Effect::Contrast { amount } => (*amount - 1.0).abs() < 0.001,
            Effect::Pixelate { block_size } => *block_size <= 1.0,
            Effect::Noise { intensity } => *intensity <= 0.0,
            Effect::Glitch { intensity, .. } => *intensity <= 0.0,
            Effect::Dissolve { threshold } => *threshold <= 0.0,
            Effect::Swirl { angle, .. } => angle.abs() < 0.001,
            Effect::Bulge { strength, .. } => strength.abs() < 0.001,
            Effect::HeatHaze { amplitude, .. } => *amplitude <= 0.0,
            Effect::DirectionalBlur { radius, .. } => *radius <= 0.0,
            Effect::RadialBlur { intensity } => *intensity <= 0.0,
            Effect::ColorGrade { lift, gamma, gain } => lift.abs() < 0.001 && (*gamma - 1.0).abs() < 0.001 && (*gain - 1.0).abs() < 0.001,
            Effect::Hologram { intensity, .. } => *intensity <= 0.0,
            Effect::Refraction { distortion, .. } => distortion.abs() < 0.001,
            Effect::LensFlare { intensity, .. } => *intensity <= 0.0,
            Effect::MaskReveal { progress, .. } => *progress <= 0.0,
            Effect::Chain(effects) => effects.iter().all(|e| e.is_identity()),
            _ => false,
        }
    }

    pub fn postprocess_type(&self) -> Option<(f32, f32, [f32; 4], [f32; 4])> {
        let z = [0.0; 4];
        match self {
            Effect::Grayscale { amount } => Some((0.0, *amount, z, z)),
            Effect::Sepia { amount } => Some((1.0, *amount, z, z)),
            Effect::Invert { amount } => Some((2.0, *amount, z, z)),
            Effect::HsbAdjust { hue, saturation, brightness } => {
                Some((3.0, 1.0, [*hue, *saturation, *brightness, 0.0], z))
            }
            Effect::Brightness { amount } => Some((4.0, *amount, z, z)),
            Effect::Contrast { amount } => Some((5.0, *amount, z, z)),
            Effect::Pixelate { block_size } => Some((6.0, 1.0, [*block_size, 0.0, 0.0, 0.0], z)),
            Effect::EdgeDetection { threshold } => Some((7.0, *threshold, z, z)),
            Effect::ChromaticAberration { offset } => Some((8.0, 1.0, [*offset, 0.0, 0.0, 0.0], z)),
            Effect::Scanlines { density, opacity } => Some((9.0, *opacity, [*density, 0.0, 0.0, 0.0], z)),
            Effect::Displacement { amplitude, frequency } => {
                Some((10.0, 1.0, [*amplitude, *frequency, 0.0, 0.0], z))
            }
            Effect::Vignette { radius, softness } => Some((11.0, 1.0, [*radius, *softness, 0.0, 0.0], z)),
            Effect::Noise { intensity } => Some((12.0, *intensity, z, z)),
            Effect::Glitch { intensity, block_size } => {
                Some((13.0, *intensity, [*block_size, 0.0, 0.0, 0.0], z))
            }
            Effect::Dissolve { threshold } => Some((14.0, *threshold, z, z)),
            Effect::Swirl { angle, radius } => {
                Some((15.0, 1.0, [0.5, 0.5, *angle, *radius], z))
            }
            Effect::Bulge { strength, radius } => {
                Some((16.0, 1.0, [0.5, 0.5, *strength, *radius], z))
            }
            Effect::GradientMap { dark, light } => {
                Some((17.0, 1.0,
                    [dark.r, dark.g, dark.b, 0.0],
                    [light.r, light.g, light.b, 0.0]))
            }
            Effect::Duotone { shadow, highlight } => {
                Some((18.0, 1.0,
                    [shadow.r, shadow.g, shadow.b, 0.0],
                    [highlight.r, highlight.g, highlight.b, 0.0]))
            }
            Effect::Silhouette { color } => {
                Some((19.0, 1.0, [color.r, color.g, color.b, color.a], z))
            }
            Effect::HeatHaze { amplitude, speed } => {
                Some((20.0, 1.0, [*amplitude, *speed, 0.0, 0.0], z))
            }
            Effect::RadialBlur { intensity } => {
                Some((21.0, *intensity, z, z))
            }
            Effect::ColorGrade { lift, gamma, gain } => {
                Some((22.0, 1.0, [*lift, *gamma, *gain, 0.0], z))
            }
            Effect::Hologram { color, intensity } => {
                Some((23.0, *intensity, [color.r, color.g, color.b, 0.0], z))
            }
            Effect::Refraction { distortion, ior } => {
                Some((24.0, 1.0, [*distortion, *ior, 0.0, 0.0], z))
            }
            Effect::LensFlare { threshold, intensity } => {
                Some((25.0, *intensity, [*threshold, 0.0, 0.0, 0.0], z))
            }
            Effect::MaskReveal { progress, direction } => {
                Some((26.0, *progress, [*direction, 0.0, 0.0, 0.0], z))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DisplayListStats {
    pub command_count: usize,
    pub overlay_command_count: usize,
    pub capacity: usize,
}
