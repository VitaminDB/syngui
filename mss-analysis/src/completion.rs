//! Автодополнение: свойства, значения, переменные, селекторы, псевдоклассы,
//! `@keyframes`.

use std::collections::BTreeSet;

use mss_lang::color::NAMED_COLORS;
use mss_lang::properties::{self, FunctionGroup, ValueKind};
use mss_lang::syntax::{Block, BlockItem, Item, Keyframes, Rule, SelectorPartKind, TokenKind};

use crate::{Context, Doc, Position, Range};

/// Вид варианта дополнения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompletionKind {
    Property,
    Value,
    Keyword,
    Function,
    Variable,
    Class,
    Element,
    PseudoClass,
    Keyframes,
    Color,
}

impl CompletionKind {
    /// Короткая метка для `CodeEditor` (`CompletionItem::kind`).
    pub fn short(self) -> &'static str {
        match self {
            CompletionKind::Property => "prop",
            CompletionKind::Value => "val",
            CompletionKind::Keyword => "kw",
            CompletionKind::Function => "fn",
            CompletionKind::Variable => "var",
            CompletionKind::Class => "class",
            CompletionKind::Element => "type",
            CompletionKind::PseudoClass => "pseudo",
            CompletionKind::Keyframes => "anim",
            CompletionKind::Color => "color",
        }
    }
}

/// Вариант дополнения. `insert_text` заменяет [`Completion::range`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    /// Коротко: тип свойства, значение переменной, образец вызова.
    pub detail: String,
    /// Описание (Markdown).
    pub documentation: String,
    pub insert_text: String,
}

/// Варианты и заменяемый ими диапазон (набранная часть слова до каретки).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub range: Range,
    pub items: Vec<CompletionItem>,
}

fn item(
    label: impl Into<String>,
    kind: CompletionKind,
    detail: impl Into<String>,
    doc: impl Into<String>,
) -> CompletionItem {
    let label = label.into();
    CompletionItem {
        insert_text: label.clone(),
        label,
        kind,
        detail: detail.into(),
        documentation: doc.into(),
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

enum Container<'d> {
    Top,
    Rule(&'d Block),
    Root(&'d Block),
    KeyframesBody,
    Step(&'d Block),
}

fn inside(block: &Block, offset: usize) -> bool {
    block.open.end <= offset && block.close.as_ref().is_none_or(|c| offset <= c.start)
}

fn find_in_rule(rule: &Rule, offset: usize) -> Option<Container<'_>> {
    let b = rule.block.as_ref()?;
    if !inside(b, offset) {
        return None;
    }
    for child in b.rules() {
        if let Some(c) = find_in_rule(child, offset) {
            return Some(c);
        }
    }
    Some(if rule.is_root {
        Container::Root(b)
    } else {
        Container::Rule(b)
    })
}

fn find_in_keyframes(k: &Keyframes, offset: usize) -> Option<Container<'_>> {
    let open = k.open.as_ref()?;
    if offset < open.end || k.close.as_ref().is_some_and(|c| offset > c.start) {
        return None;
    }
    for s in &k.steps {
        if let Some(b) = &s.block {
            if inside(b, offset) {
                return Some(Container::Step(b));
            }
        }
    }
    Some(Container::KeyframesBody)
}

fn container_at<'d>(doc: &'d Doc, offset: usize) -> Container<'d> {
    for item in &doc.sheet.items {
        let found = match item {
            Item::Rule(r) => find_in_rule(r, offset),
            Item::Keyframes(k) => find_in_keyframes(k, offset),
            Item::AtRule(_) => None,
        };
        if let Some(c) = found {
            return c;
        }
    }
    Container::Top
}

/// Где курсор внутри блока.
enum Place {
    /// Имя свойства (или пустое место для него).
    Name,
    /// Значение свойства с этим именем.
    Value(String),
    /// Селектор вложенного правила.
    Selector,
}

fn place_in_block(doc: &Doc, block: &Block, offset: usize) -> Place {
    let close = block.close.as_ref().map_or(doc.text.len(), |c| c.start);
    for (k, it) in block.items.iter().enumerate() {
        let next_start = block
            .items
            .get(k + 1)
            .map(|n| match n {
                BlockItem::Declaration(d) => d.span.start,
                BlockItem::Rule(r) => r.span.start,
            })
            .unwrap_or(close);
        match it {
            BlockItem::Declaration(d) => {
                let end = d.semicolon.as_ref().map_or(next_start, |s| s.start);
                if d.name_span.start <= offset && offset <= end {
                    return match &d.colon {
                        Some(c) if offset >= c.end => Place::Value(d.name.clone()),
                        _ => Place::Name,
                    };
                }
            }
            BlockItem::Rule(r) => {
                if r.selector_span.start <= offset && offset <= r.selector_span.end {
                    return Place::Selector;
                }
            }
        }
    }
    Place::Name
}

