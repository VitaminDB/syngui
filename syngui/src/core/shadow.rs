use super::color::Color;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Shadow {
    pub color: Color,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread: f32,
    pub inset: bool,
}

impl Shadow {
    pub const fn new(color: Color, offset_x: f32, offset_y: f32, blur_radius: f32) -> Self {
        Self {
            color,
            offset_x,
            offset_y,
            blur_radius,
            spread: 0.0,
            inset: false,
        }
    }

    pub const fn inset(color: Color, offset_x: f32, offset_y: f32, blur_radius: f32) -> Self {
        Self {
            color,
            offset_x,
            offset_y,
            blur_radius,
            spread: 0.0,
            inset: true,
        }
    }

    pub const fn with_spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    pub const fn with_inset(mut self, inset: bool) -> Self {
        self.inset = inset;
        self
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut depth = 0;
        for c in s.chars() {
            match c {
                '(' => { depth += 1; current.push(c); }
                ')' => { depth -= 1; current.push(c); }
                ' ' | '\t' if depth == 0 => {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() { tokens.push(trimmed); }
                    current.clear();
                }
                _ => current.push(c),
            }
        }
        let trimmed = current.trim().to_string();
        if !trimmed.is_empty() { tokens.push(trimmed); }
        if tokens.len() < 3 { return None; }
        let mut inset = false;
        let tokens: Vec<String> = tokens.into_iter().filter(|t| {
            if t == "inset" { inset = true; false } else { true }
        }).collect();
        let mut color = None;
        let mut color_idx = 0;
        for (i, token) in tokens.iter().enumerate() {
            if token.starts_with("rgba(") || token.starts_with("rgb(") || token.starts_with('#') {
                color = Self::parse_color(token);
                if color.is_some() { color_idx = i; break; }
            }
        }
        let color = color?;
        let mut lengths = Vec::new();
        for (i, token) in tokens.iter().enumerate() {
            if i == color_idx { continue; }
            if let Some(val) = Self::parse_length(token) { lengths.push(val); }
        }
        if lengths.len() < 2 { return None; }
        Some(Self {
            color,
            offset_x: lengths[0],
            offset_y: lengths[1],
            blur_radius: lengths.get(2).copied().unwrap_or(0.0),
            spread: lengths.get(3).copied().unwrap_or(0.0),
            inset,
        })
    }

    fn parse_color(s: &str) -> Option<Color> {
        let s = s.trim();
        if s.starts_with('#') {
            let hex = &s[1..];
            match hex.len() {
                6 => {
                    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                    return Some(Color::from_srgb(r, g, b, 1.0));
                }
                8 => {
                    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                    let a = u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0;
                    return Some(Color::from_srgb(r, g, b, a));
                }
                _ => {}
            }
        }
        if s.starts_with("rgba(") {
            let inner = s.trim_start_matches("rgba(").trim_end_matches(')');
            let vals: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if vals.len() >= 4 {
                let r = vals[0].parse::<u8>().ok()?;
                let g = vals[1].parse::<u8>().ok()?;
                let b = vals[2].parse::<u8>().ok()?;
                let a = vals[3].parse::<f32>().ok()?;
                return Some(Color::from_srgb(r, g, b, a));
            }
        }
        if s.starts_with("rgb(") {
            let inner = s.trim_start_matches("rgb(").trim_end_matches(')');
            let vals: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            if vals.len() >= 3 {
                let r = vals[0].parse::<u8>().ok()?;
                let g = vals[1].parse::<u8>().ok()?;
                let b = vals[2].parse::<u8>().ok()?;
                return Some(Color::from_srgb(r, g, b, 1.0));
            }
        }
        None
    }

    fn parse_length(s: &str) -> Option<f32> {
        let s = s.trim();
        if s.ends_with("px") { s[..s.len()-2].parse().ok() } else { s.parse().ok() }
    }

    pub fn lerp(&self, other: &Shadow, t: f32) -> Shadow {
        Shadow {
            color: self.color.lerp(&other.color, t),
            offset_x: self.offset_x + (other.offset_x - self.offset_x) * t,
            offset_y: self.offset_y + (other.offset_y - self.offset_y) * t,
            blur_radius: self.blur_radius + (other.blur_radius - self.blur_radius) * t,
            spread: self.spread + (other.spread - self.spread) * t,
            inset: other.inset,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shadows(pub Vec<Shadow>);

impl Shadows {
    pub fn new() -> Self { Self(Vec::new()) }
    pub fn push(&mut self, shadow: Shadow) { self.0.push(shadow); }
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
    pub fn as_slice(&self) -> &[Shadow] { &self.0 }

    pub fn lerp(&self, other: &Shadows, t: f32) -> Shadows {
        let len = self.0.len().max(other.0.len());
        let mut result = Vec::with_capacity(len);
        for i in 0..len {
            let a = self.0.get(i).copied().unwrap_or_default();
            let b = other.0.get(i).copied().unwrap_or_default();
            result.push(a.lerp(&b, t));
        }
        Shadows(result)
    }

    pub fn parse(s: &str) -> Option<Self> {
        let mut shadows = Self::new();
        let mut depth = 0;
        let mut current = String::new();
        for c in s.chars() {
            match c {
                '(' => { depth += 1; current.push(c); }
                ')' => { depth -= 1; current.push(c); }
                ',' if depth == 0 => {
                    if let Some(shadow) = Shadow::parse(&current) { shadows.push(shadow); }
                    current.clear();
                }
                _ => current.push(c),
            }
        }
        if !current.is_empty() {
            if let Some(shadow) = Shadow::parse(&current) { shadows.push(shadow); }
        }
        if shadows.is_empty() { None } else { Some(shadows) }
    }
}

impl IntoIterator for Shadows {
    type Item = Shadow;
    type IntoIter = std::vec::IntoIter<Shadow>;
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

impl<'a> IntoIterator for &'a Shadows {
    type Item = &'a Shadow;
    type IntoIter = std::slice::Iter<'a, Shadow>;
    fn into_iter(self) -> Self::IntoIter { self.0.iter() }
}
