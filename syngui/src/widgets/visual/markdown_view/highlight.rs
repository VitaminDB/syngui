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

/// Угадать язык по тексту кода — для блоков без метки языка (отступный код
/// или ``` без info-строки): модели и люди часто её не ставят, и код
/// оставался без подсветки. Только уверенные совпадения: сумма весов
/// характерных маркеров не ниже порога и с отрывом от второго кандидата;
/// иначе `None` — блок остаётся обычным текстом.
pub fn guess_language(code: &str) -> Option<&'static str> {
    const MIN_SCORE: u32 = 6;
    const MIN_MARGIN: u32 = 3;
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = trimmed.to_ascii_uppercase();
    let lines: Vec<&str> = trimmed.lines().map(str::trim).collect();
    let mut scores: Vec<(&'static str, u32)> = LANGUAGE_MARKERS
        .iter()
        .map(|(lang, markers)| {
            let score = markers
                .iter()
                .map(|(m, weight)| m.count(trimmed, &upper, &lines).min(3) * weight)
                .sum::<u32>();
            (*lang, score)
        })
        .collect();
    scores.sort_by(|a, b| b.1.cmp(&a.1));
    let (best, best_score) = scores[0];
    let second = scores.get(1).map(|s| s.1).unwrap_or(0);
    (best_score >= MIN_SCORE && best_score - second >= MIN_MARGIN).then_some(best)
}

