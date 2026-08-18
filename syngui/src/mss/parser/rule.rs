use std::collections::HashMap;

use super::super::stylesheet::*;
use super::super::ParseError;
use super::super::value::StyleValue;
use super::utils::ParserCursor;
use super::value::*;

fn insert_with_shorthand_expansion(
    declarations: &mut HashMap<String, StyleValue>,
    property: String,
    value: StyleValue,
) {
    let expanded = match property.as_str() {
        "border" => expand_border_shorthand(&value),
        "border-top" | "border-right" | "border-bottom" | "border-left" => {
            expand_border_side_shorthand(property.as_str(), &value)
        }
        "padding" => expand_edge_shorthand(
            ["padding-top", "padding-right", "padding-bottom", "padding-left"],
            &value,
        ),
        "margin" => expand_edge_shorthand(
            ["margin-top", "margin-right", "margin-bottom", "margin-left"],
            &value,
        ),
        "border-width" => expand_edge_shorthand(
            [
                "border-top-width",
                "border-right-width",
                "border-bottom-width",
                "border-left-width",
            ],
            &value,
        ),
        "border-radius" => expand_border_radius_shorthand(&value),
        "transform" => super::transform::expand_transform_shorthand(&value),
        _ => None,
    };
    match expanded {
        Some(entries) => {
            for (p, v) in entries {
                insert_with_shorthand_expansion(declarations, p, v);
            }
        }
        None => {
            declarations.insert(property, value);
        }
    }
}

pub(super) fn parse_root_variables(cursor: &mut ParserCursor, stylesheet: &mut StyleSheet) -> Result<(), ParseError> {
    cursor.consume(":root");
    cursor.skip_whitespace();

    if cursor.peek() != Some('{') {
        return Err(ParseError::UnexpectedToken(
            format!("Expected '{{', got {:?}", cursor.peek()),
            cursor.line
        ));
    }
    cursor.consume("{");
    cursor.skip_whitespace();

    while cursor.peek() != Some('}') && !cursor.is_eof() {
        cursor.skip_whitespace();
        if cursor.peek() == Some('}') || cursor.is_eof() { break; }

        if cursor.peek() == Some('/') && cursor.peek_next() == Some('*') {
            cursor.skip_comment();
            continue;
        }

        let name = parse_variable_name(cursor)?;
        cursor.skip_whitespace();

        if cursor.peek() != Some(':') {
            return Err(ParseError::UnexpectedToken(
                format!("Expected ':', got {:?}", cursor.peek()),
                cursor.line
            ));
        }
        cursor.consume(":");
        cursor.skip_whitespace();

        let value = parse_value(cursor)?;
        stylesheet.set_variable(name, value);

        cursor.skip_whitespace();
        if cursor.peek() == Some(';') {
            cursor.consume(";");
        }
        cursor.skip_whitespace();
    }

    if cursor.peek() == Some('}') {
        cursor.consume("}");
    } else {
        return Err(ParseError::UnclosedBlock("root".to_string(), cursor.line));
    }

    Ok(())
}

pub(super) fn parse_keyframes(cursor: &mut ParserCursor, stylesheet: &mut StyleSheet) -> Result<(), ParseError> {
    cursor.consume("@keyframes");
    cursor.skip_whitespace();

    let name = parse_identifier(cursor)?;
    cursor.skip_whitespace();

    if cursor.peek() != Some('{') {
        return Err(ParseError::UnexpectedToken(
            format!("Expected '{{' after @keyframes {}", name),
            cursor.line
        ));
    }
    cursor.consume("{");
    cursor.skip_whitespace();

    let mut steps = Vec::new();

    while cursor.peek() != Some('}') && !cursor.is_eof() {
        cursor.skip_whitespace();
        if cursor.peek() == Some('}') || cursor.is_eof() { break; }

        if cursor.peek() == Some('/') && cursor.peek_next() == Some('*') {
            cursor.skip_comment();
            continue;
        }

        let position = if cursor.starts_with("from") {
            cursor.consume("from");
            0.0
        } else if cursor.starts_with("to") {
            cursor.consume("to");
            1.0
        } else {
            let start = cursor.position;
            while !cursor.is_eof() {
                let c = cursor.peek().unwrap_or('\0');
                if c.is_ascii_digit() || c == '.' {
                    cursor.advance();
                } else {
                    break;
                }
            }
            let num_str = &cursor.input[start..cursor.position];
            let pct: f32 = num_str.parse().map_err(|_| {
                ParseError::InvalidValue(num_str.to_string(), "keyframe percentage".to_string(), cursor.line)
            })?;
            if cursor.peek() == Some('%') {
                cursor.consume("%");
            }
            pct / 100.0
        };

        cursor.skip_whitespace();

        if cursor.peek() != Some('{') {
            return Err(ParseError::UnexpectedToken(
                format!("Expected '{{' in @keyframes step"),
                cursor.line
            ));
        }
        cursor.consume("{");
        cursor.skip_whitespace();

        let mut declarations = std::collections::HashMap::new();
        while cursor.peek() != Some('}') && !cursor.is_eof() {
            cursor.skip_whitespace();
            if cursor.peek() == Some('}') || cursor.is_eof() { break; }

            if cursor.peek() == Some('/') && cursor.peek_next() == Some('*') {
                cursor.skip_comment();
                continue;
            }

            let property = parse_identifier(cursor)?;
            cursor.skip_whitespace();
            if cursor.peek() != Some(':') {
                return Err(ParseError::UnexpectedToken(
                    format!("Expected ':' in @keyframes declaration"),
                    cursor.line
                ));
            }
            cursor.consume(":");
            cursor.skip_whitespace();

            let value = parse_value(cursor)?;
            declarations.insert(property, value);

            cursor.skip_whitespace();
            if cursor.peek() == Some(';') {
                cursor.consume(";");
            }
            cursor.skip_whitespace();
        }

        if cursor.peek() == Some('}') {
            cursor.consume("}");
        }

        steps.push(KeyframeStep { position, declarations });
        cursor.skip_whitespace();
    }

    if cursor.peek() == Some('}') {
        cursor.consume("}");
    } else {
        return Err(ParseError::UnclosedBlock(format!("@keyframes {}", name), cursor.line));
    }

    steps.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));

    stylesheet.add_keyframes(KeyframesDefinition {
        name: name.clone(),
        steps,
    });

    Ok(())
}

