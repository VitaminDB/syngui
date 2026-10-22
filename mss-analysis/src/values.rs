//! Проверка значений свойств по типу из реестра.

use mss_lang::color::Color;
use mss_lang::properties::{self, FunctionGroup, ValueKind};
use mss_lang::syntax::{Declaration, Span, Token, TokenKind};

use crate::Severity;

/// Имена в `transition`, кроме свойств: `size` — размер целиком
/// (AnimatedSize, PopupPanel).
const TRANSITION_EXTRA: &[&str] = &["all", "none", "size"];

/// Замечание к значению.
pub(crate) struct Problem {
    pub span: Span,
    pub severity: Severity,
    pub message: String,
}

fn problem(span: Span, severity: Severity, message: impl Into<String>) -> Problem {
    Problem {
        span,
        severity,
        message: message.into(),
    }
}

/// Значимые (не пробелы и не комментарии) токены значения.
pub(crate) fn value_tokens<'t>(tokens: &'t [Token], d: &Declaration) -> Vec<&'t Token> {
    tokens[d.value_tokens.clone()]
        .iter()
        .filter(|t| !t.kind.is_trivia())
        .collect()
}

/// Вызов функции: имя, индекс токена имени, индекс парной `)` (или конец).
pub(crate) struct Call {
    pub name: String,
    pub name_span: Span,
    pub start: usize,
    pub end: usize,
    /// Индекс внешнего вызова, внутри которого этот.
    pub parent: Option<usize>,
}

/// Вызовы функций в значимых токенах.
pub(crate) fn calls(text: &str, toks: &[&Token]) -> Vec<Call> {
    let mut out: Vec<Call> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut k = 0;
    while k < toks.len() {
        let t = toks[k];
        let is_call = t.kind == TokenKind::Ident
            && toks
                .get(k + 1)
                .is_some_and(|n| n.kind == TokenKind::LParen && n.span.start == t.span.end);
        if is_call {
            out.push(Call {
                name: text[t.span.clone()].to_string(),
                name_span: t.span.clone(),
                start: k,
                end: toks.len(),
                parent: stack.last().copied(),
            });
            stack.push(out.len() - 1);
            k += 2;
            continue;
        }
        match t.kind {
            TokenKind::LParen => stack.push(usize::MAX),
            TokenKind::RParen => {
                if let Some(c) = stack.pop() {
                    if c != usize::MAX {
                        out[c].end = k + 1;
                    }
                }
            }
            _ => {}
        }
        k += 1;
    }
    out
}

/// Цвет так, как его поймёт основной парсер (`#hex`, `rgb()`, `rgba()`,
/// имя, `darken()`, `lighten()`).
pub(crate) fn parse_color(v: &str) -> Option<Color> {
    let v = v.trim();
    Color::parse_color_function(v).or_else(|| {
        if !v.contains(' ') || v.starts_with("rgb(") || v.starts_with("rgba(") {
            Color::parse(v)
        } else {
            None
        }
    })
}

fn is_global_keyword(v: &str) -> bool {
    properties::GLOBAL_KEYWORDS.iter().any(|(k, _)| *k == v)
}

/// Число и единица токена-размерности (`12px` → ("12", "px")).
fn split_dimension(s: &str) -> (&str, &str) {
    let i = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(s.len());
    (&s[..i], &s[i..])
}

