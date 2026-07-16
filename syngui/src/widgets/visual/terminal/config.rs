use std::path::PathBuf;

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
        }
    }
}
