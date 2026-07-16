use super::super::value::{Color, StyleValue};

pub fn parse_gradient(value: &str) -> Option<StyleValue> {
    let value = value.trim();
    if value.starts_with("linear-gradient(") && value.ends_with(')') {
        let inner = &value["linear-gradient(".len()..value.len() - 1];
        return parse_linear_gradient(inner).map(StyleValue::Gradient);
    }
    if value.starts_with("radial-gradient(") && value.ends_with(')') {
        let inner = &value["radial-gradient(".len()..value.len() - 1];
        return parse_radial_gradient(inner).map(StyleValue::Gradient);
    }
    if value.starts_with("conic-gradient(") && value.ends_with(')') {
        let inner = &value["conic-gradient(".len()..value.len() - 1];
        return parse_conic_gradient(inner).map(StyleValue::Gradient);
    }
    None
}

fn parse_linear_gradient(inner: &str) -> Option<crate::core::Gradient> {
    use crate::core::Gradient;

    let parts = split_gradient_args(inner);
    if parts.len() < 2 {
        return None;
    }

    let mut idx = 0;
    let angle_deg;

    let first = parts[0].trim();
    if let Some(angle) = parse_angle_or_direction(first) {
        angle_deg = angle;
        idx = 1;
    } else {
        angle_deg = 180.0;
    }

    let stops = parse_color_stops(&parts[idx..])?;
    if stops.len() < 2 {
        return None;
    }

    Some(Gradient::Linear { angle_deg, stops })
}

fn parse_radial_gradient(inner: &str) -> Option<crate::core::Gradient> {
    use crate::core::{Gradient, GradientShape};

    let parts = split_gradient_args(inner);
    if parts.len() < 2 {
        return None;
    }

    let mut idx = 0;
    let mut shape = GradientShape::Ellipse;
    let mut center = (0.5f32, 0.5f32);

    let first = parts[0].trim().to_lowercase();
    let is_descriptor = first.contains("circle") || first.contains("ellipse")
        || first.starts_with("at ") || first.contains(" at ");

    if is_descriptor {
        idx = 1;
        if first.contains("circle") {
            shape = GradientShape::Circle;
        }
        if let Some(at_pos) = first.find("at ") {
            let pos_str = first[at_pos + 3..].trim();
            center = parse_gradient_position(pos_str);
        }
    }

    let stops = parse_color_stops(&parts[idx..])?;
    if stops.len() < 2 {
        return None;
    }

    Some(Gradient::Radial { shape, center, stops, quality: crate::core::GRADIENT_DEFAULT_QUALITY })
}

fn parse_conic_gradient(inner: &str) -> Option<crate::core::Gradient> {
    use crate::core::Gradient;

    let parts = split_gradient_args(inner);
    if parts.len() < 2 {
        return None;
    }

    let mut idx = 0;
    let mut from_angle = 0.0f32;
    let mut center = (0.5f32, 0.5f32);

    let first = parts[0].trim().to_lowercase();
    let is_descriptor = first.starts_with("from ") || first.starts_with("at ")
        || first.contains(" at ");

    if is_descriptor {
        idx = 1;
        if let Some(from_pos) = first.find("from ") {
            let rest = &first[from_pos + 5..];
            let angle_end = rest.find(" at ").unwrap_or(rest.len());
            let angle_str = rest[..angle_end].trim();
            from_angle = parse_angle_value(angle_str).unwrap_or(0.0);
        }
        if let Some(at_pos) = first.find("at ") {
            let pos_str = first[at_pos + 3..].trim();
            center = parse_gradient_position(pos_str);
        }
    }

    let stops = parse_color_stops(&parts[idx..])?;
    if stops.len() < 2 {
        return None;
    }

    Some(Gradient::Conic { from_angle, center, stops, quality: crate::core::GRADIENT_DEFAULT_QUALITY })
}

