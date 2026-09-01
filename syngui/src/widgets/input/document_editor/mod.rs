//! DocumentEditor — WYSIWYG-редактор блочных документов (Notion-стиль).
//!
//! Слои модуля:
//! - [`model`] — блочная модель документа (плоские стилевые раны, runtime-id);
//! - [`attrs`] — инлайн-атрибуты `{key=value flag}`;
//! - [`parse`] / [`serialize`] — markdown ↔ модель с расширениями:
//!   `[[wiki-ссылки]]`, `![[врезки]]`, callout/toggle-цитаты, медиа-блоки,
//!   инлайн-атрибуты.
//!
//! Сам виджет редактора добавляется следующими этапами; модель и конвертация
//! самодостаточны и покрыты round-trip корпусом в tests/document_roundtrip.rs.

pub mod attrs;
mod build;
mod chrome;
pub mod linebox;
pub mod model;
pub mod parse;
pub mod rows;
pub mod serialize;
pub mod style;
mod widget;

pub use model::{
    Attrs, BlockId, BlockKind, DocAlign, DocBlock, DocModel, InlineRun, InlineStyle, InlineText,
    LinkTarget, MediaKind,
};
pub use parse::parse_document;
pub use serialize::serialize_document;
pub use style::DocStyle;
pub use widget::DocumentEditor;
