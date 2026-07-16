use super::super::value::{StyleValue, Unit};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformOrigin {
    pub x: TransformOriginAxis,
    pub y: TransformOriginAxis,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformOriginAxis {
    Percent(f32),
    Px(f32),
}

impl TransformOriginAxis {
    pub fn resolve(&self, extent: f32) -> f32 {
        match self {
            TransformOriginAxis::Percent(p) => extent * p,
            TransformOriginAxis::Px(v) => *v,
        }
    }
}

impl TransformOrigin {
    pub const CENTER: TransformOrigin = TransformOrigin {
        x: TransformOriginAxis::Percent(0.5),
        y: TransformOriginAxis::Percent(0.5),
    };

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let tokens: Vec<&str> = s.split_ascii_whitespace().collect();
        match tokens.len() {
            1 => {
                if let Some((x, y)) = parse_origin_keyword_pair(tokens[0]) {
                    return Some(TransformOrigin { x, y });
                }
                let axis = parse_origin_axis(tokens[0])?;
                Some(TransformOrigin {
                    x: axis,
                    y: TransformOriginAxis::Percent(0.5),
                })
            }
            2 => parse_pair(tokens[0], tokens[1]),
            _ => None,
        }
    }
}

fn parse_pair(a: &str, b: &str) -> Option<TransformOrigin> {
    use TransformOriginAxis::Percent;

    fn classify(tok: &str) -> Option<(KeywordAxis, TransformOriginAxis)> {
        let lower = tok.to_ascii_lowercase();
        match lower.as_str() {
            "left" => Some((KeywordAxis::X, Percent(0.0))),
            "right" => Some((KeywordAxis::X, Percent(1.0))),
            "top" => Some((KeywordAxis::Y, Percent(0.0))),
            "bottom" => Some((KeywordAxis::Y, Percent(1.0))),
            "center" => Some((KeywordAxis::Either, Percent(0.5))),
            _ => parse_origin_axis(tok).map(|a| (KeywordAxis::Either, a)),
        }
    }

    let (a_axis, a_val) = classify(a)?;
    let (b_axis, b_val) = classify(b)?;

    if matches!((a_axis, b_axis), (KeywordAxis::X, KeywordAxis::X) | (KeywordAxis::Y, KeywordAxis::Y)) {
        return None;
    }

    let (x, y) = match (a_axis, b_axis) {
        (KeywordAxis::Y, _) => (b_val, a_val),
        _ => (a_val, b_val),
    };
    Some(TransformOrigin { x, y })
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum KeywordAxis { X, Y, Either }

fn parse_origin_keyword_pair(kw: &str) -> Option<(TransformOriginAxis, TransformOriginAxis)> {
    use TransformOriginAxis::Percent;
    let half = Percent(0.5);
    let zero = Percent(0.0);
    let full = Percent(1.0);
    match kw.to_ascii_lowercase().as_str() {
        "center" => Some((half, half)),
        "top" => Some((half, zero)),
        "bottom" => Some((half, full)),
        "left" => Some((zero, half)),
        "right" => Some((full, half)),
        _ => None,
    }
}

fn parse_origin_axis(s: &str) -> Option<TransformOriginAxis> {
    let s = s.trim();
    if let Some(stripped) = s.strip_suffix('%') {
        return stripped.trim().parse::<f32>().ok()
            .map(|v| TransformOriginAxis::Percent(v / 100.0));
    }
    if let Some(stripped) = s.strip_suffix("px") {
        return stripped.trim().parse::<f32>().ok().map(TransformOriginAxis::Px);
    }
    s.parse::<f32>().ok().map(TransformOriginAxis::Px)
}

pub(super) fn expand_transform_shorthand(value: &StyleValue) -> Option<Vec<(String, StyleValue)>> {
    let s = match value {
        StyleValue::String(s) => s.as_str(),
        _ => return None,
    };

    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }

    let funcs = tokenize_transform_functions(s)?;
    let mut tx = 0.0f32;
    let mut ty = 0.0f32;
    let mut rotate_deg = 0.0f32;
    let mut sx = 1.0f32;
    let mut sy = 1.0f32;
    let mut any = false;

    for (name, args) in funcs {
        match name.to_ascii_lowercase().as_str() {
            "translate" => {
                let (a, b) = parse_two_lengths(&args)?;
                tx += a;
                ty += b.unwrap_or(0.0);
                any = true;
            }
            "translatex" => {
                tx += parse_one_length(&args)?;
                any = true;
            }
            "translatey" => {
                ty += parse_one_length(&args)?;
                any = true;
            }
            "scale" => {
                let (a, b) = parse_two_numbers(&args)?;
                let scale_x = a;
                let scale_y = b.unwrap_or(a);
                sx *= scale_x;
                sy *= scale_y;
                any = true;
            }
            "scalex" => {
                sx *= parse_one_number(&args)?;
                any = true;
            }
            "scaley" => {
                sy *= parse_one_number(&args)?;
                any = true;
            }
            "rotate" => {
                rotate_deg += parse_angle_deg(&args)?;
                any = true;
            }
            _ => {}
        }
    }

    if !any {
        return Some(Vec::new());
    }

    let mut out = Vec::with_capacity(4);
    if tx != 0.0 {
        out.push(("translate-x".to_string(), StyleValue::Length(tx, Unit::Px)));
    }
    if ty != 0.0 {
        out.push(("translate-y".to_string(), StyleValue::Length(ty, Unit::Px)));
    }
    if rotate_deg != 0.0 {
        out.push(("rotate".to_string(), StyleValue::Number(rotate_deg)));
    }
    if (sx - 1.0).abs() > f32::EPSILON || (sy - 1.0).abs() > f32::EPSILON {
        if (sx - sy).abs() <= f32::EPSILON {
            out.push(("scale".to_string(), StyleValue::Number(sx)));
        } else {
            out.push(("scale-x".to_string(), StyleValue::Number(sx)));
            out.push(("scale-y".to_string(), StyleValue::Number(sy)));
        }
    }
    Some(out)
}

