pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[repr(C)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::new(r, g, b, a)
    }

    pub fn from_srgb(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self::new(
            srgb_to_linear(r as f32 / 255.0),
            srgb_to_linear(g as f32 / 255.0),
            srgb_to_linear(b as f32 / 255.0),
            a,
        )
    }

    pub fn from_hex(hex: &str) -> Self {
        let hex = hex.trim_start_matches('#');
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f32 / 255.0;
        let a = if hex.len() >= 8 {
            u8::from_str_radix(&hex[6..8], 16).unwrap_or(255) as f32 / 255.0
        } else {
            1.0
        };
        Self::new(srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), a)
    }

    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn to_premultiplied_array(&self) -> [f32; 4] {
        [self.r * self.a, self.g * self.a, self.b * self.a, self.a]
    }

    pub const fn white() -> Self { Self::rgb(1.0, 1.0, 1.0) }
    pub const fn black() -> Self { Self::rgb(0.0, 0.0, 0.0) }
    pub const fn red() -> Self { Self::rgb(1.0, 0.0, 0.0) }
    pub const fn green() -> Self { Self::rgb(0.0, 1.0, 0.0) }
    pub const fn blue() -> Self { Self::rgb(0.0, 0.0, 1.0) }
    pub const fn transparent() -> Self { Self::rgba(0.0, 0.0, 0.0, 0.0) }

    pub const WHITE: Self = Self::white();
    pub const BLACK: Self = Self::black();
    pub const RED: Self = Self::red();
    pub const GREEN: Self = Self::green();
    pub const BLUE: Self = Self::blue();
    pub const TRANSPARENT: Self = Self::transparent();

    pub fn with_alpha(&self, a: f32) -> Self {
        Self::new(self.r, self.g, self.b, a)
    }

    pub fn multiply_alpha(&self, alpha: f32) -> Self {
        Self::new(self.r, self.g, self.b, self.a * alpha)
    }

    pub fn lerp(&self, other: &Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let (a, b) = if self.a < 0.001 && other.a > 0.001 {
            (Color::new(other.r, other.g, other.b, 0.0), *other)
        } else if other.a < 0.001 && self.a > 0.001 {
            (*self, Color::new(self.r, self.g, self.b, 0.0))
        } else {
            (*self, *other)
        };
        Color::new(
            a.r + (b.r - a.r) * t,
            a.g + (b.g - a.g) * t,
            a.b + (b.b - a.b) * t,
            a.a + (b.a - a.a) * t,
        )
    }

    pub fn darken(&self, factor: f32) -> Self {
        Self::new(
            self.r * (1.0 - factor),
            self.g * (1.0 - factor),
            self.b * (1.0 - factor),
            self.a,
        )
    }

    pub fn lighten(&self, factor: f32) -> Self {
        Self::new(
            self.r + (1.0 - self.r) * factor,
            self.g + (1.0 - self.g) * factor,
            self.b + (1.0 - self.b) * factor,
            self.a,
        )
    }
}
