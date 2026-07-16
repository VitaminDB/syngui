use super::color::{Color, linear_to_srgb};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop {
    pub color: Color,
    pub position: Option<f32>,
}

impl ColorStop {
    pub fn new(color: Color, position: f32) -> Self {
        Self { color, position: Some(position) }
    }

    pub fn auto(color: Color) -> Self {
        Self { color, position: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum GradientShape {
    Circle,
    #[default]
    Ellipse,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Gradient {
    Linear {
        angle_deg: f32,
        stops: Vec<ColorStop>,
    },
    Radial {
        shape: GradientShape,
        center: (f32, f32),
        stops: Vec<ColorStop>,
        quality: u8,
    },
    Conic {
        from_angle: f32,
        center: (f32, f32),
        stops: Vec<ColorStop>,
        quality: u8,
    },
}

pub const GRADIENT_DEFAULT_QUALITY: u8 = 24;

impl Gradient {
    pub fn radial(shape: GradientShape, center: (f32, f32), stops: Vec<ColorStop>) -> Self {
        Gradient::Radial { shape, center, stops, quality: GRADIENT_DEFAULT_QUALITY }
    }

    pub fn conic(from_angle: f32, center: (f32, f32), stops: Vec<ColorStop>) -> Self {
        Gradient::Conic { from_angle, center, stops, quality: GRADIENT_DEFAULT_QUALITY }
    }

    pub fn with_quality(mut self, quality: u8) -> Self {
        match &mut self {
            Gradient::Radial { quality: q, .. } => *q = quality,
            Gradient::Conic { quality: q, .. } => *q = quality,
            Gradient::Linear { .. } => {}
        }
        self
    }

    pub fn resolve_stops(stops: &[ColorStop]) -> Vec<(Color, f32)> {
        if stops.is_empty() {
            return vec![];
        }
        if stops.len() == 1 {
            return vec![(stops[0].color, 0.0)];
        }

        let mut result: Vec<(Color, Option<f32>)> = stops
            .iter()
            .map(|s| (s.color, s.position))
            .collect();

        if result[0].1.is_none() {
            result[0].1 = Some(0.0);
        }
        let last = result.len() - 1;
        if result[last].1.is_none() {
            result[last].1 = Some(1.0);
        }

        let mut i = 0;
        while i < result.len() {
            if result[i].1.is_some() {
                i += 1;
                continue;
            }
            let start = i - 1;
            let mut end = i + 1;
            while end < result.len() && result[end].1.is_none() {
                end += 1;
            }
            let start_pos = result[start].1.unwrap();
            let end_pos = result[end].1.unwrap();
            let count = end - start;
            for j in 1..count {
                result[start + j].1 = Some(start_pos + (end_pos - start_pos) * j as f32 / count as f32);
            }
            i = end + 1;
        }

        result.into_iter().map(|(c, p)| (c, p.unwrap())).collect()
    }

    pub fn resolved_stops(&self) -> Vec<(Color, f32)> {
        let stops = match self {
            Gradient::Linear { stops, .. } => stops,
            Gradient::Radial { stops, .. } => stops,
            Gradient::Conic { stops, .. } => stops,
        };
        Self::resolve_stops(stops)
    }

    pub fn sample(&self, t: f32) -> Color {
        let stops = self.resolved_stops();
        if stops.is_empty() {
            return Color::TRANSPARENT;
        }
        if stops.len() == 1 {
            return stops[0].0;
        }

        let t = t.clamp(0.0, 1.0);

        if t <= stops[0].1 {
            return stops[0].0;
        }
        if t >= stops[stops.len() - 1].1 {
            return stops[stops.len() - 1].0;
        }

        for i in 0..stops.len() - 1 {
            let (c0, p0) = stops[i];
            let (c1, p1) = stops[i + 1];
            if t >= p0 && t <= p1 {
                let range = p1 - p0;
                if range < 1e-6 {
                    return c0;
                }
                let local_t = (t - p0) / range;
                return c0.lerp(&c1, local_t);
            }
        }

        stops[stops.len() - 1].0
    }

    pub fn rasterize(&self, width: usize) -> Vec<u8> {
        let mut data = Vec::with_capacity(width * 4);
        for i in 0..width {
            let t = i as f32 / (width - 1).max(1) as f32;
            let c = self.sample(t);
            data.push((linear_to_srgb(c.r) * 255.0).round() as u8);
            data.push((linear_to_srgb(c.g) * 255.0).round() as u8);
            data.push((linear_to_srgb(c.b) * 255.0).round() as u8);
            data.push((c.a * 255.0).round() as u8);
        }
        data
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Background {
    Solid(Color),
    Gradient(Gradient),
}
