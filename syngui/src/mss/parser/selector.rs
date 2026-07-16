use super::super::stylesheet::*;
use super::super::ParseError;
use super::utils::ParserCursor;

#[derive(Debug, Clone)]
enum SelectorToken {
    Part(SelectorPart),
    PartPseudo(SelectorPart, String),
    Combinator(Combinator),
    PseudoOnly(String),
}

pub(super) fn parse_selector_chains(cursor: &ParserCursor, s: &str) -> Result<Vec<SelectorChain>, ParseError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(ParseError::EmptySelector(cursor.line));
    }

    let parts = cursor.split_by_comma(s);
    let mut chains = Vec::new();

    for part in parts {
        let part = part.trim();
        if part.is_empty() { continue; }
        chains.push(parse_single_chain(cursor, part)?);
    }

    if chains.is_empty() {
        return Err(ParseError::EmptySelector(cursor.line));
    }

    Ok(chains)
}

fn parse_single_chain(cursor: &ParserCursor, s: &str) -> Result<SelectorChain, ParseError> {
    let s = s.trim();

    let (leading_combinator, rest) = extract_leading_combinator(s);

    if rest.is_empty() {
        return Ok(SelectorChain {
            segments: vec![],
            combinators: vec![],
            pseudo: None,
            leading_combinator,
        });
    }

    let tokens = tokenize_selector(cursor, rest)?;

    if tokens.is_empty() {
        return Ok(SelectorChain {
            segments: vec![],
            combinators: vec![],
            pseudo: None,
            leading_combinator,
        });
    }

    let mut segments: Vec<SelectorPart> = Vec::new();
    let mut combinators = Vec::new();
    let mut pseudo = None;
    let mut expect_segment = true;

    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            SelectorToken::Part(part) => {
                if !expect_segment {
                    if let Some(last) = segments.last_mut() {
                        *last = merge_into_compound(last.clone(), part.clone());
                        i += 1;
                        continue;
                    }
                }
                segments.push(part.clone());
                expect_segment = false;
                i += 1;
            }
            SelectorToken::PartPseudo(part, ps) => {
                if !expect_segment {
                    if let Some(last) = segments.last_mut() {
                        *last = merge_into_compound(last.clone(), part.clone());
                        pseudo = Some(ps.clone());
                        i += 1;
                        continue;
                    }
                }
                segments.push(part.clone());
                pseudo = Some(ps.clone());
                expect_segment = false;
                i += 1;
            }
            SelectorToken::Combinator(comb) => {
                combinators.push(*comb);
                expect_segment = true;
                i += 1;
            }
            SelectorToken::PseudoOnly(ps) => {
                pseudo = Some(ps.clone());
                i += 1;
            }
        }
    }

    Ok(SelectorChain {
        segments,
        combinators,
        pseudo,
        leading_combinator,
    })
}

fn extract_leading_combinator(s: &str) -> (Option<Combinator>, &str) {
    let s = s.trim();

    if s.starts_with('&') {
        let rest = s[1..].trim_start();
        if rest.is_empty() {
            return (None, "");
        }
        if rest.starts_with(':') {
            return (None, rest);
        }
        if rest.starts_with('.') || rest.starts_with('#') {
            return (Some(Combinator::Descendant), rest);
        }
        if rest.starts_with('>') {
            let after = rest[1..].trim_start();
            return (Some(Combinator::Child), after);
        }
        if rest.starts_with('+') {
            let after = rest[1..].trim_start();
            return (Some(Combinator::AdjacentSibling), after);
        }
        if rest.starts_with('~') {
            let after = rest[1..].trim_start();
            return (Some(Combinator::GeneralSibling), after);
        }
        return (Some(Combinator::Descendant), rest);
    }

    if s.starts_with('>') {
        let rest = s[1..].trim_start();
        return (Some(Combinator::Child), rest);
    }
    if s.starts_with('+') {
        let rest = s[1..].trim_start();
        return (Some(Combinator::AdjacentSibling), rest);
    }
    if s.starts_with('~') {
        let rest = s[1..].trim_start();
        return (Some(Combinator::GeneralSibling), rest);
    }

    (None, s)
}

