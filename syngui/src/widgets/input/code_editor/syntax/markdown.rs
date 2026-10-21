//! Построчная подсветка Markdown.
//!
//! Через строки переносится только блочное состояние: fenced-код (```` ``` ````
//! / `~~~`) и HTML-комментарий. Всё инлайновое (`*курсив*`, `**жирный**`,
//! `` `код` ``, ссылки, `~~зачёркнутый~~`) ищет пару только в своей строке:
//! одиночная `*` или `` ` `` не красит текст до конца файла.

use super::capture_names::TokenClass;
use super::highlight_cache::{LineSpans, Span};
use super::highlighter::LineHighlighter;

const NORMAL: u32 = 0;
const COMMENT: u32 = 1;
/// Внутри fenced-кода: `FENCE | (тильда << 7) | длина_ограды`.
const FENCE: u32 = 0x100;

const HEADING: TokenClass = TokenClass::Keyword;
const BOLD: TokenClass = TokenClass::Keyword;
const ITALIC: TokenClass = TokenClass::Type;
const CODE: TokenClass = TokenClass::String;
const LINK_TEXT: TokenClass = TokenClass::StringSpecial;
const LINK_URL: TokenClass = TokenClass::String;
const MARKER: TokenClass = TokenClass::Operator;
const CHECKBOX: TokenClass = TokenClass::Attribute;
const QUOTE: TokenClass = TokenClass::Comment;
const MUTED: TokenClass = TokenClass::Comment;

pub struct MarkdownHighlighter;

impl LineHighlighter for MarkdownHighlighter {
    fn highlight_line(&self, line: &str, state: u32, out: &mut LineSpans) -> u32 {
        let b = line.as_bytes();
        let len = b.len() as u32;

        if state & FENCE != 0 {
            out.push(Span::new(0, len, CODE));
            let tilde = state & 0x80 != 0;
            let need = (state & 0x7F) as usize;
            if let Some((t, n, rest)) = fence(line) {
                if t == tilde && n >= need && rest.trim().is_empty() {
                    return NORMAL;
                }
            }
            return state;
        }

        let mut i;
        if state == COMMENT {
            match line.find("-->") {
                Some(p) => {
                    i = p + 3;
                    out.push(Span::new(0, i as u32, MUTED));
                }
                None => {
                    out.push(Span::new(0, len, MUTED));
                    return COMMENT;
                }
            }
        } else {
            if let Some((tilde, n, _)) = fence(line) {
                out.push(Span::new(0, len, CODE));
                return FENCE | (u32::from(tilde) << 7) | n.min(0x7F) as u32;
            }
            let indent = b.iter().take_while(|c| **c == b' ' || **c == b'\t').count();
            let t = &b[indent..];
            if heading_level(t) > 0 {
                out.push(Span::new(indent as u32, len, HEADING));
                return NORMAL;
            }
            if is_rule(t) {
                out.push(Span::new(indent as u32, len, MARKER));
                return NORMAL;
            }
            if t.first() == Some(&b'>') {
                out.push(Span::new(indent as u32, len, QUOTE));
                return NORMAL;
            }
            i = indent + list_marker(t).map_or(0, |m| {
                out.push(Span::new(indent as u32, (indent + m) as u32, MARKER));
                m
            });
            if let Some(n) = checkbox(&b[i..]) {
                out.push(Span::new(i as u32, (i + n) as u32, CHECKBOX));
                i += n;
            }
        }
        inline(line, i, out)
    }
}

/// Открывающая/закрывающая ограда кода: (тильда?, длина, хвост после ограды).
fn fence(line: &str) -> Option<(bool, usize, &str)> {
    let b = line.as_bytes();
    let indent = b.iter().take_while(|c| **c == b' ').count();
    if indent > 3 {
        return None;
    }
    let ch = *b.get(indent)?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let n = b[indent..].iter().take_while(|c| **c == ch).count();
    if n < 3 {
        return None;
    }
    let rest = &line[indent + n..];
    // у ``` в info-строке не может быть обратных кавычек
    if ch == b'`' && rest.contains('`') {
        return None;
    }
    Some((ch == b'~', n, rest))
}

fn heading_level(t: &[u8]) -> usize {
    let n = t.iter().take_while(|c| **c == b'#').count();
    if (1..=6).contains(&n) && matches!(t.get(n), None | Some(b' ') | Some(b'\t')) {
        n
    } else {
        0
    }
}

/// `---`, `***`, `___` (с пробелами между), не меньше трёх знаков.
fn is_rule(t: &[u8]) -> bool {
    let Some(&ch) = t.first() else { return false };
    if !matches!(ch, b'-' | b'*' | b'_') {
        return false;
    }
    let mut n = 0;
    for &c in t {
        if c == ch {
            n += 1;
        } else if c != b' ' && c != b'\t' {
            return false;
        }
    }
    n >= 3
}