pub(super) fn parse_rule(
    cursor: &mut ParserCursor,
    stylesheet: &mut StyleSheet,
    parent_chain: Option<&SelectorChain>,
) -> Result<(), ParseError> {
    let selector_str = parse_selector_string(cursor)?;

    if selector_str.is_empty() {
        return Err(ParseError::EmptySelector(cursor.line));
    }

    cursor.skip_whitespace();

    if cursor.peek() != Some('{') {
        return Err(ParseError::UnexpectedToken(
            format!("Expected '{{', got {:?}", cursor.peek()),
            cursor.line
        ));
    }
    cursor.consume("{");
    cursor.skip_whitespace();

    let mut declarations = std::collections::HashMap::new();

    while cursor.peek() != Some('}') && !cursor.is_eof() {
        cursor.skip_whitespace();
        if cursor.peek() == Some('}') || cursor.is_eof() {
            break;
        }

        if cursor.peek() == Some('/') && cursor.peek_next() == Some('*') {
            cursor.skip_comment();
            continue;
        }

        if is_nested_rule_start(cursor) {
            let current_selector = build_selector_for_nesting(
                cursor, &selector_str, parent_chain,
            )?;
            if !declarations.is_empty() {
                let selector = build_final_selector(cursor, &selector_str, parent_chain)?;
                let selector_str_full = build_selector_str(cursor, &selector_str, parent_chain);
                stylesheet.add_rule(StyleRule {
                    selector,
                    selector_str: selector_str_full,
                    declarations: declarations.clone(),
                });
                declarations.clear();
            }
            parse_rule(cursor, stylesheet, Some(&current_selector))?;
            cursor.skip_whitespace();
            continue;
        }

        let property = parse_identifier(cursor)?;
        cursor.skip_whitespace();

        if cursor.peek() != Some(':') {
            return Err(ParseError::UnexpectedToken(
                format!("Expected ':', got {:?}", cursor.peek()),
                cursor.line
            ));
        }
        cursor.consume(":");
        cursor.skip_whitespace();

        let value = parse_value(cursor)?;
        insert_with_shorthand_expansion(&mut declarations, property, value);

        cursor.skip_whitespace();
        if cursor.peek() == Some(';') {
            cursor.consume(";");
        }
        cursor.skip_whitespace();
    }

    if cursor.peek() == Some('}') {
        cursor.consume("}");
    } else {
        return Err(ParseError::UnclosedBlock(selector_str.clone(), cursor.line));
    }

    if !declarations.is_empty() {
        let selector = build_final_selector(cursor, &selector_str, parent_chain)?;
        let selector_str_full = build_selector_str(cursor, &selector_str, parent_chain);
        stylesheet.add_rule(StyleRule {
            selector,
            selector_str: selector_str_full,
            declarations,
        });
    }

    Ok(())
}

fn is_nested_rule_start(cursor: &ParserCursor) -> bool {
    let remaining = &cursor.input[cursor.position..];
    let first = match remaining.chars().next() {
        Some(c) => c,
        None => return false,
    };

    if matches!(first, '.' | '&' | '>' | '+' | '~' | '*') {
        return true;
    }

    if first.is_ascii_uppercase() {
        return true;
    }

    false
}