/// Дополнение в позиции `pos`.
pub fn completion(text: &str, pos: Position, ctx: &Context) -> Option<Completion> {
    let doc = Doc::new(text);
    let offset = doc.offset(pos);

    // В комментарии и строке не дополняем.
    if let Some(i) = doc.sheet.token_at(offset) {
        let t = &doc.sheet.tokens[i];
        if matches!(t.kind, TokenKind::Comment | TokenKind::String)
            && t.span.start < offset
            && offset < t.span.end
        {
            return None;
        }
    }

    let word_start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word_char(*c))
        .last()
        .map_or(offset, |(i, _)| i);
    let prefix = text[..word_start].chars().next_back();
    let range = Range {
        start: doc.pos(word_start),
        end: doc.pos(offset),
    };

    let items = match container_at(&doc, offset) {
        Container::Top => {
            if prefix == Some('@') {
                vec![item(
                    "keyframes",
                    CompletionKind::Keyword,
                    "@keyframes имя { … }",
                    "Ключевые кадры анимации.",
                )]
            } else {
                selector_items(&doc, ctx, prefix, true, word_start..offset)
            }
        }
        Container::Rule(b) => match place_in_block(&doc, b, offset) {
            Place::Selector => selector_items(&doc, ctx, prefix, false, word_start..offset),
            Place::Name if matches!(prefix, Some('.' | ':' | '&')) => {
                selector_items(&doc, ctx, prefix, false, word_start..offset)
            }
            Place::Name => property_items(),
            Place::Value(name) => value_items(&doc, ctx, &name, word_start),
        },
        Container::Step(b) => match place_in_block(&doc, b, offset) {
            Place::Value(name) => value_items(&doc, ctx, &name, word_start),
            _ => property_items(),
        },
        Container::Root(b) => match place_in_block(&doc, b, offset) {
            Place::Value(name) => value_items(&doc, ctx, &name, word_start),
            _ => {
                let declared: BTreeSet<_> =
                    doc.variables().iter().map(|d| d.name.clone()).collect();
                ctx.variables
                    .iter()
                    .filter(|v| !declared.contains(&v.name))
                    .map(|v| {
                        let mut it = item(&v.name, CompletionKind::Variable, &v.value, &v.doc);
                        it.insert_text = format!("{}: ", v.name);
                        it
                    })
                    .collect()
            }
        },
        Container::KeyframesBody => vec![
            item("from", CompletionKind::Keyword, "0%", "Начало анимации."),
            item("to", CompletionKind::Keyword, "100%", "Конец анимации."),
        ],
    };
    if items.is_empty() {
        return None;
    }
    let mut seen = BTreeSet::new();
    let items = items
        .into_iter()
        .filter(|i| seen.insert((i.label.clone(), i.kind.short())))
        .collect();
    Some(Completion { range, items })
}

fn property_items() -> Vec<CompletionItem> {
    properties::PROPERTIES
        .iter()
        .filter(|p| !p.internal)
        .map(|p| {
            let mut it = item(p.name, CompletionKind::Property, p.kind.label(), p.doc);
            it.insert_text = format!("{}: ", p.name);
            it
        })
        .collect()
}

