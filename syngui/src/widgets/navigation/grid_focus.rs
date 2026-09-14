//! `GridFocus` — модель фокуса для D-pad/пульта: полки (строки) с разным
//! числом карточек, память колонки для каждой строки, движение по
//! [`RemoteKey`]. Хранится в `RwSignal<GridFocus>`, UI читает `row()`/`col()`.
//!
//! ```ignore
//! let focus = use_signal(GridFocus::new(vec![12, 10, 8]));
//! EventHook::new().on_remote(move |k| {
//!     let mut f = focus.get_untracked();
//!     if f.step(k) { focus.set(f); true } else { false }
//! })
//! ```

use crate::widgets::input::RemoteKey;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GridFocus {
    row_lens: Vec<usize>,
    row: usize,
    /// Запомненная колонка каждой строки — при возврате в строку курсор
    /// встаёт на прежнюю карточку, как в Netflix/Apple TV.
    cols: Vec<usize>,
}

impl GridFocus {
    pub fn new(row_lens: Vec<usize>) -> Self {
        let cols = vec![0; row_lens.len()];
        Self {
            row_lens,
            row: 0,
            cols,
        }
    }

    pub fn row(&self) -> usize {
        self.row
    }

    pub fn col(&self) -> usize {
        self.cols.get(self.row).copied().unwrap_or(0)
    }

    /// Колонка, запомненная для произвольной строки.
    pub fn col_of(&self, row: usize) -> usize {
        self.cols.get(row).copied().unwrap_or(0)
    }

    pub fn row_count(&self) -> usize {
        self.row_lens.len()
    }

    pub fn row_len(&self, row: usize) -> usize {
        self.row_lens.get(row).copied().unwrap_or(0)
    }

    pub fn is_focused(&self, row: usize, col: usize) -> bool {
        self.row == row && self.col() == col
    }

    pub fn set(&mut self, row: usize, col: usize) {
        if row >= self.row_lens.len() {
            return;
        }
        self.row = row;
        let max = self.row_lens[row].saturating_sub(1);
        self.cols[row] = col.min(max);
    }

    /// Обновить размеры строк (полки перестроились), сохранив позицию.
    pub fn resize(&mut self, row_lens: Vec<usize>) {
        self.cols.resize(row_lens.len(), 0);
        self.row_lens = row_lens;
        if self.row >= self.row_lens.len() {
            self.row = self.row_lens.len().saturating_sub(1);
        }
        for (i, len) in self.row_lens.iter().enumerate() {
            self.cols[i] = self.cols[i].min(len.saturating_sub(1));
        }
    }

    /// Сдвинуть фокус по кнопке. `true` — позиция изменилась (нужно
    /// перерисовать и считать событие обработанным).
    pub fn step(&mut self, key: RemoteKey) -> bool {
        match key {
            RemoteKey::Left => self.left(),
            RemoteKey::Right => self.right(),
            RemoteKey::Up => self.up(),
            RemoteKey::Down => self.down(),
            _ => false,
        }
    }

    pub fn left(&mut self) -> bool {
        let c = self.col();
        if c == 0 {
            return false;
        }
        self.cols[self.row] = c - 1;
        true
    }

    pub fn right(&mut self) -> bool {
        let c = self.col();
        if c + 1 >= self.row_len(self.row) {
            return false;
        }
        self.cols[self.row] = c + 1;
        true
    }

    pub fn up(&mut self) -> bool {
        if self.row == 0 {
            return false;
        }
        self.row -= 1;
        true
    }

    pub fn down(&mut self) -> bool {
        if self.row + 1 >= self.row_lens.len() {
            return false;
        }
        self.row += 1;
        true
    }
}

/// Смещение полки (в карточках) так, чтобы фокус был виден: пока
/// хватает места, полка не двигается, дальше — прокручивается, оставляя
/// `keep_left` карточек слева от фокуса.
pub fn shelf_offset(col: usize, visible: usize, keep_left: usize) -> usize {
    let visible = visible.max(1);
    let keep_left = keep_left.min(visible - 1);
    col.saturating_sub(visible - 1 - keep_left)
        .min(col.saturating_sub(keep_left))
        .max(col.saturating_sub(visible - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_column_per_row() {
        let mut f = GridFocus::new(vec![5, 3]);
        assert!(f.right());
        assert!(f.right());
        assert!(f.down());
        assert_eq!(f.col(), 0);
        assert!(f.right());
        assert!(f.up());
        assert_eq!((f.row(), f.col()), (0, 2));
        assert!(f.down());
        assert_eq!((f.row(), f.col()), (1, 1));
    }

    #[test]
    fn edges_do_not_move() {
        let mut f = GridFocus::new(vec![2]);
        assert!(!f.left());
        assert!(!f.up());
        assert!(!f.down());
        assert!(f.right());
        assert!(!f.right());
    }

    #[test]
    fn resize_clamps() {
        let mut f = GridFocus::new(vec![10, 10]);
        f.set(1, 9);
        f.resize(vec![10, 3]);
        assert_eq!((f.row(), f.col()), (1, 2));
    }

    #[test]
    fn offset_keeps_focus_visible() {
        assert_eq!(shelf_offset(0, 7, 1), 0);
        assert_eq!(shelf_offset(5, 7, 1), 0);
        assert_eq!(shelf_offset(6, 7, 1), 1);
        assert_eq!(shelf_offset(11, 7, 1), 6);
        assert_eq!(shelf_offset(3, 1, 1), 3);
    }
}
