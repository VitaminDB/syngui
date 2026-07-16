use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use bitflags::bitflags;

use super::mouse::{MouseEncoding, MouseMode};

const MAX_SCROLLBACK: usize = 5_000;
const MAX_LINK_POOL: usize = 4_096;

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub struct CellFlags: u8 {
        const BOLD       = 0b0000_0001;
        const FAINT      = 0b0000_0010;
        const ITALIC     = 0b0000_0100;
        const UNDERLINE  = 0b0000_1000;
        const REVERSE    = 0b0001_0000;
        const STRIKE     = 0b0010_0000;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CellColor {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Default for CellColor {
    fn default() -> Self {
        CellColor::Default
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: CellColor,
    pub bg: CellColor,
    pub flags: CellFlags,
    pub link_id: Option<u32>,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: CellColor::Default,
            bg: CellColor::Default,
            flags: CellFlags::empty(),
            link_id: None,
        }
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Attr {
    pub fg: CellColor,
    pub bg: CellColor,
    pub flags: CellFlags,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    pub wrap_pending: bool,
}

struct InactiveBuffer {
    cells: Vec<Vec<Cell>>,
    wrapped: Vec<bool>,
    cursor: Cursor,
    current_attr: Attr,
    saved_cursor: Option<Cursor>,
}

pub struct Grid {
    rows: usize,
    cols: usize,
    cells: Vec<Vec<Cell>>,
    wrapped_view: Vec<bool>,
    scrollback: VecDeque<Vec<Cell>>,
    wrapped_scrollback: VecDeque<bool>,
    cursor: Cursor,
    saved_cursor: Option<Cursor>,
    current_attr: Attr,
    scroll_top: usize,
    scroll_bottom: usize,
    revision: u64,

    on_alt: bool,
    inactive: Option<InactiveBuffer>,

    mouse_mode: MouseMode,
    mouse_encoding: MouseEncoding,
    bracketed_paste: bool,
    focus_events: bool,
    cursor_visible: bool,
    alt_scroll: bool,
    current_link_id: Option<u32>,
    links: Vec<Arc<str>>,
    link_lookup: HashMap<Arc<str>, u32>,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            rows,
            cols,
            cells: vec![vec![Cell::default(); cols]; rows],
            scrollback: VecDeque::new(),
            wrapped_view: vec![false; rows],
            wrapped_scrollback: VecDeque::new(),
            cursor: Cursor::default(),
            saved_cursor: None,
            current_attr: Attr::default(),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            revision: 0,
            on_alt: false,
            inactive: None,
            mouse_mode: MouseMode::default(),
            mouse_encoding: MouseEncoding::default(),
            bracketed_paste: false,
            focus_events: false,
            cursor_visible: true,
            alt_scroll: true,
            current_link_id: None,
            links: Vec::new(),
            link_lookup: HashMap::new(),
        }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }
    pub fn cols(&self) -> usize {
        self.cols
    }
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }
    #[allow(dead_code)]
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    pub fn line(&self, row: usize) -> &[Cell] {
        &self.cells[row.min(self.rows - 1)]
    }

    pub fn scrollback_line(&self, idx: usize) -> &[Cell] {
        &self.scrollback[idx]
    }

    pub fn line_global(&self, line: i32) -> Option<&[Cell]> {
        if line < 0 {
            return None;
        }
        let l = line as usize;
        let sb = self.scrollback.len();
        if l < sb {
            Some(&self.scrollback[l])
        } else if l < sb + self.rows {
            Some(&self.cells[l - sb])
        } else {
            None
        }
    }

    pub fn cell_at_global(&self, line: i32, col: u16) -> Option<&Cell> {
        self.line_global(line).and_then(|row| row.get(col as usize))
    }

    pub fn total_lines(&self) -> usize {
        self.scrollback.len() + self.rows
    }

    pub fn is_wrapped(&self, line: i32) -> bool {
        if line < 0 {
            return false;
        }
        let l = line as usize;
        let sb = self.wrapped_scrollback.len();
        if l < sb {
            self.wrapped_scrollback[l]
        } else if l < sb + self.wrapped_view.len() {
            self.wrapped_view[l - sb]
        } else {
            false
        }
    }

    pub fn set_attr(&mut self, attr: Attr) {
        self.current_attr = attr;
        self.revision += 1;
    }

    pub fn current_attr(&self) -> Attr {
        self.current_attr
    }

    pub fn mouse_mode(&self) -> MouseMode {
        self.mouse_mode
    }
    pub fn set_mouse_mode(&mut self, m: MouseMode) {
        self.mouse_mode = m;
    }
    pub fn mouse_encoding(&self) -> MouseEncoding {
        self.mouse_encoding
    }
    pub fn set_mouse_encoding(&mut self, enc: MouseEncoding) {
        self.mouse_encoding = enc;
    }
    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }
    pub fn set_bracketed_paste(&mut self, on: bool) {
        self.bracketed_paste = on;
    }
    pub fn focus_events(&self) -> bool {
        self.focus_events
    }
    pub fn set_focus_events(&mut self, on: bool) {
        self.focus_events = on;
    }
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }
    pub fn set_cursor_visible(&mut self, v: bool) {
        self.cursor_visible = v;
    }
    pub fn alt_scroll(&self) -> bool {
        self.alt_scroll
    }
    pub fn set_alt_scroll(&mut self, on: bool) {
        self.alt_scroll = on;
    }
    pub fn on_alt(&self) -> bool {
        self.on_alt
    }

    pub fn enter_alt_screen(&mut self, clear: bool) {
        if self.on_alt {
            return;
        }
        self.swap_with_inactive();
        self.on_alt = true;
        if clear {
            self.clear_active();
        }
        self.revision += 1;
    }

    pub fn exit_alt_screen(&mut self) {
        if !self.on_alt {
            return;
        }
        self.swap_with_inactive();
        self.on_alt = false;
        self.revision += 1;
    }

    fn clear_active(&mut self) {
        for row in self.cells.iter_mut() {
            for c in row.iter_mut() {
                *c = Cell::default();
            }
        }
        for w in self.wrapped_view.iter_mut() {
            *w = false;
        }
        self.cursor = Cursor::default();
        self.current_attr = Attr::default();
        self.saved_cursor = None;
    }

    fn swap_with_inactive(&mut self) {
        let active_snapshot = InactiveBuffer {
            cells: std::mem::take(&mut self.cells),
            wrapped: std::mem::take(&mut self.wrapped_view),
            cursor: self.cursor,
            current_attr: self.current_attr,
            saved_cursor: self.saved_cursor,
        };
        let restore = self.inactive.take().unwrap_or_else(|| InactiveBuffer {
            cells: vec![vec![Cell::default(); self.cols]; self.rows],
            wrapped: vec![false; self.rows],
            cursor: Cursor::default(),
            current_attr: Attr::default(),
            saved_cursor: None,
        });
        self.cells = restore.cells;
        self.wrapped_view = restore.wrapped;
        self.cursor = restore.cursor;
        self.current_attr = restore.current_attr;
        self.saved_cursor = restore.saved_cursor;
        self.inactive = Some(active_snapshot);
    }

    pub fn intern_link(&mut self, uri: &str) -> u32 {
        if let Some(&id) = self.link_lookup.get(uri) {
            return id;
        }
        if self.links.len() >= MAX_LINK_POOL {
            log::warn!(
                "[syngui terminal] OSC 8 link pool overflow ({}), resetting",
                self.links.len()
            );
            self.links.clear();
            self.link_lookup.clear();
        }
        let arc: Arc<str> = Arc::from(uri);
        let id = self.links.len() as u32;
        self.links.push(arc.clone());
        self.link_lookup.insert(arc, id);
        id
    }

    pub fn link(&self, id: u32) -> Option<&str> {
        self.links.get(id as usize).map(|s| s.as_ref())
    }

    #[allow(dead_code)]
    pub fn current_link_id(&self) -> Option<u32> {
        self.current_link_id
    }
    pub fn set_current_link(&mut self, id: Option<u32>) {
        self.current_link_id = id;
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        for line in self.cells.iter_mut() {
            line.resize(cols, Cell::default());
        }
        for line in self.scrollback.iter_mut() {
            line.resize(cols, Cell::default());
        }
        if rows < self.rows {
            let drop = self.rows - rows;
            let drained: Vec<Vec<Cell>> = self.cells.drain(..drop).collect();
            let drained_w: Vec<bool> = self.wrapped_view.drain(..drop).collect();
            if !self.on_alt {
                for (line, w) in drained.into_iter().zip(drained_w) {
                    self.push_scrollback_with_wrap(line, w);
                }
            }
        } else if rows > self.rows {
            for _ in 0..(rows - self.rows) {
                self.cells.push(vec![Cell::default(); cols]);
                self.wrapped_view.push(false);
            }
        }
        if let Some(inactive) = self.inactive.as_mut() {
            for line in inactive.cells.iter_mut() {
                line.resize(cols, Cell::default());
            }
            if rows < inactive.cells.len() {
                inactive.cells.truncate(rows);
                inactive.wrapped.truncate(rows);
            } else {
                while inactive.cells.len() < rows {
                    inactive.cells.push(vec![Cell::default(); cols]);
                    inactive.wrapped.push(false);
                }
            }
            inactive.cursor.row = inactive.cursor.row.min(rows - 1);
            inactive.cursor.col = inactive.cursor.col.min(cols - 1);
            inactive.cursor.wrap_pending = false;
        }
        self.cols = cols;
        self.rows = rows;
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
        self.cursor.row = self.cursor.row.min(rows - 1);
        self.cursor.col = self.cursor.col.min(cols - 1);
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    fn push_scrollback_with_wrap(&mut self, line: Vec<Cell>, wrapped: bool) {
        if self.scrollback.len() == MAX_SCROLLBACK {
            self.scrollback.pop_front();
            self.wrapped_scrollback.pop_front();
        }
        self.scrollback.push_back(line);
        self.wrapped_scrollback.push_back(wrapped);
    }

    pub fn print(&mut self, ch: char) {
        if self.cursor.wrap_pending {
            let prev_row = self.cursor.row;
            if prev_row < self.wrapped_view.len() {
                self.wrapped_view[prev_row] = true;
            }
            self.lf();
            self.cr();
            self.cursor.wrap_pending = false;
        }
        if self.cursor.col >= self.cols {
            self.cursor.col = self.cols - 1;
        }
        let cell = Cell {
            ch,
            fg: self.current_attr.fg,
            bg: self.current_attr.bg,
            flags: self.current_attr.flags,
            link_id: self.current_link_id,
        };
        let row = self.cursor.row;
        let col = self.cursor.col;
        if row < self.rows && col < self.cols {
            self.cells[row][col] = cell;
            if col + 1 < self.cols {
                self.wrapped_view[row] = false;
            }
        }
        if self.cursor.col + 1 >= self.cols {
            self.cursor.wrap_pending = true;
        } else {
            self.cursor.col += 1;
        }
        self.revision += 1;
    }

    pub fn lf(&mut self) {
        if self.cursor.row == self.scroll_bottom {
            self.scroll_up_region(1);
        } else if self.cursor.row + 1 < self.rows {
            self.cursor.row += 1;
        }
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn cr(&mut self) {
        self.cursor.col = 0;
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn bs(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn tab(&mut self) {
        let next = ((self.cursor.col / 8) + 1) * 8;
        self.cursor.col = next.min(self.cols.saturating_sub(1));
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn move_to(&mut self, row: usize, col: usize) {
        self.cursor.row = row.min(self.rows - 1);
        self.cursor.col = col.min(self.cols - 1);
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn move_relative(&mut self, drow: i32, dcol: i32) {
        let new_row = (self.cursor.row as i32 + drow).clamp(0, self.rows as i32 - 1) as usize;
        let new_col = (self.cursor.col as i32 + dcol).clamp(0, self.cols as i32 - 1) as usize;
        self.cursor.row = new_row;
        self.cursor.col = new_col;
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn save_cursor(&mut self) {
        self.saved_cursor = Some(self.cursor);
    }

    pub fn restore_cursor(&mut self) {
        if let Some(c) = self.saved_cursor {
            self.cursor = c;
            self.cursor.row = self.cursor.row.min(self.rows - 1);
            self.cursor.col = self.cursor.col.min(self.cols - 1);
            self.revision += 1;
        }
    }

    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        let top = top.min(self.rows - 1);
        let bottom = bottom.min(self.rows - 1);
        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
        } else {
            self.scroll_top = 0;
            self.scroll_bottom = self.rows - 1;
        }
        self.revision += 1;
    }

    pub fn erase_display(&mut self, mode: u16) {
        let blank = Cell {
            bg: self.current_attr.bg,
            ..Cell::default()
        };
        match mode {
            0 => {
                let r = self.cursor.row;
                let c = self.cursor.col;
                if r < self.rows {
                    for col in c..self.cols {
                        self.cells[r][col] = blank;
                    }
                    for row in (r + 1)..self.rows {
                        for col in 0..self.cols {
                            self.cells[row][col] = blank;
                        }
                        if row < self.wrapped_view.len() {
                            self.wrapped_view[row] = false;
                        }
                    }
                }
            }
            1 => {
                let r = self.cursor.row;
                let c = self.cursor.col;
                for row in 0..r {
                    for col in 0..self.cols {
                        self.cells[row][col] = blank;
                    }
                    if row < self.wrapped_view.len() {
                        self.wrapped_view[row] = false;
                    }
                }
                if r < self.rows {
                    for col in 0..=c.min(self.cols - 1) {
                        self.cells[r][col] = blank;
                    }
                }
            }
            _ => {
                for row in 0..self.rows {
                    for col in 0..self.cols {
                        self.cells[row][col] = blank;
                    }
                }
                for w in self.wrapped_view.iter_mut() {
                    *w = false;
                }
            }
        }
        self.revision += 1;
    }

    pub fn erase_line(&mut self, mode: u16) {
        let blank = Cell {
            bg: self.current_attr.bg,
            ..Cell::default()
        };
        let r = self.cursor.row;
        if r >= self.rows {
            return;
        }
        match mode {
            0 => {
                for col in self.cursor.col..self.cols {
                    self.cells[r][col] = blank;
                }
                if r < self.wrapped_view.len() {
                    self.wrapped_view[r] = false;
                }
            }
            1 => {
                for col in 0..=self.cursor.col.min(self.cols - 1) {
                    self.cells[r][col] = blank;
                }
            }
            _ => {
                for col in 0..self.cols {
                    self.cells[r][col] = blank;
                }
                if r < self.wrapped_view.len() {
                    self.wrapped_view[r] = false;
                }
            }
        }
        self.revision += 1;
    }

    pub fn scroll_up_region(&mut self, n: usize) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - top + 1);
        let full_screen = top == 0 && bottom == self.rows - 1;
        let push_to_scrollback = full_screen && !self.on_alt;
        for _ in 0..n {
            let line = self.cells.remove(top);
            let wrapped = if top < self.wrapped_view.len() {
                self.wrapped_view.remove(top)
            } else {
                false
            };
            if push_to_scrollback {
                self.push_scrollback_with_wrap(line, wrapped);
            }
            self.cells.insert(bottom, vec![Cell::default(); self.cols]);
            if bottom <= self.wrapped_view.len() {
                self.wrapped_view.insert(bottom, false);
            } else {
                self.wrapped_view.push(false);
            }
        }
        self.revision += 1;
    }

    pub fn scroll_down_region(&mut self, n: usize) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - top + 1);
        for _ in 0..n {
            let _ = self.cells.remove(bottom);
            if bottom < self.wrapped_view.len() {
                let _ = self.wrapped_view.remove(bottom);
            }
            self.cells.insert(top, vec![Cell::default(); self.cols]);
            if top <= self.wrapped_view.len() {
                self.wrapped_view.insert(top, false);
            } else {
                self.wrapped_view.push(false);
            }
        }
        self.revision += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_link_dedups() {
        let mut g = Grid::new(40, 5);
        let id1 = g.intern_link("https://a.test");
        let id2 = g.intern_link("https://a.test");
        let id3 = g.intern_link("https://b.test");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert_eq!(g.link(id1), Some("https://a.test"));
        assert_eq!(g.link(id3), Some("https://b.test"));
    }

    #[test]
    fn intern_link_pool_overflow_resets() {
        let mut g = Grid::new(40, 5);
        for i in 0..(MAX_LINK_POOL + 5) {
            let _ = g.intern_link(&format!("https://{}.test", i));
        }
        assert!(g.links.len() <= MAX_LINK_POOL);
    }

    #[test]
    fn line_global_indexes_scrollback_then_viewport() {
        let mut g = Grid::new(5, 2);
        for ch in "AAAAA".chars() { g.print(ch); }
        g.cr(); g.lf();
        for ch in "BBBBB".chars() { g.print(ch); }
        g.cr(); g.lf();
        for ch in "CCCCC".chars() { g.print(ch); }
        g.cr(); g.lf();
        for ch in "DDDDD".chars() { g.print(ch); }
        let total = g.total_lines();
        assert!(total >= 4);
        let last = g.line_global((total - 1) as i32).unwrap();
        let s: String = last.iter().map(|c| c.ch).collect();
        assert!(s.starts_with("DDDDD"), "last line was '{s}'");
    }

    #[test]
    fn wrap_flag_set_on_wraparound() {
        let mut g = Grid::new(3, 2);
        for ch in "ABCD".chars() { g.print(ch); }
        assert!(g.is_wrapped(0));
        assert!(!g.is_wrapped(1));
    }

    #[test]
    fn cell_link_id_propagates() {
        let mut g = Grid::new(5, 1);
        let id = g.intern_link("https://x.test");
        g.set_current_link(Some(id));
        for ch in "abc".chars() { g.print(ch); }
        g.set_current_link(None);
        for ch in "de".chars() { g.print(ch); }
        let row = g.line(0);
        assert_eq!(row[0].link_id, Some(id));
        assert_eq!(row[1].link_id, Some(id));
        assert_eq!(row[2].link_id, Some(id));
        assert_eq!(row[3].link_id, None);
        assert_eq!(row[4].link_id, None);
    }

    #[test]
    fn mouse_state_setters() {
        let mut g = Grid::new(40, 5);
        assert_eq!(g.mouse_mode(), MouseMode::Off);
        g.set_mouse_mode(MouseMode::Normal);
        assert_eq!(g.mouse_mode(), MouseMode::Normal);
        g.set_bracketed_paste(true);
        assert!(g.bracketed_paste());
        g.set_cursor_visible(false);
        assert!(!g.cursor_visible());
    }
}
