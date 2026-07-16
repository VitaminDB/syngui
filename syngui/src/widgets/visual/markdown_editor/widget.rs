use std::any::Any;
use std::sync::Arc;

use crate::core::sync::Mutex;
use crate::signal::{use_signal, RwSignal};
use crate::widget::{Element, ElementId, ElementTree, Widget};

use super::element::MarkdownEditorElement;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorMode {
    Edit,
    Preview,
    Split,
}

impl Default for EditorMode {
    fn default() -> Self {
        EditorMode::Split
    }
}

pub struct MarkdownEditor {
    pub(crate) text: RwSignal<String>,
    pub(crate) mode: RwSignal<EditorMode>,
    pub(crate) show_toolbar: bool,
    pub(crate) syntax_highlight: bool,
    pub(crate) copy_code: bool,
    pub(crate) line_numbers: bool,
    pub(crate) rows: usize,
    pub(crate) split_ratio: f32,
    pub(crate) on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
}

impl MarkdownEditor {
    pub fn new(text: RwSignal<String>) -> Self {
        Self {
            text,
            mode: use_signal(EditorMode::default()),
            show_toolbar: true,
            syntax_highlight: false,
            copy_code: true,
            line_numbers: false,
            rows: 14,
            split_ratio: 0.5,
            on_change: None,
        }
    }

    pub fn mode(mut self, m: RwSignal<EditorMode>) -> Self {
        self.mode = m;
        self
    }

    pub fn initial_mode(self, m: EditorMode) -> Self {
        self.mode.set(m);
        self
    }

    pub fn show_toolbar(mut self, show: bool) -> Self {
        self.show_toolbar = show;
        self
    }

    pub fn syntax_highlight(mut self, on: bool) -> Self {
        self.syntax_highlight = on;
        self
    }

    pub fn copy_code(mut self, on: bool) -> Self {
        self.copy_code = on;
        self
    }

    pub fn line_numbers(mut self, on: bool) -> Self {
        self.line_numbers = on;
        self
    }

    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows;
        self
    }

    pub fn split_ratio(mut self, r: f32) -> Self {
        self.split_ratio = r.clamp(0.05, 0.95);
        self
    }

    pub fn on_change(mut self, cb: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(cb)));
        self
    }
}

impl Widget for MarkdownEditor {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MarkdownEditorElement::new(
            self.text,
            self.mode,
            self.show_toolbar,
            self.syntax_highlight,
            self.copy_code,
            self.line_numbers,
            self.rows,
            self.split_ratio,
            self.on_change.clone(),
        ))
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}
