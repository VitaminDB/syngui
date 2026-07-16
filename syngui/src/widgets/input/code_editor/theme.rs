use super::syntax::TokenClass;
use crate::core::Color;
use crate::mss::code_editor::CodeEditorPalette;
use crate::mss::MssFields;

#[derive(Debug, Clone)]
pub struct Theme {
    pub palette: CodeEditorPalette,
}

impl Theme {
    pub fn new() -> Self {
        Self {
            palette: CodeEditorPalette::default(),
        }
    }

    pub fn from_palette(palette: CodeEditorPalette) -> Self {
        Self { palette }
    }

    pub fn bg(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_bg
            .filter(opaque_enough)
            .or_else(|| mss.background_color.filter(opaque_enough))
            .unwrap_or(DEFAULT_BG)
    }

    pub fn fg(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_fg
            .filter(opaque_enough)
            .or_else(|| mss.color.filter(opaque_enough))
            .unwrap_or(DEFAULT_FG)
    }

    pub fn gutter_bg(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_gutter_bg
            .filter(opaque_enough)
            .or_else(|| mss.gutter_color.filter(opaque_enough))
            .unwrap_or_else(|| self.bg(mss).darken(0.04))
    }

    pub fn gutter_fg(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_gutter_fg
            .filter(opaque_enough)
            .unwrap_or_else(|| self.fg(mss).with_alpha(0.4))
    }

    pub fn cursor(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_cursor
            .filter(opaque_enough)
            .or_else(|| mss.caret_color.filter(opaque_enough))
            .or_else(|| mss.accent_color.filter(opaque_enough))
            .unwrap_or(DEFAULT_CURSOR)
    }

    pub fn selection(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_selection
            .filter(opaque_enough)
            .unwrap_or_else(|| self.cursor(mss).with_alpha(0.25))
    }

    pub fn current_line(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_current_line
            .filter(opaque_enough)
            .unwrap_or_else(|| self.bg(mss).lighten(0.04))
    }

    pub fn token(&self, class: TokenClass, mss: &MssFields) -> Color {
        self.palette
            .token(class.dotted())
            .filter(opaque_enough)
            .unwrap_or_else(|| self.fg(mss))
    }

    pub fn find_match(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_find_match
            .filter(opaque_enough)
            .unwrap_or_else(|| self.selection(mss).with_alpha(FALLBACK_FIND_MATCH_ALPHA))
    }

    pub fn find_current(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_find_current
            .filter(opaque_enough)
            .unwrap_or_else(|| self.cursor(mss).with_alpha(FALLBACK_FIND_CURRENT_ALPHA))
    }

    pub fn bracket_match(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_bracket_match
            .filter(opaque_enough)
            .unwrap_or_else(|| self.cursor(mss).with_alpha(FALLBACK_BRACKET_MATCH_ALPHA))
    }

    pub fn indent_guide(&self, mss: &MssFields) -> Color {
        self.palette
            .editor_whitespace
            .filter(opaque_enough)
            .unwrap_or_else(|| self.gutter_fg(mss).with_alpha(FALLBACK_INDENT_GUIDE_ALPHA))
    }
}

const FALLBACK_FIND_MATCH_ALPHA: f32 = 0.5;
const FALLBACK_FIND_CURRENT_ALPHA: f32 = 0.6;
const FALLBACK_BRACKET_MATCH_ALPHA: f32 = 0.5;
const FALLBACK_INDENT_GUIDE_ALPHA: f32 = 0.4;

#[inline]
fn opaque_enough(c: &Color) -> bool {
    c.a > 0.05
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

const DEFAULT_BG: Color = Color {
    r: 0.157,
    g: 0.173,
    b: 0.204,
    a: 1.0,
};
const DEFAULT_FG: Color = Color {
    r: 0.671,
    g: 0.694,
    b: 0.733,
    a: 1.0,
};
const DEFAULT_CURSOR: Color = Color {
    r: 0.380,
    g: 0.686,
    b: 0.937,
    a: 1.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_theme_uses_defaults() {
        let t = Theme::new();
        let mss = MssFields::new();
        assert_eq!(t.bg(&mss), DEFAULT_BG);
        assert_eq!(t.fg(&mss), DEFAULT_FG);
    }

    #[test]
    fn palette_overrides_defaults() {
        let mut palette = CodeEditorPalette::default();
        palette.editor_bg = Some(Color::new(1.0, 0.0, 0.0, 1.0));
        let t = Theme::from_palette(palette);
        let mss = MssFields::new();
        assert_eq!(t.bg(&mss), Color::new(1.0, 0.0, 0.0, 1.0));
    }

    #[test]
    fn token_falls_back_to_fg() {
        let t = Theme::new();
        let mss = MssFields::new();
        assert_eq!(t.token(TokenClass::Keyword, &mss), DEFAULT_FG);
    }

    #[test]
    fn token_uses_palette_when_set() {
        let mut palette = CodeEditorPalette::default();
        let purple = Color::new(0.776, 0.471, 0.867, 1.0);
        palette.tokens.insert("keyword".into(), purple);
        let t = Theme::from_palette(palette);
        let mss = MssFields::new();
        assert_eq!(t.token(TokenClass::Keyword, &mss), purple);
        assert_eq!(t.token(TokenClass::KeywordControl, &mss), purple);
    }
}
