//! Форматирование: отступ 4 пробела, `имя: значение;` по строке, `{` на
//! строке селектора. Комментарии и одиночные пустые строки сохраняются,
//! значения не переписываются (только отступы их продолжений).

use mss_lang::syntax::{
    Block, BlockItem, Declaration, Item, Rule, Selector, SelectorPartKind, Span, TokenKind,
};

use crate::Doc;

const INDENT: &str = "    ";

/// Отформатировать документ. Документ с синтаксическими ошибками
/// возвращается как есть: чинить его — дело автора.
pub fn format(text: &str) -> String {
    let doc = Doc::new(text);
    if !doc.sheet.errors.is_empty() || text.trim().is_empty() {
        return text.to_string();
    }
    let mut f = Fmt {
        doc: &doc,
        out: String::new(),
    };
    let spans: Vec<Span> = doc
        .sheet
        .items
        .iter()
        .map(|i| match i {
            Item::Rule(r) => r.span.clone(),
            Item::Keyframes(k) => k.span.clone(),
            Item::AtRule(a) => a.span.clone(),
        })
        .collect();
    f.container(0, &spans, 0..text.len(), |f, k| match &doc.sheet.items[k] {
        Item::Rule(r) => f.rule(r, 0),
        Item::Keyframes(kf) => f.keyframes(kf),
        Item::AtRule(a) => {
            let raw = doc.text[a.span.clone()].to_string();
            f.line(0, &raw);
        }
    });
    let mut out = f.out.trim_end().to_string();
    out.push('\n');
    out
}

struct Fmt<'d, 'a> {
    doc: &'d Doc<'a>,
    out: String,
}

