use crate::core::Color;
use crate::mss::style_engine::ComputedStyle;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct CodeEditorPalette {
    pub editor_bg: Option<Color>,
    pub editor_fg: Option<Color>,
    pub editor_gutter_bg: Option<Color>,
    pub editor_gutter_fg: Option<Color>,
    pub editor_cursor: Option<Color>,
    pub editor_selection: Option<Color>,
    pub editor_current_line: Option<Color>,
    pub editor_bracket_match: Option<Color>,
    pub editor_whitespace: Option<Color>,
    pub editor_find_match: Option<Color>,
    pub editor_find_current: Option<Color>,

    pub tokens: HashMap<String, Color>,
}

impl CodeEditorPalette {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_style(style: &ComputedStyle) -> Self {
        let mut palette = Self::default();

        palette.editor_bg = read_color(style, "editor-bg");
        palette.editor_fg = read_color(style, "editor-fg");
        palette.editor_gutter_bg = read_color(style, "editor-gutter-bg");
        palette.editor_gutter_fg = read_color(style, "editor-gutter-fg");
        palette.editor_cursor = read_color(style, "editor-cursor");
        palette.editor_selection = read_color(style, "editor-selection");
        palette.editor_current_line = read_color(style, "editor-current-line");
        palette.editor_bracket_match = read_color(style, "editor-bracket-match");
        palette.editor_whitespace = read_color(style, "editor-whitespace");
        palette.editor_find_match = read_color(style, "editor-find-match");
        palette.editor_find_current = read_color(style, "editor-find-current");

        for (prop_name, _) in style.properties() {
            if let Some(rest) = prop_name.strip_prefix("token-") {
                if let Some(color) = read_color(style, prop_name) {
                    let class_key = rest.replace('-', ".");
                    palette.tokens.insert(class_key, color);
                }
            }
        }

        palette
    }

    pub fn token(&self, class: &str) -> Option<Color> {
        let mut current = class;
        loop {
            if let Some(c) = self.tokens.get(current) {
                return Some(*c);
            }
            match current.rfind('.') {
                Some(idx) => current = &current[..idx],
                None => return None,
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

fn read_color(style: &ComputedStyle, name: &str) -> Option<Color> {
    style
        .get(name)
        .and_then(|v| v.as_color())
        .map(|c| Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_palette_returns_none_for_all_tokens() {
        let p = CodeEditorPalette::new();
        assert!(p.token("keyword").is_none());
        assert!(p.token("keyword.control.return").is_none());
        assert!(p.editor_bg.is_none());
    }

    #[test]
    fn token_fallback_walk() {
        let mut p = CodeEditorPalette::new();
        let red = Color::new(1.0, 0.0, 0.0, 1.0);
        p.tokens.insert("keyword".to_string(), red);
        assert_eq!(p.token("keyword"), Some(red));
        assert_eq!(p.token("keyword.control"), Some(red));
        assert_eq!(p.token("keyword.control.return"), Some(red));
        assert!(p.token("string").is_none());
    }

    #[test]
    fn token_specific_overrides_general() {
        let mut p = CodeEditorPalette::new();
        let red = Color::new(1.0, 0.0, 0.0, 1.0);
        let blue = Color::new(0.0, 0.0, 1.0, 1.0);
        p.tokens.insert("keyword".to_string(), red);
        p.tokens.insert("keyword.control".to_string(), blue);
        assert_eq!(p.token("keyword.control.return"), Some(blue));
        assert_eq!(p.token("keyword"), Some(red));
        assert_eq!(p.token("keyword.modifier"), Some(red));
    }
}
