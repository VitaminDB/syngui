#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Json,
    Toml,
    Markdown,
    TypeScript,
    Tsx,
    Python,
    C,
    Cpp,
    Asm,
    Shell,
    Go,
    JavaScript,
    Java,
    CSharp,
    Lua,
    Yaml,
    Html,
    Css,
    Xml,
    Sql,
    Diff,
    Ruby,
    Kotlin,
    Swift,
    Php,
    PlainText,
}

impl Language {
    /// Все языки (для выбора в интерфейсе).
    pub const ALL: &'static [Language] = &[Language::Rust, Language::Json, Language::Toml, Language::Markdown, Language::TypeScript, Language::Tsx, Language::Python, Language::C, Language::Cpp, Language::Asm, Language::Shell, Language::Go, Language::JavaScript, Language::Java, Language::CSharp, Language::Lua, Language::Yaml, Language::Html, Language::Css, Language::Xml, Language::Sql, Language::Diff, Language::Ruby, Language::Kotlin, Language::Swift, Language::Php, Language::PlainText];

    /// Расширение файла для `synoptic::from_extension` (`txt` — без подсветки).
    pub fn extension(self) -> &'static str {
        match self {
            Language::Rust => "rs",
            Language::Json => "json",
            Language::Toml => "toml",
            Language::Markdown => "md",
            Language::TypeScript => "ts",
            Language::Tsx => "tsx",
            Language::Python => "py",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Asm => "asm",
            Language::Shell => "sh",
            Language::Go => "go",
            Language::JavaScript => "js",
            Language::Java => "java",
            Language::CSharp => "cs",
            Language::Lua => "lua",
            Language::Yaml => "yaml",
            Language::Html => "html",
            Language::Css => "css",
            Language::Xml => "xml",
            Language::Sql => "sql",
            Language::Diff => "diff",
            Language::Ruby => "rb",
            Language::Kotlin => "kt",
            Language::Swift => "swift",
            Language::Php => "php",
            Language::PlainText => "txt",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Json => "JSON",
            Language::Toml => "TOML",
            Language::Markdown => "Markdown",
            Language::TypeScript => "TypeScript",
            Language::Tsx => "TSX",
            Language::Python => "Python",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::Asm => "Ассемблер",
            Language::Shell => "Shell",
            Language::Go => "Go",
            Language::JavaScript => "JavaScript",
            Language::Java => "Java",
            Language::CSharp => "C#",
            Language::Lua => "Lua",
            Language::Yaml => "YAML",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Xml => "XML",
            Language::Sql => "SQL",
            Language::Diff => "Diff",
            Language::Ruby => "Ruby",
            Language::Kotlin => "Kotlin",
            Language::Swift => "Swift",
            Language::Php => "PHP",
            Language::PlainText => "Текст",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_returns_human_readable() {
        assert_eq!(Language::Rust.name(), "Rust");
        assert_eq!(Language::Json.name(), "JSON");
    }

    #[test]
    fn extension_matches_synoptic() {
        assert_eq!(Language::Rust.extension(), "rs");
        assert_eq!(Language::Tsx.extension(), "tsx");
        assert_eq!(Language::Markdown.extension(), "md");
    }
}
