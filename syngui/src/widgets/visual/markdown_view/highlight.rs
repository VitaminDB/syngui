use crate::core::Color;
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct HighlightToken {
    pub range: Range<usize>,
    pub color: Color,
}

pub trait CodeHighlighter: Send + Sync {
    fn highlight(&self, code: &str, language: Option<&str>) -> Vec<HighlightToken>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoHighlight;

impl CodeHighlighter for NoHighlight {
    fn highlight(&self, _code: &str, _language: Option<&str>) -> Vec<HighlightToken> {
        Vec::new()
    }
}

#[cfg(feature = "markdown-syntax")]
pub use self::syntect_impl::SyntectHighlighter;

#[cfg(feature = "markdown-syntax")]
mod syntect_impl {
    use super::*;
    use std::sync::OnceLock;
    use syntect::easy::HighlightLines;
    use syntect::highlighting::{Style, Theme, ThemeSet};
    use syntect::parsing::{SyntaxReference, SyntaxSet};
    use syntect::util::LinesWithEndings;

    fn syntax_set() -> &'static SyntaxSet {
        static SET: OnceLock<SyntaxSet> = OnceLock::new();
        SET.get_or_init(SyntaxSet::load_defaults_newlines)
    }

    fn theme_set() -> &'static ThemeSet {
        static SET: OnceLock<ThemeSet> = OnceLock::new();
        SET.get_or_init(ThemeSet::load_defaults)
    }

    pub struct SyntectHighlighter {
        theme: &'static Theme,
    }

    impl SyntectHighlighter {
        pub fn new() -> Self {
            Self::with_theme("base16-ocean.dark")
        }

        pub fn with_theme(name: &str) -> Self {
            let ts = theme_set();
            let theme: &'static Theme = ts
                .themes
                .get(name)
                .or_else(|| ts.themes.get("base16-ocean.dark"))
                .or_else(|| ts.themes.values().next())
                .expect("syntect ships with at least one theme");
            Self { theme }
        }

        fn pick_syntax<'a>(
            &self,
            ss: &'a SyntaxSet,
            language: Option<&str>,
        ) -> &'a SyntaxReference {
            if let Some(lang) = language {
                if let Some(s) = ss.find_syntax_by_token(lang) {
                    return s;
                }
                let lc = lang.to_ascii_lowercase();
                let canonical = match lc.as_str() {
                    "ts" | "typescript" | "tsx" => "JavaScript",
                    "sh" | "bash" | "zsh" => "Shell-Unix-Generic",
                    "yml" => "YAML",
                    "rs" => "Rust",
                    "cpp" | "c++" | "cc" | "hpp" => "C++",
                    "py" => "Python",
                    "kt" => "Kotlin",
                    _ => lc.as_str(),
                };
                if let Some(s) = ss.find_syntax_by_name(canonical) {
                    return s;
                }
            }
            ss.find_syntax_plain_text()
        }
    }

    impl Default for SyntectHighlighter {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CodeHighlighter for SyntectHighlighter {
        fn highlight(&self, code: &str, language: Option<&str>) -> Vec<HighlightToken> {
            let ss = syntax_set();
            let syntax = self.pick_syntax(ss, language);
            let mut h = HighlightLines::new(syntax, self.theme);

            let mut tokens = Vec::new();
            let mut byte_offset: usize = 0;

            for line in LinesWithEndings::from(code) {
                let regions = match h.highlight_line(line, ss) {
                    Ok(r) => r,
                    Err(e) => {
                        log::warn!("syntect highlight failed: {e}");
                        return tokens;
                    }
                };
                for (style, frag) in regions {
                    let len = frag.len();
                    if len > 0 {
                        tokens.push(HighlightToken {
                            range: byte_offset..byte_offset + len,
                            color: style_to_color(style),
                        });
                    }
                    byte_offset += len;
                }
            }
            tokens
        }
    }

    fn style_to_color(style: Style) -> Color {
        let c = style.foreground;
        Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0)
    }
}