fn tokenize_transform_functions(s: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name_start = i;
        while i < bytes.len() && bytes[i] != b'(' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let name = s[name_start..i].to_string();
        if name.is_empty() {
            return None;
        }
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if i >= bytes.len() || bytes[i] != b'(' {
            return None;
        }
        i += 1;
        let args_start = i;
        let mut depth = 1i32;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            if depth == 0 { break; }
            i += 1;
        }
        if depth != 0 {
            return None;
        }
        let args = s[args_start..i].to_string();
        i += 1;
        out.push((name, args));
    }
    Some(out)
}

fn parse_one_length(s: &str) -> Option<f32> {
    parse_length_value(s.trim())
}

fn parse_two_lengths(s: &str) -> Option<(f32, Option<f32>)> {
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    match parts.len() {
        1 => Some((parse_length_value(parts[0])?, None)),
        2 => Some((parse_length_value(parts[0])?, Some(parse_length_value(parts[1])?))),
        _ => None,
    }
}

fn parse_one_number(s: &str) -> Option<f32> {
    let s = s.trim();
    s.strip_suffix('%')
        .and_then(|n| n.trim().parse::<f32>().ok().map(|v| v / 100.0))
        .or_else(|| s.parse::<f32>().ok())
}

fn parse_two_numbers(s: &str) -> Option<(f32, Option<f32>)> {
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    match parts.len() {
        1 => Some((parse_one_number(parts[0])?, None)),
        2 => Some((parse_one_number(parts[0])?, Some(parse_one_number(parts[1])?))),
        _ => None,
    }
}

fn parse_length_value(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(stripped) = s.strip_suffix("px") {
        return stripped.trim().parse::<f32>().ok();
    }
    s.parse::<f32>().ok()
}

