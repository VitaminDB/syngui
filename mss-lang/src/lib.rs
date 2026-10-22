//! Язык стилей MSS без графики.
//!
//! - [`syntax`] — толерантный разбор без потерь со span'ами для инструментов
//!   (редактор тем, `mss-analysis`, `mss-lsp`);
//! - [`properties`] — реестр свойств, функций, ключевых слов, псевдоклассов;
//! - [`color`] — цвет MSS и его разбор.
//!
//! syngui реэкспортирует модули как `syngui::mss::…`.

pub mod color;
pub mod properties;
pub mod syntax;
