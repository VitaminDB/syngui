use super::edit::{Edit, EditKind, InverseEdit};
use ropey::Rope;
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct RopeBuffer {
    rope: Rope,
}

impl RopeBuffer {
    pub fn from_str(s: &str) -> Self {
        Self {
            rope: Rope::from_str(s),
        }
    }

    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
        }
    }

    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines().max(1)
    }

    pub fn byte_to_line(&self, byte: usize) -> usize {
        let byte = byte.min(self.len_bytes());
        self.rope.byte_to_line(byte)
    }

    pub fn line_to_byte(&self, line: usize) -> usize {
        if line >= self.len_lines() {
            return self.len_bytes();
        }
        self.rope.line_to_byte(line)
    }

    pub fn line_byte_range(&self, line: usize) -> Range<usize> {
        let start = self.line_to_byte(line);
        let next = if line + 1 < self.len_lines() {
            self.line_to_byte(line + 1).saturating_sub(1)
        } else {
            self.len_bytes()
        };
        let end = next.max(start);
        start..end
    }

    pub fn byte_to_line_col(&self, byte: usize) -> (usize, usize) {
        let byte = byte.min(self.len_bytes());
        let line = self.byte_to_line(byte);
        let line_start_byte = self.line_to_byte(line);
        let line_start_char = self.rope.byte_to_char(line_start_byte);
        let target_char = self.rope.byte_to_char(byte);
        let col = target_char - line_start_char;
        (line, col)
    }

    pub fn line_col_to_byte(&self, line: usize, col: usize) -> usize {
        if line >= self.len_lines() {
            return self.len_bytes();
        }
        let line_byte_start = self.line_to_byte(line);
        let line_char_start = self.rope.byte_to_char(line_byte_start);
        let line_text = self.line_str(line);
        let max_col = line_text.chars().count();
        let clamped_col = col.min(max_col);
        let target_char = line_char_start + clamped_col;
        self.rope.char_to_byte(target_char)
    }

    pub fn line_str(&self, line: usize) -> String {
        let range = self.line_byte_range(line);
        self.byte_slice(range)
    }

    pub fn byte_slice(&self, range: Range<usize>) -> String {
        let start = range.start.min(self.len_bytes());
        let end = range.end.min(self.len_bytes());
        if start >= end {
            return String::new();
        }
        let start_char = self.rope.byte_to_char(start);
        let end_char = self.rope.byte_to_char(end);
        self.rope.slice(start_char..end_char).to_string()
    }

    pub fn to_string(&self) -> String {
        self.rope.to_string()
    }

    pub fn apply_edit(&mut self, edit: &Edit) -> InverseEdit {
        let start = edit.range.start.min(self.len_bytes());
        let end = edit.range.end.min(self.len_bytes());
        let start = start.min(end);

        let removed = self.byte_slice(start..end);

        let start_char = self.rope.byte_to_char(start);
        let end_char = self.rope.byte_to_char(end);

        if start_char != end_char {
            self.rope.remove(start_char..end_char);
        }

        if !edit.replacement.is_empty() {
            self.rope.insert(start_char, &edit.replacement);
        }

        let new_end = start + edit.replacement.len();
        InverseEdit {
            range: start..new_end,
            replacement: removed,
            kind: match edit.kind {
                EditKind::Insert => EditKind::Delete,
                EditKind::Delete => EditKind::Insert,
                EditKind::Replace => EditKind::Replace,
            },
        }
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }
}

impl Default for RopeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer_basics() {
        let b = RopeBuffer::new();
        assert_eq!(b.len_bytes(), 0);
        assert_eq!(b.len_lines(), 1, "empty buffer has 1 logical line");
        assert_eq!(b.line_str(0), "");
    }

    #[test]
    fn line_indexing() {
        let b = RopeBuffer::from_str("foo\nbar\nbaz");
        assert_eq!(b.len_lines(), 3);
        assert_eq!(b.line_str(0), "foo");
        assert_eq!(b.line_str(1), "bar");
        assert_eq!(b.line_str(2), "baz");
        assert_eq!(b.line_to_byte(0), 0);
        assert_eq!(b.line_to_byte(1), 4);
        assert_eq!(b.line_to_byte(2), 8);
    }

    #[test]
    fn byte_to_line_col_ascii() {
        let b = RopeBuffer::from_str("foo\nbar\nbaz");
        assert_eq!(b.byte_to_line_col(0), (0, 0));
        assert_eq!(b.byte_to_line_col(2), (0, 2));
        assert_eq!(b.byte_to_line_col(4), (1, 0));
        assert_eq!(b.byte_to_line_col(6), (1, 2));
        assert_eq!(b.byte_to_line_col(11), (2, 3));
    }

    #[test]
    fn byte_to_line_col_utf8() {
        let b = RopeBuffer::from_str("Привет\nмир");
        assert_eq!(b.byte_to_line_col(12), (0, 6));
        assert_eq!(b.byte_to_line_col(13), (1, 0));
    }

    #[test]
    fn line_col_to_byte_roundtrip() {
        let b = RopeBuffer::from_str("hello\nworld");
        for byte in [0, 3, 5, 6, 8, 11].iter().copied() {
            let (l, c) = b.byte_to_line_col(byte);
            assert_eq!(b.line_col_to_byte(l, c), byte, "roundtrip for byte={}", byte);
        }
    }

    #[test]
    fn apply_insert_edit() {
        let mut b = RopeBuffer::from_str("hello");
        let inv = b.apply_edit(&Edit::insert(5, "!"));
        assert_eq!(b.to_string(), "hello!");
        assert_eq!(inv.range, 5..6);
        assert_eq!(inv.replacement, "");
        assert_eq!(inv.kind, EditKind::Delete);
    }

    #[test]
    fn apply_delete_edit() {
        let mut b = RopeBuffer::from_str("hello world");
        let inv = b.apply_edit(&Edit::delete(5..11));
        assert_eq!(b.to_string(), "hello");
        assert_eq!(inv.range, 5..5);
        assert_eq!(inv.replacement, " world");
        assert_eq!(inv.kind, EditKind::Insert);
    }

    #[test]
    fn apply_replace_edit() {
        let mut b = RopeBuffer::from_str("hello world");
        let inv = b.apply_edit(&Edit::replace(0..5, "goodbye"));
        assert_eq!(b.to_string(), "goodbye world");
        assert_eq!(inv.range, 0..7);
        assert_eq!(inv.replacement, "hello");
        assert_eq!(inv.kind, EditKind::Replace);
    }

    #[test]
    fn apply_then_invert_returns_to_original() {
        let mut b = RopeBuffer::from_str("alpha beta gamma");
        let original = b.to_string();
        let edit = Edit::replace(6..10, "delta");
        let inv = b.apply_edit(&edit);
        assert_eq!(b.to_string(), "alpha delta gamma");
        let inv_edit = Edit {
            range: inv.range,
            replacement: inv.replacement,
            kind: inv.kind,
        };
        b.apply_edit(&inv_edit);
        assert_eq!(b.to_string(), original);
    }

    #[test]
    fn line_byte_range_correct() {
        let b = RopeBuffer::from_str("aa\nbbb\nc");
        assert_eq!(b.line_byte_range(0), 0..2);
        assert_eq!(b.line_byte_range(1), 3..6);
        assert_eq!(b.line_byte_range(2), 7..8);
    }
}