impl Fmt<'_, '_> {
    fn line(&mut self, depth: usize, s: &str) {
        for _ in 0..depth {
            self.out.push_str(INDENT);
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    /// Есть ли комментарий среди токенов диапазона.
    fn has_comment(&self, span: &Span) -> bool {
        self.doc.sheet.tokens.iter().any(|t| {
            t.kind == TokenKind::Comment && t.span.start >= span.start && t.span.end <= span.end
        })
    }

    /// Элементы контейнера (`spans` — их места) с комментариями и пустыми
    /// строками между ними; `region` — внутренность контейнера.
    fn container(
        &mut self,
        depth: usize,
        spans: &[Span],
        region: Span,
        mut emit: impl FnMut(&mut Self, usize),
    ) {
        let mut cursor = region.start;
        for (k, span) in spans.iter().enumerate() {
            self.gap(depth, cursor..span.start, k > 0 || cursor > region.start);
            emit(self, k);
            cursor = span.end;
        }
        self.gap(depth, cursor..region.end, !spans.is_empty());
    }

    /// Промежуток между элементами: комментарии (в строку с предыдущим
    /// элементом, если стояли на той же строке) и не больше одной пустой
    /// строки подряд.
    fn gap(&mut self, depth: usize, span: Span, after_item: bool) {
        let mut newlines = 0usize;
        let mut first = true;
        let toks: Vec<_> = self
            .doc
            .sheet
            .tokens
            .iter()
            .filter(|t| t.span.start >= span.start && t.span.end <= span.end)
            .cloned()
            .collect();
        for t in toks {
            match t.kind {
                TokenKind::Whitespace => {
                    newlines += self.doc.text[t.span.clone()].matches('\n').count()
                }
                TokenKind::Comment => {
                    let c = self.doc.text[t.span.clone()].to_string();
                    if first && after_item && newlines == 0 && self.out.ends_with('\n') {
                        self.out.pop();
                        self.out.push(' ');
                        self.out.push_str(&reindent(&c, depth));
                        self.out.push('\n');
                    } else {
                        if newlines >= 2 && !self.out.is_empty() && !self.out.ends_with("{\n") {
                            self.out.push('\n');
                        }
                        let c = reindent(&c, depth);
                        self.line(depth, &c);
                    }
                    newlines = 0;
                    first = false;
                }
                _ => {}
            }
        }
        if newlines >= 2
            && after_item
            && span.end < self.doc.text.len()
            && !self.out.ends_with("\n\n")
        {
            // Пустая строка перед следующим элементом (не перед `}`).
            let next_is_close = self.doc.text[span.end..].starts_with('}');
            if !next_is_close {
                self.out.push('\n');
            }
        }
    }

    fn rule(&mut self, r: &Rule, depth: usize) {
        let header = self.selectors(r, depth);
        let Some(b) = &r.block else {
            self.line(depth, &header);
            return;
        };
        self.block(&header, b, depth, |f, item, d| match item {
            BlockItem::Declaration(decl) => f.declaration(decl, d),
            BlockItem::Rule(r) => f.rule(r, d),
        });
    }

    fn block(
        &mut self,
        header: &str,
        b: &Block,
        depth: usize,
        mut emit: impl FnMut(&mut Self, &BlockItem, usize),
    ) {
        let inner = b.open.end..b.close.as_ref().map_or(self.doc.text.len(), |c| c.start);
        if b.items.is_empty() && !self.has_comment(&inner) {
            self.line(depth, &format!("{header} {{}}"));
            return;
        }
        self.line(depth, &format!("{header} {{"));
        let spans: Vec<Span> = b
            .items
            .iter()
            .map(|i| match i {
                BlockItem::Declaration(d) => d.span.clone(),
                BlockItem::Rule(r) => r.span.clone(),
            })
            .collect();
        self.container(depth + 1, &spans, inner, |f, k| {
            emit(f, &b.items[k], depth + 1)
        });
        self.line(depth, "}");
    }

    fn selectors(&self, r: &Rule, depth: usize) -> String {
        let raw = &self.doc.text[r.selector_span.clone()];
        if self.has_comment(&r.selector_span) {
            return raw.to_string();
        }
        let multiline = raw.contains('\n') && r.selectors.len() > 1;
        let parts: Vec<String> = r.selectors.iter().map(|s| self.selector(s)).collect();
        if multiline {
            let sep = format!(",\n{}", INDENT.repeat(depth));
            parts.join(&sep)
        } else {
            parts.join(", ")
        }
    }

    fn selector(&self, s: &Selector) -> String {
        let mut out = String::new();
        for p in &s.parts {
            match p.kind {
                SelectorPartKind::Combinator(' ') => out.push(' '),
                SelectorPartKind::Combinator(c) => {
                    if !out.is_empty() && !out.ends_with(' ') {
                        out.push(' ');
                    }
                    out.push(c);
                    out.push(' ');
                }
                _ => out.push_str(&self.doc.text[p.span.clone()]),
            }
        }
        out.trim().to_string()
    }

    fn declaration(&mut self, d: &Declaration, depth: usize) {
        // Комментарий между именем и значением — оставляем объявление как есть.
        if self.has_comment(&(d.name_span.end..d.value_span.start)) || d.colon.is_none() {
            let raw = self.doc.text[d.span.clone()]
                .trim_end_matches(';')
                .trim()
                .to_string();
            self.line(depth, &format!("{raw};"));
            return;
        }
        let value = reindent(&d.value, depth + 1);
        self.line(depth, &format!("{}: {};", d.name, value));
    }

    fn keyframes(&mut self, k: &mss_lang::syntax::Keyframes) {
        let name = k.name.as_ref().map_or("", |n| n.0.as_str());
        let (Some(open), Some(close)) = (&k.open, &k.close) else {
            let raw = self.doc.text[k.span.clone()].to_string();
            self.line(0, &raw);
            return;
        };
        self.line(0, &format!("@keyframes {name} {{"));
        let spans: Vec<Span> = k.steps.iter().map(|s| s.span.clone()).collect();
        self.container(1, &spans, open.end..close.start, |f, i| {
            let step = &k.steps[i];
            let header = match &step.block {
                Some(b) => f.doc.text[step.span.start..b.open.start].trim().to_string(),
                None => f.doc.text[step.span.clone()].trim().to_string(),
            };
            let header_span =
                step.span.start..step.block.as_ref().map_or(step.span.end, |b| b.open.start);
            let header = if f.has_comment(&header_span) {
                header
            } else {
                step.selectors
                    .iter()
                    .map(|s| s.0.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            match &step.block {
                Some(b) => f.block(&header, b, 1, |f, item, d| {
                    if let BlockItem::Declaration(decl) = item {
                        f.declaration(decl, d);
                    }
                }),
                None => f.line(1, &header),
            }
        });
        self.line(0, "}");
    }
}

/// Продолжения многострочного текста — с отступом `depth`.
fn reindent(s: &str, depth: usize) -> String {
    if !s.contains('\n') {
        return s.to_string();
    }
    let mut lines = s.lines();
    let mut out = lines.next().unwrap_or("").trim_end().to_string();
    let pad = INDENT.repeat(depth);
    for l in lines {
        out.push('\n');
        let t = l.trim();
        if !t.is_empty() {
            out.push_str(&pad);
            out.push_str(t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_rules_and_keeps_comments() {
        let src = "/* тема */\n:root{--a:#fff;--b : 4px}\n\n\n.a,.b>  .c{color:red;/* хвост */\n  background:linear-gradient(\n90deg,\n red, blue);&:hover{color:blue}}\n@keyframes k{from{opacity:0}50%,to{opacity:1}}";
        let want = "/* тема */\n:root {\n    --a: #fff;\n    --b: 4px;\n}\n\n.a, .b > .c {\n    color: red; /* хвост */\n    background: linear-gradient(\n        90deg,\n        red, blue);\n    &:hover {\n        color: blue;\n    }\n}\n@keyframes k {\n    from {\n        opacity: 0;\n    }\n    50%, to {\n        opacity: 1;\n    }\n}\n";
        let got = format(src);
        assert_eq!(got, want, "\n{got}");
        assert_eq!(format(&got), got, "идемпотентность");
    }

    #[test]
    fn broken_document_is_untouched() {
        let src = ".a { color red; }";
        assert_eq!(format(src), src);
    }

    #[test]
    fn empty_blocks_and_comment_lines() {
        let src = ".a {}\n\n/* один */\n/* два */\n.b {\n    /* внутри */\n}\n";
        assert_eq!(format(src), src);
    }

    /// На всех .mss syngui и synshell: форматирование не теряет значимых
    /// токенов (кроме добавленных «;»), идемпотентно, разбирается без ошибок.
    #[test]
    fn corpus_roundtrip() {
        fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return;
            };
            for e in rd.flatten() {
                let p = e.path();
                let n = e.file_name().to_string_lossy().to_string();
                if p.is_dir() {
                    if n != "target" && !n.starts_with('.') {
                        collect(&p, out);
                    }
                } else if n.ends_with(".mss") {
                    out.push(p);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut files = Vec::new();
        collect(&root, &mut files);
        collect(&root.join("../synshell"), &mut files);
        assert!(files.len() > 10);
        let significant = |s: &str| -> Vec<String> {
            let (toks, _) = mss_lang::syntax::tokenize(s);
            toks.iter()
                .filter(|t| t.kind != TokenKind::Whitespace && t.kind != TokenKind::Semicolon)
                .map(|t| {
                    // Пробелы внутри комментариев переотступаются.
                    let x = t.text(s);
                    if t.kind == TokenKind::Comment {
                        x.split_whitespace().collect::<Vec<_>>().join(" ")
                    } else {
                        x.to_string()
                    }
                })
                .collect()
        };
        for p in files {
            let src = std::fs::read_to_string(&p).unwrap();
            let out = format(&src);
            if mss_lang::syntax::parse(&src).errors.is_empty() {
                assert!(
                    mss_lang::syntax::parse(&out).errors.is_empty(),
                    "{}",
                    p.display()
                );
                assert_eq!(significant(&src), significant(&out), "{}", p.display());
                assert_eq!(format(&out), out, "{}: не идемпотентно", p.display());
            }
        }
    }
}
