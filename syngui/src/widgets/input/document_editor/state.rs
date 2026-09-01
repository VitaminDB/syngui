//! Состояние редактирования: каретка, выделение, геометрия строк.
//!
//! Каретка адресуется парой (BlockId, байтовое смещение в конкатенации
//! ранов блока). Порядок блоков — предзаказный обход текстонесущих блоков;
//! он совпадает с визуальным порядком строк.
//!
//! Геометрию строк публикуют сами TextRow-элементы (строки и абсолютный
//! origin) в общий [`GeomMap`]; контейнер по ней делает хит-тест мыши,
//! позиционирует каретку и рисует выделение.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::core::Point;

use super::model::{BlockId, BlockKind, DocBlock, DocModel};

/// Позиция каретки: блок + байтовое смещение в его тексте.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaretPos {
    pub block: BlockId,
    pub offset: usize,
}

/// Выделение: якорь (где начали) и голова (куда пришли).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocSelection {
    pub anchor: CaretPos,
    pub head: CaretPos,
}

impl DocSelection {
    pub fn caret(pos: CaretPos) -> Self {
        Self { anchor: pos, head: pos }
    }

    pub fn is_caret(&self) -> bool {
        self.anchor == self.head
    }

    /// (начало, конец) в порядке документа.
    pub fn ordered(&self, order: &BlockOrder) -> (CaretPos, CaretPos) {
        if order.cmp(self.anchor, self.head).is_le() {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

/// Предзаказный порядок текстонесущих блоков документа.
pub struct BlockOrder {
    pub ids: Vec<BlockId>,
    index: HashMap<BlockId, usize>,
}

impl BlockOrder {
    pub fn of(model: &DocModel) -> Self {
        let mut ids = Vec::new();
        fn walk(blocks: &[DocBlock], ids: &mut Vec<BlockId>) {
            for b in blocks {
                if b.kind.text().is_some() {
                    ids.push(b.id);
                }
                if let Some(children) = b.kind.children() {
                    walk(children, ids);
                }
            }
        }
        walk(&model.blocks, &mut ids);
        let index = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        Self { ids, index }
    }

    pub fn idx(&self, id: BlockId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    pub fn cmp(&self, a: CaretPos, b: CaretPos) -> std::cmp::Ordering {
        let ia = self.idx(a.block).unwrap_or(usize::MAX);
        let ib = self.idx(b.block).unwrap_or(usize::MAX);
        ia.cmp(&ib).then(a.offset.cmp(&b.offset))
    }

    pub fn prev(&self, id: BlockId) -> Option<BlockId> {
        let i = self.idx(id)?;
        (i > 0).then(|| self.ids[i - 1])
    }

    pub fn next(&self, id: BlockId) -> Option<BlockId> {
        let i = self.idx(id)?;
        self.ids.get(i + 1).copied()
    }
}

/// Тип блока, в котором смысл клика по гаттеру: чекбокс или шеврон toggle.
pub fn gutter_action(kind: &BlockKind) -> Option<GutterAction> {
    match kind {
        BlockKind::Todo { .. } => Some(GutterAction::ToggleTodo),
        BlockKind::Toggle { .. } => Some(GutterAction::ToggleCollapse),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GutterAction {
    ToggleTodo,
    ToggleCollapse,
}

// ─── Геометрия строк ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct RowGeom {
    /// Абсолютный левый верхний угол элемента строки.
    pub origin: Point,
    /// Ширина гаттера маркера.
    pub gutter: f32,
    /// Высота строки текста.
    pub line_h: f32,
    pub lines: Vec<GeomLine>,
}

#[derive(Clone, Debug, Default)]
pub struct GeomLine {
    /// Y верхней кромки строки относительно origin.
    pub y: f32,
    pub segs: Vec<GeomSeg>,
}

#[derive(Clone, Debug)]
pub struct GeomSeg {
    /// X относительно origin (гаттер уже учтён).
    pub x: f32,
    pub width: f32,
    pub text: String,
    /// Байтовое смещение начала сегмента в тексте блока.
    pub abs_start: usize,
    pub bold: bool,
    pub font_size: f32,
}

impl GeomSeg {
    pub fn abs_end(&self) -> usize {
        self.abs_start + self.text.len()
    }
}

impl RowGeom {
    /// Строка, содержащая смещение (или ближайшая).
    pub fn line_of_offset(&self, offset: usize) -> usize {
        for (i, line) in self.lines.iter().enumerate() {
            let Some(last) = line.segs.last() else { continue };
            // Конец строки принадлежит ей, если следующая строка не
            // начинается с того же смещения (мягкий перенос).
            let end = last.abs_end();
            let next_starts_here = self
                .lines
                .get(i + 1)
                .and_then(|l| l.segs.first())
                .map(|s| s.abs_start == end)
                .unwrap_or(false);
            if offset < end || (offset == end && !next_starts_here) {
                if line.segs.first().map(|s| s.abs_start <= offset).unwrap_or(false) {
                    return i;
                }
            }
        }
        self.lines.len().saturating_sub(1)
    }
}

pub type GeomMap = Arc<Mutex<HashMap<BlockId, RowGeom>>>;

pub fn new_geom_map() -> GeomMap {
    Arc::new(Mutex::new(HashMap::new()))
}
