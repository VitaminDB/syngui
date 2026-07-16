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

pub(super) fn expand_border_shorthand(value: &StyleValue) -> Option<Vec<(String, StyleValue)>> {
    let s = match value {
        StyleValue::String(s) => s.as_str(),
        _ => return None,
    };

    let mut tokens: Vec<&str> = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < s.len() {
        let ch = bytes[i] as char;
        match ch {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            c if c.is_ascii_whitespace() && depth == 0 => {
                if i > start {
                    let tok = s[start..i].trim();
                    if !tok.is_empty() { tokens.push(tok); }
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < s.len() {
        let tok = s[start..].trim();
        if !tok.is_empty() { tokens.push(tok); }
    }

    if tokens.len() < 2 {
        return None;
    }

    let width_str = tokens[0];
    if !width_str.ends_with("px")
        && width_str.parse::<f32>().is_err()
    {
        return None;
    }

    const STYLE_KEYWORDS: &[&str] = &["solid", "dashed", "dotted", "double", "groove", "ridge", "inset", "outset", "none", "hidden"];
    let mut color_start = 1usize;
    if tokens.len() >= 3 && STYLE_KEYWORDS.contains(&tokens[1]) {
        color_start = 2;
    } else if tokens.len() == 2 && STYLE_KEYWORDS.contains(&tokens[1]) {
        return None;
    }

    let color_str = tokens[color_start..].join(" ");
    if color_str.is_empty() {
        return None;
    }

    let mut width_cursor = ParserCursor::new(width_str);
    let width_value = parse_value(&mut width_cursor).ok()?;

    let mut color_cursor = ParserCursor::new(&color_str);
    let color_value = parse_value(&mut color_cursor).ok()?;

    Some(vec![
        ("border-width".to_string(), width_value),
        ("border-color".to_string(), color_value),
    ])
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
