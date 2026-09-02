//! Свойства блока: пер-блочные переопределения стиля и снимок дерева
//! блоков для панели свойств хоста.
//!
//! Переопределения живут в инлайн-атрибутах блока (`{color=#e05 size=22}`),
//! то есть переживают дублирование, перенос и round-trip markdown. Пустое
//! значение = «как в теме»: атрибут просто снимается.

use crate::core::Color;

use super::model::{Attrs, BlockKind, DocBlock};

/// Цвет текста блока (`#rrggbb`).
pub const COLOR: &str = "color";
/// Цвет подложки блока.
pub const BG: &str = "bg";
/// Кегль в пикселях.
pub const SIZE: &str = "size";
/// Начертание: `bold` | `normal`.
pub const WEIGHT: &str = "weight";
/// Выравнивание: `left` | `center` | `right`.
pub const ALIGN: &str = "align";

/// Ключи, которыми управляет панель свойств.
pub const STYLE_KEYS: [&str; 5] = [COLOR, BG, SIZE, WEIGHT, ALIGN];

pub fn color_of(attrs: &Attrs, key: &str) -> Option<Color> {
    let v = attrs.get(key)?;
    (v.starts_with('#') && (v.len() == 7 || v.len() == 9)).then(|| Color::from_hex(v))
}

pub fn size_of(attrs: &Attrs) -> Option<f32> {
    let v = attrs.get(SIZE)?.parse::<f32>().ok()?;
    (6.0..=160.0).contains(&v).then_some(v)
}

pub fn bold_of(attrs: &Attrs) -> Option<bool> {
    match attrs.get(WEIGHT)? {
        "bold" => Some(true),
        "normal" => Some(false),
        _ => None,
    }
}

/// Доля свободного места слева: 0 — влево, 0.5 — по центру, 1 — вправо.
pub fn align_factor(attrs: &Attrs) -> f32 {
    match attrs.get(ALIGN) {
        Some("center") => 0.5,
        Some("right") => 1.0,
        _ => 0.0,
    }
}

/// Применить/снять свойство (`None` — вернуть к теме).
pub fn set(attrs: &mut Attrs, key: &str, value: Option<&str>) {
    match value {
        Some(v) if !v.is_empty() => attrs.set(key, v),
        _ => {
            attrs.remove(key);
        }
    }
}

// ─── Дерево блоков ──────────────────────────────────────────────────────────

/// Операции над таблицей из панели свойств.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableOp {
    AddRow,
    AddColumn,
    DeleteRow,
    DeleteColumn,
}

/// Снимок блока для дерева блоков и панели свойств хоста.
///
/// Нужен потому, что пустой блок в документе не видно: в дереве он всё
/// равно есть строкой со своим типом, его можно выбрать и настроить.
#[derive(Clone, Debug)]
pub struct BlockOutline {
    pub id: super::model::BlockId,
    /// Машинный тип блока: `paragraph`, `heading`, `table`, …
    pub kind: &'static str,
    /// Уровень заголовка (иначе 0).
    pub level: u8,
    /// Короткая подпись строки (текст блока либо его содержимое).
    pub label: String,
    /// Закреплён на холсте свободной раскладки.
    pub pinned: bool,
    pub children: Vec<BlockOutline>,
}

pub fn kind_name(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Paragraph(_) => "paragraph",
        BlockKind::Heading { .. } => "heading",
        BlockKind::Bullet { .. } => "bullet",
        BlockKind::Numbered { .. } => "numbered",
        BlockKind::Todo { .. } => "todo",
        BlockKind::Toggle { .. } => "toggle",
        BlockKind::Quote(_) => "quote",
        BlockKind::Callout { .. } => "callout",
        BlockKind::CodeBlock { .. } => "code",
        BlockKind::Table { .. } => "table",
        BlockKind::Divider => "divider",
        BlockKind::Media { .. } => "media",
        BlockKind::Embed { .. } => "embed",
        BlockKind::Shape { .. } => "shape",
    }
}

/// Подпись блока в дереве: текст, а для блоков без текста — их суть.
pub fn label_of(block: &DocBlock) -> String {
    let text = block.kind.text().map(|t| t.text()).unwrap_or_default();
    let text = text.trim();
    if !text.is_empty() {
        return text.chars().take(60).collect();
    }
    match &block.kind {
        BlockKind::CodeBlock { language, code } => {
            let head = code.lines().next().unwrap_or("").trim();
            match (language.as_deref(), head.is_empty()) {
                (Some(l), true) => l.to_string(),
                (Some(l), false) => format!("{l}: {}", head.chars().take(40).collect::<String>()),
                (None, false) => head.chars().take(50).collect(),
                (None, true) => String::new(),
            }
        }
        BlockKind::Table { headers, rows, .. } => {
            let cols = headers.len();
            let head: Vec<String> =
                headers.iter().map(|h| h.text()).filter(|t| !t.trim().is_empty()).collect();
            if head.is_empty() {
                format!("{}×{}", rows.len() + 1, cols)
            } else {
                head.join(" · ").chars().take(50).collect()
            }
        }
        BlockKind::Media { url, alt, .. } => {
            if alt.trim().is_empty() { url.chars().take(50).collect() } else { alt.clone() }
        }
        BlockKind::Embed { target } => target.clone(),
        BlockKind::Shape { shape } => shape.name().to_string(),
        _ => String::new(),
    }
}

pub fn outline_of(blocks: &[DocBlock]) -> Vec<BlockOutline> {
    blocks
        .iter()
        .map(|b| BlockOutline {
            id: b.id,
            kind: kind_name(&b.kind),
            level: match &b.kind {
                BlockKind::Heading { level, .. } => *level,
                _ => 0,
            },
            label: label_of(b),
            pinned: super::free::pos_of(&b.attrs).is_some(),
            children: b.kind.children().map(outline_of).unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::input::document_editor::model::{BlockId, InlineText};

    #[test]
    fn style_attrs_roundtrip() {
        let mut attrs = Attrs::default();
        set(&mut attrs, COLOR, Some("#ff8800"));
        set(&mut attrs, SIZE, Some("24"));
        set(&mut attrs, WEIGHT, Some("bold"));
        assert!(color_of(&attrs, COLOR).is_some());
        assert_eq!(size_of(&attrs), Some(24.0));
        assert_eq!(bold_of(&attrs), Some(true));
        set(&mut attrs, COLOR, None);
        assert!(color_of(&attrs, COLOR).is_none());
        // Мусор в значении не должен становиться цветом или кеглем.
        attrs.set(COLOR, "красный");
        attrs.set(SIZE, "огромный");
        assert!(color_of(&attrs, COLOR).is_none());
        assert_eq!(size_of(&attrs), None);
    }

    #[test]
    fn outline_shows_empty_blocks() {
        let blocks = vec![
            DocBlock::new(BlockId(1), BlockKind::Paragraph(InlineText::default())),
            DocBlock::new(BlockId(2), BlockKind::Divider),
            DocBlock::new(
                BlockId(3),
                BlockKind::Table {
                    headers: vec![InlineText::default(), InlineText::default()],
                    rows: vec![vec![InlineText::default(), InlineText::default()]],
                    aligns: vec![Default::default(), Default::default()],
                },
            ),
        ];
        let out = outline_of(&blocks);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].kind, "paragraph");
        assert!(out[0].label.is_empty(), "пустой блок всё равно есть строкой");
        assert_eq!(out[1].kind, "divider");
        assert_eq!(out[2].label, "2×2");
    }
}
