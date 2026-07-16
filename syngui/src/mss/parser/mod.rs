mod utils;
mod rule;
mod selector;
mod value;
pub mod gradient;
pub mod transform;

#[cfg(test)]
mod tests;

use super::stylesheet::*;
use utils::ParserCursor;

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    UnexpectedToken(String, usize),
    UnclosedBlock(String, usize),
    InvalidProperty(String, usize),
    InvalidValue(String, String, usize),
    MissingSemicolon(usize),
    EmptySelector(usize),
    InvalidSelector(String, usize),
}

#[derive(Debug, Clone)]
pub struct ParseWarning {
    pub message: String,
    pub line: usize,
}

pub struct MssParser<'a> {
    cursor: ParserCursor<'a>,
}

impl<'a> MssParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            cursor: ParserCursor::new(input),
        }
    }

    pub fn parse(&mut self) -> Result<(StyleSheet, Vec<ParseWarning>), ParseError> {
        let mut stylesheet = StyleSheet::new();
        let mut warnings = Vec::new();

        self.cursor.skip_whitespace();

        while !self.cursor.is_eof() {
            self.cursor.skip_whitespace();
            if self.cursor.is_eof() { break; }

            if self.cursor.peek() == Some('/') && self.cursor.peek_next() == Some('*') {
                self.cursor.skip_comment();
                continue;
            }

            if self.cursor.starts_with(":root") {
                if let Err(e) = rule::parse_root_variables(&mut self.cursor, &mut stylesheet) {
                    warnings.push(ParseWarning { message: format!("{:?}", e), line: self.cursor.line });
                    skip_to_next_rule(&mut self.cursor);
                }
                continue;
            }

            if self.cursor.starts_with("@keyframes") {
                if let Err(e) = rule::parse_keyframes(&mut self.cursor, &mut stylesheet) {
                    warnings.push(ParseWarning { message: format!("{:?}", e), line: self.cursor.line });
                    skip_to_next_rule(&mut self.cursor);
                }
                continue;
            }

            if let Err(e) = rule::parse_rule(&mut self.cursor, &mut stylesheet, None) {
                warnings.push(ParseWarning { message: format!("{:?}", e), line: self.cursor.line });
                skip_to_next_rule(&mut self.cursor);
            }
        }

        Ok((stylesheet, warnings))
    }

    pub fn parse_strict(&mut self) -> Result<StyleSheet, ParseError> {
        let mut stylesheet = StyleSheet::new();

        self.cursor.skip_whitespace();

        while !self.cursor.is_eof() {
            self.cursor.skip_whitespace();
            if self.cursor.is_eof() { break; }

            if self.cursor.peek() == Some('/') && self.cursor.peek_next() == Some('*') {
                self.cursor.skip_comment();
                continue;
            }

            if self.cursor.starts_with(":root") {
                rule::parse_root_variables(&mut self.cursor, &mut stylesheet)?;
                continue;
            }

            if self.cursor.starts_with("@keyframes") {
                rule::parse_keyframes(&mut self.cursor, &mut stylesheet)?;
                continue;
            }

            rule::parse_rule(&mut self.cursor, &mut stylesheet, None)?;
        }

        Ok(stylesheet)
    }
}

fn skip_to_next_rule(cursor: &mut ParserCursor) {
    let mut depth = 0i32;
    while !cursor.is_eof() {
        match cursor.peek() {
            Some('{') => { depth += 1; cursor.advance(); }
            Some('}') => {
                cursor.advance();
                if depth <= 1 { return; }
                depth -= 1;
            }
            Some('\n') => { cursor.line += 1; cursor.advance(); }
            _ => { cursor.advance(); }
        }
    }
}