fn split_gradient_args(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for c in s.chars() {
        match c {
            '(' => { depth += 1; current.push(c); }
            ')' => { depth -= 1; current.push(c); }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        parts.push(trimmed);
    }
    parts
}

fn parse_angle_value(s: &str) -> Option<f32> {
    let s = s.trim();
    if s.ends_with("deg") {
        s[..s.len() - 3].trim().parse::<f32>().ok()
    } else if s.ends_with("turn") {
        s[..s.len() - 4].trim().parse::<f32>().ok().map(|t| t * 360.0)
    } else if s.ends_with("grad") {
        s[..s.len() - 4].trim().parse::<f32>().ok().map(|g| g * 360.0 / 400.0)
    } else if s.ends_with("rad") {
        s[..s.len() - 3].trim().parse::<f32>().ok().map(|r| r.to_degrees())
    } else {
        s.parse::<f32>().ok()
    }
}

fn parse_angle_or_direction(s: &str) -> Option<f32> {
    let s = s.trim();

    if let Some(angle) = parse_angle_value(s) {
        return Some(angle);
    }

    match s.to_lowercase().as_str() {
        "to top" => Some(0.0),
        "to right" => Some(90.0),
        "to bottom" => Some(180.0),
        "to left" => Some(270.0),
        "to top right" | "to right top" => Some(45.0),
        "to bottom right" | "to right bottom" => Some(135.0),
        "to bottom left" | "to left bottom" => Some(225.0),
        "to top left" | "to left top" => Some(315.0),
        _ => None,
    }
}

fn parse_gradient_position(s: &str) -> (f32, f32) {
    let s = s.trim();
    match s {
        "center" => (0.5, 0.5),
        "top" => (0.5, 0.0),
        "bottom" => (0.5, 1.0),
        "left" => (0.0, 0.5),
        "right" => (1.0, 0.5),
        _ => {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() >= 2 {
                let x = parse_position_component(parts[0]);
                let y = parse_position_component(parts[1]);
                (x, y)
            } else if parts.len() == 1 {
                let v = parse_position_component(parts[0]);
                (v, v)
            } else {
                (0.5, 0.5)
            }
        }
    }
}

fn parse_position_component(s: &str) -> f32 {
    match s.trim() {
        "center" => 0.5,
        "left" | "top" => 0.0,
        "right" | "bottom" => 1.0,
        other => {
            if other.ends_with('%') {
                other[..other.len() - 1].parse::<f32>().unwrap_or(50.0) / 100.0
            } else if other.ends_with("px") {
                other[..other.len() - 2].parse::<f32>().unwrap_or(0.5)
            } else {
                other.parse::<f32>().unwrap_or(0.5)
            }
        }
    }
}

fn parse_color_stops(parts: &[String]) -> Option<Vec<crate::core::ColorStop>> {
    use crate::core::ColorStop;

    let mut stops = Vec::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        let (color_str, pos_str) = split_color_and_position(part);
        let mss_color = Color::parse(color_str)?;
        let core_color = crate::animation::transition::mss_color_to_core(mss_color);

        let position = pos_str.and_then(|p| {
            let p = p.trim();
            if p.ends_with('%') {
                p[..p.len() - 1].parse::<f32>().ok().map(|v| v / 100.0)
            } else {
                p.parse::<f32>().ok()
            }
        });

        stops.push(ColorStop { color: core_color, position });
    }

    if stops.is_empty() {
        None
    } else {
        Some(stops)
    }
}

fn split_color_and_position(s: &str) -> (&str, Option<&str>) {
    let s = s.trim();

    if s.starts_with("rgb(") || s.starts_with("rgba(") {
        if let Some(close) = s.find(')') {
            let after = s[close + 1..].trim();
            if after.is_empty() {
                return (s, None);
            } else {
                return (&s[..close + 1], Some(after));
            }
        }
    }

    if let Some(last_space) = s.rfind(' ') {
        let (left, right) = s.split_at(last_space);
        let right = right.trim();
        if right.ends_with('%') || right.parse::<f32>().is_ok() {
            return (left.trim(), Some(right));
        }
    }

    (s, None)
}
