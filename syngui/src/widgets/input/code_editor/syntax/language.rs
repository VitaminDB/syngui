#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Json,
    Toml,
    Markdown,
    TypeScript,
    Tsx,
    Python,
}

impl Language {
    pub fn extension(self) -> &'static str {
        match self {
            Language::Rust => "rs",
            Language::Json => "json",
            Language::Toml => "toml",
            Language::Markdown => "md",
            Language::TypeScript => "ts",
            Language::Tsx => "tsx",
            Language::Python => "py",
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
