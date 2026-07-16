use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub pos: usize,
    pub anchor: Option<usize>,
    pub sticky_col: Option<u32>,
}

impl Cursor {
    pub fn new(pos: usize) -> Self {
        Self {
            pos,
            anchor: None,
            sticky_col: None,
        }
    }

    pub fn has_selection(&self) -> bool {
        match self.anchor {
            Some(a) => a != self.pos,
            None => false,
        }
    }

    pub fn selection_range(&self) -> Option<Range<usize>> {
        let a = self.anchor?;
        if a == self.pos {
            return None;
        }
        Some(if a < self.pos { a..self.pos } else { self.pos..a })
    }

    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    pub fn start_or_extend_selection(&mut self) {
        if self.anchor.is_none() {
            self.anchor = Some(self.pos);
        }
    }

    pub fn move_to(&mut self, new_pos: usize, extend_selection: bool) {
        if extend_selection {
            self.start_or_extend_selection();
        } else {
            self.clear_selection();
        }
        self.pos = new_pos;
        self.sticky_col = None;
    }

    pub fn move_vertical(&mut self, new_pos: usize, sticky_col: u32, extend_selection: bool) {
        if extend_selection {
            self.start_or_extend_selection();
        } else {
            self.clear_selection();
        }
        self.pos = new_pos;
        self.sticky_col = Some(sticky_col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cursor_has_no_selection() {
        let c = Cursor::new(5);
        assert_eq!(c.pos, 5);
        assert!(c.anchor.is_none());
        assert!(!c.has_selection());
        assert!(c.selection_range().is_none());
    }

    #[test]
    fn anchor_equal_to_pos_no_selection() {
        let c = Cursor {
            pos: 7,
            anchor: Some(7),
            sticky_col: None,
        };
        assert!(!c.has_selection());
        assert!(c.selection_range().is_none());
    }

    #[test]
    fn selection_range_normalized() {
        let c1 = Cursor {
            pos: 10,
            anchor: Some(3),
            sticky_col: None,
        };
        assert_eq!(c1.selection_range(), Some(3..10));
        let c2 = Cursor {
            pos: 3,
            anchor: Some(10),
            sticky_col: None,
        };
        assert_eq!(c2.selection_range(), Some(3..10));
    }

    #[test]
    fn move_to_clears_sticky_when_not_extending() {
        let mut c = Cursor {
            pos: 5,
            anchor: None,
            sticky_col: Some(3),
        };
        c.move_to(8, false);
        assert_eq!(c.pos, 8);
        assert!(c.sticky_col.is_none());
    }

    #[test]
    fn move_to_extending_starts_anchor() {
        let mut c = Cursor::new(5);
        c.move_to(10, true);
        assert_eq!(c.anchor, Some(5));
        assert_eq!(c.pos, 10);
    }

    #[test]
    fn move_to_not_extending_clears_anchor() {
        let mut c = Cursor {
            pos: 5,
            anchor: Some(2),
            sticky_col: None,
        };
        c.move_to(10, false);
        assert!(c.anchor.is_none());
    }

    #[test]
    fn move_vertical_preserves_sticky_col() {
        let mut c = Cursor::new(5);
        c.move_vertical(20, 4, false);
        assert_eq!(c.pos, 20);
        assert_eq!(c.sticky_col, Some(4));
    }
}
