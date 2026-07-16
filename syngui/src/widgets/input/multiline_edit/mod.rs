mod element;

use std::sync::Arc;
use crate::core::sync::Mutex;

pub struct MultilineTextEdit {
    pub text: String,
    pub placeholder: String,
    pub rows: usize,
    pub read_only: bool,
    pub show_line_numbers: bool,
    pub soft_wrap: bool,
    pub auto_height: bool,
    pub max_rows: Option<usize>,
    pub on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    pub submit_on_enter: bool,
    pub on_submit: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
}

impl MultilineTextEdit {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            placeholder: String::new(),
            rows: 5,
            read_only: false,
            show_line_numbers: false,
            soft_wrap: true,
            auto_height: false,
            max_rows: None,
            on_change: None,
            submit_on_enter: false,
            on_submit: None,
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn show_line_numbers(mut self, show: bool) -> Self {
        self.show_line_numbers = show;
        self
    }

    pub fn soft_wrap(mut self, wrap: bool) -> Self {
        self.soft_wrap = wrap;
        self
    }

    pub fn auto_height(mut self, auto: bool) -> Self {
        self.auto_height = auto;
        self
    }

    pub fn max_rows(mut self, n: usize) -> Self {
        self.max_rows = Some(n);
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn submit_on_enter(mut self, enabled: bool) -> Self {
        self.submit_on_enter = enabled;
        self
    }

    pub fn on_submit(mut self, callback: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_submit = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Default for MultilineTextEdit {
    fn default() -> Self {
        Self::new()
    }
}