fn tokenize_selector(cursor: &ParserCursor, s: &str) -> Result<Vec<SelectorToken>, ParseError> {
    let s = s.trim();
    let mut tokens = Vec::new();
    let mut pos = 0;
    let chars: Vec<char> = s.chars().collect();

    while pos < chars.len() {
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        if pos >= chars.len() { break; }

        let c = chars[pos];

        if c == '>' {
            tokens.push(SelectorToken::Combinator(Combinator::Child));
            pos += 1;
            continue;
        }
        if c == '+' {
            tokens.push(SelectorToken::Combinator(Combinator::AdjacentSibling));
            pos += 1;
            continue;
        }
        if c == '~' {
            tokens.push(SelectorToken::Combinator(Combinator::GeneralSibling));
            pos += 1;
            continue;
        }

        if c == '.' {
            pos += 1;
            let start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '-' || chars[pos] == '_') {
                pos += 1;
            }
            let name: String = chars[start..pos].iter().collect();
            if name.is_empty() {
                return Err(ParseError::InvalidSelector(s.to_string(), cursor.line));
            }
            if pos < chars.len() && chars[pos] == ':' {
                pos += 1;
                let ps_start = pos;
                while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '-' || chars[pos] == '_') {
                    pos += 1;
                }
                let pseudo: String = chars[ps_start..pos].iter().collect();
                tokens.push(SelectorToken::PartPseudo(SelectorPart::Class(name), pseudo));
            } else {
                tokens.push(SelectorToken::Part(SelectorPart::Class(name)));
            }
            maybe_insert_descendant(&chars, pos, &mut tokens);
            continue;
        }

        if c == '#' {
            pos += 1;
            let start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '-' || chars[pos] == '_') {
                pos += 1;
            }
            let name: String = chars[start..pos].iter().collect();
            tokens.push(SelectorToken::Part(SelectorPart::Id(name)));
            maybe_insert_descendant(&chars, pos, &mut tokens);
            continue;
        }

        if c == '*' {
            pos += 1;
            tokens.push(SelectorToken::Part(SelectorPart::Universal));
            maybe_insert_descendant(&chars, pos, &mut tokens);
            continue;
        }

        if c.is_alphabetic() {
            let start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '-' || chars[pos] == '_') {
                pos += 1;
            }
            let name: String = chars[start..pos].iter().collect();
            if pos < chars.len() && chars[pos] == ':' {
                pos += 1;
                let ps_start = pos;
                while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '-' || chars[pos] == '_') {
                    pos += 1;
                }
                let pseudo: String = chars[ps_start..pos].iter().collect();
                tokens.push(SelectorToken::PartPseudo(SelectorPart::Element(name), pseudo));
            } else {
                tokens.push(SelectorToken::Part(SelectorPart::Element(name)));
            }
            maybe_insert_descendant(&chars, pos, &mut tokens);
            continue;
        }

        if c == ':' {
            pos += 1;
            let ps_start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '-' || chars[pos] == '_') {
                pos += 1;
            }
            let pseudo: String = chars[ps_start..pos].iter().collect();
            tokens.push(SelectorToken::PseudoOnly(pseudo));
            continue;
        }

        pos += 1;
    }

    Ok(tokens)
}

fn maybe_insert_descendant(chars: &[char], pos: usize, tokens: &mut Vec<SelectorToken>) {
    if pos >= chars.len() { return; }
    let next_immediate = chars[pos];
    if matches!(next_immediate, '.' | '#') {
        return;
    }
    let mut p = pos;
    while p < chars.len() && chars[p].is_whitespace() {
        p += 1;
    }
    if p >= chars.len() { return; }
    let next = chars[p];
    if matches!(next, '.' | '#' | '*') || next.is_alphabetic() {
        if !matches!(tokens.last(), Some(SelectorToken::Combinator(_))) {
            tokens.push(SelectorToken::Combinator(Combinator::Descendant));
        }
    }
}

pub(super) fn merge_into_compound(existing: SelectorPart, next: SelectorPart) -> SelectorPart {
    let (mut element, mut id, mut classes) = match existing {
        SelectorPart::Element(e) => (Some(e), None, Vec::new()),
        SelectorPart::Class(c) => (None, None, vec![c]),
        SelectorPart::Id(i) => (None, Some(i), Vec::new()),
        SelectorPart::Universal => (None, None, Vec::new()),
        SelectorPart::Compound { element, id, classes } => (element, id, classes),
    };
    match next {
        SelectorPart::Element(e) => element = Some(e),
        SelectorPart::Class(c) => classes.push(c),
        SelectorPart::Id(i) => id = Some(i),
        SelectorPart::Universal => {}
        SelectorPart::Compound { element: e, id: i, classes: c } => {
            if e.is_some() { element = e; }
            if i.is_some() { id = i; }
            classes.extend(c);
        }
    }
    SelectorPart::Compound { element, id, classes }
}
