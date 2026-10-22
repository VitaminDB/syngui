//! Подсказка под курсором и переход к определению.

use mss_lang::color::Color;
use mss_lang::properties::{self, ValueKind};
use mss_lang::syntax::{Declaration, SelectorPartKind, Token, TokenKind};

use crate::values::parse_color;
use crate::{variable_value, Context, Doc, Position, Range};

/// Подсказка: диапазон слова, текст (Markdown) и цвет, если слово —
/// цвет или переменная с цветом.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    pub range: Range,
    pub markdown: String,
    pub color: Option<Color>,
}

fn word_like(t: &Token) -> bool {
    matches!(
        t.kind,
        TokenKind::Ident
            | TokenKind::Hash
            | TokenKind::AtKeyword
            | TokenKind::Dimension
            | TokenKind::Number
    )
}

/// Токен под курсором: содержащий смещение, а в конце слова — само слово.
pub(crate) fn token_index_at(doc: &Doc, offset: usize) -> Option<usize> {
    let toks = &doc.sheet.tokens;
    let i = doc.sheet.token_at(offset)?;
    if word_like(&toks[i]) && toks[i].span.start <= offset {
        return Some(i);
    }
    if i > 0 && toks[i - 1].span.end == offset && word_like(&toks[i - 1]) {
        return Some(i - 1);
    }
    (toks[i].span.start <= offset && offset < toks[i].span.end).then_some(i)
}

/// Объявление, в значении которого лежит смещение.
fn declaration_with_value_at<'d>(doc: &'d Doc, offset: usize) -> Option<&'d Declaration> {
    doc.sheet
        .all_declarations()
        .into_iter()
        .find(|d| d.colon.is_some() && d.value_span.start <= offset && offset <= d.value_span.end)
}

/// Значение переменной с раскрытием цепочки `var(--a)` → `var(--b)` → …
fn resolve_var(doc: &Doc, ctx: &Context, name: &str) -> Option<String> {
    let mut v = variable_value(doc, ctx, name)?;
    for _ in 0..8 {
        let t = v.trim();
        let Some(inner) = t.strip_prefix("var(").and_then(|r| r.strip_suffix(')')) else {
            break;
        };
        let next = inner.split(',').next().unwrap_or("").trim();
        match variable_value(doc, ctx, next) {
            Some(n) => v = n,
            None => break,
        }
    }
    Some(v)
}

fn color_line(c: Color) -> String {
    format!(
        "Цвет `{}` — rgba({}, {}, {}, {})",
        c.to_hex(),
        c.r,
        c.g,
        c.b,
        (c.a as f32 / 255.0 * 100.0).round() / 100.0
    )
}

