#[derive(Debug, Clone, Default)]
pub struct TextSelectionState {
    pub anchor: Option<usize>,
    pub mouse_selecting: bool,
}

impl TextSelectionState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn range(&self, cursor_pos: usize) -> Option<(usize, usize)> {
        self.anchor.map(|anchor| {
            if anchor <= cursor_pos {
                (anchor, cursor_pos)
            } else {
                (cursor_pos, anchor)
            }
        }).filter(|(s, e)| s != e)
    }

    pub fn start(&mut self, pos: usize) {
        self.anchor = Some(pos);
    }

    pub fn extend_or_start(&mut self, cursor_pos: usize) {
        if self.anchor.is_none() {
            self.anchor = Some(cursor_pos);
        }
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.mouse_selecting = false;
    }

    pub fn has_selection(&self, cursor_pos: usize) -> bool {
        self.range(cursor_pos).is_some()
    }

    pub fn selected_text<'a>(&self, text: &'a str, cursor_pos: usize) -> Option<&'a str> {
        self.range(cursor_pos).map(|(start, end)| {
            &text[start..end]
        })
    }

    pub fn delete_selection(&mut self, text: &mut String, cursor_pos: &mut usize) -> bool {
        if let Some((start, end)) = self.range(*cursor_pos) {
            text.drain(start..end);
            *cursor_pos = start;
            self.clear();
            true
        } else {
            false
        }
    }

    pub fn replace_selection(&mut self, text: &mut String, cursor_pos: &mut usize, replacement: &str) {
        if let Some((start, end)) = self.range(*cursor_pos) {
            text.drain(start..end);
            text.insert_str(start, replacement);
            *cursor_pos = start + replacement.len();
            self.clear();
        } else {
            text.insert_str(*cursor_pos, replacement);
            *cursor_pos += replacement.len();
        }
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
    }

    pub fn find_word_boundaries(text: &str, byte_offset: usize) -> (usize, usize) {
        let offset = byte_offset.min(text.len());

        let offset = if text.is_char_boundary(offset) {
            offset
        } else {
            (0..offset).rev().find(|&b| text.is_char_boundary(b)).unwrap_or(0)
        };

        let is_word_char = |ch: char| ch.is_alphanumeric() || ch == '_';

        let current_char = text[offset..].chars().next();
        let on_word = current_char.map_or(false, |ch| is_word_char(ch));

        if on_word {
            let mut start = offset;
            while start > 0 {
                let prev_start = text[..start].char_indices().next_back().map(|(i, _)| i);
                match prev_start {
                    Some(ps) if is_word_char(text[ps..].chars().next().unwrap()) => start = ps,
                    _ => break,
                }
            }
            let mut end = offset;
            for ch in text[offset..].chars() {
                if !is_word_char(ch) { break; }
                end += ch.len_utf8();
            }
            (start, end)
        } else {
            if let Some(ch) = current_char {
                (offset, offset + ch.len_utf8())
            } else if offset > 0 {
                let prev = text[..offset].char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
                (prev, offset)
            } else {
                (0, 0)
            }
        }
    }

    pub fn select_word(&mut self, text: &str, byte_offset: usize) -> usize {
        let (start, end) = Self::find_word_boundaries(text, byte_offset);
        self.anchor = Some(start);
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_no_selection() {
        let s = TextSelectionState::new();
        assert!(s.anchor.is_none());
        assert!(!s.mouse_selecting);
    }

    #[test]
    fn default_same_as_new() {
        let a = TextSelectionState::new();
        let b = TextSelectionState::default();
        assert_eq!(a.anchor, b.anchor);
        assert_eq!(a.mouse_selecting, b.mouse_selecting);
    }

    #[test]
    fn range_no_anchor() {
        let s = TextSelectionState::new();
        assert_eq!(s.range(5), None);
    }

    #[test]
    fn range_anchor_before_cursor() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(2);
        assert_eq!(s.range(10), Some((2, 10)));
    }

    #[test]
    fn range_anchor_after_cursor() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(10);
        assert_eq!(s.range(2), Some((2, 10)));
    }

    #[test]
    fn range_anchor_equals_cursor_is_none() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(5);
        assert_eq!(s.range(5), None);
    }

    #[test]
    fn start_sets_anchor() {
        let mut s = TextSelectionState::new();
        s.start(7);
        assert_eq!(s.anchor, Some(7));
    }

    #[test]
    fn extend_or_start_creates_anchor() {
        let mut s = TextSelectionState::new();
        s.extend_or_start(3);
        assert_eq!(s.anchor, Some(3));
    }

    #[test]
    fn extend_or_start_keeps_existing_anchor() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(2);
        s.extend_or_start(10);
        assert_eq!(s.anchor, Some(2));
    }

    #[test]
    fn clear_resets_all() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(5);
        s.mouse_selecting = true;
        s.clear();
        assert!(s.anchor.is_none());
        assert!(!s.mouse_selecting);
    }

    #[test]
    fn has_selection_true() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(0);
        assert!(s.has_selection(5));
    }

    #[test]
    fn has_selection_false_no_anchor() {
        let s = TextSelectionState::new();
        assert!(!s.has_selection(5));
    }

    #[test]
    fn has_selection_false_same_pos() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(5);
        assert!(!s.has_selection(5));
    }

    #[test]
    fn selected_text_returns_slice() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(0);
        let text = "Hello World";
        assert_eq!(s.selected_text(text, 5), Some("Hello"));
    }

    #[test]
    fn selected_text_reversed() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(6);
        assert_eq!(s.selected_text("Hello World", 0), Some("Hello "));
    }

    #[test]
    fn selected_text_none() {
        let s = TextSelectionState::new();
        assert_eq!(s.selected_text("Hello", 3), None);
    }

    #[test]
    fn selected_text_utf8() {
        let mut s = TextSelectionState::new();
        let text = "Привет мир";
        s.anchor = Some(0);
        let selected = s.selected_text(text, 12);
        assert_eq!(selected, Some("Привет"));
    }

    #[test]
    fn delete_selection_removes_text() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(5);
        let mut text = "Hello World".to_string();
        let mut cursor = 11;
        assert!(s.delete_selection(&mut text, &mut cursor));
        assert_eq!(text, "Hello");
        assert_eq!(cursor, 5);
        assert!(s.anchor.is_none());
    }

    #[test]
    fn delete_selection_reversed() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(5);
        let mut text = "Hello World".to_string();
        let mut cursor = 0;
        assert!(s.delete_selection(&mut text, &mut cursor));
        assert_eq!(text, " World");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn delete_selection_no_selection() {
        let mut s = TextSelectionState::new();
        let mut text = "Hello".to_string();
        let mut cursor = 3;
        assert!(!s.delete_selection(&mut text, &mut cursor));
        assert_eq!(text, "Hello");
        assert_eq!(cursor, 3);
    }

    #[test]
    fn replace_selection_with_text() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(0);
        let mut text = "Hello World".to_string();
        let mut cursor = 5;
        s.replace_selection(&mut text, &mut cursor, "Goodbye");
        assert_eq!(text, "Goodbye World");
        assert_eq!(cursor, 7);
    }

    #[test]
    fn replace_selection_no_selection_inserts() {
        let mut s = TextSelectionState::new();
        let mut text = "Hello".to_string();
        let mut cursor = 5;
        s.replace_selection(&mut text, &mut cursor, " World");
        assert_eq!(text, "Hello World");
        assert_eq!(cursor, 11);
    }

    #[test]
    fn replace_selection_with_empty() {
        let mut s = TextSelectionState::new();
        s.anchor = Some(0);
        let mut text = "Hello World".to_string();
        let mut cursor = 5;
        s.replace_selection(&mut text, &mut cursor, "");
        assert_eq!(text, " World");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn select_all_sets_anchor_to_zero() {
        let mut s = TextSelectionState::new();
        s.select_all();
        assert_eq!(s.anchor, Some(0));
    }

    #[test]
    fn word_boundaries_in_middle() {
        let (start, end) = TextSelectionState::find_word_boundaries("Hello World", 3);
        assert_eq!(start, 0);
        assert_eq!(end, 5);
    }

    #[test]
    fn word_boundaries_second_word() {
        let (start, end) = TextSelectionState::find_word_boundaries("Hello World", 8);
        assert_eq!(start, 6);
        assert_eq!(end, 11);
    }

    #[test]
    fn word_boundaries_at_start() {
        let (start, end) = TextSelectionState::find_word_boundaries("Hello World", 0);
        assert_eq!(start, 0);
        assert_eq!(end, 5);
    }

    #[test]
    fn word_boundaries_on_space() {
        let (start, end) = TextSelectionState::find_word_boundaries("Hello World", 5);
        assert_eq!(start, 5);
        assert_eq!(end, 6);
    }

    #[test]
    fn word_boundaries_at_end() {
        let text = "Hello";
        let (start, end) = TextSelectionState::find_word_boundaries(text, text.len());
        assert_eq!(start, 4);
        assert_eq!(end, 5);
    }

    #[test]
    fn word_boundaries_empty_string() {
        let (start, end) = TextSelectionState::find_word_boundaries("", 0);
        assert_eq!(start, 0);
        assert_eq!(end, 0);
    }

    #[test]
    fn word_boundaries_underscore() {
        let (start, end) = TextSelectionState::find_word_boundaries("hello_world foo", 3);
        assert_eq!(start, 0);
        assert_eq!(end, 11);
    }

    #[test]
    fn word_boundaries_utf8() {
        let text = "Привет мир";
        let (start, end) = TextSelectionState::find_word_boundaries(text, 4);
        assert_eq!(start, 0);
        assert_eq!(end, 12);
    }

    #[test]
    fn word_boundaries_offset_past_end() {
        let (start, end) = TextSelectionState::find_word_boundaries("Hi", 100);
        assert_eq!(start, 1);
        assert_eq!(end, 2);
    }

    #[test]
    fn select_word_sets_anchor_and_returns_end() {
        let mut s = TextSelectionState::new();
        let end = s.select_word("Hello World", 3);
        assert_eq!(s.anchor, Some(0));
        assert_eq!(end, 5);
    }

    #[test]
    fn select_word_second_word() {
        let mut s = TextSelectionState::new();
        let end = s.select_word("Hello World", 8);
        assert_eq!(s.anchor, Some(6));
        assert_eq!(end, 11);
    }
}
