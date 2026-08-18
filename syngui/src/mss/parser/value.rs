use super::utils::ParserCursor;
use super::super::value::*;

pub(super) fn parse_identifier(cursor: &mut ParserCursor) -> Result<String, super::super::ParseError> {
    let start = cursor.position;

    while !cursor.is_eof() {
        let c = cursor.peek().unwrap_or('\0');
        if c.is_alphanumeric() || c == '-' || c == '_' {
            cursor.advance();
        } else {
            break;
        }
    }

    if start == cursor.position {
        return Err(super::super::ParseError::UnexpectedToken(
            "Expected identifier".to_string(),
            cursor.line
        ));
    }

    Ok(cursor.input[start..cursor.position].to_string())
}

pub(super) fn parse_variable_name(cursor: &mut ParserCursor) -> Result<String, super::super::ParseError> {
    if !cursor.starts_with("--") {
        return Err(super::super::ParseError::InvalidProperty(
            "Expected CSS variable (--name)".to_string(),
            cursor.line
        ));
    }

    cursor.consume("--");
    let name = parse_identifier(cursor)?;
    Ok(format!("--{}", name))
}

pub(super) fn parse_length(_cursor: &ParserCursor, s: &str) -> Option<StyleValue> {
    let num_end = s.find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')?;
    let num_str = &s[..num_end];
    let unit_str = &s[num_end..];

    let value: f32 = num_str.parse().ok()?;
    let unit: Unit = unit_str.parse().ok()?;

    Some(StyleValue::Length(value, unit))
}

pub(super) fn parse_var_function(cursor: &mut ParserCursor) -> Result<StyleValue, super::super::ParseError> {
    cursor.consume("var(");
    cursor.skip_whitespace();

    if !cursor.starts_with("--") {
        return Err(super::super::ParseError::InvalidValue(
            "var()".to_string(),
            "Expected --variable".to_string(),
            cursor.line
        ));
    }

    let name = parse_variable_name(cursor)?;
    cursor.skip_whitespace();

    match cursor.peek() {
        Some(')') => {
            cursor.consume(")");
            Ok(StyleValue::Var(name))
        }
        Some(',') => {
            cursor.advance();
            cursor.skip_whitespace();

            let fallback_start = cursor.position;
            let mut depth = 0i32;
            while !cursor.is_eof() {
                match cursor.peek() {
                    Some('(') => { depth += 1; cursor.advance(); }
                    Some(')') if depth == 0 => break,
                    Some(')') => { depth -= 1; cursor.advance(); }
                    _ => cursor.advance(),
                }
            }
            let fallback_str = cursor.input[fallback_start..cursor.position].trim();

            if cursor.peek() != Some(')') {
                return Err(super::super::ParseError::UnexpectedToken(
                    format!("Expected ')' after var() fallback, got {:?}", cursor.peek()),
                    cursor.line
                ));
            }
            cursor.consume(")");

            if fallback_str.is_empty() {
                return Err(super::super::ParseError::InvalidValue(
                    "var()".to_string(),
                    "Empty fallback after ','".to_string(),
                    cursor.line
                ));
            }

            let mut fallback_cursor = ParserCursor::new(fallback_str);
            let fallback = parse_value(&mut fallback_cursor)?;
            Ok(StyleValue::VarWithFallback(name, Box::new(fallback)))
        }
        _ => Err(super::super::ParseError::UnexpectedToken(
            format!("Expected ')' or ',', got {:?}", cursor.peek()),
            cursor.line
        )),
    }
}

