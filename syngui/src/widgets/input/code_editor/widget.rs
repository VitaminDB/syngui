use super::element::CodeEditorElement;
use super::languages::detect_by_path;
use super::syntax::Language;
use crate::core::sync::Mutex;
use crate::signal::RwSignal;
use crate::widget::{Element, ElementId, ElementTree, Widget};
use std::any::Any;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorCommand {
    Copy,
    Cut,
    Paste,
    SelectAll,
    Reload(String),
}

pub struct CodeEditorChange<'a> {
    pub full_text: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct CursorInfo {
    pub line: usize,
    pub col: usize,
    pub total_lines: usize,
    pub selection_len: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EditorPersistedState {
    pub cursor_offset: usize,
    pub scroll_lines: usize,
    pub scroll_x: f32,
}

pub struct CodeEditor {
    pub(crate) initial_text: String,
    pub(crate) language: Option<Language>,
    pub(crate) read_only: bool,
    pub(crate) show_line_numbers: bool,
    pub(crate) soft_wrap: bool,
    pub(crate) size_limit_mb: usize,
    pub(crate) tab_width: u8,
    pub(crate) insert_spaces: bool,
    pub(crate) on_change: Option<Arc<Mutex<dyn FnMut(CodeEditorChange) + Send>>>,
    pub(crate) on_save: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    pub(crate) on_cursor: Option<Arc<Mutex<dyn FnMut(CursorInfo) + Send>>>,
    pub(crate) command_signal: Option<RwSignal<Option<EditorCommand>>>,
    pub(crate) state_signal: Option<RwSignal<EditorPersistedState>>,
    pub(crate) classes: Vec<String>,
}

impl CodeEditor {
    pub fn new() -> Self {
        Self {
            initial_text: String::new(),
            language: None,
            read_only: false,
            show_line_numbers: true,
            soft_wrap: false,
            size_limit_mb: 50,
            tab_width: 4,
            insert_spaces: true,
            on_change: None,
            on_save: None,
            on_cursor: None,
            command_signal: None,
            state_signal: None,
            classes: vec!["code-editor".to_string()],
        }
    }

    pub fn text(mut self, t: impl Into<String>) -> Self {
        self.initial_text = t.into();
        self
    }

    pub fn language(mut self, lang: Language) -> Self {
        self.language = Some(lang);
        self
    }

    pub fn auto_detect_language(mut self, path: &Path) -> Self {
        if let Some(lang) = detect_by_path(path) {
            self.language = Some(lang);
        }
        self
    }

    pub fn read_only(mut self, b: bool) -> Self {
        self.read_only = b;
        self
    }

    pub fn show_line_numbers(mut self, b: bool) -> Self {
        self.show_line_numbers = b;
        self
    }

    pub fn soft_wrap(mut self, b: bool) -> Self {
        self.soft_wrap = b;
        self
    }

    pub fn size_limit_mb(mut self, n: usize) -> Self {
        self.size_limit_mb = n.max(1);
        self
    }

    pub fn tab_width(mut self, n: u8) -> Self {
        self.tab_width = n.clamp(1, 8);
        self
    }

    pub fn insert_spaces(mut self, b: bool) -> Self {
        self.insert_spaces = b;
        self
    }

    pub fn on_change(mut self, cb: impl FnMut(CodeEditorChange) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_save(mut self, cb: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_save = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_cursor(mut self, cb: impl FnMut(CursorInfo) + Send + 'static) -> Self {
        self.on_cursor = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn command_signal(mut self, signal: RwSignal<Option<EditorCommand>>) -> Self {
        self.command_signal = Some(signal);
        self
    }

    pub fn state_signal(mut self, signal: RwSignal<EditorPersistedState>) -> Self {
        self.state_signal = Some(signal);
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.classes.push(c.into());
        self
    }
}

impl Default for CodeEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for CodeEditor {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(CodeEditorElement::new(self))
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_sensible() {
        let e = CodeEditor::new();
        assert_eq!(e.size_limit_mb, 50);
        assert_eq!(e.tab_width, 4);
        assert!(e.insert_spaces);
        assert!(e.show_line_numbers);
        assert!(!e.soft_wrap);
        assert!(!e.read_only);
        assert_eq!(e.classes, vec!["code-editor".to_string()]);
    }

    #[test]
    fn builder_chains() {
        let e = CodeEditor::new()
            .text("fn main() {}")
            .language(Language::Rust)
            .read_only(true)
            .tab_width(2)
            .class("editor")
            .class("theme-one-dark");
        assert_eq!(e.initial_text, "fn main() {}");
        assert_eq!(e.language, Some(Language::Rust));
        assert!(e.read_only);
        assert_eq!(e.tab_width, 2);
        assert_eq!(
            e.classes,
            vec!["code-editor", "editor", "theme-one-dark"]
        );
    }

    #[test]
    fn widget_classes_override_returns_classes() {
        let e = CodeEditor::new().class("foo").class("bar");
        let classes = <CodeEditor as Widget>::widget_classes(&e);
        assert_eq!(
            classes,
            &[
                "code-editor".to_string(),
                "foo".to_string(),
                "bar".to_string(),
            ]
        );
    }

    #[test]
    fn auto_detect_picks_language_from_path() {
        use std::path::PathBuf;
        let e = CodeEditor::new().auto_detect_language(&PathBuf::from("foo.rs"));
        assert_eq!(e.language, Some(Language::Rust));
        let e = CodeEditor::new().auto_detect_language(&PathBuf::from("foo.unknown"));
        assert_eq!(e.language, None);
    }

    #[test]
    fn size_limit_clamps_to_min_1() {
        let e = CodeEditor::new().size_limit_mb(0);
        assert_eq!(e.size_limit_mb, 1);
    }

    #[test]
    fn tab_width_clamps_to_1_8() {
        assert_eq!(CodeEditor::new().tab_width(0).tab_width, 1);
        assert_eq!(CodeEditor::new().tab_width(20).tab_width, 8);
        assert_eq!(CodeEditor::new().tab_width(4).tab_width, 4);
    }
}