/// Длина маркера списка вместе с пробелом: `- `, `* `, `+ `, `1. `, `2) `.
fn list_marker(t: &[u8]) -> Option<usize> {
    let ws = |i: usize| matches!(t.get(i), Some(b' ') | Some(b'\t'));
    match t.first()? {
        b'-' | b'*' | b'+' if ws(1) => Some(2),
        b'0'..=b'9' => {
            let d = t.iter().take_while(|c| c.is_ascii_digit()).count();
            (d <= 9 && matches!(t.get(d), Some(b'.') | Some(b')')) && ws(d + 1)).then_some(d + 2)
        }
        _ => None,
    }
}

/// Флажок задачи `[ ]`, `[x]`, `[~]` в начале пункта списка.
fn checkbox(t: &[u8]) -> Option<usize> {
    (t.len() >= 3 && t[0] == b'[' && t[2] == b']' && !matches!(t[1], b']' | b'[')).then_some(3)
}

fn inline(line: &str, mut i: usize, out: &mut LineSpans) -> u32 {
    let b = line.as_bytes();
    while i < b.len() {
        let c = b[i];
        match c {
            b'\\' => {
                i += 1 + line[i + 1..].chars().next().map_or(0, char::len_utf8);
                continue;
            }
            b'`' => {
                let n = run(b, i, b'`');
                if let Some(end) = find_code_close(b, i + n, n) {
                    out.push(Span::new(i as u32, (end + n) as u32, CODE));
                    i = end + n;
                } else {
                    i += n;
                }
                continue;
            }
            b'<' if line[i..].starts_with("<!--") => match line[i + 4..].find("-->") {
                Some(p) => {
                    let end = i + 4 + p + 3;
                    out.push(Span::new(i as u32, end as u32, MUTED));
                    i = end;
                    continue;
                }
                None => {
                    out.push(Span::new(i as u32, b.len() as u32, MUTED));
                    return COMMENT;
                }
            },
            b'<' => {
                if let Some(n) = autolink_angle(&line[i..]) {
                    out.push(Span::new(i as u32, (i + n) as u32, LINK_URL));
                    i += n;
                    continue;
                }
            }
            b'*' | b'_' | b'~' => {
                if let Some((end, class)) = emphasis(line, i) {
                    out.push(Span::new(i as u32, end as u32, class));
                    i = end;
                } else {
                    i += run(b, i, c);
                }
                continue;
            }
            b'!' | b'[' => {
                let open = if c == b'!' { i + 1 } else { i };
                if b.get(open) == Some(&b'[') {
                    if let Some((text_end, end)) = link(b, open) {
                        out.push(Span::new(i as u32, text_end as u32, LINK_TEXT));
                        out.push(Span::new(text_end as u32, end as u32, LINK_URL));
                        i = end;
                        continue;
                    }
                }
            }
            b'h' | b'w' => {
                if let Some(n) = bare_url(line, i) {
                    out.push(Span::new(i as u32, (i + n) as u32, LINK_URL));
                    i += n;
                    continue;
                }
            }
            _ => {}
        }
        i += line[i..].chars().next().map_or(1, char::len_utf8);
    }
    NORMAL
}

fn run(b: &[u8], i: usize, ch: u8) -> usize {
    b[i..].iter().take_while(|c| **c == ch).count()
}

/// Закрывающая серия ровно из `n` обратных кавычек.
fn find_code_close(b: &[u8], mut i: usize, n: usize) -> Option<usize> {
    while i < b.len() {
        if b[i] == b'`' {
            let m = run(b, i, b'`');
            if m == n {
                return Some(i);
            }
            i += m;
        } else {
            i += 1;
        }
    }
    None
}

fn prev_char(line: &str, i: usize) -> Option<char> {
    line[..i].chars().next_back()
}

fn next_char(line: &str, i: usize) -> Option<char> {
    line.get(i..).and_then(|s| s.chars().next())
}

/// `*x*`, `**x**`, `_x_`, `__x__`, `~~x~~` в пределах строки → (конец, класс).
fn emphasis(line: &str, i: usize) -> Option<(usize, TokenClass)> {
    let b = line.as_bytes();
    let ch = b[i];
    let n = run(b, i, ch).min(3);
    if ch == b'~' && n != 2 {
        return None;
    }
    // открывающий маркер — перед непробельным символом
    if next_char(line, i + n).is_none_or(char::is_whitespace) {
        return None;
    }
    // `_` внутри слова (snake_case) — не выделение
    if ch == b'_' && prev_char(line, i).is_some_and(char::is_alphanumeric) {
        return None;
    }
    let mut j = i + n;
    while j < b.len() {
        match b[j] {
            b'\\' => {
                j += 1 + next_char(line, j + 1).map_or(0, char::len_utf8);
                continue;
            }
            b'`' => {
                // код внутри выделения: маркеры в нём не считаются
                let m = run(b, j, b'`');
                j = find_code_close(b, j + m, m).map_or(j + m, |e| e + m);
                continue;
            }
            c if c == ch => {
                let m = run(b, j, ch);
                let closes = m >= n
                    && !prev_char(line, j).is_some_and(char::is_whitespace)
                    && !(ch == b'_' && next_char(line, j + m).is_some_and(char::is_alphanumeric));
                if closes {
                    let class = match (ch, n) {
                        (b'~', _) => MUTED,
                        (_, 1) => ITALIC,
                        _ => BOLD,
                    };
                    return Some((j + n, class));
                }
                j += m;
                continue;
            }
            _ => {}
        }
        j += next_char(line, j).map_or(1, char::len_utf8);
    }
    None
}

