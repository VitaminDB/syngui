//! Модель блочного документа для DocumentEditor.
//!
//! В отличие от `MdBlock` из markdown_view (read-only, вложенные инлайны),
//! модель редактора построена вокруг правок:
//! - у каждого блока стабильный runtime-id (undo, реконсиляция, виртуализация);
//! - инлайны — плоские стилевые раны: каретка = байтовое смещение в
//!   конкатенации ранов, переключение стиля = split/merge ранов;
//! - списки — не контейнеры, а самостоятельные блоки с детьми (Enter = split,
//!   Tab = reparent, как в Notion); группировку соседних элементов в один
//!   md-список выполняет сериализатор.

use std::collections::BTreeMap;

/// Runtime-идентификатор блока. Не сериализуется: раздаётся заново при каждом
/// парсе, но стабилен на всё время жизни модели в памяти.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u64);

/// Документ целиком: плоский список верхнеуровневых блоков.
#[derive(Clone, Debug, Default)]
pub struct DocModel {
    pub blocks: Vec<DocBlock>,
    next_id: u64,
}

impl DocModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_blocks(blocks: Vec<DocBlock>, next_id: u64) -> Self {
        Self { blocks, next_id }
    }

    /// Выдаёт следующий свободный id блока.
    pub fn alloc_id(&mut self) -> BlockId {
        let id = BlockId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Обходит все блоки дерева (включая детей) в предзаказе.
    pub fn for_each(&self, f: &mut impl FnMut(&DocBlock)) {
        fn walk(blocks: &[DocBlock], f: &mut impl FnMut(&DocBlock)) {
            for b in blocks {
                f(b);
                if let Some(children) = b.kind.children() {
                    walk(children, f);
                }
            }
        }
        walk(&self.blocks, f);
    }
}

/// Один блок документа: тип + произвольные атрибуты (`{key=value}` в md).
#[derive(Clone, Debug, PartialEq)]
pub struct DocBlock {
    pub id: BlockId,
    pub kind: BlockKind,
    pub attrs: Attrs,
}

