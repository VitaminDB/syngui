//! Контекст анализа для рабочего каталога: переменные и `@keyframes` из
//! всех `.mss` каталога плюс файл контекста `.mss-context.toml` /
//! `mss-context.json` (переменные темы, классы программы).

use std::path::{Path, PathBuf};

use mss_analysis::{Context, VarInfo};
use serde::Deserialize;

/// Имена файла контекста — ищутся в корне и выше по дереву.
pub const CONTEXT_FILES: &[&str] = &[
    ".mss-context.toml",
    "mss-context.toml",
    ".mss-context.json",
    "mss-context.json",
];

/// Формат файла контекста (TOML или JSON — поля те же).
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextFile {
    /// Переменные: имя с `--`, значение, описание.
    pub variables: Vec<ContextVar>,
    /// Классы, которые ставит программа.
    pub classes: Vec<String>,
    /// Имена `@keyframes` из других таблиц.
    pub keyframes: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextVar {
    pub name: String,
    pub value: String,
    pub doc: String,
}

/// Разобрать файл контекста по расширению.
pub fn parse_context_file(path: &Path, text: &str) -> Result<ContextFile, String> {
    if path.extension().is_some_and(|e| e == "json") {
        serde_json::from_str(text).map_err(|e| e.to_string())
    } else {
        toml::from_str(text).map_err(|e| e.to_string())
    }
}

/// Найти файл контекста в `root` и выше.
pub fn find_context_file(root: &Path) -> Option<PathBuf> {
    let mut dir = Some(root);
    while let Some(d) = dir {
        for name in CONTEXT_FILES {
            let p = d.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
        dir = d.parent();
    }
    None
}

/// Все `.mss` под `root` (без `target`, скрытых каталогов и `node_modules`).
pub fn mss_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
        if depth > 12 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if name != "target" && name != "node_modules" && !name.starts_with('.') {
                    walk(&e.path(), out, depth + 1);
                }
            } else if ft.is_file() && name.ends_with(".mss") {
                out.push(e.path());
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out, 0);
    out.sort();
    out
}

/// Собранный контекст рабочего каталога.
#[derive(Debug, Default)]
pub struct Workspace {
    pub root: Option<PathBuf>,
    /// Переменные и анимации по файлам (чтобы не учитывать файл, который
    /// сейчас открыт и правится).
    files: Vec<(PathBuf, Vec<VarInfo>, Vec<String>)>,
    file_ctx: ContextFile,
    /// Ошибка чтения файла контекста — для журнала клиента.
    pub context_error: Option<String>,
    pub context_path: Option<PathBuf>,
}

impl Workspace {
    pub fn load(root: Option<PathBuf>) -> Self {
        let mut ws = Workspace {
            root,
            ..Default::default()
        };
        ws.rescan();
        ws
    }

    /// Перечитать `.mss` каталога и файл контекста.
    pub fn rescan(&mut self) {
        self.files.clear();
        self.file_ctx = ContextFile::default();
        self.context_error = None;
        self.context_path = None;
        let Some(root) = self.root.clone() else {
            return;
        };
        for path in mss_files(&root) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let (vars, kf) = scan_text(&text, &path, &root);
            self.files.push((path, vars, kf));
        }
        if let Some(p) = find_context_file(&root) {
            match std::fs::read_to_string(&p)
                .map_err(|e| e.to_string())
                .and_then(|t| parse_context_file(&p, &t))
            {
                Ok(c) => self.file_ctx = c,
                Err(e) => self.context_error = Some(format!("{}: {e}", p.display())),
            }
            self.context_path = Some(p);
        }
    }

    /// Обновить переменные одного файла (после сохранения).
    pub fn update_file(&mut self, path: &Path, text: &str) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if !path.starts_with(&root) {
            return;
        }
        let (vars, kf) = scan_text(text, path, &root);
        match self.files.iter_mut().find(|f| f.0 == path) {
            Some(f) => {
                f.1 = vars;
                f.2 = kf;
            }
            None => self.files.push((path.to_path_buf(), vars, kf)),
        }
    }

    /// Контекст для документа `current` (его собственные объявления берутся
    /// из текста документа, а не со диска).
    pub fn context_for(&self, current: Option<&Path>) -> Context {
        let mut ctx = Context {
            variables: self
                .file_ctx
                .variables
                .iter()
                .map(|v| VarInfo {
                    name: v.name.clone(),
                    value: v.value.clone(),
                    doc: v.doc.clone(),
                })
                .collect(),
            known_classes: self.file_ctx.classes.clone(),
            keyframes: self.file_ctx.keyframes.clone(),
        };
        for (path, vars, kf) in &self.files {
            if Some(path.as_path()) == current {
                continue;
            }
            for v in vars {
                if !ctx.variables.iter().any(|x| x.name == v.name) {
                    ctx.variables.push(v.clone());
                }
            }
            for k in kf {
                if !ctx.keyframes.contains(k) {
                    ctx.keyframes.push(k.clone());
                }
            }
        }
        ctx
    }
}

