use super::grid::{Cell, Grid};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SelectionMode {
    #[default]
    Simple,
    Word,
    Line,
    Block,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct GridPos {
    pub line: i32,
    pub col: u16,
}

impl GridPos {
    pub fn new(line: i32, col: u16) -> Self {
        Self { line, col }
    }
}

fn normalize_pair(a: GridPos, b: GridPos) -> (GridPos, GridPos) {
    if (a.line, a.col) <= (b.line, b.col) { (a, b) } else { (b, a) }
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || "_-./:~?#%&=+@".contains(ch)
}

pub(super) fn expand_word_boundaries(cells: &[Cell], col: usize) -> (usize, usize) {
    if cells.is_empty() {
        return (0, 0);
    }
    let col = col.min(cells.len() - 1);
    if !is_word_char(cells[col].ch) {
        return (col, col);
    }
    let mut left = col;
    while left > 0 && is_word_char(cells[left - 1].ch) {
        left -= 1;
    }
    let mut right = col;
    while right + 1 < cells.len() && is_word_char(cells[right + 1].ch) {
        right += 1;
    }
    (left, right)
}

#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    anchor: Option<GridPos>,
    cursor: Option<GridPos>,
    mode: SelectionMode,
    pub mouse_selecting: bool,
}

impl SelectionState {
    pub fn start(&mut self, pos: GridPos, mode: SelectionMode) {
        self.anchor = Some(pos);
        self.cursor = Some(pos);
        self.mode = mode;
    }

    pub fn update_cursor(&mut self, pos: GridPos) {
        if self.anchor.is_some() {
            self.cursor = Some(pos);
        }
    }

    pub fn extend_word(&mut self, grid: &Grid) {
        let pos = match self.cursor {
            Some(p) => p,
            None => return,
        };
        let cells = match grid.line_global(pos.line) {
            Some(c) => c,
            None => return,
        };
        let (l, r) = expand_word_boundaries(cells, pos.col as usize);
        self.anchor = Some(GridPos::new(pos.line, l as u16));
        self.cursor = Some(GridPos::new(pos.line, r as u16));
        self.mode = SelectionMode::Word;
    }

    pub fn extend_line(&mut self, grid: &Grid) {
        let pos = match self.cursor {
            Some(p) => p,
            None => return,
        };
        let mut top = pos.line;
        while top > 0 && grid.is_wrapped(top - 1) {
            top -= 1;
        }
        let mut bottom = pos.line;
        while grid.is_wrapped(bottom) {
            bottom += 1;
            // safety: не уйдём за total_lines, потому что grid.is_wrapped
        }
        let last_col = grid.cols().saturating_sub(1) as u16;
        self.anchor = Some(GridPos::new(top, 0));
        self.cursor = Some(GridPos::new(bottom, last_col));
        self.mode = SelectionMode::Line;
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.cursor = None;
        self.mouse_selecting = false;
    }

    pub fn is_active(&self) -> bool {
        self.anchor.is_some() && self.cursor.is_some()
    }

    #[allow(dead_code)]
    pub fn mode(&self) -> SelectionMode {
        self.mode
    }

    pub fn range(&self) -> Option<(GridPos, GridPos)> {
        match (self.anchor, self.cursor) {
            (Some(a), Some(c)) => Some(normalize_pair(a, c)),
            _ => None,
        }
    }