fn build_selector_for_nesting(
    cursor: &ParserCursor,
    selector_str: &str,
    parent_chain: Option<&SelectorChain>,
) -> Result<SelectorChain, ParseError> {
    let chains = super::selector::parse_selector_chains(cursor, selector_str)?;
    let first_chain = chains.into_iter().next()
        .ok_or_else(|| ParseError::EmptySelector(cursor.line))?;

    if let Some(parent) = parent_chain {
        Ok(combine_chains(cursor, parent, &first_chain))
    } else {
        let mut chain = first_chain;
        chain.pseudo = None;
        Ok(chain)
    }
}

fn build_final_selector(
    cursor: &ParserCursor,
    selector_str: &str,
    parent_chain: Option<&SelectorChain>,
) -> Result<Selector, ParseError> {
    let chains = super::selector::parse_selector_chains(cursor, selector_str)?;

    if let Some(parent) = parent_chain {
        let combined: Vec<SelectorChain> = chains.iter()
            .map(|c| combine_chains(cursor, parent, c))
            .collect();

        if combined.len() == 1 {
            let chain = combined.into_iter().next().unwrap();
            chain_to_selector(chain)
        } else {
            Ok(Selector::Group(combined))
        }
    } else {
        if chains.len() == 1 {
            let chain = chains.into_iter().next().unwrap();
            chain_to_selector(chain)
        } else {
            Ok(Selector::Group(chains))
        }
    }
}

fn chain_to_selector(chain: SelectorChain) -> Result<Selector, ParseError> {
    if chain.is_simple() {
        let part = chain.segments.into_iter().next().unwrap();
        match (part, chain.pseudo) {
            (SelectorPart::Class(c), None) => Ok(Selector::Class(c)),
            (SelectorPart::Class(c), Some(p)) => Ok(Selector::ClassPseudo(c, p)),
            (SelectorPart::Element(e), None) => Ok(Selector::Element(e)),
            (SelectorPart::Element(e), Some(p)) => Ok(Selector::ElementPseudo(e, p)),
            (SelectorPart::Universal, _) => Ok(Selector::Universal),
            (SelectorPart::Id(id), _) => Ok(Selector::Id(id)),
            (compound @ SelectorPart::Compound { .. }, pseudo) => {
                Ok(Selector::Complex(SelectorChain {
                    segments: vec![compound],
                    combinators: vec![],
                    pseudo,
                    leading_combinator: None,
                }))
            }
        }
    } else {
        Ok(Selector::Complex(chain))
    }
}

fn build_selector_str(cursor: &ParserCursor, selector_str: &str, parent_chain: Option<&SelectorChain>) -> String {
    if let Some(parent) = parent_chain {
        let parent_str = chain_to_string(cursor, parent);
        let child_str = selector_str.trim();
        if child_str.starts_with('&') {
            format!("{}{}", parent_str, &child_str[1..])
        } else if child_str.starts_with('>') || child_str.starts_with('+') || child_str.starts_with('~') {
            format!("{} {}", parent_str, child_str)
        } else {
            format!("{} {}", parent_str, child_str)
        }
    } else {
        selector_str.to_string()
    }
}

fn chain_to_string(_cursor: &ParserCursor, chain: &SelectorChain) -> String {
    let mut result = String::new();
    for (i, seg) in chain.segments.iter().enumerate() {
        if i > 0 {
            if let Some(comb) = chain.combinators.get(i - 1) {
                match comb {
                    Combinator::Descendant => result.push(' '),
                    Combinator::Child => result.push_str(" > "),
                    Combinator::AdjacentSibling => result.push_str(" + "),
                    Combinator::GeneralSibling => result.push_str(" ~ "),
                }
            }
        }
        match seg {
            SelectorPart::Class(c) => { result.push('.'); result.push_str(c); }
            SelectorPart::Element(e) => result.push_str(e),
            SelectorPart::Universal => result.push('*'),
            SelectorPart::Id(id) => { result.push('#'); result.push_str(id); }
            SelectorPart::Compound { element, id, classes } => {
                if let Some(e) = element { result.push_str(e); }
                if let Some(i) = id { result.push('#'); result.push_str(i); }
                for c in classes { result.push('.'); result.push_str(c); }
            }
        }
    }
    if let Some(pseudo) = &chain.pseudo {
        result.push(':');
        result.push_str(pseudo);
    }
    result
}

fn combine_chains(_cursor: &ParserCursor, parent: &SelectorChain, child: &SelectorChain) -> SelectorChain {
    if child.segments.is_empty() {
        let mut combined = parent.clone();
        combined.pseudo = child.pseudo.clone();
        return combined;
    }

    let mut segments = parent.segments.clone();
    let mut combinators = parent.combinators.clone();

    let leading = child.leading_combinator.unwrap_or(Combinator::Descendant);
    combinators.push(leading);

    segments.extend(child.segments.iter().cloned());
    combinators.extend(child.combinators.iter().cloned());

    SelectorChain {
        segments,
        combinators,
        pseudo: child.pseudo.clone(),
        leading_combinator: None,
    }
}
