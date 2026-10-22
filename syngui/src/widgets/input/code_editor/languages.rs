use super::syntax::Language;
use std::path::Path;

pub fn detect_by_path(path: &Path) -> Option<Language> {
    if let Some(l) = path.file_name().and_then(|n| n.to_str()).and_then(detect_by_name) {
        return Some(l);
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    detect_by_extension(&ext)
}

pub fn detect_by_extension(ext: &str) -> Option<Language> {
    Some(match ext {
        "rs" => Language::Rust,
        "json" | "jsonc" => Language::Json,
        "toml" => Language::Toml,
        "md" | "markdown" => Language::Markdown,
        "ts" | "mts" | "cts" => Language::TypeScript,
        "tsx" | "jsx" => Language::Tsx,
        "py" | "pyi" | "pyw" => Language::Python,
        "c" | "h" => Language::C,
        "cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx" | "c++" | "ino" => Language::Cpp,
        "asm" | "s" | "nasm" | "inc" => Language::Asm,
        "sh" | "bash" | "zsh" | "bashrc" | "zshrc" | "profile" => Language::Shell,
        "go" => Language::Go,
        "js" | "mjs" | "cjs" => Language::JavaScript,
        "java" => Language::Java,
        "cs" => Language::CSharp,
        "lua" => Language::Lua,
        "yaml" | "yml" => Language::Yaml,
        "html" | "htm" | "xhtml" => Language::Html,
        "css" | "mss" | "scss" => Language::Css,
        "xml" | "svg" | "svd" | "plist" => Language::Xml,
        "sql" => Language::Sql,
        "diff" | "patch" => Language::Diff,
        "rb" => Language::Ruby,
        "kt" | "kts" => Language::Kotlin,
        "swift" => Language::Swift,
        "php" => Language::Php,
        "txt" | "log" | "text" | "cfg" | "ini" | "conf" => Language::PlainText,
        _ => return None,
    })
}

/// Язык по имени файла без расширения (`Makefile`, `PKGBUILD`, `.bashrc`).
pub fn detect_by_name(name: &str) -> Option<Language> {
    Some(match name {
        "PKGBUILD" | ".bashrc" | ".zshrc" | ".profile" | ".bash_profile" | "configure" => Language::Shell,
        "Makefile" | "makefile" | "GNUmakefile" | "CMakeLists.txt" | "Dockerfile" => Language::Shell,
        _ => return None,
    })
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
        assert_eq!(detect_by_path(&PathBuf::from("file.xyz")), None);
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
