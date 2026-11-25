use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use bitflags::bitflags;
use unicode_width::UnicodeWidthChar;

use super::mouse::{MouseEncoding, MouseMode};

/// Строк истории по умолчанию ([`TerminalConfig::scrollback`](super::TerminalConfig::scrollback)).
pub const DEFAULT_SCROLLBACK: usize = 5_000;
const MAX_LINK_POOL: usize = 4_096;

bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub struct CellFlags: u8 {
        const BOLD        = 0b0000_0001;
        const FAINT       = 0b0000_0010;
        const ITALIC      = 0b0000_0100;
        const UNDERLINE   = 0b0000_1000;
        const REVERSE     = 0b0001_0000;
        const STRIKE      = 0b0010_0000;
        /// Первая ячейка двухколоночного глифа (CJK, эмодзи).
        const WIDE        = 0b0100_0000;
        /// Вторая (пустая) ячейка-продолжение двухколоночного глифа.
        const WIDE_SPACER = 0b1000_0000;
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

#[derive(Clone, Copy, Default, Debug)]
struct SavedCursor {
    cursor: Cursor,
    attr: Attr,
    origin_mode: bool,
}

struct InactiveBuffer {
    cells: Vec<Vec<Cell>>,
    wrapped: Vec<bool>,
    cursor: Cursor,
    current_attr: Attr,
    saved_cursor: Option<SavedCursor>,
    scroll_top: usize,
    scroll_bottom: usize,
    origin_mode: bool,
}

pub struct Grid {
    rows: usize,
    cols: usize,
    cells: Vec<Vec<Cell>>,
    wrapped_view: Vec<bool>,
    scrollback: VecDeque<Vec<Cell>>,
    wrapped_scrollback: VecDeque<bool>,
    cursor: Cursor,
    saved_cursor: Option<SavedCursor>,
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
    /// IRM (`CSI 4 h/l`): вставка символов сдвигает хвост строки вправо.
    insert_mode: bool,
    /// DECCKM (`CSI ? 1 h/l`): стрелки/Home/End шлют SS3 (`\x1bO...`).
    app_cursor: bool,
    /// DECOM (`CSI ? 6 h/l`): координаты CUP/VPA относительно scroll region.
    origin_mode: bool,
    /// DECAWM (`CSI ? 7 h/l`): автоперенос в конце строки.
    auto_wrap: bool,
    /// Synchronized Output (`CSI ? 2026 h/l`): буферизация перерисовки кадра.
    sync_update: bool,
    /// Последний напечатанный графический символ (для `CSI n b` REP).
    last_printed: Option<char>,
    current_link_id: Option<u32>,
    links: Vec<Arc<str>>,
    link_lookup: HashMap<Arc<str>, u32>,
    max_scrollback: usize,
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
            insert_mode: false,
            app_cursor: false,
            origin_mode: false,
            auto_wrap: true,
            sync_update: false,
            last_printed: None,
            current_link_id: None,
            links: Vec::new(),
            link_lookup: HashMap::new(),
            max_scrollback: DEFAULT_SCROLLBACK,
        }
    }

    /// Предел истории; лишние старые строки отбрасываются сразу.
    pub fn set_max_scrollback(&mut self, n: usize) {
        self.max_scrollback = n;
        while self.scrollback.len() > n {
            self.scrollback.pop_front();
            self.wrapped_scrollback.pop_front();
        }
        self.revision += 1;
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
    #[allow(dead_code)]
    pub fn insert_mode(&self) -> bool {
        self.insert_mode
    }
    pub fn set_insert_mode(&mut self, on: bool) {
        self.insert_mode = on;
    }
    pub fn app_cursor(&self) -> bool {
        self.app_cursor
    }
    pub fn set_app_cursor(&mut self, on: bool) {
        self.app_cursor = on;
    }
    #[allow(dead_code)]
    pub fn origin_mode(&self) -> bool {
        self.origin_mode
    }
    pub fn set_origin_mode(&mut self, on: bool) {
        self.origin_mode = on;
        self.cursor.row = if on { self.scroll_top } else { 0 };
        self.cursor.col = 0;
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }
    #[allow(dead_code)]
    pub fn auto_wrap(&self) -> bool {
        self.auto_wrap
    }
    pub fn set_auto_wrap(&mut self, on: bool) {
        self.auto_wrap = on;
    }
    pub fn sync_update(&self) -> bool {
        self.sync_update
    }
    pub fn set_sync_update(&mut self, on: bool) {
        self.sync_update = on;
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

    /// Полный сброс терминала (`ESC c`, RIS).
    pub fn reset(&mut self) {
        if self.on_alt {
            self.exit_alt_screen();
        }
        self.clear_active();
        self.mouse_mode = MouseMode::default();
        self.mouse_encoding = MouseEncoding::default();
        self.bracketed_paste = false;
        self.focus_events = false;
        self.cursor_visible = true;
        self.alt_scroll = true;
        self.insert_mode = false;
        self.app_cursor = false;
        self.origin_mode = false;
        self.auto_wrap = true;
        self.sync_update = false;
        self.current_link_id = None;
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
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
        self.origin_mode = false;
        self.last_printed = None;
    }

    fn swap_with_inactive(&mut self) {
        let active_snapshot = InactiveBuffer {
            cells: std::mem::take(&mut self.cells),
            wrapped: std::mem::take(&mut self.wrapped_view),
            cursor: self.cursor,
            current_attr: self.current_attr,
            saved_cursor: self.saved_cursor,
            scroll_top: self.scroll_top,
            scroll_bottom: self.scroll_bottom,
            origin_mode: self.origin_mode,
        };
        let restore = self.inactive.take().unwrap_or_else(|| InactiveBuffer {
            cells: vec![vec![Cell::default(); self.cols]; self.rows],
            wrapped: vec![false; self.rows],
            cursor: Cursor::default(),
            current_attr: Attr::default(),
            saved_cursor: None,
            scroll_top: 0,
            scroll_bottom: self.rows.saturating_sub(1),
            origin_mode: false,
        });
        self.cells = restore.cells;
        self.wrapped_view = restore.wrapped;
        self.cursor = restore.cursor;
        self.current_attr = restore.current_attr;
        self.saved_cursor = restore.saved_cursor;
        self.scroll_top = restore.scroll_top.min(self.rows.saturating_sub(1));
        self.scroll_bottom = restore
            .scroll_bottom
            .min(self.rows.saturating_sub(1))
            .max(self.scroll_top);
        self.origin_mode = restore.origin_mode;
        self.last_printed = None;
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

    /// Смена размера. Строки — как в xterm/alacritty: при уменьшении сначала
    /// убираются строки ниже курсора, и только если их не хватило — верхние
    /// (в историю), курсор сдвигается вместе с текстом; при увеличении строки
    /// возвращаются из истории наверх. Раньше лишние строки всегда снимались
    /// сверху, а курсор оставался на месте: открылась экранная клавиатура —
    /// и уже выведенный текст (приглашение sudo) уезжал в историю, экран
    /// выглядел пустым до следующего вывода.
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
            let (dropped, _) = shrink_rows(
                &mut self.cells,
                &mut self.wrapped_view,
                &mut self.cursor,
                self.saved_cursor.as_mut().map(|s| &mut s.cursor),
                rows,
            );
            if !self.on_alt {
                for (line, w) in dropped {
                    self.push_scrollback_with_wrap(line, w);
                }
            }
        } else if rows > self.rows {
            let add = rows - self.rows;
            // На основном экране — вернуть строки из истории наверх.
            let pull = if self.on_alt {
                0
            } else {
                add.min(self.scrollback.len())
            };
            for _ in 0..pull {
                let line = self
                    .scrollback
                    .pop_back()
                    .unwrap_or_else(|| vec![Cell::default(); cols]);
                let w = self.wrapped_scrollback.pop_back().unwrap_or(false);
                self.cells.insert(0, line);
                self.wrapped_view.insert(0, w);
            }
            self.cursor.row += pull;
            if let Some(s) = self.saved_cursor.as_mut() {
                s.cursor.row += pull;
            }
            for _ in 0..(add - pull) {
                self.cells.push(vec![Cell::default(); cols]);
                self.wrapped_view.push(false);
            }
        }
        if let Some(inactive) = self.inactive.as_mut() {
            for line in inactive.cells.iter_mut() {
                line.resize(cols, Cell::default());
            }
            if rows < inactive.cells.len() {
                let (dropped, _) = shrink_rows(
                    &mut inactive.cells,
                    &mut inactive.wrapped,
                    &mut inactive.cursor,
                    inactive.saved_cursor.as_mut().map(|s| &mut s.cursor),
                    rows,
                );
                // Неактивный буфер при on_alt — основной экран: его строки — в историю.
                if self.on_alt {
                    for (line, w) in dropped {
                        if self.max_scrollback == 0 {
                            break;
                        }
                        while self.scrollback.len() >= self.max_scrollback {
                            self.scrollback.pop_front();
                            self.wrapped_scrollback.pop_front();
                        }
                        self.scrollback.push_back(line);
                        self.wrapped_scrollback.push_back(w);
                    }
                }
            } else {
                while inactive.cells.len() < rows {
                    inactive.cells.push(vec![Cell::default(); cols]);
                    inactive.wrapped.push(false);
                }
            }
            inactive.cursor.row = inactive.cursor.row.min(rows - 1);
            inactive.cursor.col = inactive.cursor.col.min(cols - 1);
            inactive.cursor.wrap_pending = false;
            inactive.scroll_top = 0;
            inactive.scroll_bottom = rows - 1;
        }
        self.cols = cols;
        self.rows = rows;
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
        self.cursor.row = self.cursor.row.min(rows - 1);
        self.cursor.col = self.cursor.col.min(cols - 1);
        self.cursor.wrap_pending = false;
        if let Some(s) = self.saved_cursor.as_mut() {
            s.cursor.row = s.cursor.row.min(rows - 1);
            s.cursor.col = s.cursor.col.min(cols - 1);
        }
        self.revision += 1;
    }

    fn push_scrollback_with_wrap(&mut self, line: Vec<Cell>, wrapped: bool) {
        if self.max_scrollback == 0 {
            return;
        }
        while self.scrollback.len() >= self.max_scrollback {
            self.scrollback.pop_front();
            self.wrapped_scrollback.pop_front();
        }
        self.scrollback.push_back(line);
        self.wrapped_scrollback.push_back(wrapped);
    }

    fn blank_cell(&self) -> Cell {
        Cell {
            bg: self.current_attr.bg,
            ..Cell::default()
        }
    }

    /// Если в `(row, col)` лежит половина двухколоночного символа, зачищает
    /// его парную ячейку, чтобы на экране не оставались «осиротевшие» половинки.
    fn clear_wide_at(&mut self, row: usize, col: usize) {
        if row >= self.rows || col >= self.cols {
            return;
        }
        let flags = self.cells[row][col].flags;
        if flags.contains(CellFlags::WIDE) && col + 1 < self.cols {
            let next = &mut self.cells[row][col + 1];
            if next.flags.contains(CellFlags::WIDE_SPACER) {
                next.ch = ' ';
                next.flags.remove(CellFlags::WIDE | CellFlags::WIDE_SPACER);
            }
        } else if flags.contains(CellFlags::WIDE_SPACER) && col > 0 {
            let prev = &mut self.cells[row][col - 1];
            if prev.flags.contains(CellFlags::WIDE) {
                prev.ch = ' ';
                prev.flags.remove(CellFlags::WIDE | CellFlags::WIDE_SPACER);
            }
        }
    }

    pub fn print(&mut self, ch: char) {
        let width = ch.width().unwrap_or(0);
        if width == 0 {
            return;
        }
        let w = if width >= 2 && self.cols >= 2 { 2 } else { 1 };

        if self.cursor.wrap_pending {
            if self.auto_wrap {
                let prev_row = self.cursor.row;
                if prev_row < self.wrapped_view.len() {
                    self.wrapped_view[prev_row] = true;
                }
                self.lf();
                self.cr();
            }
            self.cursor.wrap_pending = false;
        }

        if self.cursor.col >= self.cols {
            self.cursor.col = self.cols - 1;
        }

        // Двухколоночный глиф не помещается в последней колонке — переносим
        // целиком на следующую строку, а последнюю ячейку текущей очищаем.
        if w == 2 && self.cursor.col + 1 >= self.cols {
            if self.auto_wrap {
                let row = self.cursor.row;
                let last = self.cols - 1;
                self.clear_wide_at(row, last);
                self.cells[row][last] = self.blank_cell();
                if row < self.wrapped_view.len() {
                    self.wrapped_view[row] = true;
                }
                self.lf();
                self.cr();
            } else {
                self.cursor.col = self.cols - 2;
            }
        }

        if self.insert_mode {
            self.insert_chars(w);
        }

        let row = self.cursor.row;
        let col = self.cursor.col;
        let base_flags = self
            .current_attr
            .flags
            .difference(CellFlags::WIDE | CellFlags::WIDE_SPACER);

        if row < self.rows && col < self.cols {
            self.clear_wide_at(row, col);
            if w == 2 && col + 1 < self.cols {
                self.clear_wide_at(row, col + 1);
            }
            self.cells[row][col] = Cell {
                ch,
                fg: self.current_attr.fg,
                bg: self.current_attr.bg,
                flags: if w == 2 {
                    base_flags | CellFlags::WIDE
                } else {
                    base_flags
                },
                link_id: self.current_link_id,
            };
            if w == 2 && col + 1 < self.cols {
                self.cells[row][col + 1] = Cell {
                    ch: ' ',
                    fg: self.current_attr.fg,
                    bg: self.current_attr.bg,
                    flags: base_flags | CellFlags::WIDE_SPACER,
                    link_id: self.current_link_id,
                };
            }
            if col + w < self.cols {
                self.wrapped_view[row] = false;
            }
        }

        self.last_printed = Some(ch);
        if col + w >= self.cols {
            self.cursor.col = self.cols - 1;
            self.cursor.wrap_pending = self.auto_wrap;
        } else {
            self.cursor.col = col + w;
        }
        self.revision += 1;
    }

    /// `CSI n b` (REP) — повторить последний напечатанный графический символ `n` раз.
    pub fn repeat_char(&mut self, n: usize) {
        let Some(ch) = self.last_printed else {
            return;
        };
        let cap = self.cols.saturating_mul(self.rows).max(1);
        for _ in 0..n.min(cap) {
            self.print(ch);
        }
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

    /// `ESC M` (RI — Reverse Index): сдвинуть курсор на строку вверх; если
    /// курсор на верхней границе scroll region — прокрутить область вниз.
    pub fn reverse_index(&mut self) {
        if self.cursor.row == self.scroll_top {
            self.scroll_down_region(1);
        } else if self.cursor.row > 0 {
            self.cursor.row -= 1;
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

    /// Позиционирование с учётом DECOM (Origin Mode, `CSI ? 6 h`).
    pub fn move_to_cup(&mut self, row: usize, col: usize) {
        let target_row = if self.origin_mode {
            self.scroll_top.saturating_add(row).min(self.scroll_bottom)
        } else {
            row.min(self.rows - 1)
        };
        self.cursor.row = target_row;
        self.cursor.col = col.min(self.cols - 1);
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn move_relative(&mut self, drow: i32, dcol: i32) {
        let new_row = if drow < 0 {
            let min_row = if self.cursor.row >= self.scroll_top {
                self.scroll_top as i32
            } else {
                0
            };
            (self.cursor.row as i32 + drow).max(min_row) as usize
        } else if drow > 0 {
            let max_row = if self.cursor.row <= self.scroll_bottom {
                self.scroll_bottom as i32
            } else {
                self.rows as i32 - 1
            };
            (self.cursor.row as i32 + drow).min(max_row) as usize
        } else {
            self.cursor.row
        };
        let new_col = (self.cursor.col as i32 + dcol).clamp(0, self.cols as i32 - 1) as usize;
        self.cursor.row = new_row;
        self.cursor.col = new_col;
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn save_cursor(&mut self) {
        self.saved_cursor = Some(SavedCursor {
            cursor: self.cursor,
            attr: self.current_attr,
            origin_mode: self.origin_mode,
        });
    }

    pub fn restore_cursor(&mut self) {
        if let Some(saved) = self.saved_cursor {
            self.cursor = saved.cursor;
            self.cursor.row = self.cursor.row.min(self.rows - 1);
            self.cursor.col = self.cursor.col.min(self.cols - 1);
            self.current_attr = saved.attr;
            self.origin_mode = saved.origin_mode;
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
        // По спецификации VT100/xterm DECSTBM всегда возвращает курсор в начало.
        self.cursor.row = if self.origin_mode { self.scroll_top } else { 0 };
        self.cursor.col = 0;
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    /// `CSI n L` (IL — Insert Lines): вставить `n` пустых строк на текущей
    /// строке внутри scroll region, сдвинув нижние строки вниз.
    pub fn insert_lines(&mut self, n: usize) {
        if self.cursor.row < self.scroll_top || self.cursor.row > self.scroll_bottom || n == 0 {
            return;
        }
        let row = self.cursor.row;
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - row + 1);
        let blank = self.blank_cell();
        for _ in 0..n {
            let _ = self.cells.remove(bottom);
            if bottom < self.wrapped_view.len() {
                let _ = self.wrapped_view.remove(bottom);
            }
            self.cells.insert(row, vec![blank; self.cols]);
            if row <= self.wrapped_view.len() {
                self.wrapped_view.insert(row, false);
            } else {
                self.wrapped_view.push(false);
            }
        }
        self.cursor.col = 0;
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    /// `CSI n M` (DL — Delete Lines): удалить `n` строк начиная с текущей
    /// внутри scroll region, сдвинув нижние строки вверх.
    pub fn delete_lines(&mut self, n: usize) {
        if self.cursor.row < self.scroll_top || self.cursor.row > self.scroll_bottom || n == 0 {
            return;
        }
        let row = self.cursor.row;
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - row + 1);
        let blank = self.blank_cell();
        for _ in 0..n {
            let _ = self.cells.remove(row);
            if row < self.wrapped_view.len() {
                let _ = self.wrapped_view.remove(row);
            }
            self.cells.insert(bottom, vec![blank; self.cols]);
            if bottom <= self.wrapped_view.len() {
                self.wrapped_view.insert(bottom, false);
            } else {
                self.wrapped_view.push(false);
            }
        }
        self.cursor.col = 0;
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    /// `CSI n @` (ICH — Insert Blank Characters): вставить `n` пробелов в
    /// позиции курсора со сдвигом хвоста строки вправо.
    pub fn insert_chars(&mut self, n: usize) {
        let r = self.cursor.row;
        let c = self.cursor.col;
        if r >= self.rows || c >= self.cols || n == 0 {
            return;
        }
        let n = n.min(self.cols - c);
        let blank = self.blank_cell();
        self.clear_wide_at(r, c);
        let row = &mut self.cells[r];
        row.copy_within(c..(self.cols - n), c + n);
        for cell in &mut row[c..(c + n)] {
            *cell = blank;
        }
        // Если двухколоночный символ оказался разрезан правым краем — очищаем.
        if row[self.cols - 1].flags.contains(CellFlags::WIDE) {
            row[self.cols - 1] = blank;
        }
        if r < self.wrapped_view.len() {
            self.wrapped_view[r] = false;
        }
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    /// `CSI n P` (DCH — Delete Characters): удалить `n` символов в позиции
    /// курсора со сдвигом хвоста строки влево.
    pub fn delete_chars(&mut self, n: usize) {
        let r = self.cursor.row;
        let c = self.cursor.col;
        if r >= self.rows || c >= self.cols || n == 0 {
            return;
        }
        let n = n.min(self.cols - c);
        let blank = self.blank_cell();
        self.clear_wide_at(r, c);
        if c + n < self.cols {
            self.clear_wide_at(r, c + n);
        }
        let row = &mut self.cells[r];
        row.copy_within((c + n)..self.cols, c);
        for cell in &mut row[(self.cols - n)..self.cols] {
            *cell = blank;
        }
        if r < self.wrapped_view.len() {
            self.wrapped_view[r] = false;
        }
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    /// `CSI n X` (ECH — Erase Characters): очистить `n` ячеек начиная с
    /// позиции курсора без его перемещения.
    pub fn erase_chars(&mut self, n: usize) {
        let r = self.cursor.row;
        let c = self.cursor.col;
        if r >= self.rows || c >= self.cols || n == 0 {
            return;
        }
        let end = (c + n).min(self.cols);
        let blank = self.blank_cell();
        self.clear_wide_at(r, c);
        if end < self.cols {
            self.clear_wide_at(r, end);
        } else {
            self.clear_wide_at(r, self.cols - 1);
        }
        for cell in &mut self.cells[r][c..end] {
            *cell = blank;
        }
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn erase_display(&mut self, mode: u16) {
        let blank = self.blank_cell();
        match mode {
            0 => {
                let r = self.cursor.row;
                let c = self.cursor.col;
                if r < self.rows {
                    if c < self.cols {
                        self.clear_wide_at(r, c);
                    }
                    for col in c..self.cols {
                        self.cells[r][col] = blank;
                    }
                    if r < self.wrapped_view.len() {
                        self.wrapped_view[r] = false;
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
                    let end = c.min(self.cols - 1);
                    self.clear_wide_at(r, end);
                    for col in 0..=end {
                        self.cells[r][col] = blank;
                    }
                }
            }
            2 => {
                for row in 0..self.rows {
                    for col in 0..self.cols {
                        self.cells[row][col] = blank;
                    }
                }
                for w in self.wrapped_view.iter_mut() {
                    *w = false;
                }
            }
            3 => {
                self.scrollback.clear();
                self.wrapped_scrollback.clear();
            }
            _ => {}
        }
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn erase_line(&mut self, mode: u16) {
        let blank = self.blank_cell();
        let r = self.cursor.row;
        if r >= self.rows {
            return;
        }
        match mode {
            0 => {
                let c = self.cursor.col;
                if c < self.cols {
                    self.clear_wide_at(r, c);
                }
                for col in c..self.cols {
                    self.cells[r][col] = blank;
                }
                if r < self.wrapped_view.len() {
                    self.wrapped_view[r] = false;
                }
            }
            1 => {
                let end = self.cursor.col.min(self.cols - 1);
                self.clear_wide_at(r, end);
                for col in 0..=end {
                    self.cells[r][col] = blank;
                }
            }
            2 => {
                for col in 0..self.cols {
                    self.cells[r][col] = blank;
                }
                if r < self.wrapped_view.len() {
                    self.wrapped_view[r] = false;
                }
            }
            _ => {}
        }
        self.cursor.wrap_pending = false;
        self.revision += 1;
    }

    pub fn scroll_up_region(&mut self, n: usize) {
        let top = self.scroll_top;
        let bottom = self.scroll_bottom;
        let n = n.min(bottom - top + 1);
        let full_screen = top == 0 && bottom == self.rows - 1;
        let push_to_scrollback = full_screen && !self.on_alt;
        let blank = self.blank_cell();
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
            self.cells.insert(bottom, vec![blank; self.cols]);
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
        let blank = self.blank_cell();
        for _ in 0..n {
            let _ = self.cells.remove(bottom);
            if bottom < self.wrapped_view.len() {
                let _ = self.wrapped_view.remove(bottom);
            }
            self.cells.insert(top, vec![blank; self.cols]);
            if top <= self.wrapped_view.len() {
                self.wrapped_view.insert(top, false);
            } else {
                self.wrapped_view.push(false);
            }
        }
        self.revision += 1;
    }
}

/// Уменьшить экран до `rows` строк: сначала убрать строки ниже курсора,
/// затем — верхние (их возвращает вызывающий, для истории); курсор и
/// сохранённый курсор сдвигаются вместе с текстом.
fn shrink_rows(
    cells: &mut Vec<Vec<Cell>>,
    wrapped: &mut Vec<bool>,
    cursor: &mut Cursor,
    saved: Option<&mut Cursor>,
    rows: usize,
) -> (Vec<(Vec<Cell>, bool)>, usize) {
    let cur_len = cells.len();
    if rows >= cur_len {
        return (Vec::new(), 0);
    }
    let excess = cur_len - rows;
    let below = cur_len.saturating_sub(cursor.row + 1);
    let trim_bottom = excess.min(below);
    cells.truncate(cur_len - trim_bottom);
    wrapped.truncate(cur_len - trim_bottom);
    let drop = excess - trim_bottom;
    let lines: Vec<Vec<Cell>> = cells.drain(..drop).collect();
    let ws: Vec<bool> = wrapped.drain(..drop.min(wrapped.len())).collect();
    cursor.row = cursor.row.saturating_sub(drop);
    if let Some(c) = saved {
        c.row = c.row.saturating_sub(drop);
    }
    let out = lines
        .into_iter()
        .enumerate()
        .map(|(i, l)| (l, ws.get(i).copied().unwrap_or(false)))
        .collect();
    (out, drop)
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
        for ch in "AAAAA".chars() {
            g.print(ch);
        }
        g.cr();
        g.lf();
        for ch in "BBBBB".chars() {
            g.print(ch);
        }
        g.cr();
        g.lf();
        for ch in "CCCCC".chars() {
            g.print(ch);
        }
        g.cr();
        g.lf();
        for ch in "DDDDD".chars() {
            g.print(ch);
        }
        let total = g.total_lines();
        assert!(total >= 4);
        let last = g.line_global((total - 1) as i32).unwrap();
        let s: String = last.iter().map(|c| c.ch).collect();
        assert!(s.starts_with("DDDDD"), "last line was '{s}'");
    }

    #[test]
    fn wrap_flag_set_on_wraparound() {
        let mut g = Grid::new(3, 2);
        for ch in "ABCD".chars() {
            g.print(ch);
        }
        assert!(g.is_wrapped(0));
        assert!(!g.is_wrapped(1));
    }

    #[test]
    fn cell_link_id_propagates() {
        let mut g = Grid::new(5, 1);
        let id = g.intern_link("https://x.test");
        g.set_current_link(Some(id));
        for ch in "abc".chars() {
            g.print(ch);
        }
        g.set_current_link(None);
        for ch in "de".chars() {
            g.print(ch);
        }
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

    fn row_text(g: &Grid, r: usize) -> String {
        g.line(r)
            .iter()
            .map(|c| c.ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn shrink_keeps_text_above_cursor_visible() {
        // Экранная клавиатура уменьшила высоту: вывод сверху остаётся на экране.
        let mut g = Grid::new(10, 20);
        for ch in "hello".chars() {
            g.print(ch);
        }
        g.cr();
        g.lf();
        for ch in "pw:".chars() {
            g.print(ch);
        }
        g.resize(10, 8);
        assert_eq!(row_text(&g, 0), "hello");
        assert_eq!(row_text(&g, 1), "pw:");
        assert_eq!(g.cursor().row, 1);
        assert_eq!(g.scrollback_len(), 0);
        // и обратно
        g.resize(10, 20);
        assert_eq!(row_text(&g, 0), "hello");
        assert_eq!(g.cursor().row, 1);
    }

    #[test]
    fn shrink_below_cursor_scrolls_top_into_history_and_grow_restores() {
        let mut g = Grid::new(5, 4);
        for (i, w) in ["a", "b", "c", "d"].iter().enumerate() {
            for ch in w.chars() {
                g.print(ch);
            }
            if i < 3 {
                g.cr();
                g.lf();
            }
        }
        assert_eq!(g.cursor().row, 3);
        g.resize(5, 2);
        assert_eq!(row_text(&g, 0), "c");
        assert_eq!(row_text(&g, 1), "d");
        assert_eq!(g.cursor().row, 1);
        assert_eq!(g.scrollback_len(), 2);
        g.resize(5, 4);
        assert_eq!(row_text(&g, 0), "a");
        assert_eq!(row_text(&g, 3), "d");
        assert_eq!(g.cursor().row, 3);
        assert_eq!(g.scrollback_len(), 0);
    }
}
