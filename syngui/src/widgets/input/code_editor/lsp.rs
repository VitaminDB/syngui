//! Подсказки языкового сервера в `CodeEditor`: диагностики, автодополнение,
//! всплывающая справка, переход к определению.
//!
//! Позиции везде одинаковые, как у [`CursorInfo`](super::CursorInfo):
//! `line` — номер строки с нуля, `col` — номер символа (Unicode scalar, не
//! байта и не UTF-16) в строке с нуля. В UTF-16 LSP приложение переводит само.

/// Важность диагностики: порядок — от самой серьёзной.
#[derive(Clone, Debug, PartialEq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

impl DiagnosticSeverity {
    /// Меньше — серьёзнее (для «худшей» метки строки).
    pub(crate) fn rank(&self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Info => 2,
            Self::Hint => 3,
        }
    }
}

/// Диагностика на диапазоне `[line:col, end_line:end_col)`. Пустой диапазон
/// подчёркивает слово (или один символ) от начала.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

/// Вариант автодополнения. `kind` — короткая метка («fn», «var», «field»,
/// «kw»), может быть пустой; `insert_text` пустой — вставляется `label`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CompletionItem {
    pub label: String,
    pub detail: String,
    pub insert_text: String,
    pub kind: String,
}

/// Ответ на запрос дополнения: варианты заменяют текст строки `line` от
/// `start_col` до каретки.
#[derive(Clone, Debug, PartialEq)]
pub struct CompletionList {
    pub line: usize,
    pub start_col: usize,
    pub items: Vec<CompletionItem>,
}

/// Запрос дополнения: позиция каретки, символ-триггер (`.`, `:`) и ручной
/// вызов (Ctrl+Space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompletionTrigger {
    pub line: usize,
    pub col: usize,
    pub trigger_char: Option<char>,
    pub manual: bool,
}

/// Справка над символом `line:col` (ответ на `on_hover`).
#[derive(Clone, Debug, PartialEq)]
pub struct HoverInfo {
    pub line: usize,
    pub col: usize,
    pub text: String,
}

pub(crate) fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Варианты, подходящие под набранное `filter`: сначала начинающиеся с него
/// (без учёта регистра), потом содержащие, потом с его буквами по порядку.
/// Внутри группы — исходный порядок сервера.
pub fn filter_completion(items: &[CompletionItem], filter: &str) -> Vec<usize> {
    if filter.is_empty() {
        return (0..items.len()).collect();
    }
    let f = filter.to_lowercase();
    let mut tiers: [Vec<usize>; 3] = Default::default();
    for (i, it) in items.iter().enumerate() {
        let label = it.label.to_lowercase();
        if label.starts_with(&f) {
            tiers[0].push(i);
        } else if label.contains(&f) {
            tiers[1].push(i);
        } else if is_subsequence(&f, &label) {
            tiers[2].push(i);
        }
    }
    let [a, b, c] = tiers;
    a.into_iter().chain(b).chain(c).collect()
}

fn is_subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|n| it.any(|h| h == n))
}

/// Символы `[start, end)` строки `line`, которые подчёркивает диагностика.
/// `end` может быть на один больше длины строки — «за концом строки»
/// (ошибка на переводе строки или в пустой строке).
pub(crate) fn diag_cols_on_line(d: &Diagnostic, line: usize, line_text: &str) -> Option<(usize, usize)> {
    let (end_line, end_col) = if (d.end_line, d.end_col) < (d.line, d.col) {
        (d.line, d.col)
    } else {
        (d.end_line, d.end_col)
    };
    if line < d.line || line > end_line {
        return None;
    }
    let n = line_text.chars().count();
    let start = if line == d.line { d.col.min(n) } else { 0 };
    let end = if line == end_line { end_col.min(n) } else { n };
    if end > start {
        return Some((start, end));
    }
    if line != d.line {
        // многострочная диагностика, хвост — пустое начало строки
        return None;
    }
    // пустой диапазон — до конца слова или один символ
    let chars: Vec<char> = line_text.chars().collect();
    if start >= n {
        return Some((n, n + 1));
    }
    if is_ident_char(chars[start]) {
        let mut e = start;
        while e < n && is_ident_char(chars[e]) {
            e += 1;
        }
        Some((start, e))
    } else {
        Some((start, start + 1))
    }
}