/// Общие проверки любого значения: шестнадцатеричные цвета, имена функций,
/// `var()` внутри функций.
pub(crate) fn check_common(
    text: &str,
    toks: &[&Token],
    var_value: &dyn Fn(&str) -> Option<String>,
    _shorthand: bool,
) -> Vec<Problem> {
    let mut out = Vec::new();
    for t in toks {
        if t.kind == TokenKind::Hash {
            let hex = &text[t.span.start + 1..t.span.end];
            if !matches!(hex.len(), 3 | 6 | 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                out.push(problem(
                    t.span.clone(),
                    Severity::Warning,
                    format!("«#{hex}» — не цвет: нужно 3, 6 или 8 шестнадцатеричных цифр"),
                ));
            }
        }
    }
    let calls = calls(text, toks);
    for c in &calls {
        if properties::function(&c.name).is_none() {
            out.push(problem(
                c.name_span.clone(),
                Severity::Warning,
                format!("неизвестная функция «{}()»", c.name),
            ));
            continue;
        }
        if c.name != "var" {
            continue;
        }
        let parent = c.parent.map(|p| &calls[p]);
        let name = toks
            .get(c.start + 2)
            .filter(|t| t.kind == TokenKind::Ident)
            .map(|t| &text[t.span.clone()]);
        let span = c.name_span.start..toks[c.end.min(toks.len()) - 1].span.end;
        if let Some(p) = parent.filter(|p| matches!(p.name.as_str(), "rgb" | "rgba")) {
            // rgb()/rgba() разбирает только числа; var() подставляется
            // текстом — годятся числа («59, 130, 246»), цвет даёт мусор.
            // Значение неизвестно (тема) — не судим.
            let value = name.and_then(var_value);
            let numeric = |v: &str| {
                v.split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|s| !s.is_empty())
                    .all(|s| s.parse::<f32>().is_ok())
            };
            if let Some(v) = value.filter(|v| !v.trim().is_empty() && !numeric(v)) {
                let why = format!("подставится «{}», а {}() ждёт числа", v.trim(), p.name);
                out.push(problem(
                    span,
                    Severity::Warning,
                    format!("var() внутри {}() не работает: {why}", p.name),
                ));
            }
        }
    }
    out
}

/// Проверка значения по типу свойства. Значения с `var()` по типу не
/// проверяются: тип переменной известен только при применении.
pub(crate) fn check_kind(
    text: &str,
    kind: ValueKind,
    toks: &[&Token],
    d: &Declaration,
) -> Vec<Problem> {
    let mut out = Vec::new();
    if toks.is_empty() {
        return out;
    }
    let value = d.value.trim();
    if is_global_keyword(value) {
        return out;
    }
    if toks
        .iter()
        .any(|t| t.kind == TokenKind::Ident && &text[t.span.clone()] == "var")
    {
        return out;
    }
    if value.contains("/*") {
        return out;
    }
    let span = d.value_span.clone();
    let tok_text = |t: &Token| &text[t.span.clone()];
    let mut warn = |span: Span, msg: String| out.push(problem(span, Severity::Warning, msg));

    match kind {
        ValueKind::Color => {
            if parse_color(value).is_none() {
                warn(span, not_a_color(value));
            }
        }
        ValueKind::Background => {
            let gradient = toks.len() > 1
                && toks[0].kind == TokenKind::Ident
                && tok_text(toks[0]).ends_with("gradient");
            if !gradient && value != "none" && parse_color(value).is_none() {
                warn(span, format!("{} или градиент", not_a_color(value)));
            }
        }
        ValueKind::Length | ValueKind::Size => {
            if toks.len() != 1 {
                warn(span, "ожидалось одно значение".into());
            } else if let Some(m) = check_length(text, toks[0], kind == ValueKind::Size) {
                warn(toks[0].span.clone(), m);
            }
        }
        ValueKind::Lengths | ValueKind::Sizes => {
            if toks.len() > 4 {
                warn(span, format!("ожидалось 1–4 значения, а их {}", toks.len()));
            } else {
                for t in toks {
                    if let Some(m) = check_length(text, t, kind == ValueKind::Sizes) {
                        warn(t.span.clone(), m);
                    }
                }
            }
        }
        ValueKind::Number | ValueKind::LineHeight => {
            let ok = toks.len() == 1
                && match toks[0].kind {
                    TokenKind::Number => true,
                    TokenKind::Dimension => split_dimension(tok_text(toks[0])).1 == "px",
                    _ => false,
                };
            if !ok {
                warn(span, format!("ожидалось число, а не «{value}»"));
            }
        }
        ValueKind::Time => {
            let ok = toks.len() == 1
                && match toks[0].kind {
                    TokenKind::Number => true,
                    TokenKind::Dimension => {
                        properties::TIME_UNITS.contains(&split_dimension(tok_text(toks[0])).1)
                    }
                    _ => false,
                };
            if !ok {
                warn(
                    span,
                    format!("ожидалось время (200ms, 0.3s), а не «{value}»"),
                );
            }
        }
        ValueKind::Angle => {
            let ok = toks.len() == 1
                && match toks[0].kind {
                    TokenKind::Number => true,
                    TokenKind::Dimension => {
                        properties::ANGLE_UNITS.contains(&split_dimension(tok_text(toks[0])).1)
                    }
                    _ => false,
                };
            if !ok {
                warn(
                    span,
                    format!("ожидался угол (45deg, 0.5turn, 1rad), а не «{value}»"),
                );
            }
        }
        ValueKind::Keyword(list) => {
            if !list.contains(&value) {
                warn(
                    span,
                    format!("«{value}» не подходит: ожидалось {}", one_of(list)),
                );
            }
        }
        ValueKind::FontWeight => {
            let ok = properties::FONT_WEIGHTS.contains(&value) || value.parse::<u16>().is_ok();
            if !ok {
                warn(span, format!("«{value}» не подходит: ожидалось normal, bold, lighter, bolder или число 100–900"));
            }
        }
        ValueKind::Easing => {
            let ok = properties::EASINGS.contains(&value)
                || calls(text, toks).first().is_some_and(|c| {
                    properties::function(&c.name).is_some_and(|f| f.group == FunctionGroup::Easing)
                });
            if !ok {
                warn(span, format!("неизвестная кривая «{value}»"));
            }
        }
        ValueKind::Transform | ValueKind::Filter => {
            let group = if kind == ValueKind::Transform {
                FunctionGroup::Transform
            } else {
                FunctionGroup::Filter
            };
            if value != "none" {
                for c in calls(text, toks) {
                    if let Some(f) = properties::function(&c.name) {
                        if f.group != group {
                            warn(
                                c.name_span.clone(),
                                format!(
                                    "{}() здесь не к месту: ожидались {}",
                                    c.name,
                                    kind.label()
                                ),
                            );
                        }
                    }
                }
                if calls(text, toks).is_empty() {
                    warn(
                        span,
                        format!("ожидались функции ({}), а не «{value}»", kind.label()),
                    );
                }
            }
        }
        ValueKind::PropertyList => {
            for t in toks.iter().filter(|t| t.kind == TokenKind::Ident) {
                let n = tok_text(t);
                if !TRANSITION_EXTRA.contains(&n) && !properties::is_known_property(n) {
                    warn(t.span.clone(), format!("неизвестное свойство «{n}»"));
                }
            }
        }
        ValueKind::Transition => {
            // Первое слово каждого перехода — свойство.
            let mut first = true;
            for t in toks {
                match t.kind {
                    TokenKind::Comma => first = true,
                    TokenKind::Ident if first => {
                        let n = tok_text(t);
                        first = false;
                        if !TRANSITION_EXTRA.contains(&n)
                            && !properties::EASINGS.contains(&n)
                            && !properties::is_known_property(n)
                        {
                            warn(
                                t.span.clone(),
                                format!("неизвестное свойство «{n}» в transition"),
                            );
                        }
                    }
                    _ => first = false,
                }
            }
        }
        _ => {}
    }
    out
}