/// `[текст](url)` или `[текст][ref]`: (конец текста, конец ссылки).
fn link(b: &[u8], open: usize) -> Option<(usize, usize)> {
    let mut depth = 0i32;
    let mut j = open;
    let close = loop {
        match *b.get(j)? {
            b'\\' => j += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break j;
                }
            }
            _ => {}
        }
        j += 1;
    };
    let (o, c) = match b.get(close + 1)? {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        _ => return None,
    };
    let mut depth = 0i32;
    let mut k = close + 1;
    loop {
        let x = *b.get(k)?;
        if x == b'\\' {
            k += 2;
            continue;
        }
        if x == o {
            depth += 1;
        } else if x == c {
            depth -= 1;
            if depth == 0 {
                return Some((close + 1, k + 1));
            }
        }
        k += 1;
    }
}

/// `<https://…>`
fn autolink_angle(s: &str) -> Option<usize> {
    let end = s.find('>')?;
    let inner = &s[1..end];
    (inner.contains("://") && !inner.contains(char::is_whitespace)).then_some(end + 1)
}

/// `https://…`, `http://…`, `www.…` до пробела; хвостовая пунктуация не входит.
fn bare_url(line: &str, i: usize) -> Option<usize> {
    let s = &line[i..];
    if !(s.starts_with("https://") || s.starts_with("http://") || s.starts_with("www.")) {
        return None;
    }
    if prev_char(line, i).is_some_and(char::is_alphanumeric) {
        return None;
    }
    let mut n = s.find(|c: char| c.is_whitespace() || c == '<').unwrap_or(s.len());
    while n > 0 && matches!(s.as_bytes()[n - 1], b'.' | b',' | b';' | b':' | b')' | b']' | b'!' | b'?' | b'\'' | b'"') {
        n -= 1;
    }
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(text: &str) -> Vec<Vec<(String, TokenClass)>> {
        let mut state = NORMAL;
        text.split('\n')
            .map(|line| {
                let mut ls = LineSpans::new();
                state = MarkdownHighlighter.highlight_line(line, state, &mut ls);
                ls.iter()
                    .map(|s| (line[s.byte_start as usize..s.byte_end as usize].to_string(), s.class))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn lone_star_does_not_leak() {
        let r = spans("операнды [base+index*scale+disp]\nследующая *строка*");
        assert!(r[0].iter().all(|(_, c)| *c != ITALIC));
        assert_eq!(r[1], vec![("*строка*".to_string(), ITALIC)]);
    }

    #[test]
    fn lone_backtick_does_not_leak() {
        let r = spans("a ` b\nc `код` d");
        assert!(r[0].is_empty());
        assert_eq!(r[1], vec![("`код`".to_string(), CODE)]);
    }

    #[test]
    fn task_list_and_inline() {
        let r = spans("- [x] **Жирный** и `код`, [ссылка](http://x.y)");
        let parts: Vec<&str> = r[0].iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(parts, ["- ", "[x]", "**Жирный**", "`код`", "[ссылка]", "(http://x.y)"]);
    }

    #[test]
    fn fence_spans_lines() {
        let r = spans("```rust\nlet a = *b;\n```\n*курсив*");
        assert_eq!(r[1][0].1, CODE);
        assert_eq!(r[2][0].1, CODE);
        assert_eq!(r[3], vec![("*курсив*".to_string(), ITALIC)]);
    }

    #[test]
    fn snake_case_is_not_italic() {
        let r = spans("find_oep и static_rhai");
        assert!(r[0].is_empty());
    }

    #[test]
    fn heading_and_comment() {
        let r = spans("## Заголовок\n<!-- a\nb --> *c*");
        assert_eq!(r[0][0].1, HEADING);
        assert_eq!(r[1][0].1, MUTED);
        assert_eq!(r[2], vec![("b -->".to_string(), MUTED), ("*c*".to_string(), ITALIC)]);
    }

    #[test]
    fn bare_url_trims_punctuation() {
        let r = spans("см. https://example.com/a.");
        assert_eq!(r[0], vec![("https://example.com/a".to_string(), LINK_URL)]);
    }
}