/// Слово под символом `col` (в символах): идентификатор целиком, иначе сам
/// символ.
pub(crate) fn word_range(line_text: &str, col: usize) -> (usize, usize) {
    let chars: Vec<char> = line_text.chars().collect();
    if col >= chars.len() {
        return (col, col + 1);
    }
    if !is_ident_char(chars[col]) {
        return (col, col + 1);
    }
    let mut s = col;
    while s > 0 && is_ident_char(chars[s - 1]) {
        s -= 1;
    }
    let mut e = col;
    while e < chars.len() && is_ident_char(chars[e]) {
        e += 1;
    }
    (s, e)
}

/// Перенос текста подсказки: строки не длиннее `max_chars` символов (по
/// пробелам, длинное слово режется), не больше `max_lines` строк.
pub(crate) fn wrap_text(text: &str, max_chars: usize, max_lines: usize) -> Vec<String> {
    let max_chars = max_chars.max(8);
    let mut out = Vec::new();
    'outer: for raw in text.lines() {
        let raw = raw.trim_end();
        if raw.chars().count() <= max_chars {
            out.push(raw.to_string());
            if out.len() >= max_lines {
                break;
            }
            continue;
        }
        let mut cur = String::new();
        let mut cur_n = 0usize;
        for word in raw.split_inclusive(' ') {
            let wn = word.chars().count();
            if cur_n + wn > max_chars && cur_n > 0 {
                out.push(cur.trim_end().to_string());
                if out.len() >= max_lines {
                    break 'outer;
                }
                cur.clear();
                cur_n = 0;
            }
            let mut w = word;
            while w.chars().count() > max_chars {
                let cut = w.char_indices().nth(max_chars).map(|(b, _)| b).unwrap_or(w.len());
                out.push(w[..cut].to_string());
                if out.len() >= max_lines {
                    break 'outer;
                }
                w = &w[cut..];
            }
            cur.push_str(w);
            cur_n += w.chars().count();
        }
        if !cur.is_empty() {
            out.push(cur.trim_end().to_string());
            if out.len() >= max_lines {
                break;
            }
        }
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str) -> CompletionItem {
        CompletionItem { label: label.into(), ..Default::default() }
    }

    #[test]
    fn filter_prefix_then_contains_then_subsequence() {
        let items = vec![item("to_string"), item("String"), item("len"), item("strip"), item("as_str")];
        let got = filter_completion(&items, "str");
        // начало: String, strip; содержит: to_string, as_str; подпоследовательность — нет
        assert_eq!(got, vec![1, 3, 0, 4]);
        assert_eq!(filter_completion(&items, "ts"), vec![0]);
        assert_eq!(filter_completion(&items, ""), vec![0, 1, 2, 3, 4]);
        assert!(filter_completion(&items, "zzz").is_empty());
    }

    #[test]
    fn diag_range_zero_width_covers_word() {
        let d = Diagnostic { line: 0, col: 4, end_line: 0, end_col: 4, severity: DiagnosticSeverity::Error, message: String::new() };
        assert_eq!(diag_cols_on_line(&d, 0, "let foo = 1;"), Some((4, 7)));
        assert_eq!(diag_cols_on_line(&d, 0, "let"), Some((3, 4)));
        assert_eq!(diag_cols_on_line(&d, 1, "let foo"), None);
    }

    #[test]
    fn diag_range_multiline() {
        let d = Diagnostic { line: 1, col: 2, end_line: 3, end_col: 1, severity: DiagnosticSeverity::Warning, message: String::new() };
        assert_eq!(diag_cols_on_line(&d, 0, "abcd"), None);
        assert_eq!(diag_cols_on_line(&d, 1, "abcd"), Some((2, 4)));
        assert_eq!(diag_cols_on_line(&d, 2, "xyz"), Some((0, 3)));
        assert_eq!(diag_cols_on_line(&d, 3, "xyz"), Some((0, 1)));
    }

    #[test]
    fn word_range_and_wrap() {
        assert_eq!(word_range("foo.bar_baz()", 6), (4, 11));
        assert_eq!(word_range("foo.bar", 3), (3, 4));
        let w = wrap_text("aaa bbb ccc ddd", 8, 10);
        assert_eq!(w, vec!["aaa bbb", "ccc ddd"]);
        assert_eq!(wrap_text("a\nb\nc", 10, 2), vec!["a", "b"]);
    }
}
