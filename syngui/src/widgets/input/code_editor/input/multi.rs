use super::cursor::Cursor;
use smallvec::SmallVec;

pub type CursorId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdentifiedCursor {
    id: CursorId,
    cursor: Cursor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursors {
    inner: SmallVec<[IdentifiedCursor; 1]>,
    next_id: CursorId,
    primary_id: CursorId,
}

impl Cursors {
    pub fn single(cursor: Cursor) -> Self {
        let mut inner = SmallVec::new();
        inner.push(IdentifiedCursor { id: 0, cursor });
        Self {
            inner,
            next_id: 1,
            primary_id: 0,
        }
    }

    pub fn at_origin() -> Self {
        Self::single(Cursor::new(0))
    }

    pub fn primary(&self) -> &Cursor {
        let idx = self.primary_index();
        &self.inner[idx].cursor
    }

    pub fn primary_mut(&mut self) -> &mut Cursor {
        let idx = self.primary_index();
        &mut self.inner[idx].cursor
    }

    fn primary_index(&self) -> usize {
        self.inner
            .iter()
            .position(|c| c.id == self.primary_id)
            .expect("primary cursor must exist")
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_single(&self) -> bool {
        self.inner.len() == 1
    }

    pub fn add_cursor(&mut self, pos: usize) -> CursorId {
        if let Some(existing) = self.inner.iter().find(|c| c.cursor.pos == pos && c.cursor.anchor.is_none()) {
            return existing.id;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.inner.push(IdentifiedCursor {
            id,
            cursor: Cursor::new(pos),
        });
        id
    }

    pub fn clear_secondary(&mut self) {
        if self.inner.len() <= 1 {
            return;
        }
        let primary_idx = self.primary_index();
        let primary = self.inner[primary_idx];
        self.inner.clear();
        self.inner.push(primary);
        self.primary_id = primary.id;
    }

    pub fn iter(&self) -> impl Iterator<Item = &Cursor> {
        self.inner.iter().map(|c| &c.cursor)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Cursor> {
        self.inner.iter_mut().map(|c| &mut c.cursor)
    }

    pub fn indices_descending(&self) -> SmallVec<[usize; 4]> {
        let mut idxs: SmallVec<[usize; 4]> = (0..self.inner.len()).collect();
        idxs.sort_by(|&a, &b| {
            self.inner[b]
                .cursor
                .pos
                .cmp(&self.inner[a].cursor.pos)
        });
        idxs
    }

    pub fn get(&self, idx: usize) -> Option<&Cursor> {
        self.inner.get(idx).map(|c| &c.cursor)
    }

    pub fn get_mut(&mut self, idx: usize) -> Option<&mut Cursor> {
        self.inner.get_mut(idx).map(|c| &mut c.cursor)
    }

    pub fn shift_after(&mut self, from_byte: usize, edit_end_old: usize, delta: isize) {
        for ic in self.inner.iter_mut() {
            shift_one(&mut ic.cursor.pos, from_byte, edit_end_old, delta);
            if let Some(a) = ic.cursor.anchor {
                let mut new_a = a;
                shift_one(&mut new_a, from_byte, edit_end_old, delta);
                ic.cursor.anchor = if new_a == ic.cursor.pos { None } else { Some(new_a) };
            }
        }
    }

    pub fn merge_overlapping(&mut self) {
        if self.inner.len() <= 1 {
            return;
        }
        let primary_id = self.primary_id;
        let mut seen: SmallVec<[(usize, Option<usize>); 4]> = SmallVec::new();
        let mut result: SmallVec<[IdentifiedCursor; 1]> = SmallVec::new();
        if let Some(&primary) = self.inner.iter().find(|c| c.id == primary_id) {
            seen.push((primary.cursor.pos, primary.cursor.anchor));
            result.push(primary);
        }
        for ic in self.inner.iter() {
            if ic.id == primary_id {
                continue;
            }
            let key = (ic.cursor.pos, ic.cursor.anchor);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            result.push(*ic);
        }
        self.inner = result;
    }

    pub fn clear_sticky_col(&mut self) {
        for ic in self.inner.iter_mut() {
            ic.cursor.sticky_col = None;
        }
    }
}

fn shift_one(byte: &mut usize, edit_start: usize, edit_end_old: usize, delta: isize) {
    let pos = *byte;
    if pos <= edit_start {
    } else if pos >= edit_end_old {
        *byte = (pos as isize + delta).max(0) as usize;
    } else {
        *byte = edit_start;
    }
}

impl Default for Cursors {
    fn default() -> Self {
        Self::at_origin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_origin_is_single_zero() {
        let c = Cursors::at_origin();
        assert_eq!(c.len(), 1);
        assert_eq!(c.primary().pos, 0);
        assert!(!c.primary().has_selection());
    }

    #[test]
    fn primary_mutable() {
        let mut c = Cursors::at_origin();
        c.primary_mut().pos = 42;
        assert_eq!(c.primary().pos, 42);
    }

    #[test]
    fn add_cursor_creates_new() {
        let mut c = Cursors::at_origin();
        c.add_cursor(5);
        c.add_cursor(10);
        assert_eq!(c.len(), 3);
    }

    #[test]
    fn add_cursor_dedups_same_pos() {
        let mut c = Cursors::at_origin();
        c.add_cursor(5);
        c.add_cursor(5);
        assert_eq!(c.len(), 2, "одинаковая pos без selection — не плодим");
    }

    #[test]
    fn clear_secondary_keeps_primary() {
        let mut c = Cursors::at_origin();
        c.add_cursor(5);
        c.add_cursor(10);
        assert_eq!(c.len(), 3);
        c.clear_secondary();
        assert_eq!(c.len(), 1);
        assert_eq!(c.primary().pos, 0);
    }

    #[test]
    fn indices_descending_sorted_by_pos() {
        let mut c = Cursors::at_origin();
        c.add_cursor(20);
        c.add_cursor(5);
        c.add_cursor(15);
        let order = c.indices_descending();
        let positions: Vec<usize> = order.iter().map(|&i| c.get(i).unwrap().pos).collect();
        assert_eq!(positions, vec![20, 15, 5, 0]);
    }

    #[test]
    fn shift_after_moves_right_cursors() {
        let mut c = Cursors::at_origin();
        c.add_cursor(10);
        c.add_cursor(20);
        c.shift_after(5, 5, 3);
        let positions: Vec<usize> = c.iter().map(|cur| cur.pos).collect();
        assert_eq!(positions, vec![0, 13, 23]);
    }

    #[test]
    fn shift_after_does_not_move_left_cursors() {
        let mut c = Cursors::at_origin();
        c.add_cursor(20);
        c.shift_after(10, 10, 5);
        let positions: Vec<usize> = c.iter().map(|cur| cur.pos).collect();
        assert_eq!(positions, vec![0, 25]);
    }

    #[test]
    fn shift_after_clamps_to_edit_start_inside_range() {
        let mut c = Cursors::at_origin();
        c.add_cursor(7);
        c.shift_after(5, 10, -5);
        let positions: Vec<usize> = c.iter().map(|cur| cur.pos).collect();
        assert_eq!(positions, vec![0, 5]);
    }

    #[test]
    fn merge_overlapping_drops_duplicate_pos() {
        let mut c = Cursors::single(Cursor::new(5));
        c.add_cursor(10);
        c.add_cursor(5);
        c.merge_overlapping();
        assert_eq!(c.len(), 2);
        assert!(c.iter().any(|cur| cur.pos == 5));
        assert!(c.iter().any(|cur| cur.pos == 10));
    }

    #[test]
    fn primary_id_preserved_through_clear_secondary() {
        let mut c = Cursors::at_origin();
        let primary_id = c.primary_id;
        c.add_cursor(5);
        c.add_cursor(10);
        c.clear_secondary();
        assert_eq!(c.primary_id, primary_id);
    }
}