/// `typing` — набираемое слово: оно само в варианты не попадает.
fn selector_items(
    doc: &Doc,
    ctx: &Context,
    prefix: Option<char>,
    top: bool,
    typing: std::ops::Range<usize>,
) -> Vec<CompletionItem> {
    match prefix {
        Some('.') => {
            let mut classes: BTreeSet<String> = ctx.known_classes.iter().cloned().collect();
            for (rule, _) in doc.sheet.walk_rules() {
                for s in &rule.selectors {
                    for p in &s.parts {
                        if p.kind == SelectorPartKind::Class && p.name_span != typing {
                            classes.insert(p.name.clone());
                        }
                    }
                }
            }
            classes
                .into_iter()
                .map(|c| item(c, CompletionKind::Class, "класс", ""))
                .collect()
        }
        Some(':') => {
            let mut v: Vec<_> = properties::PSEUDO_CLASSES
                .iter()
                .map(|(n, d)| item(*n, CompletionKind::PseudoClass, "псевдокласс", *d))
                .collect();
            if top {
                v.push(item(
                    "root",
                    CompletionKind::PseudoClass,
                    ":root { --x: … }",
                    "Блок переменных.",
                ));
            }
            v
        }
        Some('#') => Vec::new(),
        _ => {
            let mut elements = BTreeSet::new();
            for (rule, _) in doc.sheet.walk_rules() {
                for s in &rule.selectors {
                    for p in &s.parts {
                        if p.kind == SelectorPartKind::Element && p.name_span != typing {
                            elements.insert(p.name.clone());
                        }
                    }
                }
            }
            elements
                .into_iter()
                .map(|e| item(e, CompletionKind::Element, "виджет", ""))
                .collect()
        }
    }
}