impl DocBlock {
    pub fn new(id: BlockId, kind: BlockKind) -> Self {
        Self { id, kind, attrs: Attrs::default() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlockKind {
    Paragraph(InlineText),
    Heading {
        level: u8,
        text: InlineText,
    },
    /// Элемент маркированного списка.
    Bullet {
        text: InlineText,
        children: Vec<DocBlock>,
    },
    /// Элемент нумерованного списка. `number` — фактический номер; парсер
    /// всегда нумерует последовательно от start списка, так что round-trip
    /// стабилен со второго прохода.
    Numbered {
        number: u64,
        text: InlineText,
        children: Vec<DocBlock>,
    },
    /// Пункт чек-листа.
    Todo {
        checked: bool,
        text: InlineText,
        children: Vec<DocBlock>,
    },
    /// Сворачиваемый блок; в md — callout `> [!toggle]`.
    Toggle {
        summary: InlineText,
        children: Vec<DocBlock>,
        collapsed: bool,
    },
    Quote(Vec<DocBlock>),
    /// Callout Obsidian-стиля: `> [!warning]{color=#e0a030} Заголовок`.
    Callout {
        kind: String,
        title: InlineText,
        children: Vec<DocBlock>,
    },
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    /// Простая md-таблица в тексте (не «база данных»).
    Table {
        headers: Vec<InlineText>,
        rows: Vec<Vec<InlineText>>,
        aligns: Vec<DocAlign>,
    },
    Divider,
    /// Медиа-блок: `![alt](url){attrs}`. В Notion-модели картинка/видео —
    /// всегда блок; инлайн-картинки при парсе раскалывают параграф.
    Media {
        media: MediaKind,
        url: String,
        alt: String,
    },
    /// Живая врезка другой страницы/базы/канваса: `![[Имя]]`.
    Embed {
        target: String,
    },
}

impl BlockKind {
    /// Дети блока, если тип их поддерживает.
    pub fn children(&self) -> Option<&[DocBlock]> {
        match self {
            BlockKind::Bullet { children, .. }
            | BlockKind::Numbered { children, .. }
            | BlockKind::Todo { children, .. }
            | BlockKind::Toggle { children, .. }
            | BlockKind::Callout { children, .. } => Some(children),
            BlockKind::Quote(children) => Some(children),
            _ => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<DocBlock>> {
        match self {
            BlockKind::Bullet { children, .. }
            | BlockKind::Numbered { children, .. }
            | BlockKind::Todo { children, .. }
            | BlockKind::Toggle { children, .. }
            | BlockKind::Callout { children, .. } => Some(children),
            BlockKind::Quote(children) => Some(children),
            _ => None,
        }
    }

    /// Основной редактируемый текст блока, если он есть.
    pub fn text(&self) -> Option<&InlineText> {
        match self {
            BlockKind::Paragraph(text)
            | BlockKind::Heading { text, .. }
            | BlockKind::Bullet { text, .. }
            | BlockKind::Numbered { text, .. }
            | BlockKind::Todo { text, .. } => Some(text),
            BlockKind::Toggle { summary, .. } => Some(summary),
            BlockKind::Callout { title, .. } => Some(title),
            _ => None,
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut InlineText> {
        match self {
            BlockKind::Paragraph(text)
            | BlockKind::Heading { text, .. }
            | BlockKind::Bullet { text, .. }
            | BlockKind::Numbered { text, .. }
            | BlockKind::Todo { text, .. } => Some(text),
            BlockKind::Toggle { summary, .. } => Some(summary),
            BlockKind::Callout { title, .. } => Some(title),
            _ => None,
        }
    }

    /// Элемент списка (для правил «между соседями списка нет пустой строки»).
    pub fn is_list_item(&self) -> bool {
        matches!(
            self,
            BlockKind::Bullet { .. } | BlockKind::Numbered { .. } | BlockKind::Todo { .. }
        )
    }
}

/// Выравнивание колонок md-таблицы.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Тип медиа-блока. В md не кодируется: выводится заново из расширения url
/// либо переопределяется атрибутом `kind=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    File,
}

impl MediaKind {
    /// Определяет тип по url (расширению) с учётом переопределения в attrs.
    pub fn detect(url: &str, attrs: &Attrs) -> Self {
        match attrs.get("kind") {
            Some("image") => return MediaKind::Image,
            Some("video") => return MediaKind::Video,
            Some("audio") => return MediaKind::Audio,
            Some("file") => return MediaKind::File,
            _ => {}
        }
        // Хвост после последней точки; query/fragment у blob: и файловых
        // ссылок не встречаются, для http обрезаем на всякий случай.
        let tail = url.split(['?', '#']).next().unwrap_or(url);
        let ext = tail.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" => MediaKind::Image,
            "mp4" | "webm" | "mkv" | "mov" | "avi" | "m4v" => MediaKind::Video,
            "mp3" | "wav" | "flac" | "ogg" | "opus" | "m4a" | "aac" => MediaKind::Audio,
            _ if tail.contains('.') => MediaKind::File,
            // Без расширения (голый http-url) считаем картинкой — самый
            // частый случай `![](https://…)`.
            _ => MediaKind::Image,
        }
    }
}

/// Текст блока: плоская последовательность стилевых ранов.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineText(pub Vec<InlineRun>);

impl InlineText {
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        if text.is_empty() {
            Self::default()
        } else {
            Self(vec![InlineRun { text, style: InlineStyle::default() }])
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|r| r.text.is_empty())
    }

    /// Конкатенация текста всех ранов.
    pub fn text(&self) -> String {
        self.0.iter().map(|r| r.text.as_str()).collect()
    }

    pub fn len_bytes(&self) -> usize {
        self.0.iter().map(|r| r.text.len()).sum()
    }

    /// Убирает пустые раны и сливает соседние с одинаковым стилем.
    pub fn normalize(&mut self) {
        let runs = std::mem::take(&mut self.0);
        for run in runs {
            if run.text.is_empty() {
                continue;
            }
            match self.0.last_mut() {
                Some(last) if last.style == run.style => last.text.push_str(&run.text),
                _ => self.0.push(run),
            }
        }
    }

    pub fn push_run(&mut self, text: impl Into<String>, style: InlineStyle) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        match self.0.last_mut() {
            Some(last) if last.style == style => last.text.push_str(&text),
            _ => self.0.push(InlineRun { text, style }),
        }
    }
}

/// Непрерывный кусок текста с единым стилем.
#[derive(Clone, Debug, PartialEq)]
pub struct InlineRun {
    pub text: String,
    pub style: InlineStyle,
}

/// Стиль рана. `Eq` нужен для merge при нормализации.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<LinkTarget>,
}

impl InlineStyle {
    /// Ран без какого-либо оформления.
    pub fn plain(&self) -> bool {
        !self.bold && !self.italic && !self.strike && !self.code && self.link.is_none()
    }
}

/// Куда ведёт ссылка. Для wiki-ссылки отображаемый текст живёт в самом ране:
/// `[[target|алиас]]` → run.text = "алиас", target здесь.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    Url(String),
    Wiki { target: String },
}

/// Инлайн-атрибуты элемента: `{width=70% align=center loop}`.
/// Флаги хранятся как ключи с пустым значением. BTreeMap — детерминированный
/// порядок сериализации.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attrs(pub BTreeMap<String, String>);

impl Attrs {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    /// Присутствует ли флаг (ключ без значения либо со значением).
    pub fn flag(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.0.insert(key.into(), value.into());
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.0.remove(key)
    }
}
