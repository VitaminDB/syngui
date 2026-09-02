//! Slash-меню «/»: каталог вставляемых блоков.
//!
//! Открывается вводом `/` в начале пустого параграфа или после пробела;
//! набор после `/` фильтрует список (текст печатается и в документ — как
//! в Notion, отмена оставляет его). Пункты по умолчанию можно дополнить
//! или заменить через `DocumentEditor::slash_items` (хост локализует
//! подписи и добавляет свои действия — «База», «Канвас» и т.п.).

/// Действие пункта меню.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlashAction {
    Paragraph,
    Heading(u8),
    Bullet,
    Numbered,
    Todo,
    Toggle,
    Quote,
    Callout,
    CodeBlock,
    Divider,
    Table,
    /// Векторный примитив (см. [`super::shape`]).
    Shape(super::model::ShapeKind),
    /// Кастомное действие хоста: id уходит в колбэк `on_slash_custom`.
    Custom(String),
}

#[derive(Clone, Debug)]
pub struct SlashItem {
    pub action: SlashAction,
    pub label: String,
    /// Дополнительные ключевые слова для фильтра («h1», «code»...).
    pub keywords: String,
}

impl SlashItem {
    pub fn new(action: SlashAction, label: impl Into<String>, keywords: impl Into<String>) -> Self {
        Self { action, label: label.into(), keywords: keywords.into() }
    }
}

/// Каталог по умолчанию (подписи на английском — хост переопределяет).
pub fn default_items() -> Vec<SlashItem> {
    use super::model::ShapeKind;
    use SlashAction::*;
    vec![
        SlashItem::new(Paragraph, "Text", "text paragraph plain"),
        SlashItem::new(Heading(1), "Heading 1", "h1 title"),
        SlashItem::new(Heading(2), "Heading 2", "h2"),
        SlashItem::new(Heading(3), "Heading 3", "h3"),
        SlashItem::new(Bullet, "Bulleted list", "ul bullet item"),
        SlashItem::new(Numbered, "Numbered list", "ol ordered"),
        SlashItem::new(Todo, "To-do", "todo task checkbox"),
        SlashItem::new(Toggle, "Toggle", "toggle collapse fold"),
        SlashItem::new(Quote, "Quote", "quote blockquote"),
        SlashItem::new(Callout, "Callout", "callout note warning info"),
        SlashItem::new(CodeBlock, "Code", "code fence snippet"),
        SlashItem::new(Divider, "Divider", "divider hr separator line"),
        SlashItem::new(Table, "Table", "table grid"),
        SlashItem::new(Shape(ShapeKind::Rect), "Rectangle", "rect shape box"),
        SlashItem::new(Shape(ShapeKind::Ellipse), "Ellipse", "ellipse circle oval shape"),
        SlashItem::new(Shape(ShapeKind::Line), "Line", "line shape"),
        SlashItem::new(Shape(ShapeKind::Arrow), "Arrow", "arrow shape"),
    ]
}

/// Состояние открытого меню.
#[derive(Clone, Debug)]
pub struct SlashState {
    pub block: super::model::BlockId,
    /// Смещение символа `/` в тексте блока.
    pub start: usize,
    pub query: String,
    pub selected: usize,
}

/// Фильтрация: подстрока по label+keywords без учёта регистра.
pub fn filter_items<'a>(items: &'a [SlashItem], query: &str) -> Vec<&'a SlashItem> {
    let q = query.to_lowercase();
    items
        .iter()
        .filter(|it| {
            q.is_empty()
                || it.label.to_lowercase().contains(&q)
                || it.keywords.to_lowercase().contains(&q)
        })
        .collect()
}