fn not_a_color(value: &str) -> String {
    let names: Vec<&str> = mss_lang::color::NAMED_COLORS
        .iter()
        .map(|(n, _)| *n)
        .collect();
    format!(
        "«{value}» — не цвет: нужен #hex, rgb(), rgba(), darken(), lighten() или имя ({})",
        names.join(", ")
    )
}

fn one_of(list: &[&str]) -> String {
    if list.len() > 12 {
        format!("{}…", list[..12].join(", "))
    } else {
        list.join(", ")
    }
}

/// Длина (`size` — разрешены %, auto и прочие размеры).
fn check_length(text: &str, t: &Token, size: bool) -> Option<String> {
    let s = &text[t.span.clone()];
    match t.kind {
        TokenKind::Number => None,
        TokenKind::Dimension => {
            let (_, unit) = split_dimension(s);
            if unit == "px" {
                None
            } else if properties::LENGTH_UNITS.contains(&unit) {
                Some(format!(
                    "единица «{unit}» разбирается, но не применяется: это свойство берёт только px"
                ))
            } else {
                Some(format!(
                    "неизвестная единица «{unit}»: {}",
                    properties::LENGTH_UNITS.join(", ")
                ))
            }
        }
        TokenKind::Percentage if size => None,
        TokenKind::Percentage => Some("проценты здесь не применяются: только px".into()),
        TokenKind::Ident if size && properties::LENGTH_KEYWORDS.contains(&s) => None,
        _ => Some(if size {
            format!("ожидался размер (px, %, auto, fit-content…), а не «{s}»")
        } else {
            format!("ожидалась длина в px, а не «{s}»")
        }),
    }
}
