use super::syntax::Language;
use std::path::Path;

pub fn detect_by_path(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    detect_by_extension(&ext)
}

pub fn detect_by_extension(ext: &str) -> Option<Language> {
    match ext {
        "rs" => Some(Language::Rust),
        "json" => Some(Language::Json),
        "toml" => Some(Language::Toml),
        "md" | "markdown" => Some(Language::Markdown),
        "ts" | "mts" | "cts" => Some(Language::TypeScript),
        "tsx" => Some(Language::Tsx),
        "jsx" => Some(Language::Tsx),
        "py" | "pyi" => Some(Language::Python),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn rust_files() {
        assert_eq!(
            detect_by_path(&PathBuf::from("src/main.rs")),
            Some(Language::Rust)
        );
        assert_eq!(
            detect_by_path(&PathBuf::from("/foo/bar/lib.rs")),
            Some(Language::Rust)
        );
    }

    #[test]
    fn json_toml_md() {
        assert_eq!(
            detect_by_path(&PathBuf::from("Cargo.toml")),
            Some(Language::Toml)
        );
        assert_eq!(
            detect_by_path(&PathBuf::from("data.json")),
            Some(Language::Json)
        );
        assert_eq!(
            detect_by_path(&PathBuf::from("README.md")),
            Some(Language::Markdown)
        );
        assert_eq!(
            detect_by_path(&PathBuf::from("doc.markdown")),
            Some(Language::Markdown)
        );
    }

    #[test]
    fn unknown_returns_none() {
        assert_eq!(detect_by_path(&PathBuf::from("file.txt")), None);
        assert_eq!(detect_by_path(&PathBuf::from("noext")), None);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            detect_by_path(&PathBuf::from("Foo.RS")),
            Some(Language::Rust)
        );
    }

    #[test]
    fn typescript_python_extensions() {
        assert_eq!(
            detect_by_path(&PathBuf::from("app.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            detect_by_path(&PathBuf::from("Component.tsx")),
            Some(Language::Tsx)
        );
        assert_eq!(
            detect_by_path(&PathBuf::from("script.py")),
            Some(Language::Python)
        );
        assert_eq!(
            detect_by_path(&PathBuf::from("types.d.ts")),
            Some(Language::TypeScript)
        );
    }
}