/// Переменные и имена `@keyframes` текста.
fn scan_text(text: &str, path: &Path, root: &Path) -> (Vec<VarInfo>, Vec<String>) {
    let sheet = mss_analysis::syntax::parse(text);
    let lines = mss_analysis::syntax::LineIndex::new(text);
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string();
    let mut vars: Vec<VarInfo> = Vec::new();
    for d in sheet
        .all_declarations()
        .into_iter()
        .filter(|d| d.is_variable())
    {
        let line = lines.position(text, d.name_span.start).0 + 1;
        let info = VarInfo {
            name: d.name.clone(),
            value: d.value.clone(),
            doc: format!("Из {rel}:{line}"),
        };
        match vars.iter_mut().find(|v| v.name == d.name) {
            Some(v) => *v = info,
            None => vars.push(info),
        }
    }
    let kf = sheet
        .keyframes_names()
        .into_iter()
        .map(|k| k.0.to_string())
        .collect();
    (vars, kf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_and_json_formats() {
        let t = r##"
classes = ["panel", "dock"]
keyframes = ["pulse"]

[[variables]]
name = "--bg"
value = "#1e1e2e"
doc = "Фон окна"

[[variables]]
name = "--radius"
value = "8px"
"##;
        let c = parse_context_file(Path::new("a/.mss-context.toml"), t).unwrap();
        assert_eq!(c.variables.len(), 2);
        assert_eq!(c.variables[1].doc, "");
        assert_eq!(c.classes, ["panel", "dock"]);
        let j = r##"{"variables":[{"name":"--bg","value":"#000"}],"classes":["x"]}"##;
        let c = parse_context_file(Path::new("mss-context.json"), j).unwrap();
        assert_eq!(c.variables[0].value, "#000");
        assert!(parse_context_file(Path::new("x.toml"), "colours = 1").is_err());
    }

    #[test]
    fn workspace_scan_and_context_file() {
        let dir = std::env::temp_dir().join(format!("mss-lsp-test-{}", std::process::id()));
        let sub = dir.join("styles");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(
            sub.join("theme.mss"),
            ":root { --accent: #f00; }\n@keyframes spin { to { opacity: 0; } }",
        )
        .unwrap();
        std::fs::write(sub.join("app.mss"), ".a { --local: 1px; }").unwrap();
        std::fs::write(dir.join("target/skip.mss"), ":root { --skip: 1; }").unwrap();
        std::fs::write(
            dir.join(".mss-context.toml"),
            "classes = [\"panel\"]\n[[variables]]\nname = \"--bg\"\nvalue = \"#000\"\n",
        )
        .unwrap();

        let ws = Workspace::load(Some(dir.clone()));
        let ctx = ws.context_for(Some(&sub.join("app.mss")));
        let names: Vec<_> = ctx.variables.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["--bg", "--accent"]);
        assert_eq!(ctx.keyframes, ["spin"]);
        assert_eq!(ctx.known_classes, ["panel"]);
        assert!(ctx.variables[1].doc.contains("theme.mss:1"));
        // Файл контекста ищется и выше корня.
        assert!(find_context_file(&sub).is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
