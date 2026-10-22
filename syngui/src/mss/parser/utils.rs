use super::{ParseError, ParseErrorKind};

pub(super) struct ParserCursor<'a> {
    pub input: &'a str,
    pub position: usize,
    /// Смещение `input` в исходном тексте: у вложенного курсора (значение
    /// по умолчанию в `var()`) диапазоны ошибок — в байтах всего файла.
    pub base: usize,
    /// Нефатальные замечания (неизвестный псевдокласс…): правило разобрано,
    /// но что-то в нём пропущено. Позиции — абсолютные байты, строку и
    /// колонку проставляет [`super::MssParser`].
    pub soft: Vec<ParseError>,
}

impl<'a> ParserCursor<'a> {
    pub fn new(input: &'a str) -> Self {
        Self::with_base(input, 0)
    }

    pub fn with_base(input: &'a str, base: usize) -> Self {
        Self {
            input,
            position: 0,
            base,
            soft: Vec::new(),
        }
    }

    /// Ошибка на текущем символе (на конце файла — пустой диапазон).
    pub fn error(&self, kind: ParseErrorKind) -> ParseError {
        let end = self.position + self.peek().map_or(0, char::len_utf8);
        self.error_span(kind, self.position, end)
    }

    /// Ошибка на диапазоне `[start, end)` байт этого курсора.
    pub fn error_span(&self, kind: ParseErrorKind, start: usize, end: usize) -> ParseError {
        ParseError::new(kind, self.base + start..self.base + end.max(start))
    }

    /// «Ожидалось `expected`» с найденным символом.
    pub fn expected(&self, expected: &str) -> ParseError {
        self.error(ParseErrorKind::Expected {
            expected: expected.to_string(),
            found: self.peek(),
        })
    }

    pub fn skip_whitespace(&mut self) {
        while let Some(' ' | '\t' | '\n' | '\r') = self.peek() {
            self.advance();
        }
    }

    pub fn skip_comment(&mut self) {
        if self.peek() == Some('/') && self.peek_next() == Some('*') {
            self.advance();
            self.advance();

            while !self.is_eof() {
                if self.peek() == Some('*') && self.peek_next() == Some('/') {
                    self.advance();
                    self.advance();
                    break;
                }
                self.advance();
            }
        }
    }

    pub fn starts_with(&self, s: &str) -> bool {
        self.input[self.position..].starts_with(s)
    }

    pub fn peek(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }

    pub fn peek_next(&self) -> Option<char> {
        let mut chars = self.input[self.position..].chars();
        chars.next();
        chars.next()
    }

    pub fn advance(&mut self) {
        if let Some(c) = self.input[self.position..].chars().next() {
            self.position += c.len_utf8();
        }
    }

    pub fn is_eof(&self) -> bool {
        self.position >= self.input.len()
    }

    pub fn consume(&mut self, s: &str) {
        for c in s.chars() {
            if self.peek() == Some(c) {
                self.advance();
            }
        }
    }

    pub fn split_by_comma<'b>(&self, s: &'b str) -> Vec<&'b str> {
        let mut parts = Vec::new();
        let mut depth = 0;
        let mut start = 0;

        for (i, c) in s.char_indices() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        parts.push(&s[start..]);
        parts
    }
}

/// Строка и колонка (обе с нуля, колонка — в символах Unicode) байтового
/// смещения `offset` в `text`. Смещение за концом текста — конец текста,
/// внутри символа — его начало.
pub fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.bytes().filter(|&b| b == b'\n').count();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[line_start..].chars().count())
}