/// Маркер языка: где искать подстроку.
enum Marker {
    /// Где угодно в коде.
    Has(&'static str),
    /// Где угодно, без учёта регистра (SQL).
    HasCi(&'static str),
    /// В начале строки (после отступа).
    Starts(&'static str),
    /// В конце строки.
    Ends(&'static str),
    /// В самом начале блока.
    Doc(&'static str),
}

impl Marker {
    fn count(&self, code: &str, upper: &str, lines: &[&str]) -> u32 {
        match self {
            Marker::Has(s) => code.matches(s).count() as u32,
            Marker::HasCi(s) => upper.matches(s).count() as u32,
            Marker::Starts(s) => lines.iter().filter(|l| l.starts_with(s)).count() as u32,
            Marker::Ends(s) => lines.iter().filter(|l| l.ends_with(s)).count() as u32,
            Marker::Doc(s) => code.starts_with(s) as u32,
        }
    }
}

use Marker::{Doc, Ends, Has, HasCi, Starts};

/// Языки и их маркеры с весами. Имена — токены syntect (см. `pick_syntax`).
/// Вес 1 — общий для нескольких языков признак, 3–5 — почти однозначный.
const LANGUAGE_MARKERS: &[(&str, &[(Marker, u32)])] = &[
    (
        "rust",
        &[
            (Has("fn "), 3),
            (Has("let mut "), 3),
            (Has("println!"), 4),
            (Has("&str"), 3),
            (Has("impl "), 3),
            (Has("pub fn"), 3),
            (Has("match "), 2),
            (Has("Some("), 2),
            (Has("Option<"), 3),
            (Has("Vec<"), 3),
            (Has("#[derive"), 4),
            (Has("&self"), 3),
            (Has(".unwrap()"), 3),
            (Has("use std"), 4),
            (Has("&mut "), 3),
            (Has("-> "), 1),
            (Has("::"), 1),
            (Has("let "), 1),
            (Has("struct "), 1),
            (Has("enum "), 1),
            (Has("String"), 1),
            (Has("Result<"), 3),
            (Has("Ok("), 2),
            (Has("Err("), 2),
        ],
    ),
    (
        "python",
        &[
            (Starts("def "), 3),
            (Starts("import "), 2),
            (Starts("from "), 1),
            (Has("print("), 3),
            (Has("self."), 2),
            (Starts("elif "), 3),
            (Has("__init__"), 4),
            (Starts("class "), 1),
            (Ends(":"), 1),
            (Has("lambda "), 2),
            (Has(" is not "), 2),
            (Has("range("), 2),
            (Has("len("), 1),
            (Starts("if __name__"), 5),
            (Has("None"), 1),
            (Has("True"), 1),
            (Has("False"), 1),
            (Starts("return "), 1),
            (Has(" in "), 1),
        ],
    ),
    (
        "javascript",
        &[
            (Has("const "), 2),
            (Has("function "), 3),
            (Has("=>"), 2),
            (Has("console.log"), 4),
            (Has("var "), 2),
            (Has("==="), 3),
            (Has("!=="), 3),
            (Has("document."), 3),
            (Has("require("), 3),
            (Starts("export "), 2),
            (Has("async "), 1),
            (Has("await "), 1),
            (Has("let "), 1),
            (Has("null"), 1),
            (Has("undefined"), 3),
            (Has("this."), 1),
            (Has(".then("), 2),
            (Has("typeof "), 2),
            (Has("new "), 1),
            (Has("interface "), 2),
            (Has(": string"), 2),
            (Has(": number"), 2),
            (Has("import "), 1),
        ],
    ),
    (
        "json",
        &[
            (Doc("{"), 3),
            (Doc("["), 2),
            (Has("\": "), 2),
            (Has("\":"), 1),
            (Has("null"), 1),
            (Has("true"), 1),
            (Has("false"), 1),
            (Ends(","), 1),
        ],
    ),
    (
        "bash",
        &[
            (Doc("#!"), 5),
            (Has("#!/bin"), 5),
            (Starts("echo "), 2),
            (Starts("cd "), 2),
            (Starts("export "), 1),
            (Starts("sudo "), 3),
            (Starts("$ "), 3),
            (Has(" | "), 1),
            (Has("&&"), 1),
            (Starts("fi"), 2),
            (Starts("done"), 2),
            (Starts("esac"), 3),
            (Has("$("), 2),
            (Has("${"), 2),
            (Starts("if ["), 3),
            (Starts("for "), 1),
            (Starts("cargo "), 2),
            (Starts("git "), 2),
            (Starts("npm "), 2),
            (Starts("pip "), 2),
            (Starts("apt "), 2),
            (Starts("pacman "), 2),
            (Starts("ls "), 2),
            (Starts("mkdir "), 2),
            (Starts("rm "), 2),
            (Starts("cp "), 1),
            (Starts("mv "), 1),
            (Starts("curl "), 2),
            (Starts("chmod "), 2),
            (Has(" --"), 1),
            (Starts("then"), 2),
            (Starts("else"), 1),
        ],
    ),
    (
        "html",
        &[
            (Doc("<!DOCTYPE"), 5),
            (Doc("<!doctype"), 5),
            (Doc("<html"), 5),
            (Doc("<?xml"), 5),
            (Doc("<"), 1),
            (Has("<div"), 3),
            (Has("</"), 2),
            (Has("<span"), 2),
            (Has("<p>"), 2),
            (Has("<a "), 2),
            (Has("<script"), 3),
            (Has("<body"), 3),
            (Has("<head"), 3),
            (Has("<ul"), 1),
            (Has("<li"), 1),
            (Has("<h1"), 2),
            (Has("/>"), 1),
            (Has("<svg"), 3),
            (Has("<button"), 3),
            (Has("<input"), 3),
            (Has("<table"), 2),
        ],
    ),
    (
        "sql",
        &[
            (HasCi("SELECT "), 4),
            (HasCi(" FROM "), 3),
            (HasCi(" WHERE "), 3),
            (HasCi("INSERT INTO"), 4),
            (HasCi("CREATE TABLE"), 4),
            (HasCi(" JOIN "), 3),
            (HasCi("GROUP BY"), 3),
            (HasCi("ORDER BY"), 3),
            (HasCi("UPDATE "), 1),
            (HasCi(" SET "), 1),
            (HasCi("DELETE FROM"), 4),
            (HasCi("PRIMARY KEY"), 4),
            (HasCi("VARCHAR"), 3),
            (HasCi("ALTER TABLE"), 4),
            (HasCi(" VALUES "), 2),
        ],
    ),
    (
        "cpp",
        &[
            (Starts("#include"), 5),
            (Has("int main("), 5),
            (Has("printf("), 3),
            (Has("std::"), 4),
            (Has("cout <<"), 4),
            (Has("cout<<"), 4),
            (Has("#define"), 3),
            (Has("nullptr"), 4),
            (Has("void "), 2),
            (Has("char *"), 2),
            (Has("char*"), 2),
            (Has("using namespace"), 5),
            (Has("template<"), 3),
            (Has("template <"), 3),
            (Has("int "), 1),
            (Has("return 0;"), 3),
            (Has("->"), 1),
            (Has("#pragma"), 3),
            (Has("size_t"), 2),
            (Has("malloc("), 3),
            (Has("scanf("), 3),
            (Has("std::vector"), 3),
            (Has("public:"), 3),
            (Has("private:"), 3),
        ],
    ),
    (
        "go",
        &[
            (Starts("package "), 3),
            (Has("func "), 4),
            (Has("fmt."), 4),
            (Has(":="), 3),
            (Has("import ("), 3),
            (Has("func main()"), 5),
            (Has("chan "), 3),
            (Has("err != nil"), 5),
            (Has("defer "), 3),
            (Has("struct {"), 2),
            (Has("interface {"), 2),
            (Has("go "), 1),
        ],
    ),
    (
        "css",
        &[
            (Has("px;"), 3),
            (Has("color:"), 3),
            (Has("margin"), 2),
            (Has("padding"), 2),
            (Has("display:"), 3),
            (Has("background"), 2),
            (Has("font-size"), 3),
            (Has("border"), 1),
            (Has("@media"), 4),
            (Has("!important"), 4),
            (Has("rem;"), 2),
            (Has("em;"), 1),
            (Has("%;"), 1),
            (Has("width:"), 2),
            (Has("height:"), 2),
            (Starts("."), 1),
            (Has(":hover"), 4),
            (Has("flex"), 1),
        ],
    ),
];

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

        /// Синтаксис по метке блока; без метки (или с незнакомой) —
        /// по догадке `guess_language`; иначе — обычный текст.
        fn pick_syntax<'a>(
            &self,
            ss: &'a SyntaxSet,
            language: Option<&str>,
            code: &str,
        ) -> &'a SyntaxReference {
            language
                .and_then(|lang| Self::find_syntax(ss, lang))
                .or_else(|| guess_language(code).and_then(|lang| Self::find_syntax(ss, lang)))
                .unwrap_or_else(|| ss.find_syntax_plain_text())
        }

        fn find_syntax<'a>(ss: &'a SyntaxSet, lang: &str) -> Option<&'a SyntaxReference> {
            if let Some(s) = ss.find_syntax_by_token(lang) {
                return Some(s);
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
            ss.find_syntax_by_name(canonical)
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
            let syntax = self.pick_syntax(ss, language, code);
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

#[cfg(test)]
mod guess_tests {
    use super::guess_language;

    #[test]
    fn guesses_common_languages() {
        let rust = "fn first_word(s: &str) -> &str {\n    let mut index = 0;\n    for (i, c) in s.char_indices() {\n        if c == ' ' { break; }\n    }\n    &s[..index]\n}\nfn main() { println!(\"{}\", first_word(\"hello world\")); }";
        assert_eq!(guess_language(rust), Some("rust"));
        let python = "def area(r):\n    return 3.14 * r * r\n\nprint(area(2))";
        assert_eq!(guess_language(python), Some("python"));
        let js =
            "const xs = [1, 2, 3];\nconst doubled = xs.map(x => x * 2);\nconsole.log(doubled);";
        assert_eq!(guess_language(js), Some("javascript"));
        let json = "{\n  \"name\": \"synthos\",\n  \"version\": 1,\n  \"tags\": [\"a\", \"b\"]\n}";
        assert_eq!(guess_language(json), Some("json"));
        let sh = "#!/bin/bash\ncd /tmp\necho \"hi\" | grep h\nls -la";
        assert_eq!(guess_language(sh), Some("bash"));
        let html = "<!DOCTYPE html>\n<html><body><div class=\"x\">hi</div></body></html>";
        assert_eq!(guess_language(html), Some("html"));
        let sql = "select id, name from users where age > 18 order by name;";
        assert_eq!(guess_language(sql), Some("sql"));
        let cpp = "#include <iostream>\nint main() {\n    std::cout << \"hi\";\n    return 0;\n}";
        assert_eq!(guess_language(cpp), Some("cpp"));
        let go = "package main\nimport \"fmt\"\nfunc main() {\n    x := 1\n    fmt.Println(x)\n}";
        assert_eq!(guess_language(go), Some("go"));
        let css = ".card {\n  padding: 8px;\n  color: red;\n  font-size: 14px;\n}\n.card:hover { background: blue; }";
        assert_eq!(guess_language(css), Some("css"));
    }

    #[test]
    fn plain_text_stays_plain() {
        assert_eq!(guess_language(""), None);
        assert_eq!(guess_language("x = 1"), None);
        assert_eq!(
            guess_language("Hello, world. This is a sentence about nothing."),
            None
        );
        // Вывод инструмента: список файлов, лог — не код.
        assert_eq!(guess_language("src/main.rs\nsrc/lib.rs\nCargo.toml"), None);
        assert_eq!(
            guess_language("[INFO] started\n[WARN] slow query 120 ms\n[INFO] done"),
            None
        );
    }
}
