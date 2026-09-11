//! DocumentEditor — WYSIWYG-редактор блочных документов (Notion-стиль).
//!
//! Слои модуля:
//! - [`model`] — блочная модель документа (плоские стилевые раны, runtime-id);
//! - [`attrs`] — инлайн-атрибуты `{key=value flag}`;
//! - [`free`] — свободная раскладка: координаты блоков, привязка, сетка;
//! - [`props`] — свойства блока (цвет, кегль, выравнивание) и дерево блоков;
//! - [`shape`] — векторные примитивы (прямоугольник, овал, линия, стрелка);
//! - [`parse`] / [`serialize`] — markdown ↔ модель с расширениями:
//!   `[[wiki-ссылки]]`, `![[врезки]]`, callout/toggle-цитаты, медиа-блоки,
//!   инлайн-атрибуты.
//!
//! Сам виджет редактора добавляется следующими этапами; модель и конвертация
//! самодостаточны и покрыты round-trip корпусом в tests/document_roundtrip.rs.

pub mod attrs;
mod build;
mod chrome;
pub mod edit;
pub mod free;
pub mod history;
pub mod linebox;
pub mod links;
#[cfg(feature = "ffmpeg")]
pub mod media_block;
pub mod model;
pub mod parse;
pub mod props;
pub mod rows;
pub mod serialize;
pub mod shape;
pub mod shortcuts;
pub mod slash;
pub mod state;
pub mod style;
mod widget;

pub use free::{DocGrid, DocLayout};
pub use links::{
    DocLinkProvider, DocMediaResolver, EmbedCtx, EmbedFactory, LinkCandidate, ResolvedMedia,
};
pub use model::{
    Attrs, BlockId, BlockKind, DocAlign, DocBlock, DocModel, InlineRun, InlineStyle, InlineText,
    LinkTarget, MediaKind, ShapeKind,
};
pub use parse::parse_document;
pub use props::{BlockOutline, TableOp};
pub use serialize::serialize_document;
pub use shape::ShapeStyle;
pub use slash::{SlashAction, SlashItem};
pub use style::DocStyle;
pub use widget::{BlockProps, ClipboardKey, DocOp, DocumentEditor, DocumentEditorHandle};
