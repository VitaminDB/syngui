mod anchors;
mod highlight;
mod model;
mod parser;
mod plain_text;
mod renderer;
mod resolve;
mod selection_map;
mod widget;

#[cfg(feature = "markdown-syntax")]
pub use highlight::SyntectHighlighter;
pub use highlight::{CodeHighlighter, HighlightToken, NoHighlight};
// Кэш подсветки: `highlight_cached` — вход для своего рендерера,
// остальное — счётчики и сброс (освободить память, когда лента закрыта).
pub use highlight::{
    clear_highlight_caches, guess_cache_stats, highlight_cache_stats, highlight_cached,
    HighlightCacheStats,
};
// Модель и парсер публичны для интеграционных тестов и диагностики того,
// во что реально разбирается конкретный текст.
pub use model::{MdBlock, MdInline};
pub use parser::parse_markdown;
pub use plain_text::{linearize as linearize_markdown_blocks, linearize_markdown_source};
pub use renderer::MdStyle;
pub use widget::MarkdownView;