pub(super) fn parse_value(cursor: &mut ParserCursor) -> Result<StyleValue, super::super::ParseError> {
    cursor.skip_whitespace();

    if cursor.starts_with("var(") {
        return parse_var_function(cursor);
    }

    let start = cursor.position;
    let mut depth = 0;

    while !cursor.is_eof() {
        let c = cursor.peek();
        match c {
            Some('(') | Some('[') | Some('{') => {
                depth += 1;
                cursor.advance();
            }
            Some(')') | Some(']') | Some('}') if depth > 0 => {
                depth -= 1;
                cursor.advance();
            }
            Some(';') | Some('}') if depth == 0 => break,
            _ => cursor.advance(),
        }
    }

    let value_str = cursor.input[start..cursor.position].trim();

    if value_str.is_empty() {
        return Err(super::super::ParseError::InvalidValue(
            "empty".to_string(),
            "value expected".to_string(),
            cursor.line
        ));
    }

    match value_str {
        "inherit" => return Ok(StyleValue::Inherit),
        "initial" => return Ok(StyleValue::Initial),
        "unset"   => return Ok(StyleValue::Unset),
        _ => {}
    }

    match value_str {
        "auto"        => return Ok(StyleValue::Length(0.0, Unit::Auto)),
        "fit-content" => return Ok(StyleValue::Length(0.0, Unit::FitContent)),
        "max-content" => return Ok(StyleValue::Length(0.0, Unit::MaxContent)),
        "min-content" => return Ok(StyleValue::Length(0.0, Unit::MinContent)),
        _ => {}
    }

    if let Some(gradient) = super::gradient::parse_gradient(value_str) {
        return Ok(gradient);
    }

    if let Some(color) = Color::parse_color_function(value_str) {
        return Ok(StyleValue::Color(color));
    }

    if let Some(color) = Color::parse(value_str) {
        if !value_str.contains(' ') || value_str.starts_with("rgb(") || value_str.starts_with("rgba(") {
            return Ok(StyleValue::Color(color));
        }
    }

    if !value_str.contains(' ') {
        if let Some(len) = parse_length(cursor, value_str) {
            return Ok(len);
        }
    }

    if !value_str.contains(' ') {
        if let Ok(n) = value_str.parse::<f32>() {
            return Ok(StyleValue::Number(n));
        }
    }

    if value_str.contains(',') || value_str.contains("px") {
        return Ok(StyleValue::String(value_str.to_string()));
    }

    Ok(StyleValue::String(value_str.to_string()))
}

const BORDER_STYLE_KEYWORDS: &[&str] = &[
    "solid", "dashed", "dotted", "double", "groove", "ridge", "inset", "outset", "none", "hidden",
];

struct BorderParts {
    width: Option<StyleValue>,
    style: Option<StyleValue>,
    color: Option<StyleValue>,
}

fn parse_border_parts(value: &StyleValue) -> Option<BorderParts> {
    let s = match value {
        StyleValue::String(s) => s.clone(),
        StyleValue::Length(_, _) | StyleValue::Number(_) => {
            return Some(BorderParts { width: Some(value.clone()), style: None, color: None });
        }
        StyleValue::Color(_) | StyleValue::Gradient(_) => {
            return Some(BorderParts { width: None, style: None, color: Some(value.clone()) });
        }
        _ => return None,
    };

    let tokens = tokenize_shorthand(&s);
    if tokens.is_empty() {
        return None;
    }

    let mut width: Option<StyleValue> = None;
    let mut style: Option<StyleValue> = None;
    let mut color_tokens: Vec<&str> = Vec::new();

    for tok in tokens {
        let lower = tok.to_ascii_lowercase();
        if style.is_none() && BORDER_STYLE_KEYWORDS.contains(&lower.as_str()) {
            style = Some(StyleValue::String(lower));
            continue;
        }
        if width.is_none() {
            match parse_token(tok) {
                Some(v @ StyleValue::Length(_, _)) | Some(v @ StyleValue::Number(_)) => {
                    width = Some(v);
                    continue;
                }
                _ => {}
            }
        }
        color_tokens.push(tok);
    }

    let color = if color_tokens.is_empty() {
        None
    } else {
        parse_token(&color_tokens.join(" "))
    };

    if width.is_none() && style.is_none() && color.is_none() {
        return None;
    }

    Some(BorderParts { width, style, color })
}

fn border_parts_to_declarations(
    width_prop: &str,
    style_prop: &str,
    color_prop: &str,
    parts: BorderParts,
) -> Vec<(String, StyleValue)> {
    let hidden = matches!(
        parts.style.as_ref(),
        Some(StyleValue::String(s)) if s == "none" || s == "hidden"
    );

    let width = if hidden {
        Some(StyleValue::Length(0.0, Unit::Px))
    } else if parts.width.is_some() {
        parts.width.clone()
    } else {
        Some(StyleValue::Length(1.0, Unit::Px))
    };

    let mut out = Vec::new();
    if let Some(w) = width {
        out.push((width_prop.to_string(), w));
    }
    if let Some(s) = parts.style {
        out.push((style_prop.to_string(), s));
    }
    if let Some(c) = parts.color {
        out.push((color_prop.to_string(), c));
    }
    out
}

pub(super) fn expand_border_shorthand(value: &StyleValue) -> Option<Vec<(String, StyleValue)>> {
    let parts = parse_border_parts(value)?;
    Some(border_parts_to_declarations(
        "border-width",
        "border-style",
        "border-color",
        parts,
    ))
}

pub(super) fn expand_border_side_shorthand(
    property: &str,
    value: &StyleValue,
) -> Option<Vec<(String, StyleValue)>> {
    let side = property.strip_prefix("border-")?;
    if !matches!(side, "top" | "right" | "bottom" | "left") {
        return None;
    }
    let parts = parse_border_parts(value)?;
    Some(border_parts_to_declarations(
        &format!("border-{side}-width"),
        &format!("border-{side}-style"),
        &format!("border-{side}-color"),
        parts,
    ))
}

