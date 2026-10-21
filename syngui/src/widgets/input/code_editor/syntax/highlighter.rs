use super::capture_names::TokenClass;
use super::highlight_cache::{LineSpans, Span};
use super::language::Language;
use synoptic::TokOpt;

/// Своя подсветка для языков, которых нет среди встроенных [`Language`]
/// (ассемблер, псевдокод, DSL приложения). Построчная: `state` — состояние
/// лексера в начале строки (0 — обычное; например, «внутри /* … */»),
/// возвращается состояние в конце строки.
pub trait LineHighlighter: Send + Sync {
    fn highlight_line(&self, line: &str, state: u32, out: &mut LineSpans) -> u32;
}

enum Backend {
    Synoptic(synoptic::Highlighter),
    /// Своя подсветка и состояние лексера в начале каждой строки.
    Custom(std::sync::Arc<dyn LineHighlighter>, Vec<u32>),
}

pub struct Highlighter {
    inner: Backend,
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
        // у synoptic инлайновые правила markdown многострочные: одиночная `*`
        // или `` ` `` красила текст до следующей такой же через весь файл
        if language == Language::Markdown {
            return Self {
                inner: Backend::Custom(std::sync::Arc::new(super::markdown::MarkdownHighlighter), Vec::new()),
                lines: Vec::new(),
                language,
                tab_width,
            };
        }
        let inner = synoptic::from_extension(language.extension(), tab_width).expect(
            "synoptic::from_extension всегда возвращает Some (fallback на пустой Highlighter)",
        );
        Self {
            inner: Backend::Synoptic(inner),
            lines: Vec::new(),
            language,
            tab_width,
        }
    }

    /// Подсветка своим [`LineHighlighter`].
    pub fn custom(h: std::sync::Arc<dyn LineHighlighter>) -> Self {
        Self { inner: Backend::Custom(h, Vec::new()), lines: Vec::new(), language: Language::Rust, tab_width: 4 }
    }

    pub fn reparse(&mut self, text: &str) {
        self.lines = text.split('\n').map(String::from).collect();
        match &mut self.inner {
            Backend::Synoptic(inner) => inner.run(&self.lines),
            Backend::Custom(h, states) => {
                states.clear();
                let mut state = 0;
                let mut scratch = LineSpans::new();
                for line in &self.lines {
                    states.push(state);
                    scratch.spans.clear();
                    state = h.highlight_line(line, state, &mut scratch);
                }
            }
        }
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
            match &self.inner {
                Backend::Synoptic(inner) => {
                    let toks = inner.line(line_idx, line_str);
                    result.push(line_spans_from_tokens(&toks));
                }
                Backend::Custom(h, states) => {
                    let mut ls = LineSpans::new();
                    h.highlight_line(line_str, states.get(line_idx).copied().unwrap_or(0), &mut ls);
                    ls.sort_by_start();
                    result.push(ls);
                }
            }
        }
        result
    }

    pub fn reset(&mut self) {
        match &mut self.inner {
            Backend::Synoptic(inner) => {
                *inner = synoptic::from_extension(self.language.extension(), self.tab_width)
                    .expect("synoptic::from_extension всегда возвращает Some");
            }
            Backend::Custom(_, states) => states.clear(),
        }
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
        let has_keyword = spans[0].iter().any(|s| s.class == TokenClass::Keyword);
        assert!(
            has_keyword,
            "ожидаем хотя бы один Keyword span в `fn main() {{}}`"
        );
    }

    struct Hash;

    /// `#` до конца строки — комментарий; `{` открывает блок-комментарий до `}`.
    impl LineHighlighter for Hash {
        fn highlight_line(&self, line: &str, state: u32, out: &mut LineSpans) -> u32 {
            if state == 1 {
                let end = line.find('}').map(|i| i + 1);
                out.push(Span::new(0, end.unwrap_or(line.len()) as u32, TokenClass::Comment));
                return if end.is_some() { 0 } else { 1 };
            }
            if let Some(i) = line.find('#') {
                out.push(Span::new(i as u32, line.len() as u32, TokenClass::Comment));
            }
            if let Some(i) = line.find('{') {
                out.push(Span::new(i as u32, line.len() as u32, TokenClass::Comment));
                return 1;
            }
            0
        }
    }

    #[test]
    fn custom_highlighter_carries_state() {
        let mut h = Highlighter::custom(std::sync::Arc::new(Hash));
        let text = "mov r0 # c\na {\nb\n} c";
        h.reparse(text);
        let spans = h.highlight_lines(text, 0..4);
        assert_eq!(spans[0].spans[0].byte_start, 7);
        assert_eq!(spans[2].spans[0].class, TokenClass::Comment);
        assert_eq!(spans[3].spans[0].byte_end, 1);
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
