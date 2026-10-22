//! Цветовые литералы документа — для образцов цвета в редакторе.

use mss_lang::color::{Color, NAMED_COLORS};
use mss_lang::properties::{self, ValueKind};
use mss_lang::syntax::TokenKind;

use crate::values::{calls, parse_color, value_tokens};
use crate::{Doc, Range};

/// Цветовой литерал: место и цвет (sRGB, альфа 0–255).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorInfo {
    pub range: Range,
    pub color: Color,
}

/// Где имя цвета (`red`) — действительно цвет, а не ключевое слово.
fn names_are_colors(property: &str) -> bool {
    if property.starts_with("--") {
        return true;
    }
    match properties::property(property) {
        None => false,
        Some(p) => matches!(
            p.kind,
            ValueKind::Color
                | ValueKind::Background
                | ValueKind::Shadow
                | ValueKind::TextShadow
                | ValueKind::Border
                | ValueKind::Colors
                | ValueKind::Any
        ),
    }
}

/// Все цветовые литералы: `#hex`, `rgb()`, `rgba()`, `darken()`,
/// `lighten()` (если внутри нет `var()`), имена цветов.
pub fn document_colors(text: &str) -> Vec<ColorInfo> {
    let doc = Doc::new(text);
    let mut out = Vec::new();
    for d in doc.sheet.all_declarations() {
        let toks = value_tokens(&doc.sheet.tokens, d);
        let mut covered: Vec<std::ops::Range<usize>> = Vec::new();
        for c in calls(text, &toks) {
            if c.parent.is_some()
                || !matches!(c.name.as_str(), "rgb" | "rgba" | "darken" | "lighten")
                || c.end > toks.len()
            {
                continue;
            }
            let span = toks[c.start].span.start..toks[c.end - 1].span.end;
            covered.push(c.start..c.end);
            if let Some(color) = parse_color(&text[span.clone()]) {
                out.push(ColorInfo {
                    range: doc.range(&span),
                    color,
                });
            }
        }
        let names = names_are_colors(&d.name);
        for (k, t) in toks.iter().enumerate() {
            if covered.iter().any(|r| r.contains(&k)) {
                continue;
            }
            let s = &text[t.span.clone()];
            let color = match t.kind {
                TokenKind::Hash => Color::parse(s),
                TokenKind::Ident if names => {
                    let lower = s.to_lowercase();
                    NAMED_COLORS
                        .iter()
                        .find(|(n, _)| *n == lower)
                        .map(|(_, c)| *c)
                }
                _ => None,
            };
            if let Some(color) = color {
                out.push(ColorInfo {
                    range: doc.range(&t.span),
                    color,
                });
            }
        }
    }
    out.sort_by_key(|c| c.range.start);
    out
}

/// Варианты записи цвета (для выбора цвета в редакторе): `#hex` и `rgba()`.
pub fn color_presentations(c: Color) -> Vec<String> {
    let alpha = (c.a as f32 / 255.0 * 100.0).round() / 100.0;
    vec![
        c.to_hex(),
        format!("rgba({}, {}, {}, {})", c.r, c.g, c.b, alpha),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Position;

    #[test]
    fn finds_literals() {
        let src = ":root { --a: #ff0000; }\n.x { background: linear-gradient(90deg, #000, white); color: rgba(0, 0, 255, 0.5); border-color: darken(#ffffff, 0.5); transition: color 1s; }\n.y { color: rgba(var(--a), 1); text-align: left; }";
        let c = document_colors(src);
        let got: Vec<_> = c
            .iter()
            .map(|c| (c.range.start, c.color.to_hex()))
            .collect();
        assert_eq!(
            got,
            vec![
                (Position::new(0, 13), "#ff0000".to_string()),
                (Position::new(1, 40), "#000000".to_string()),
                (Position::new(1, 46), "#ffffff".to_string()),
                (Position::new(1, 61), "#0000ff7f".to_string()),
                (Position::new(1, 97), "#7f7f7f".to_string()),
            ]
        );
        assert_eq!(c[3].range.end, Position::new(1, 81));
    }

    #[test]
    fn presentations() {
        assert_eq!(
            color_presentations(Color::rgba(255, 0, 0, 128)),
            ["#ff000080", "rgba(255, 0, 0, 0.5)"]
        );
    }
}
