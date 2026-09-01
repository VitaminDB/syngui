//! Undo/redo редактора.
//!
//! Снапшотная схема: перед каждой правкой в стек кладётся клон модели
//! (документ страницы — единицы-десятки килобайт, клон дешевле и на
//! порядок надёжнее набора обратимых команд; при необходимости командная
//! схема может заменить внутренности без смены интерфейса). Непрерывный
//! набор/стирание в одном блоке группируется по тайм-ауту, как в
//! code_editor/buffer/undo.rs.

use web_time::Instant;

use super::model::{BlockId, DocModel};
use super::state::DocSelection;

/// Класс правки — для группировки последовательных однотипных правок.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditClass {
    /// Набор текста (CharInput/IME) — группируется.
    Typing,
    /// Стирание Backspace/Delete — группируется.
    Deleting,
    /// Структурные правки (Enter, вставка, indent, чекбоксы...) — всегда
    /// отдельный шаг.
    Structure,
}

/// Окно группировки однотипных правок.
const GROUP_WINDOW_MS: u128 = 700;
/// Предел глубины истории.
const MAX_DEPTH: usize = 200;

pub struct Snapshot {
    pub model: DocModel,
    pub selection: Option<DocSelection>,
}

pub struct UndoStack {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Option<(EditClass, Option<BlockId>, Instant)>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), last: None }
    }

    /// Зовётся ПЕРЕД правкой: сохраняет текущее состояние, если правка
    /// не сливается с предыдущей группой.
    pub fn checkpoint(
        &mut self,
        model: &DocModel,
        selection: Option<DocSelection>,
        class: EditClass,
        block: Option<BlockId>,
    ) {
        self.redo.clear();
        let now = Instant::now();
        let coalesce = match self.last {
            Some((prev_class, prev_block, at)) => {
                class != EditClass::Structure
                    && prev_class == class
                    && prev_block == block
                    && now.duration_since(at).as_millis() < GROUP_WINDOW_MS
            }
            None => false,
        };
        self.last = Some((class, block, now));
        if coalesce {
            return;
        }
        self.undo.push(Snapshot { model: model.clone(), selection });
        if self.undo.len() > MAX_DEPTH {
            self.undo.remove(0);
        }
    }

    pub fn undo(
        &mut self,
        current: &DocModel,
        selection: Option<DocSelection>,
    ) -> Option<Snapshot> {
        let snap = self.undo.pop()?;
        self.redo.push(Snapshot { model: current.clone(), selection });
        self.last = None;
        Some(snap)
    }

    pub fn redo(
        &mut self,
        current: &DocModel,
        selection: Option<DocSelection>,
    ) -> Option<Snapshot> {
        let snap = self.redo.pop()?;
        self.undo.push(Snapshot { model: current.clone(), selection });
        self.last = None;
        Some(snap)
    }

    /// Откат последнего checkpoint'а, если правка не состоялась.
    pub fn discard_last_checkpoint(&mut self) {
        self.undo.pop();
        self.last = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new()
    }
}
