mod capture_names;
mod highlight_cache;
mod highlighter;
mod language;

pub use capture_names::TokenClass;
pub use highlight_cache::{LineSpans, Span};
pub use highlighter::Highlighter;
pub use language::Language;
