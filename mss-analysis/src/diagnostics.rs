//! Диагностики: синтаксис, свойства, значения, переменные, псевдоклассы,
//! `@keyframes`.

use std::collections::{HashMap, HashSet};

use mss_lang::properties::{self, ValueKind};
use mss_lang::syntax::{Block, Declaration, Item, SelectorPartKind, Span, TokenKind};

use crate::values::{self, check_common, check_kind, value_tokens};
use crate::{suggest, variable_value, Context, Doc, Range};

/// Важность диагностики (как в LSP и `CodeEditor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Диагностика: место, важность, сообщение по-русски и код вида
/// `unknown-property` (для фильтров и LSP).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag {
    pub range: Range,
    pub severity: Severity,
    pub message: String,
    pub code: &'static str,
}

/// Слова `animation`, которые не имя анимации.
const ANIMATION_KEYWORDS: &[&str] = &[
    "none",
    "infinite",
    "normal",
    "reverse",
    "alternate",
    "alternate-reverse",
    "forwards",
    "backwards",
    "both",
    "running",
    "paused",
];

struct Sink<'d, 'a> {
    doc: &'d Doc<'a>,
    out: Vec<(Span, Severity, String, &'static str)>,
}

impl Sink<'_, '_> {
    fn push(
        &mut self,
        span: Span,
        severity: Severity,
        message: impl Into<String>,
        code: &'static str,
    ) {
        self.out.push((span, severity, message.into(), code));
    }
}

/// Все диагностики документа, по порядку в тексте.
pub fn diagnostics(text: &str, ctx: &Context) -> Vec<Diag> {
    let doc = Doc::new(text);
    let mut sink = Sink {
        doc: &doc,
        out: Vec::new(),
    };

    for e in &doc.sheet.errors {
        sink.push(e.span.clone(), Severity::Error, e.message.clone(), "syntax");
    }

    // Переменные и @keyframes, известные документу и контексту.
    let mut vars: HashSet<&str> = doc.variables().iter().map(|d| d.name.as_str()).collect();
    vars.extend(ctx.variables.iter().map(|v| v.name.as_str()));
    let mut keyframes: HashSet<&str> = doc.sheet.keyframes_names().iter().map(|k| k.0).collect();
    keyframes.extend(ctx.keyframes.iter().map(String::as_str));

    // Селекторы: псевдоклассы.
    for (rule, _) in doc.sheet.walk_rules() {
        if rule.is_root {
            continue;
        }
        for sel in &rule.selectors {
            let pseudos: Vec<_> = sel
                .parts
                .iter()
                .filter(|p| p.kind == SelectorPartKind::Pseudo)
                .collect();
            for p in &pseudos {
                if !properties::is_known_pseudo(&p.name) {
                    let hint = suggest(&p.name, properties::PSEUDO_CLASSES.iter().map(|(n, _)| *n))
                        .map(|s| format!(" Может быть, :{s}?"))
                        .unwrap_or_default();
                    sink.push(
                        p.span.clone(),
                        Severity::Warning,
                        format!(
                            "неизвестный псевдокласс :{} — селектор «{}» пропускается.{hint}",
                            p.name, sel.text
                        ),
                        "unknown-pseudo",
                    );
                }
            }
            if pseudos.len() > 1 {
                let last = pseudos.last().unwrap();
                sink.push(
                    sel.span.clone(),
                    Severity::Warning,
                    format!(
                        "у селектора действует только один псевдокласс — последний (:{})",
                        last.name
                    ),
                    "multiple-pseudo",
                );
            }
        }
    }

    // Объявления по блокам (для повторов).
    let mut blocks: Vec<&Block> = Vec::new();
    for (rule, _) in doc.sheet.walk_rules() {
        if let Some(b) = &rule.block {
            blocks.push(b);
        }
    }
    for item in &doc.sheet.items {
        if let Item::Keyframes(k) = item {
            blocks.extend(k.steps.iter().filter_map(|s| s.block.as_ref()));
        }
    }
    for block in blocks {
        let mut last: HashMap<&str, &Declaration> = HashMap::new();
        for d in block.declarations() {
            if let Some(prev) = last.insert(d.name.as_str(), d) {
                sink.push(
                    prev.span.clone(),
                    Severity::Hint,
                    format!("«{}» объявлено ниже ещё раз — действует последнее", d.name),
                    "duplicate",
                );
            }
            check_declaration(&mut sink, ctx, d, &vars, &keyframes);
        }
    }

    let mut out: Vec<Diag> = sink
        .out
        .into_iter()
        .map(|(span, severity, message, code)| Diag {
            range: doc.range(&span),
            severity,
            message,
            code,
        })
        .collect();
    out.sort_by_key(|d| (d.range.start, d.severity));
    out
}

fn check_declaration(
    sink: &mut Sink,
    ctx: &Context,
    d: &Declaration,
    vars: &HashSet<&str>,
    keyframes: &HashSet<&str>,
) {
    let doc = sink.doc;
    let text = doc.text;
    let toks = value_tokens(&doc.sheet.tokens, d);

    // Ссылки на переменные.
    for c in values::calls(text, &toks) {
        if c.name != "var" {
            continue;
        }
        let Some(name_tok) = toks.get(c.start + 2).filter(|t| t.kind == TokenKind::Ident) else {
            continue;
        };
        let name = &text[name_tok.span.clone()];
        if !name.starts_with("--") || vars.contains(name) {
            continue;
        }
        let has_fallback = toks[c.start..c.end.min(toks.len())]
            .iter()
            .any(|t| t.kind == TokenKind::Comma);
        let hint = suggest(name, vars.iter().copied())
            .map(|s| format!(" Может быть, {s}?"))
            .unwrap_or_default();
        if has_fallback {
            sink.push(
                name_tok.span.clone(),
                Severity::Info,
                format!("переменная {name} не объявлена — будет запасное значение.{hint}"),
                "unknown-variable",
            );
        } else {
            sink.push(
                name_tok.span.clone(),
                Severity::Warning,
                format!("переменная {name} не объявлена — значение пропадёт.{hint}"),
                "unknown-variable",
            );
        }
    }

    if d.is_variable() || d.colon.is_none() {
        return;
    }
    let Some(spec) = properties::property(&d.name) else {
        let hint = suggest(
            &d.name,
            properties::PROPERTIES
                .iter()
                .filter(|p| !p.internal)
                .map(|p| p.name),
        )
        .map(|s| format!(" Может быть, «{s}»?"))
        .unwrap_or_default();
        sink.push(
            d.name_span.clone(),
            Severity::Warning,
            format!("неизвестное свойство «{}» — будет пропущено.{hint}", d.name),
            "unknown-property",
        );
        return;
    };

    let var_value = |n: &str| variable_value(doc, ctx, n);
    let shorthand = matches!(
        spec.kind,
        ValueKind::Border | ValueKind::Lengths | ValueKind::Sizes
    );
    for p in check_common(text, &toks, &var_value, shorthand) {
        sink.push(p.span, p.severity, p.message, "value");
    }
    for p in check_kind(text, spec.kind, &toks, d) {
        sink.push(p.span, p.severity, p.message, "invalid-value");
    }

    // Имя анимации.
    let anim = match spec.kind {
        ValueKind::AnimationName => toks.first().filter(|t| t.kind == TokenKind::Ident).copied(),
        ValueKind::Animation => toks
            .iter()
            .find(|t| {
                t.kind == TokenKind::Ident && {
                    let w = &text[t.span.clone()];
                    !ANIMATION_KEYWORDS.contains(&w)
                        && !properties::EASINGS.contains(&w)
                        && w != "var"
                }
            })
            .copied(),
        _ => None,
    };
    if let Some(t) = anim {
        let name = &text[t.span.clone()];
        if name != "none" && !keyframes.contains(name) && !properties::is_known_property(name) {
            sink.push(
                t.span.clone(),
                Severity::Warning,
                format!("@keyframes «{name}» не найден"),
                "unknown-keyframes",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Position, VarInfo};

    fn diags(src: &str) -> Vec<Diag> {
        diagnostics(src, &Context::default())
    }

    fn codes(src: &str) -> Vec<&'static str> {
        diags(src).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn clean_document_has_no_diagnostics() {
        let src = ":root { --accent: #3b82f6; --r: 8px; }\n\
            .card { background: var(--accent); border-radius: var(--r); padding: 8px 16px;\n\
              border: 1px solid #ccc; color: rgba(0, 0, 0, 0.5); width: 50%; opacity: 0.8;\n\
              transition: background-color 200ms ease-out; transform: rotate(45deg) scale(1.1);\n\
              animation: spin 1s linear infinite; font-weight: 600; cursor: pointer; }\n\
            .card:hover { background: darken(var(--accent), 0.1); }\n\
            @keyframes spin { from { rotate: 0deg; } to { rotate: 360deg; } }";
        assert_eq!(diags(src), vec![]);
    }

    #[test]
    fn syntax_error_positions() {
        let d = diags(".a {\n  color red;\n}");
        assert_eq!(d[0].code, "syntax");
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[0].range.start, Position::new(1, 8));
    }

    #[test]
    fn unknown_property_with_suggestion() {
        let d = diags(".a { backgroud-color: red; }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, "unknown-property");
        assert!(
            d[0].message.contains("background-color"),
            "{}",
            d[0].message
        );
        assert_eq!(d[0].range.start, Position::new(0, 5));
        assert_eq!(d[0].range.end, Position::new(0, 20));
    }

    #[test]
    fn value_types() {
        assert_eq!(codes(".a { color: orange; }"), ["invalid-value"]);
        assert_eq!(codes(".a { gap: 1em; }"), ["invalid-value"]);
        assert_eq!(codes(".a { width: 1em; }"), ["invalid-value"]);
        assert_eq!(
            codes(".a { width: 10%; height: auto; }"),
            Vec::<&str>::new()
        );
        assert_eq!(
            codes(".a { padding: 1px 2px 3px 4px 5px; }"),
            ["invalid-value"]
        );
        assert_eq!(codes(".a { text-align: middle; }"), ["invalid-value"]);
        assert_eq!(codes(".a { color: #12345; }"), ["value", "invalid-value"]);
        assert_eq!(
            codes(".a { transition-duration: fast; }"),
            ["invalid-value"]
        );
        assert_eq!(codes(".a { filter: rotate(4deg); }"), ["invalid-value"]);
        assert_eq!(codes(".a { color: inherit; }"), Vec::<&str>::new());
        assert_eq!(codes(".a { filter: blurr(3px); }"), ["value"]);
    }

    #[test]
    fn variables() {
        let src = ".a { color: var(--nope); background: var(--nope2, red); }";
        let d = diags(src);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].severity, Severity::Warning);
        assert_eq!(d[1].severity, Severity::Info);
        // Из контекста и из обычного правила (наследуемые переменные).
        let ctx = Context {
            variables: vec![VarInfo {
                name: "--nope".into(),
                value: "#fff".into(),
                doc: String::new(),
            }],
            ..Default::default()
        };
        let src2 = ".s { --nope2: blue; } .a { color: var(--nope); background: var(--nope2); }";
        assert!(diagnostics(src2, &ctx).is_empty());
    }

    #[test]
    fn var_inside_rgba() {
        let src = ":root { --c: #ff0000; --alpha: 0.5; --rgb: 59, 130, 246; }\n\
            .a { color: rgba(var(--c), 0.5); background: rgba(0, 0, 0, var(--alpha)); }\n\
            .b { color: rgba(var(--rgb), 0.2); }";
        let d = diags(src);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("rgba()"), "{}", d[0].message);
        assert!(d[0].message.contains("#ff0000"));
    }

