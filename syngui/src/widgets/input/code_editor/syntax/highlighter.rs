use super::capture_names::TokenClass;
use super::highlight_cache::{LineSpans, Span};
use super::language::Language;
use synoptic::TokOpt;

pub struct Highlighter {
    inner: synoptic::Highlighter,
    lines: Vec<String>,
    language: Language,
    tab_width: usize,
}

impl std::fmt::Debug for Highlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Highlighter")
            .field("language", &self.language)
            .field("lines", &self.lines.len())
            .field("tab_width", &self.tab_width)
            .finish()
    }
}

impl Highlighter {
    pub fn new(language: Language, tab_width: usize) -> Self {
        let inner = synoptic::from_extension(language.extension(), tab_width)
            .expect("synoptic::from_extension всегда возвращает Some (fallback на пустой Highlighter)");
        Self {
            inner,
            lines: Vec::new(),
            language,
            tab_width,
        }
    }

    pub fn reparse(&mut self, text: &str) {
        self.lines = text.split('\n').map(String::from).collect();
        self.inner.run(&self.lines);
    }

    pub fn highlight_lines(
        &self,
        _text: &str,
        line_range: std::ops::Range<usize>,
    ) -> Vec<LineSpans> {
        let total_lines = self.lines.len();
        let start_line = line_range.start.min(total_lines);
        let end_line = line_range.end.min(total_lines);
        if start_line >= end_line {
            return Vec::new();
        }

        let mut result: Vec<LineSpans> = Vec::with_capacity(end_line - start_line);
        for line_idx in start_line..end_line {
            let line_str = &self.lines[line_idx];
            let toks = self.inner.line(line_idx, line_str);
            result.push(line_spans_from_tokens(&toks));
        }
        result
    }

    pub fn reset(&mut self) {
        self.inner = synoptic::from_extension(self.language.extension(), self.tab_width)
            .expect("synoptic::from_extension всегда возвращает Some");
        self.lines.clear();
    }
}

fn line_spans_from_tokens(toks: &[TokOpt]) -> LineSpans {
    let mut ls = LineSpans::new();
    let mut byte: u32 = 0;
    for tok in toks {
        match tok {
            TokOpt::Some(text, kind) => {
                let len = text.len() as u32;
                if let Some(class) = TokenClass::from_synoptic_kind(kind) {
                    ls.push(Span::new(byte, byte + len, class));
                }
                byte += len;
            }
            TokOpt::None(text) => {
                byte += text.len() as u32;
            }
        }
    }
    ls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_no_highlights() {
        let mut h = Highlighter::new(Language::Rust, 4);
        h.reparse("");
        let spans = h.highlight_lines("", 0..1);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].is_empty());
    }

    #[test]
    fn rust_keyword_highlight() {
        let mut h = Highlighter::new(Language::Rust, 4);
        let text = "fn main() {}\n";
        h.reparse(text);
        let spans = h.highlight_lines(text, 0..1);
        assert_eq!(spans.len(), 1);
        let has_keyword = spans[0]
            .iter()
            .any(|s| s.class == TokenClass::Keyword);
        assert!(
            has_keyword,
            "ожидаем хотя бы один Keyword span в `fn main() {{}}`"
        );
    }

    #[test]
    fn brackets_do_not_crash_any_language() {
        let langs = [
            Language::Rust,
            Language::Json,
            Language::Toml,
            Language::Markdown,
            Language::TypeScript,
            Language::Tsx,
            Language::Python,
        ];
        for lang in langs {
            let mut h = Highlighter::new(lang, 4);
            for t in ["", "[", "[ ", "[ ]", "[ ]\n[ ]"] {
                h.reparse(t);
                let _ = h.highlight_lines(t, 0..h.lines.len());
            }
        }
    }
}