fn parse_angle_deg(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(stripped) = s.strip_suffix("deg") {
        return stripped.trim().parse::<f32>().ok();
    }
    if let Some(stripped) = s.strip_suffix("rad") {
        return stripped.trim().parse::<f32>().ok()
            .map(|v: f32| v.to_degrees());
    }
    if let Some(stripped) = s.strip_suffix("turn") {
        return stripped.trim().parse::<f32>().ok()
            .map(|v: f32| v * 360.0);
    }
    if let Ok(v) = s.parse::<f32>() {
        if v == 0.0 {
            return Some(0.0);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(s: &str) -> Vec<(String, StyleValue)> {
        expand_transform_shorthand(&StyleValue::String(s.to_string())).unwrap()
    }

    #[test]
    fn scale_uniform() {
        let v = expand("scale(1.05)");
        assert_eq!(v, vec![("scale".to_string(), StyleValue::Number(1.05))]);
    }

    #[test]
    fn scale_xy() {
        let v = expand("scale(1.5, 2.0)");
        assert_eq!(v, vec![
            ("scale-x".to_string(), StyleValue::Number(1.5)),
            ("scale-y".to_string(), StyleValue::Number(2.0)),
        ]);
    }

    #[test]
    fn scale_zero() {
        let v = expand("scale(0.0)");
        assert_eq!(v, vec![("scale".to_string(), StyleValue::Number(0.0))]);
    }

    #[test]
    fn translate_xy() {
        let v = expand("translate(10px, 20px)");
        assert_eq!(v, vec![
            ("translate-x".to_string(), StyleValue::Length(10.0, Unit::Px)),
            ("translate-y".to_string(), StyleValue::Length(20.0, Unit::Px)),
        ]);
    }

    #[test]
    fn translate_y_only() {
        let v = expand("translateY(-2px)");
        assert_eq!(v, vec![
            ("translate-y".to_string(), StyleValue::Length(-2.0, Unit::Px)),
        ]);
    }

    #[test]
    fn rotate_deg() {
        let v = expand("rotate(-8deg)");
        assert_eq!(v, vec![("rotate".to_string(), StyleValue::Number(-8.0))]);
    }

    #[test]
    fn rotate_turn_normalized_to_deg() {
        let v = expand("rotate(0.5turn)");
        assert_eq!(v, vec![("rotate".to_string(), StyleValue::Number(180.0))]);
    }

    #[test]
    fn combined_translate_rotate_scale() {
        let v = expand("translate(10px, 20px) rotate(45deg) scale(1.5)");
        assert_eq!(v, vec![
            ("translate-x".to_string(), StyleValue::Length(10.0, Unit::Px)),
            ("translate-y".to_string(), StyleValue::Length(20.0, Unit::Px)),
            ("rotate".to_string(), StyleValue::Number(45.0)),
            ("scale".to_string(), StyleValue::Number(1.5)),
        ]);
    }

    #[test]
    fn none_keyword_returns_empty() {
        let v = expand_transform_shorthand(&StyleValue::String("none".to_string())).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn unbalanced_parens_rejected() {
        assert!(expand_transform_shorthand(&StyleValue::String("scale(1.05".to_string())).is_none());
    }

    #[test]
    fn matrix_silently_skipped() {
        let v = expand("matrix(1, 0, 0, 1, 0, 0)");
        assert!(v.is_empty(), "got: {:?}", v);
    }

    #[test]
    fn transform_origin_center_keyword() {
        let o = TransformOrigin::parse("center").unwrap();
        assert_eq!(o, TransformOrigin::CENTER);
    }

    #[test]
    fn transform_origin_top_left_pair() {
        let o = TransformOrigin::parse("top left").unwrap();
        assert_eq!(o, TransformOrigin {
            x: TransformOriginAxis::Percent(0.0),
            y: TransformOriginAxis::Percent(0.0),
        });
    }

    #[test]
    fn transform_origin_percent_pair() {
        let o = TransformOrigin::parse("50% 50%").unwrap();
        assert_eq!(o, TransformOrigin::CENTER);
    }

    #[test]
    fn transform_origin_px_pair() {
        let o = TransformOrigin::parse("10px 20px").unwrap();
        assert_eq!(o, TransformOrigin {
            x: TransformOriginAxis::Px(10.0),
            y: TransformOriginAxis::Px(20.0),
        });
    }

    #[test]
    fn transform_origin_single_token_defaults_y_to_center() {
        let o = TransformOrigin::parse("25%").unwrap();
        assert_eq!(o, TransformOrigin {
            x: TransformOriginAxis::Percent(0.25),
            y: TransformOriginAxis::Percent(0.5),
        });
    }

    #[test]
    fn transform_origin_resolve_percent() {
        let o = TransformOrigin::CENTER;
        assert_eq!(o.x.resolve(100.0), 50.0);
        assert_eq!(o.y.resolve(80.0), 40.0);
    }

    #[test]
    fn end_to_end_scale_shorthand_expands_in_stylesheet() {
        use crate::mss::parser::MssParser;

        let src = ".btn:hover { transform: scale(1.05) translateY(-2px) rotate(-8deg); }";
        let (sheet, warnings) = MssParser::new(src).parse().expect("parse");
        assert!(warnings.is_empty(), "got warnings: {:?}", warnings);

        let rule = sheet.rules().first().expect("one rule");
        let decl = &rule.declarations;
        assert!(decl.get("transform").is_none(), "shorthand `transform` should be expanded away");
        assert_eq!(decl.get("scale"), Some(&StyleValue::Number(1.05)));
        assert_eq!(
            decl.get("translate-y"),
            Some(&StyleValue::Length(-2.0, Unit::Px))
        );
        assert_eq!(decl.get("rotate"), Some(&StyleValue::Number(-8.0)));
        assert!(decl.get("translate-x").is_none());
    }
}
