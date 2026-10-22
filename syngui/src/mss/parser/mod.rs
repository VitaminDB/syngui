pub mod gradient;
mod rule;
pub(crate) use rule::expand_shorthand;
mod selector;
pub mod transform;
mod utils;
mod value;

#[cfg(test)]
mod tests;

use std::fmt;
use std::ops::Range;

use super::stylesheet::*;
pub use utils::line_col;
use utils::ParserCursor;

/// Что не так в таблице стилей.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    /// Ожидался `expected`, а встречен `found` (`None` — конец текста).
    Expected {
        expected: String,
        found: Option<char>,
    },
    /// Нет закрывающей `}` у блока `0` (селектор, `:root`, `@keyframes …`).
    UnclosedBlock(String),
    /// Нет имени (свойства, `@keyframes`…) там, где оно нужно.
    ExpectedIdentifier { found: Option<char> },
    /// В `:root` и `var()` имя переменной должно начинаться с `--`.
    ExpectedVariable,
    /// Пустое значение свойства (`color: ;`).
    EmptyValue,
    /// Значение не подходит: `value` — что написано, `reason` — почему.
    InvalidValue { value: String, reason: String },
    /// Пустой селектор (`{` в начале правила).
    EmptySelector,
    /// Селектор не разбирается.
    InvalidSelector(String),
    /// Неизвестный каскаду псевдокласс — правило пропущено (замечание).
    UnknownPseudoClass { pseudo: String, selector: String },
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseErrorKind::Expected { expected, found } => {
                write!(f, "ожидалось {expected}, а встречено {}", describe_char(*found))
            }
            ParseErrorKind::UnclosedBlock(what) => {
                write!(f, "блок «{what}» не закрыт: нет «}}»")
            }
            ParseErrorKind::ExpectedIdentifier { found } => {
                write!(f, "ожидалось имя, а встречено {}", describe_char(*found))
            }
            ParseErrorKind::ExpectedVariable => {
                write!(f, "ожидалось имя переменной вида --имя")
            }
            ParseErrorKind::EmptyValue => write!(f, "пустое значение свойства"),
            ParseErrorKind::InvalidValue { value, reason } => {
                write!(f, "неверное значение «{value}»: {reason}")
            }
            ParseErrorKind::EmptySelector => write!(f, "пустой селектор"),
            ParseErrorKind::InvalidSelector(s) => write!(f, "неверный селектор «{s}»"),
            ParseErrorKind::UnknownPseudoClass { pseudo, selector } => write!(
                f,
                "неизвестный псевдокласс :{pseudo} в «{selector}» — правило пропущено"
            ),
        }
    }
}

fn describe_char(c: Option<char>) -> String {
    match c {
        None => "конец текста".to_string(),
        Some('\n') | Some('\r') => "перевод строки".to_string(),
        Some(c) => format!("«{c}»"),
    }
}

/// Ошибка разбора с местом в тексте.
///
/// `line` и `column` — с нуля, колонка — в символах Unicode (как у
/// `CodeEditor`). В человеческом выводе ([`fmt::Display`]) — с единицы.
/// `span` — диапазон байт в исходном тексте.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Range<usize>,
    pub line: usize,
    pub column: usize,
}

impl ParseError {
    fn new(kind: ParseErrorKind, span: Range<usize>) -> Self {
        Self {
            kind,
            span,
            line: 0,
            column: 0,
        }
    }

    /// Строка и колонка по `span.start` в `input`.
    fn locate(mut self, input: &str) -> Self {
        let (line, column) = line_col(input, self.span.start);
        self.line = line;
        self.column = column;
        self
    }

    /// Человеческое сообщение без позиции.
    pub fn message(&self) -> String {
        self.kind.to_string()
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "строка {}, столбец {}: {}", self.line + 1, self.column + 1, self.kind)
    }
}

impl std::error::Error for ParseError {}

/// Ошибка, после которой разбор продолжился со следующего правила, или
/// замечание о пропущенном правиле. Позиции — как у [`ParseError`].
#[derive(Debug, Clone, PartialEq)]
pub struct ParseWarning {
    /// Человеческое сообщение по-русски, без позиции.
    pub message: String,
    /// Строка с нуля.
    pub line: usize,
    /// Колонка с нуля, в символах Unicode.
    pub column: usize,
    /// Диапазон байт в исходном тексте.
    pub span: Range<usize>,
    pub kind: ParseErrorKind,
}

impl From<ParseError> for ParseWarning {
    fn from(e: ParseError) -> Self {
        Self {
            message: e.kind.to_string(),
            line: e.line,
            column: e.column,
            span: e.span,
            kind: e.kind,
        }
    }
}

impl fmt::Display for ParseWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "строка {}, столбец {}: {}", self.line + 1, self.column + 1, self.message)
    }
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

    /// Разбор с восстановлением: правило с ошибкой пропускается, ошибка
    /// уходит в предупреждения (по порядку в тексте). `Err` не бывает —
    /// сигнатура оставлена ради совместимости.
    pub fn parse(&mut self) -> Result<(StyleSheet, Vec<ParseWarning>), ParseError> {
        let mut stylesheet = StyleSheet::new();
        let mut errors = Vec::new();

        self.cursor.skip_whitespace();

        while !self.cursor.is_eof() {
            self.cursor.skip_whitespace();
            if self.cursor.is_eof() {
                break;
            }

            if self.cursor.peek() == Some('/') && self.cursor.peek_next() == Some('*') {
                self.cursor.skip_comment();
                continue;
            }

            let result = if self.cursor.starts_with(":root") {
                rule::parse_root_variables(&mut self.cursor, &mut stylesheet)
            } else if self.cursor.starts_with("@keyframes") {
                rule::parse_keyframes(&mut self.cursor, &mut stylesheet)
            } else {
                rule::parse_rule(&mut self.cursor, &mut stylesheet, None)
            };
            if let Err(e) = result {
                errors.push(e);
                skip_to_next_rule(&mut self.cursor);
            }
        }

        errors.append(&mut self.cursor.soft);
        errors.sort_by_key(|e| e.span.start);
        let input = self.cursor.input;
        let warnings = errors
            .into_iter()
            .map(|e| ParseWarning::from(e.locate(input)))
            .collect();
        Ok((stylesheet, warnings))
    }

    /// Разбор до первой ошибки. Замечания (неизвестный псевдокласс) уходят
    /// в журнал.
    pub fn parse_strict(&mut self) -> Result<StyleSheet, ParseError> {
        let input = self.cursor.input;
        let result = self.parse_strict_inner().map_err(|e| e.locate(input));
        for w in std::mem::take(&mut self.cursor.soft) {
            log::warn!("MSS: {}", w.locate(input));
        }
        result
    }

    fn parse_strict_inner(&mut self) -> Result<StyleSheet, ParseError> {
        let mut stylesheet = StyleSheet::new();

        self.cursor.skip_whitespace();

        while !self.cursor.is_eof() {
            self.cursor.skip_whitespace();
            if self.cursor.is_eof() {
                break;
            }

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
            Some('{') => {
                depth += 1;
                cursor.advance();
            }
            Some('}') => {
                cursor.advance();
                if depth <= 1 {
                    return;
                }
                depth -= 1;
            }
            _ => {
                cursor.advance();
            }
        }
    }
}