fn tokenize_shorthand(s: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < s.len() {
        let ch = bytes[i] as char;
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            c if c.is_ascii_whitespace() && depth == 0 => {
                if i > start {
                    let tok = s[start..i].trim();
                    if !tok.is_empty() {
                        tokens.push(tok);
                    }
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < s.len() {
        let tok = s[start..].trim();
        if !tok.is_empty() {
            tokens.push(tok);
        }
    }
    tokens
}

fn parse_token(tok: &str) -> Option<StyleValue> {
    let mut cursor = ParserCursor::new(tok);
    parse_value(&mut cursor).ok()
}

pub(super) fn expand_edge_shorthand(
    longhand_names: [&str; 4],
    value: &StyleValue,
) -> Option<Vec<(String, StyleValue)>> {
    let same = |v: &StyleValue| {
        longhand_names
            .iter()
            .map(|name| (name.to_string(), v.clone()))
            .collect::<Vec<_>>()
    };
    match value {
        StyleValue::Length(_, _) | StyleValue::Number(_) | StyleValue::Var(_) | StyleValue::VarWithFallback(_, _) => {
            Some(same(value))
        }
        StyleValue::String(s) => {
            let tokens = tokenize_shorthand(s);
            if tokens.is_empty() {
                return None;
            }
            let parsed: Vec<StyleValue> = tokens.iter().filter_map(|t| parse_token(t)).collect();
            if parsed.len() != tokens.len() {
                return None;
            }
            let (t, r, b, l) = match parsed.len() {
                1 => (parsed[0].clone(), parsed[0].clone(), parsed[0].clone(), parsed[0].clone()),
                2 => (parsed[0].clone(), parsed[1].clone(), parsed[0].clone(), parsed[1].clone()),
                3 => (parsed[0].clone(), parsed[1].clone(), parsed[2].clone(), parsed[1].clone()),
                4 => (parsed[0].clone(), parsed[1].clone(), parsed[2].clone(), parsed[3].clone()),
                _ => return None,
            };
            let values = [t, r, b, l];
            Some(
                longhand_names
                    .iter()
                    .zip(values.into_iter())
                    .map(|(name, v)| (name.to_string(), v))
                    .collect(),
            )
        }
        _ => None,
    }
}

pub(super) fn expand_border_radius_shorthand(
    value: &StyleValue,
) -> Option<Vec<(String, StyleValue)>> {
    let corners = [
        "border-top-left-radius",
        "border-top-right-radius",
        "border-bottom-right-radius",
        "border-bottom-left-radius",
    ];
    let same = |v: &StyleValue| {
        corners
            .iter()
            .map(|c| (c.to_string(), v.clone()))
            .collect::<Vec<_>>()
    };
    match value {
        StyleValue::Length(_, _) | StyleValue::Number(_) | StyleValue::Var(_) => Some(same(value)),
        StyleValue::String(s) => {
            let tokens = tokenize_shorthand(s);
            if tokens.is_empty() {
                return None;
            }
            let parsed: Vec<StyleValue> = tokens.iter().filter_map(|t| parse_token(t)).collect();
            if parsed.len() != tokens.len() {
                return None;
            }
            let (tl, tr, br, bl) = match parsed.len() {
                1 => (parsed[0].clone(), parsed[0].clone(), parsed[0].clone(), parsed[0].clone()),
                2 => (parsed[0].clone(), parsed[1].clone(), parsed[0].clone(), parsed[1].clone()),
                3 => (parsed[0].clone(), parsed[1].clone(), parsed[2].clone(), parsed[1].clone()),
                4 => (parsed[0].clone(), parsed[1].clone(), parsed[2].clone(), parsed[3].clone()),
                _ => return None,
            };
            Some(vec![
                (corners[0].to_string(), tl),
                (corners[1].to_string(), tr),
                (corners[2].to_string(), br),
                (corners[3].to_string(), bl),
            ])
        }
        _ => None,
    }
}

pub(super) fn parse_selector_string(cursor: &mut ParserCursor) -> Result<String, super::super::ParseError> {
    let mut result = String::new();

    while !cursor.is_eof() && cursor.peek() != Some('{') {
        if cursor.peek() == Some('/') && cursor.peek_next() == Some('*') {
            cursor.skip_comment();
            continue;
        }
        if let Some(c) = cursor.peek() {
            result.push(c);
        }
        cursor.advance();
    }

    Ok(result.trim().to_string())
}
