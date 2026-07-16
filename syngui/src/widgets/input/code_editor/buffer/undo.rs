use super::edit::{EditKind, InverseEdit};
use crate::widgets::input::code_editor::input::Cursors;
use std::time::{Duration, Instant};

const IDLE_THRESHOLD: Duration = Duration::from_millis(800);

const MAX_GROUPS: usize = 200;

#[derive(Debug, Clone)]
pub struct UndoGroup {
    pub edits: Vec<InverseEdit>,
    pub cursors_before: Cursors,
    pub cursors_after: Cursors,
}

#[derive(Debug)]
pub struct UndoStack {
    undo: Vec<UndoGroup>,
    redo: Vec<UndoGroup>,
    current: Option<UndoGroup>,
    last_kind: Option<EditKind>,
    last_time: Option<Instant>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            current: None,
            last_kind: None,
            last_time: None,
        }
    }

    pub fn push(
        &mut self,
        inverse: InverseEdit,
        cursors_before: Cursors,
        cursors_after: Cursors,
        now: Instant,
    ) {
        self.redo.clear();

        let kind = inverse.kind;
        let is_boundary = self.is_group_boundary(kind, now, &inverse);

        if is_boundary {
            self.commit_open();
            self.current = Some(UndoGroup {
                edits: vec![inverse],
                cursors_before,
                cursors_after,
            });
        } else {
            let group = self.current.as_mut().expect("group must exist when not on boundary");
            group.edits.push(inverse);
            group.cursors_after = cursors_after;
        }

        self.last_kind = Some(kind);
        self.last_time = Some(now);
    }

    pub fn commit_group(&mut self) {
        self.commit_open();
        self.last_kind = None;
        self.last_time = None;
    }

    pub fn push_group(
        &mut self,
        edits: Vec<InverseEdit>,
        cursors_before: Cursors,
        cursors_after: Cursors,
    ) {
        if edits.is_empty() {
            return;
        }
        self.redo.clear();
        self.commit_open();
        self.undo.push(UndoGroup {
            edits,
            cursors_before,
            cursors_after,
        });
        if self.undo.len() > MAX_GROUPS {
            self.undo.remove(0);
        }
        self.last_kind = None;
        self.last_time = None;
    }

    pub fn pop_undo(&mut self) -> Option<UndoGroup> {
        self.commit_open();
        self.undo.pop()
    }

    pub fn pop_redo(&mut self) -> Option<UndoGroup> {
        self.redo.pop()
    }

    pub fn push_redo(&mut self, group: UndoGroup) {
        self.redo.push(group);
        if self.redo.len() > MAX_GROUPS {
            self.redo.remove(0);
        }
    }

    pub fn push_undo(&mut self, group: UndoGroup) {
        self.undo.push(group);
        if self.undo.len() > MAX_GROUPS {
            self.undo.remove(0);
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.current = None;
        self.last_kind = None;
        self.last_time = None;
    }

    pub fn can_undo(&self) -> bool {
        self.current.is_some() || !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn commit_open(&mut self) {
        if let Some(group) = self.current.take() {
            self.undo.push(group);
            if self.undo.len() > MAX_GROUPS {
                self.undo.remove(0);
            }
        }
    }

    fn is_group_boundary(&self, kind: EditKind, now: Instant, inverse: &InverseEdit) -> bool {
        if self.current.is_none() {
            return true;
        }
        if matches!(kind, EditKind::Replace) {
            return true;
        }
        if inverse.replacement.contains('\n') || inverse_inserts_newline(inverse) {
            return true;
        }
        if let Some(last) = self.last_kind {
            if last != kind {
                return true;
            }
        }
        if let Some(last_time) = self.last_time {
            if now.saturating_duration_since(last_time) > IDLE_THRESHOLD {
                return true;
            }
        }
        false
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new()
    }
}

fn inverse_inserts_newline(_inverse: &InverseEdit) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::input::code_editor::input::{Cursor, Cursors};

    fn cursors_at(pos: usize) -> Cursors {
        Cursors::single(Cursor::new(pos))
    }

    fn ins_inverse(at: usize, len: usize) -> InverseEdit {
        InverseEdit {
            range: at..at + len,
            replacement: String::new(),
            kind: EditKind::Delete,
        }
    }

    #[test]
    fn empty_stack_has_no_undo() {
        let s = UndoStack::new();
        assert!(!s.can_undo());
        assert!(!s.can_redo());
    }

    #[test]
    fn single_edit_creates_group() {
        let mut s = UndoStack::new();
        let now = Instant::now();
        s.push(ins_inverse(0, 1), cursors_at(0), cursors_at(1), now);
        assert!(s.can_undo());
    }

    #[test]
    fn consecutive_inserts_group_into_one() {
        let mut s = UndoStack::new();
        let t0 = Instant::now();
        for i in 0..5 {
            s.push(ins_inverse(i, 1), cursors_at(i), cursors_at(i + 1), t0);
        }
        let g = s.pop_undo().expect("must have group");
        assert_eq!(g.edits.len(), 5);
        assert!(!s.can_undo());
    }

    #[test]
    fn idle_threshold_creates_new_group() {
        let mut s = UndoStack::new();
        let t0 = Instant::now();
        s.push(ins_inverse(0, 1), cursors_at(0), cursors_at(1), t0);
        let t1 = t0 + Duration::from_secs(1);
        s.push(ins_inverse(1, 1), cursors_at(1), cursors_at(2), t1);
        let g1 = s.pop_undo().expect("group 2");
        assert_eq!(g1.edits.len(), 1);
        let g2 = s.pop_undo().expect("group 1");
        assert_eq!(g2.edits.len(), 1);
    }

    #[test]
    fn replace_always_separate_group() {
        let mut s = UndoStack::new();
        let t = Instant::now();
        s.push(ins_inverse(0, 1), cursors_at(0), cursors_at(1), t);
        let replace_inv = InverseEdit {
            range: 1..3,
            replacement: "ab".into(),
            kind: EditKind::Replace,
        };
        s.push(replace_inv, cursors_at(1), cursors_at(3), t);
        s.push(ins_inverse(3, 1), cursors_at(3), cursors_at(4), t);
        assert!(s.pop_undo().is_some());
        assert!(s.pop_undo().is_some());
        assert!(s.pop_undo().is_some());
        assert!(s.pop_undo().is_none());
    }

    #[test]
    fn redo_invalidated_by_new_edit() {
        let mut s = UndoStack::new();
        let t = Instant::now();
        s.push(ins_inverse(0, 1), cursors_at(0), cursors_at(1), t);
        let g = s.pop_undo().unwrap();
        s.push_redo(g);
        assert!(s.can_redo());
        s.push(ins_inverse(0, 1), cursors_at(0), cursors_at(1), t);
        assert!(!s.can_redo());
    }

    #[test]
    fn push_group_atomic_multi_cursor() {
        let mut s = UndoStack::new();
        let cursors_before = cursors_at(0);
        let cursors_after = cursors_at(1);
        s.push_group(
            vec![ins_inverse(0, 1), ins_inverse(10, 1)],
            cursors_before.clone(),
            cursors_after.clone(),
        );
        let group = s.pop_undo().expect("multi-cursor group");
        assert_eq!(group.edits.len(), 2);
        assert_eq!(group.cursors_before, cursors_before);
        assert_eq!(group.cursors_after, cursors_after);
        assert!(s.pop_undo().is_none(), "should be only one group");
    }

    #[test]
    fn push_group_invalidates_redo() {
        let mut s = UndoStack::new();
        let t = Instant::now();
        s.push(ins_inverse(0, 1), cursors_at(0), cursors_at(1), t);
        let g = s.pop_undo().unwrap();
        s.push_redo(g);
        assert!(s.can_redo());
        s.push_group(vec![ins_inverse(0, 1)], cursors_at(0), cursors_at(1));
        assert!(!s.can_redo(), "push_group invalidates redo stack");
    }

    #[test]
    fn type_switch_creates_boundary() {
        let mut s = UndoStack::new();
        let t = Instant::now();
        s.push(ins_inverse(0, 1), cursors_at(0), cursors_at(1), t);
        s.push(ins_inverse(1, 1), cursors_at(1), cursors_at(2), t);
        let del_inv = InverseEdit {
            range: 1..1,
            replacement: "x".into(),
            kind: EditKind::Insert,
        };
        s.push(del_inv, cursors_at(2), cursors_at(1), t);
        let g_last = s.pop_undo().unwrap();
        assert_eq!(g_last.edits.len(), 1, "Delete отдельной группой");
        let g_prev = s.pop_undo().unwrap();
        assert_eq!(g_prev.edits.len(), 2, "Insert'ы вместе");
    }
}