    #[test]
    fn fallback_inside_compound_value() {
        let d =
            diags(".a { box-shadow: 0 2px 4px var(--sh, #000); border: 1px solid var(--b, red); }");
        let c: Vec<_> = d.iter().map(|d| d.code).collect();
        // Запасное значение внутри составного подставляется (с 2026-10-22).
        assert_eq!(c, ["unknown-variable", "unknown-variable"]);
    }

    #[test]
    fn pseudo_classes_and_keyframes() {
        let d = diags(".a:hovr { color: red; } .b:hover:focus { color: red; }\n.c { animation-name: fade; animation: spin 1s infinite; }\n@keyframes spin { to { opacity: 0; } }");
        let c: Vec<_> = d.iter().map(|d| d.code).collect();
        assert_eq!(
            c,
            ["unknown-pseudo", "multiple-pseudo", "unknown-keyframes"]
        );
        assert!(d[0].message.contains(":hover"));
        let ctx = Context {
            keyframes: vec!["fade".into()],
            ..Default::default()
        };
        assert!(diagnostics(".c { animation-name: fade; }", &ctx).is_empty());
    }

    #[test]
    fn duplicates_are_hints() {
        let d = diags(".a { color: red; color: blue; }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Hint);
        assert_eq!(d[0].range.start, Position::new(0, 5));
    }
}
