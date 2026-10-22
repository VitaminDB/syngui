//! Анализ таблиц стилей MSS для редакторов: диагностики, дополнение,
//! подсказки, переход к определению, образцы цветов, форматирование.
//!
//! Работает поверх `mss-lang` (толерантный разбор и реестр свойств) и не
//! тянет графический стек syngui — годится для `CodeEditor` в редакторе тем
//! и для LSP-сервера `mss-lsp`.
//!
//! Позиции — [`Position`]: строка и колонка с нуля, колонка в символах
//! Unicode (как у `CodeEditor` syngui). В UTF-16 для LSP переводит сервер.

mod colors;
mod completion;
mod diagnostics;
mod format;
mod hover;
mod values;

use mss_lang::syntax::{self, LineIndex, Span};

pub use colors::{color_presentations, document_colors, ColorInfo};
pub use completion::{completion, Completion, CompletionItem, CompletionKind};
pub use diagnostics::{diagnostics, Diag, Severity};
pub use format::format;
pub use hover::{definition, hover, Hover};
pub use mss_lang::color::Color;

/// Позиция в тексте: строка и колонка с нуля, колонка — в символах Unicode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub const fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// Диапазон `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Переменная, известная извне документа (токены темы оболочки, переменные
/// других файлов).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VarInfo {
    /// Имя вместе с `--`.
    pub name: String,
    /// Значение как в MSS (`#1e1e2e`, `8px`); пустое — неизвестно.
    pub value: String,
    /// Описание для подсказки.
    pub doc: String,
}

/// Что известно о окружении документа.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Context {
    /// Переменные помимо объявленных в самом документе.
    pub variables: Vec<VarInfo>,
    /// Классы, которые ставит программа (для дополнения селекторов).
    pub known_classes: Vec<String>,
    /// Имена `@keyframes` из других таблиц стилей (иначе анимация с таким
    /// именем считается ненайденной).
    pub keyframes: Vec<String>,
}

/// Разобранный документ с индексом строк.
pub(crate) struct Doc<'a> {
    pub text: &'a str,
    pub sheet: syntax::Stylesheet,
    pub lines: LineIndex,
}

impl<'a> Doc<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            sheet: syntax::parse(text),
            lines: LineIndex::new(text),
        }
    }

    pub fn pos(&self, offset: usize) -> Position {
        let (line, col) = self.lines.position(self.text, offset);
        Position { line, col }
    }

    pub fn range(&self, span: &Span) -> Range {
        Range {
            start: self.pos(span.start),
            end: self.pos(span.end),
        }
    }

    pub fn offset(&self, pos: Position) -> usize {
        self.lines.offset(self.text, pos.line, pos.col)
    }

    /// Переменные документа: `--x` из `:root` и из любых правил (переменные
    /// наследуются от предков, как в CSS). Порядок — по тексту.
    pub fn variables(&self) -> Vec<&syntax::Declaration> {
        self.sheet
            .all_declarations()
            .into_iter()
            .filter(|d| d.is_variable())
            .collect()
    }

    /// Последнее объявление переменной `name` в документе: в `:root` — если
    /// есть, иначе в любом правиле.
    pub fn variable(&self, name: &str) -> Option<&syntax::Declaration> {
        let root = self.sheet.root_variables();
        if let Some(d) = root.into_iter().find(|d| d.name == name) {
            return Some(d);
        }
        self.variables().into_iter().rev().find(|d| d.name == name)
    }
}

/// Значение переменной `name`: из документа, иначе из контекста.
pub(crate) fn variable_value(doc: &Doc, ctx: &Context, name: &str) -> Option<String> {
    if let Some(d) = doc.variable(name) {
        return Some(d.value.clone());
    }
    ctx.variables
        .iter()
        .find(|v| v.name == name)
        .map(|v| v.value.clone())
}

/// Расстояние Левенштейна (для «может быть, …?»).
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Ближайшее по написанию имя из `candidates` (не дальше трети длины).
pub(crate) fn suggest<'c>(
    name: &str,
    candidates: impl Iterator<Item = &'c str>,
) -> Option<&'c str> {
    let limit = (name.chars().count() / 3).max(1);
    candidates
        .map(|c| (edit_distance(name, c), c))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions() {
        assert_eq!(edit_distance("colr", "color"), 1);
        let props = mss_lang::properties::PROPERTY_NAMES.iter().copied();
        assert_eq!(suggest("backgroud-color", props), Some("background-color"));
        assert_eq!(suggest("zzz", ["color"].into_iter()), None);
    }
}