    pub fn cells_in_row(&self, line: i32, line_width: usize) -> Option<(u16, u16)> {
        if line_width == 0 {
            return None;
        }
        let (start, end) = self.range()?;
        let last_col = (line_width - 1) as u16;
        match self.mode {
            SelectionMode::Block => {
                let line_min = start.line.min(end.line);
                let line_max = start.line.max(end.line);
                if line < line_min || line > line_max {
                    return None;
                }
                let c_min = start.col.min(end.col).min(last_col);
                let c_max = start.col.max(end.col).min(last_col);
                Some((c_min, c_max))
            }
            _ => {
                if line < start.line || line > end.line {
                    return None;
                }
                let c0 = if line == start.line { start.col } else { 0 };
                let c1 = if line == end.line { end.col } else { last_col };
                let c0 = c0.min(last_col);
                let c1 = c1.min(last_col);
                if c0 > c1 {
                    None
                } else {
                    Some((c0, c1))
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn contains(&self, pos: GridPos, grid: &Grid) -> bool {
        if !self.is_active() {
            return false;
        }
        let cells = match grid.line_global(pos.line) {
            Some(c) => c,
            None => return false,
        };
        match self.cells_in_row(pos.line, cells.len()) {
            Some((c0, c1)) => pos.col >= c0 && pos.col <= c1,
            None => false,
        }
    }

    pub fn collect_text(&self, grid: &Grid) -> Option<String> {
        let (start, end) = self.range()?;
        match self.mode {
            SelectionMode::Block => Some(collect_block(grid, start, end)),
            _ => Some(collect_linear(grid, start, end)),
        }
    }
}

fn collect_linear(grid: &Grid, start: GridPos, end: GridPos) -> String {
    let mut out = String::new();
    for line in start.line..=end.line {
        let cells = match grid.line_global(line) {
            Some(c) => c,
            None => continue,
        };
        let last_col = (cells.len() - 1) as u16;
        let c0 = if line == start.line { start.col } else { 0 };
        let c1 = if line == end.line { end.col } else { last_col };
        let c0 = c0.min(last_col) as usize;
        let c1 = c1.min(last_col) as usize;
        let text: String = cells[c0..=c1].iter().map(|c| c.ch).collect();
        if line == end.line {
            out.push_str(&text);
        } else {
            out.push_str(text.trim_end_matches(|c: char| c == ' ' || c == '\t'));
            if !grid.is_wrapped(line) {
                out.push('\n');
            }
        }
    }
    out
}

fn collect_block(grid: &Grid, start: GridPos, end: GridPos) -> String {
    let line_min = start.line.min(end.line);
    let line_max = start.line.max(end.line);
    let c_min = start.col.min(end.col);
    let c_max = start.col.max(end.col);
    let mut out = String::new();
    for line in line_min..=line_max {
        if let Some(cells) = grid.line_global(line) {
            let last_col = (cells.len() - 1) as u16;
            let lo = c_min.min(last_col) as usize;
            let hi = c_max.min(last_col) as usize;
            let text: String = cells[lo..=hi].iter().map(|c| c.ch).collect();
            out.push_str(text.trim_end_matches(|c: char| c == ' ' || c == '\t'));
        }
        if line < line_max {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_with(text: &str) -> Grid {
        let mut grid = Grid::new(40, 5);
        for ch in text.chars() {
            if ch == '\n' {
                grid.cr();
                grid.lf();
            } else {
                grid.print(ch);
            }
        }
        grid
    }

    #[test]
    fn expand_word_simple() {
        let cells = grid_with("hello world").line(0).to_vec();
        let (l, r) = expand_word_boundaries(&cells, 2);
        assert_eq!((l, r), (0, 4));
    }

    #[test]
    fn expand_word_path() {
        let cells = grid_with("/usr/local/bin/cargo").line(0).to_vec();
        let (l, r) = expand_word_boundaries(&cells, 5);
        assert_eq!((l, r), (0, 19));
    }

    #[test]
    fn expand_word_url() {
        let cells = grid_with("https://example.com/path?q=1").line(0).to_vec();
        let (l, r) = expand_word_boundaries(&cells, 10);
        assert_eq!((l, r), (0, 27));
    }

    #[test]
    fn expand_word_on_separator_returns_self() {
        let cells = grid_with("foo bar").line(0).to_vec();
        let (l, r) = expand_word_boundaries(&cells, 3);
        assert_eq!((l, r), (3, 3));
    }

    #[test]
    fn cells_in_row_simple_single_line() {
        let mut sel = SelectionState::default();
        sel.start(GridPos::new(0, 2), SelectionMode::Simple);
        sel.update_cursor(GridPos::new(0, 5));
        assert_eq!(sel.cells_in_row(0, 40), Some((2, 5)));
    }

    #[test]
    fn cells_in_row_simple_multi_line() {
        let mut sel = SelectionState::default();
        sel.start(GridPos::new(0, 2), SelectionMode::Simple);
        sel.update_cursor(GridPos::new(2, 5));
        assert_eq!(sel.cells_in_row(0, 40), Some((2, 39)));
        assert_eq!(sel.cells_in_row(1, 40), Some((0, 39)));
        assert_eq!(sel.cells_in_row(2, 40), Some((0, 5)));
        assert_eq!(sel.cells_in_row(3, 40), None);
    }

    #[test]
    fn cells_in_row_block() {
        let mut sel = SelectionState::default();
        sel.start(GridPos::new(1, 5), SelectionMode::Block);
        sel.update_cursor(GridPos::new(3, 10));
        for l in 1..=3 {
            assert_eq!(sel.cells_in_row(l, 40), Some((5, 10)));
        }
        assert_eq!(sel.cells_in_row(0, 40), None);
        assert_eq!(sel.cells_in_row(4, 40), None);
    }

    #[test]
    fn collect_simple_single_line() {
        let grid = grid_with("hello world");
        let mut sel = SelectionState::default();
        sel.start(GridPos::new(0, 0), SelectionMode::Simple);
        sel.update_cursor(GridPos::new(0, 4));
        assert_eq!(sel.collect_text(&grid).unwrap(), "hello");
    }

    #[test]
    fn collect_simple_multi_line() {
        let grid = grid_with("aaa\nbbb\nccc");
        let mut sel = SelectionState::default();
        sel.start(GridPos::new(0, 0), SelectionMode::Simple);
        sel.update_cursor(GridPos::new(2, 2));
        assert_eq!(sel.collect_text(&grid).unwrap(), "aaa\nbbb\nccc");
    }

    #[test]
    fn collect_block() {
        let grid = grid_with("hello world\nfoo bar baz\n123 456 789");
        let mut sel = SelectionState::default();
        sel.start(GridPos::new(0, 0), SelectionMode::Block);
        sel.update_cursor(GridPos::new(2, 2));
        assert_eq!(sel.collect_text(&grid).unwrap(), "hel\nfoo\n123");
    }

    #[test]
    fn range_normalizes_anchor_after_cursor() {
        let mut sel = SelectionState::default();
        sel.start(GridPos::new(2, 5), SelectionMode::Simple);
        sel.update_cursor(GridPos::new(0, 0));
        let (a, b) = sel.range().unwrap();
        assert_eq!(a, GridPos::new(0, 0));
        assert_eq!(b, GridPos::new(2, 5));
    }

    #[test]
    fn extend_word_sets_mode() {
        let grid = grid_with("hello world");
        let mut sel = SelectionState::default();
        sel.start(GridPos::new(0, 7), SelectionMode::Word);
        sel.extend_word(&grid);
        let (a, b) = sel.range().unwrap();
        assert_eq!(a.col, 6);
        assert_eq!(b.col, 10);
        assert_eq!(sel.mode(), SelectionMode::Word);
    }
}
