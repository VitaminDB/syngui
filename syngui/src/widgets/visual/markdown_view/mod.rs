mod anchors;
mod highlight;
mod model;
mod parser;
mod plain_text;
mod renderer;
mod resolve;
mod selection_map;
mod widget;

pub use highlight::{CodeHighlighter, HighlightToken, NoHighlight};
#[cfg(feature = "markdown-syntax")]
pub use highlight::SyntectHighlighter;
// Модель и парсер публичны для интеграционных тестов и диагностики того,
// во что реально разбирается конкретный текст.
pub use model::{MdBlock, MdInline};
pub use parser::parse_markdown;
pub use plain_text::{linearize as linearize_markdown_blocks, linearize_markdown_source};
pub use renderer::MdStyle;
pub use widget::MarkdownView;
