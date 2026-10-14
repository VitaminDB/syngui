use std::path::PathBuf;

use crate::core::Color;

#[derive(Clone)]
pub struct TerminalConfig {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub font_size: f32,
    pub font_size_explicit: bool,
    pub font_family: String,
    pub font_family_explicit: bool,
    pub line_height: f32,
    /// Свои 16 цветов ANSI (0–7 обычные, 8–15 яркие); `None` — Tango.
    pub palette: Option<[Color; 16]>,
    /// Сколько строк истории держать над экраном.
    pub scrollback: usize,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            command: super::pty::default_shell(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            font_size: 13.0,
            font_size_explicit: false,
            font_family: "monospace".to_string(),
            font_family_explicit: false,
            line_height: 1.25,
            palette: None,
            scrollback: super::grid::DEFAULT_SCROLLBACK,
        }
    }
}