fn variable_items(doc: &Doc, ctx: &Context, wrap: bool) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mk = |name: &str, value: &str, docs: &str| {
        let label = if wrap {
            format!("var({name})")
        } else {
            name.to_string()
        };
        item(label, CompletionKind::Variable, value.trim(), docs)
    };
    for d in doc.variables().into_iter().rev() {
        if seen.insert(d.name.clone()) {
            out.push(mk(&d.name, &d.value, ""));
        }
    }
    for v in &ctx.variables {
        if seen.insert(v.name.clone()) {
            out.push(mk(&v.name, &v.value, &v.doc));
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

fn function_items(group: FunctionGroup) -> impl Iterator<Item = CompletionItem> {
    properties::FUNCTIONS
        .iter()
        .filter(move |f| f.group == group)
        .map(|f| {
            let mut it = item(f.name, CompletionKind::Function, f.signature, f.doc);
            it.insert_text = format!("{}(", f.name);
            it
        })
}

fn value_items(doc: &Doc, ctx: &Context, property: &str, word_start: usize) -> Vec<CompletionItem> {
    // Внутри `var(` — только переменные.
    let before = doc.text[..word_start].trim_end();
    if before.ends_with("var(") {
        return variable_items(doc, ctx, false);
    }

    let kind = if property.starts_with("--") {
        ValueKind::Any
    } else {
        properties::property(property).map_or(ValueKind::Any, |p| p.kind)
    };
    let mut out: Vec<CompletionItem> = kind
        .keywords()
        .iter()
        .map(|k| item(*k, CompletionKind::Keyword, kind.label(), ""))
        .collect();
    if property == "particle-emitter" {
        out.extend(
            properties::PARTICLE_EMITTERS
                .iter()
                .map(|k| item(*k, CompletionKind::Keyword, "форма", "")),
        );
    }
    let colorish = matches!(
        kind,
        ValueKind::Color
            | ValueKind::Background
            | ValueKind::Shadow
            | ValueKind::TextShadow
            | ValueKind::Border
            | ValueKind::Colors
            | ValueKind::Any
    );
    if colorish {
        out.extend(
            NAMED_COLORS
                .iter()
                .map(|(n, c)| item(*n, CompletionKind::Color, c.to_hex(), "")),
        );
        out.extend(function_items(FunctionGroup::Color));
    }
    match kind {
        ValueKind::Background | ValueKind::Any => {
            out.extend(function_items(FunctionGroup::Gradient))
        }
        ValueKind::Transform => out.extend(function_items(FunctionGroup::Transform)),
        ValueKind::Filter => out.extend(function_items(FunctionGroup::Filter)),
        ValueKind::Easing | ValueKind::Transition | ValueKind::Animation => {
            out.extend(
                properties::EASINGS
                    .iter()
                    .map(|e| item(*e, CompletionKind::Keyword, "кривая", "")),
            );
            out.extend(function_items(FunctionGroup::Easing));
        }
        _ => {}
    }
    if matches!(kind, ValueKind::Animation | ValueKind::AnimationName) {
        let mut names: BTreeSet<String> = ctx.keyframes.iter().cloned().collect();
        names.extend(doc.sheet.keyframes_names().iter().map(|k| k.0.to_string()));
        out.extend(
            names
                .into_iter()
                .map(|n| item(n, CompletionKind::Keyframes, "@keyframes", "")),
        );
    }
    if matches!(kind, ValueKind::PropertyList | ValueKind::Transition) {
        out.extend(
            properties::PROPERTIES
                .iter()
                .filter(|p| !p.internal)
                .map(|p| item(p.name, CompletionKind::Property, p.kind.label(), p.doc)),
        );
    }
    out.extend(variable_items(doc, ctx, true));
    out.extend(
        properties::GLOBAL_KEYWORDS
            .iter()
            .map(|(k, d)| item(*k, CompletionKind::Keyword, "каскад", *d)),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VarInfo;

    fn at(src: &str, ctx: &Context) -> Option<Completion> {
        // Каретка — на месте «|».
        let off = src.find('|').unwrap();
        let text = src.replace('|', "");
        let doc = Doc::new(&text);
        completion(&text, doc.pos(off), ctx)
    }

    fn labels(c: &Completion) -> Vec<&str> {
        c.items.iter().map(|i| i.label.as_str()).collect()
    }

    fn ctx() -> Context {
        Context {
            variables: vec![VarInfo {
                name: "--bg".into(),
                value: "#101010".into(),
                doc: "Фон".into(),
            }],
            known_classes: vec!["panel".into(), "dock".into()],
            keyframes: vec!["pulse".into()],
        }
    }

    #[test]
    fn property_names() {
        let c = at(".a {\n  backg|\n}", &ctx()).unwrap();
        assert!(labels(&c).contains(&"background-color"));
        assert!(!labels(&c).contains(&"-syn-icon-font-size"));
        assert_eq!(c.range.start, Position::new(1, 2));
        assert_eq!(c.range.end, Position::new(1, 7));
        let bg = c.items.iter().find(|i| i.label == "background").unwrap();
        assert_eq!(bg.insert_text, "background: ");
        // Пустая строка после объявления.
        let c = at(".a { color: red; | }", &ctx()).unwrap();
        assert!(labels(&c).contains(&"color"));
    }

    #[test]
    fn values_by_kind() {
        let c = at(".a { text-align: |", &ctx()).unwrap();
        let l = labels(&c);
        assert!(l.contains(&"center") && l.contains(&"inherit") && l.contains(&"var(--bg)"));
        assert!(!l.contains(&"red"));
        let c = at(".a { color: r|; }", &ctx()).unwrap();
        let l = labels(&c);
        assert!(l.contains(&"red") && l.contains(&"rgba"));
        assert_eq!(c.range.start, Position::new(0, 12));
        let c = at(".a { animation: |}\n@keyframes spin {}", &ctx()).unwrap();
        let l = labels(&c);
        assert!(l.contains(&"spin") && l.contains(&"pulse") && l.contains(&"ease-out"));
        let c = at(".a { filter: b| }", &ctx()).unwrap();
        assert!(labels(&c).contains(&"blur"));
    }

    #[test]
    fn variables_inside_var() {
        let c = at(
            ":root { --accent: #fff; }\n.a { color: var(--ac|) }",
            &ctx(),
        )
        .unwrap();
        assert_eq!(labels(&c), ["--accent", "--bg"]);
        assert_eq!(c.range.start, Position::new(1, 16));
        assert_eq!(c.items[1].detail, "#101010");
        assert_eq!(c.items[1].documentation, "Фон");
    }

    #[test]
    fn selectors() {
        let c = at(".x {}\n.|", &ctx()).unwrap();
        assert_eq!(labels(&c), ["dock", "panel", "x"]);
        let c = at("Button:|", &ctx()).unwrap();
        assert!(labels(&c).contains(&"hover") && labels(&c).contains(&"root"));
        let c = at(".a { &:h| }", &ctx()).unwrap();
        assert!(labels(&c).contains(&"hover"));
        let c = at(".a { .p| {} }", &ctx()).unwrap();
        assert!(labels(&c).contains(&"panel"));
        let c = at("Button {}\nB|", &ctx()).unwrap();
        assert_eq!(labels(&c), ["Button"]);
        let c = at("@k|", &ctx()).unwrap();
        assert_eq!(labels(&c), ["keyframes"]);
    }

    #[test]
    fn root_and_keyframes() {
        let c = at(":root { | }", &ctx()).unwrap();
        assert_eq!(labels(&c), ["--bg"]);
        let c = at("@keyframes k { | }", &ctx()).unwrap();
        assert_eq!(labels(&c), ["from", "to"]);
        let c = at("@keyframes k { to { op| } }", &ctx()).unwrap();
        assert!(labels(&c).contains(&"opacity"));
        assert!(at(".a { /* co| */ }", &ctx()).is_none());
    }
}