/// Подсказка под позицией `pos`.
pub fn hover(text: &str, pos: Position, ctx: &Context) -> Option<Hover> {
    let doc = Doc::new(text);
    let offset = doc.offset(pos);
    let i = token_index_at(&doc, offset)?;
    let toks = &doc.sheet.tokens;
    let t = &toks[i];
    let word = &text[t.span.clone()];
    let range = doc.range(&t.span);
    let next_is_paren = toks
        .get(i + 1)
        .is_some_and(|n| n.kind == TokenKind::LParen && n.span.start == t.span.end);

    // Переменная.
    if t.kind == TokenKind::Ident && word.starts_with("--") {
        let mut md = format!("**{word}**");
        let value = resolve_var(&doc, ctx, word);
        let raw = variable_value(&doc, ctx, word);
        match &raw {
            Some(v) => md += &format!(": `{}`", v.trim()),
            None => md += " — не объявлена",
        }
        if value != raw {
            if let Some(v) = &value {
                md += &format!(" → `{}`", v.trim());
            }
        }
        if let Some(d) = doc.variable(word) {
            md += &format!(
                "\n\nОбъявлена в строке {}",
                doc.pos(d.name_span.start).line + 1
            );
        } else if let Some(v) = ctx.variables.iter().find(|v| v.name == word) {
            if !v.doc.is_empty() {
                md += &format!("\n\n{}", v.doc);
            }
        }
        let color = value.as_deref().and_then(parse_color);
        if let Some(c) = color {
            md += &format!("\n\n{}", color_line(c));
        }
        return Some(Hover {
            range,
            markdown: md,
            color,
        });
    }

    // Имя свойства в объявлении.
    if t.kind == TokenKind::Ident {
        if let Some(d) = doc
            .sheet
            .all_declarations()
            .into_iter()
            .find(|d| d.name_span == t.span)
        {
            let p = properties::property(&d.name)?;
            let mut md = format!("**{}** — {}\n\n{}", p.name, p.kind.label(), p.doc);
            if let ValueKind::Keyword(list) = p.kind {
                md += &format!("\n\nЗначения: {}", list.join(", "));
            }
            if p.inherited {
                md += "\n\nНаследуется.";
            }
            return Some(Hover {
                range,
                markdown: md,
                color: None,
            });
        }
    }

    // Псевдокласс в селекторе.
    for (rule, _) in doc.sheet.walk_rules() {
        for sel in &rule.selectors {
            for part in &sel.parts {
                if part.kind == SelectorPartKind::Pseudo && part.span.contains(&offset) {
                    let doc_text = properties::PSEUDO_CLASSES
                        .iter()
                        .find(|(n, _)| *n == part.name)
                        .map(|(_, d)| d.to_string())
                        .unwrap_or_else(|| "Неизвестный псевдокласс: правило пропускается.".into());
                    return Some(Hover {
                        range: doc.range(&part.span),
                        markdown: format!("**:{}** — {doc_text}", part.name),
                        color: None,
                    });
                }
            }
        }
    }

    let in_value = declaration_with_value_at(&doc, offset);

    // Функция.
    if t.kind == TokenKind::Ident && next_is_paren {
        let f = properties::function(word)?;
        let mut md = format!("`{}`\n\n{}", f.signature, f.doc);
        let mut color = None;
        if matches!(f.name, "rgb" | "rgba" | "darken" | "lighten") {
            // Весь вызов до парной скобки.
            let mut depth = 0;
            let mut end = t.span.end;
            for n in &toks[i + 1..] {
                match n.kind {
                    TokenKind::LParen => depth += 1,
                    TokenKind::RParen => {
                        depth -= 1;
                        if depth == 0 {
                            end = n.span.end;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            color = parse_color(&text[t.span.start..end]);
            if let Some(c) = color {
                md += &format!("\n\n{}", color_line(c));
            }
        }
        return Some(Hover {
            range,
            markdown: md,
            color,
        });
    }

    // Цвет.
    let color = match t.kind {
        TokenKind::Hash => Color::parse(word),
        TokenKind::Ident if in_value.is_some() => mss_lang::color::NAMED_COLORS
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(word))
            .map(|(_, c)| *c),
        _ => None,
    };
    if let Some(c) = color {
        return Some(Hover {
            range,
            markdown: color_line(c),
            color: Some(c),
        });
    }

    // Имя анимации и ключевые слова значения.
    if let (Some(d), TokenKind::Ident) = (in_value, t.kind) {
        if let Some((name, span)) = doc
            .sheet
            .keyframes_names()
            .into_iter()
            .find(|k| k.0 == word)
        {
            if matches!(d.name.as_str(), "animation" | "animation-name") {
                return Some(Hover {
                    range,
                    markdown: format!(
                        "**@keyframes {name}** — строка {}",
                        doc.pos(span.start).line + 1
                    ),
                    color: None,
                });
            }
        }
        if let Some((k, kd)) = properties::GLOBAL_KEYWORDS.iter().find(|(k, _)| *k == word) {
            return Some(Hover {
                range,
                markdown: format!("**{k}** — {kd}"),
                color: None,
            });
        }
    }
    None
}

/// Переход к определению: `var(--x)` → объявление переменной в документе,
/// имя анимации → `@keyframes`.
pub fn definition(text: &str, pos: Position) -> Option<Range> {
    let doc = Doc::new(text);
    let offset = doc.offset(pos);
    let i = token_index_at(&doc, offset)?;
    let t = &doc.sheet.tokens[i];
    if t.kind != TokenKind::Ident {
        return None;
    }
    let word = &text[t.span.clone()];
    if word.starts_with("--") {
        return doc.variable(word).map(|d| doc.range(&d.name_span));
    }
    let d = declaration_with_value_at(&doc, offset)?;
    if !matches!(d.name.as_str(), "animation" | "animation-name") {
        return None;
    }
    doc.sheet
        .keyframes_names()
        .into_iter()
        .find(|k| k.0 == word)
        .map(|k| doc.range(&k.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VarInfo;

    const SRC: &str = ":root { --accent: #3b82f6; --link: var(--accent); }\n.a:hover { color: var(--link); background: rgba(0, 0, 0, 0.5); animation: spin 1s; border-color: red; }\n@keyframes spin { to { opacity: 0; } }";

    fn at(line: usize, col: usize) -> Option<Hover> {
        hover(SRC, Position::new(line, col), &Context::default())
    }

    #[test]
    fn variable_hover_resolves_chain_and_color() {
        let h = at(1, 25).unwrap(); // --link в var()
        assert!(
            h.markdown.contains("`var(--accent)` → `#3b82f6`"),
            "{}",
            h.markdown
        );
        assert_eq!(h.color, Some(Color::rgb(0x3b, 0x82, 0xf6)));
        assert_eq!(h.range.start, Position::new(1, 22));
        // В конце слова — то же слово.
        assert_eq!(at(1, 28).unwrap().range, h.range);
    }

    #[test]
    fn context_variable_doc() {
        let ctx = Context {
            variables: vec![VarInfo {
                name: "--bg".into(),
                value: "#000".into(),
                doc: "Фон окна".into(),
            }],
            ..Default::default()
        };
        let h = hover(".a { color: var(--bg); }", Position::new(0, 17), &ctx).unwrap();
        assert!(h.markdown.contains("Фон окна"));
        assert_eq!(h.color, Some(Color::rgb(0, 0, 0)));
    }

    #[test]
    fn property_pseudo_function_color() {
        let h = at(1, 12).unwrap();
        assert!(h.markdown.starts_with("**color** — цвет"), "{}", h.markdown);
        assert!(h.markdown.contains("Наследуется"));
        assert!(at(1, 4)
            .unwrap()
            .markdown
            .contains("Указатель над элементом"));
        let f = at(1, 45).unwrap();
        assert!(f.markdown.contains("rgba(r, g, b, a)"), "{}", f.markdown);
        assert_eq!(f.color, Some(Color::rgba(0, 0, 0, 127)));
        let red = at(1, 98).unwrap();
        assert_eq!(red.color, Some(Color::rgb(255, 0, 0)));
        assert!(at(1, 75).unwrap().markdown.contains("@keyframes spin"));
    }

    #[test]
    fn definitions() {
        assert_eq!(
            definition(SRC, Position::new(1, 24)).unwrap().start,
            Position::new(0, 27)
        );
        assert_eq!(
            definition(SRC, Position::new(0, 42)).unwrap().start,
            Position::new(0, 8)
        );
        assert_eq!(
            definition(SRC, Position::new(1, 74)).unwrap().start,
            Position::new(2, 11)
        );
        assert_eq!(definition(SRC, Position::new(1, 12)), None);
    }
}
