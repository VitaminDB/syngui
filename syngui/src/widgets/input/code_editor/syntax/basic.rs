//! Построчная подсветка Visual Basic (VB5/6, VBA, VB.NET; `.bas`, `.cls`, `.frm`, `.ctl`, `.vb`).
//!
//! Регистр не важен. Комментарии `'` и `Rem`, строки `"…"` (`""` внутри), даты `#1/2/2000#`, числа
//! (`&HFF`, `&O17`, суффиксы `&`, `%`, `!`, `#`, `@`), ключевые слова, слова управления, встроенные типы,
//! атрибуты `Attribute VB_Name = …` и заголовок формы (`VERSION 5.00`, `Begin VB.Form`).

use super::capture_names::TokenClass;
use super::highlight_cache::{LineSpans, Span};
use super::highlighter::LineHighlighter;

pub struct BasicHighlighter;

const KEYWORDS: &[&str] = &[
    "as", "attribute", "byref", "byval", "call", "class", "const", "declare", "dim", "enum", "event", "explicit", "friend", "function", "get", "global", "implements", "let", "lib", "alias", "me", "module",
    "new", "nothing", "option", "optional", "paramarray", "preserve", "private", "property", "public", "raiseevent", "redim", "set", "shared", "static", "structure", "sub", "type", "withevents", "and",
    "andalso", "or", "orelse", "not", "xor", "mod", "is", "isnot", "like", "true", "false", "empty", "null", "imports", "namespace", "inherits", "overrides", "overridable", "mustoverride", "base",
    "compare", "binary", "version", "object",
];

const CONTROL: &[&str] = &[
    "if", "then", "else", "elseif", "end", "select", "case", "for", "each", "in", "to", "step", "next", "do", "loop", "while", "wend", "until", "exit", "goto", "gosub", "return", "on", "error", "resume",
    "with", "begin", "beginproperty", "endproperty", "stop", "try", "catch", "finally", "throw",
];

const BUILTIN_TYPES: &[&str] = &[
    "integer", "long", "single", "double", "currency", "string", "boolean", "byte", "date", "variant", "object", "decimal", "short", "char", "longlong", "longptr",
];

fn ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

impl LineHighlighter for BasicHighlighter {
    fn highlight_line(&self, line: &str, _state: u32, out: &mut LineSpans) -> u32 {
        let b = line.as_bytes();
        let n = b.len();
        let mut i = 0usize;
        let mut first_word = true;
        while i < n {
            let c = b[i];
            if c == b' ' || c == b'\t' {
                i += 1;
                continue;
            }
            if c == b'\'' {
                out.push(Span::new(i as u32, n as u32, TokenClass::Comment));
                break;
            }
            if c == b'"' {
                let mut j = i + 1;
                while j < n {
                    if b[j] == b'"' {
                        if b.get(j + 1) == Some(&b'"') {
                            j += 2;
                            continue;
                        }
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                out.push(Span::new(i as u32, j.min(n) as u32, TokenClass::String));
                i = j;
                first_word = false;
                continue;
            }
            // дата `#1/2/2000#`
            if c == b'#' && b.get(i + 1).is_some_and(|x| x.is_ascii_digit()) {
                if let Some(e) = line[i + 1..].find('#') {
                    out.push(Span::new(i as u32, (i + e + 2) as u32, TokenClass::Constant));
                    i += e + 2;
                    continue;
                }
            }
            if c == b'&' && b.get(i + 1).is_some_and(|x| matches!(x.to_ascii_uppercase(), b'H' | b'O')) {
                let mut j = i + 2;
                while j < n && b[j].is_ascii_hexdigit() {
                    j += 1;
                }
                if j < n && b[j] == b'&' {
                    j += 1;
                }
                out.push(Span::new(i as u32, j as u32, TokenClass::Number));
                i = j;
                first_word = false;
                continue;
            }
            if c.is_ascii_digit() {
                let mut j = i;
                while j < n && (b[j].is_ascii_digit() || b[j] == b'.') {
                    j += 1;
                }
                if j < n && matches!(b[j], b'&' | b'%' | b'!' | b'#' | b'@') {
                    j += 1;
                }
                out.push(Span::new(i as u32, j as u32, TokenClass::Number));
                i = j;
                first_word = false;
                continue;
            }
            if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 {
                let mut j = i;
                while j < n && ident_char(b[j]) {
                    j += 1;
                }
                // суффикс типа `Name$`, `Count%`
                if j < n && matches!(b[j], b'$' | b'%' | b'&' | b'!' | b'#' | b'@') {
                    j += 1;
                }
                let w = &line[i..j];
                let l = w.trim_end_matches(['$', '%', '&', '!', '#', '@']).to_ascii_lowercase();
                // `Rem` в начале оператора — комментарий до конца строки
                if first_word && l == "rem" {
                    out.push(Span::new(i as u32, n as u32, TokenClass::Comment));
                    break;
                }
                let class = if CONTROL.contains(&l.as_str()) {
                    Some(TokenClass::KeywordControl)
                } else if KEYWORDS.contains(&l.as_str()) {
                    Some(TokenClass::Keyword)
                } else if BUILTIN_TYPES.contains(&l.as_str()) {
                    Some(TokenClass::TypeBuiltin)
                } else {
                    let mut k = j;
                    while k < n && b[k] == b' ' {
                        k += 1;
                    }
                    (k < n && b[k] == b'(').then_some(TokenClass::Function)
                };
                if let Some(cl) = class {
                    out.push(Span::new(i as u32, j as u32, cl));
                }
                // после `:` (несколько операторов в строке) снова начало оператора
                first_word = false;
                i = j;
                continue;
            }
            if c == b':' {
                first_word = true;
            }
            if matches!(c, b'=' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'\\' | b'^' | b'&') {
                out.push(Span::new(i as u32, (i + 1) as u32, TokenClass::Operator));
            } else if matches!(c, b',' | b'.' | b'(' | b')' | b':') {
                out.push(Span::new(i as u32, (i + 1) as u32, TokenClass::Punctuation));
            }
            i += 1;
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_tokens() {
        let line = "Private Sub cmdOK_Click() ' обработчик";
        let mut ls = LineSpans::new();
        BasicHighlighter.highlight_line(line, 0, &mut ls);
        let v: Vec<(String, TokenClass)> = ls.spans.iter().map(|s| (line[s.byte_start as usize..s.byte_end as usize].to_string(), s.class)).collect();
        assert!(v.contains(&("Private".into(), TokenClass::Keyword)));
        assert!(v.contains(&("Sub".into(), TokenClass::Keyword)));
        assert!(v.contains(&("cmdOK_Click".into(), TokenClass::Function)));
        assert!(v.iter().any(|(t, c)| t.starts_with('\'') && *c == TokenClass::Comment));
        let line = "  s$ = \"a\"\"b\" & Chr$(13) : If x > &HFF Then Exit Sub";
        let mut ls = LineSpans::new();
        BasicHighlighter.highlight_line(line, 0, &mut ls);
        let v: Vec<(String, TokenClass)> = ls.spans.iter().map(|s| (line[s.byte_start as usize..s.byte_end as usize].to_string(), s.class)).collect();
        assert!(v.contains(&("\"a\"\"b\"".into(), TokenClass::String)));
        assert!(v.contains(&("&HFF".into(), TokenClass::Number)));
        assert!(v.contains(&("If".into(), TokenClass::KeywordControl)));
    }
}
