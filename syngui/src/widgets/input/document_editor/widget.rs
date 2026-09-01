//! Корневой виджет DocumentEditor.
//!
//! Контейнер владеет моделью (через Arc — общий с [`DocumentEditorHandle`]),
//! строит по-блочные дочерние элементы и централизует ввод: фокус, каретка,
//! выделение, клавиатура и IME живут здесь, а не в дочерних строках
//! (паттерн «пассивные строки», см. план notes-режима). Геометрию строк
//! публикуют TextRow-элементы в общий [`GeomMap`]; по ней контейнер делает
//! хит-тест мыши, рисует каретку (post_build_display_list — поверх детей)
//! и выделение (build_display_list — под детьми).

use std::any::Any;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult, Key, MouseButton};
use crate::layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
use crate::mss::{ComputedStyle, TextAlign, TextDecoration};
use crate::render::DisplayList;
use crate::signal::{use_signal, RwSignal};
use crate::widget::context::{EventContext, TextMeasure, UpdateContext};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, Widget,
};

use super::build::block_widget;
use super::edit;
use super::history::{EditClass, UndoStack};
use super::model::DocModel;
use super::parse::parse_document;
use super::serialize::serialize_document;
use super::state::{
    gutter_action, new_geom_map, BlockOrder, CaretPos, DocSelection, GeomMap, GutterAction,
    RowGeom,
};
use super::style::DocStyle;

/// Ручка редактора для хоста: доступ к модели (сериализация для автосейва)
/// и сигнал ревизии, растущий на каждую правку.
#[derive(Clone)]
pub struct DocumentEditorHandle {
    model: Arc<Mutex<DocModel>>,
    revision: RwSignal<u64>,
}

impl DocumentEditorHandle {
    pub fn new() -> Self {
        Self { model: Arc::new(Mutex::new(DocModel::new())), revision: use_signal(0) }
    }

    /// Текущий документ в markdown.
    pub fn serialize(&self) -> String {
        serialize_document(&lock(&self.model))
    }

    /// Сигнал ревизии: `.get()` в эффекте — подписка на правки.
    pub fn revision(&self) -> RwSignal<u64> {
        self.revision
    }
}

impl Default for DocumentEditorHandle {
    fn default() -> Self {
        Self::new()
    }
}

fn lock(model: &Arc<Mutex<DocModel>>) -> MutexGuard<'_, DocModel> {
    model.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct DocumentEditor {
    source: String,
    read_only: bool,
    classes: Vec<String>,
    handle: Option<DocumentEditorHandle>,
    on_change: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl DocumentEditor {
    pub fn new() -> Self {
        Self {
            source: String::new(),
            read_only: false,
            classes: Vec::new(),
            handle: None,
            on_change: None,
        }
    }

    /// Markdown-исходник документа. Смена строки (по fingerprint) заменяет
    /// модель целиком — путь для set_markdown/перезагрузки с диска.
    pub fn markdown(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn handle(mut self, handle: &DocumentEditorHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }

    /// Лёгкий колбэк на каждую правку (без сериализации).
    pub fn on_change(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Default for DocumentEditor {
    fn default() -> Self {
        Self::new()
    }
}

fn fingerprint(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

impl Widget for DocumentEditor {
    fn create_element(&self) -> Box<dyn Element> {
        let model = self
            .handle
            .as_ref()
            .map(|h| h.model.clone())
            .unwrap_or_else(|| Arc::new(Mutex::new(DocModel::new())));
        *lock(&model) = parse_document(&self.source);
        Box::new(DocumentEditorElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            classes: self.classes.clone(),
            model,
            style: Arc::new(DocStyle::default()),
            source_fp: fingerprint(&self.source),
            read_only: self.read_only,
            rebuild: true,
            geom: new_geom_map(),
            tm: None,
            focused: false,
            selection: None,
            mouse_selecting: false,
            goal_x: None,
            blink_ms: 0.0,
            caret_on: true,
            preedit: None,
            revision: self.handle.as_ref().map(|h| h.revision),
            on_change: self.on_change.clone(),
            history: UndoStack::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct DocumentEditorElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    classes: Vec<String>,
    model: Arc<Mutex<DocModel>>,
    style: Arc<DocStyle>,
    source_fp: u64,
    read_only: bool,
    rebuild: bool,
    geom: GeomMap,
    tm: Option<Arc<dyn TextMeasure>>,
    focused: bool,
    selection: Option<DocSelection>,
    mouse_selecting: bool,
    /// Целевой X при вертикальной навигации.
    goal_x: Option<f32>,
    blink_ms: f32,
    caret_on: bool,
    preedit: Option<String>,
    revision: Option<RwSignal<u64>>,
    on_change: Option<Arc<dyn Fn() + Send + Sync>>,
    history: UndoStack,
}

impl DocumentEditorElement {
    fn model(&self) -> MutexGuard<'_, DocModel> {
        lock(&self.model)
    }

    /// Снимок в историю перед правкой (группировка по классу и блоку).
    fn checkpoint(&mut self, class: EditClass) {
        let block = self.caret().map(|c| c.block);
        let model = lock(&self.model);
        let snapshot_sel = self.selection;
        // NB: заимствуем guard только на время клона.
        self.history.checkpoint(&model, snapshot_sel, class, block);
    }

    fn undo(&mut self) {
        let snap = {
            let model = lock(&self.model);
            self.history.undo(&model, self.selection)
        };
        if let Some(s) = snap {
            *lock(&self.model) = s.model;
            self.selection = s.selection;
            self.after_edit();
        }
    }

    fn redo(&mut self) {
        let snap = {
            let model = lock(&self.model);
            self.history.redo(&model, self.selection)
        };
        if let Some(s) = snap {
            *lock(&self.model) = s.model;
            self.selection = s.selection;
            self.after_edit();
        }
    }

    /// Tab / Shift+Tab с записью в историю (снапшот откатывается, если
    /// правка не применилась).
    fn tab_indent_checkpointed(&mut self, outdent: bool) -> bool {
        self.checkpoint(EditClass::Structure);
        let done = self.tab_indent(outdent);
        if !done {
            self.history.discard_last_checkpoint();
        }
        done
    }

    /// Tab / Shift+Tab: отступ пункта списка.
    fn tab_indent(&mut self, outdent: bool) -> bool {
        let Some(pos) = self.caret() else { return false };
        let mut model = self.model();
        let done = if outdent {
            edit::outdent_item(&mut model, pos.block)
        } else {
            edit::indent_item(&mut model, pos.block)
        };
        drop(model);
        if done {
            self.after_edit();
        }
        done
    }

    /// Фиксация правки: перестройка детей, ревизия, колбэк.
    fn after_edit(&mut self) {
        self.rebuild = true;
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        self.caret_on = true;
        self.blink_ms = 0.0;
        if let Some(rev) = self.revision {
            rev.set(rev.get_untracked() + 1);
        }
        if let Some(cb) = &self.on_change {
            cb();
        }
    }

    // ─── Геометрия ──────────────────────────────────────────────────────────

    /// X-координата смещения внутри строки (относительно origin строки).
    fn x_of_offset(&self, row: &RowGeom, line_idx: usize, offset: usize) -> f32 {
        let Some(line) = row.lines.get(line_idx) else { return row.gutter };
        let Some(tm) = self.tm.as_deref() else { return row.gutter };
        for seg in &line.segs {
            if offset <= seg.abs_start {
                return seg.x;
            }
            if offset <= seg.abs_end() {
                let local = &seg.text[..offset - seg.abs_start];
                return seg.x
                    + tm.measure_text_width_styled(
                        local,
                        seg.font_size,
                        local.chars().count(),
                        seg.bold,
                        None,
                    );
            }
        }
        line.segs.last().map(|s| s.x + s.width).unwrap_or(row.gutter)
    }

    /// Прямоугольник каретки в абсолютных координатах.
    fn caret_rect(&self, pos: CaretPos) -> Option<Rect> {
        let map = self.geom.lock().ok()?;
        let row = map.get(&pos.block)?;
        let line_idx = row.line_of_offset(pos.offset);
        let x = self.x_of_offset(row, line_idx, pos.offset);
        let y = row.lines.get(line_idx).map(|l| l.y).unwrap_or(0.0);
        Some(Rect::new(
            Point::new(row.origin.x + x, row.origin.y + y + 1.0),
            Size::new(2.0, (row.line_h - 2.0).max(4.0)),
        ))
    }

    /// Хит-тест позиции мыши в позицию каретки.
    fn hit_caret(&self, p: Point) -> Option<CaretPos> {
        let map = self.geom.lock().ok()?;
        let tm = self.tm.as_deref()?;
        // Ближайшая по вертикали строка среди всех блоков.
        let mut best: Option<(f32, super::model::BlockId, usize)> = None;
        for (id, row) in map.iter() {
            if row.lines.is_empty() {
                let dy = dist(p.y, row.origin.y, row.origin.y + row.line_h);
                if best.map(|(d, _, _)| dy < d).unwrap_or(true) {
                    best = Some((dy, *id, 0));
                }
                continue;
            }
            for (li, line) in row.lines.iter().enumerate() {
                let top = row.origin.y + line.y;
                let dy = dist(p.y, top, top + row.line_h);
                if best.map(|(d, _, _)| dy < d).unwrap_or(true) {
                    best = Some((dy, *id, li));
                }
            }
        }
        let (_, block, line_idx) = best?;
        let row = map.get(&block)?;
        let Some(line) = row.lines.get(line_idx) else {
            return Some(CaretPos { block, offset: 0 });
        };
        let x = p.x - row.origin.x;
        // Сегмент под курсором (или ближайший).
        let mut offset = line.segs.first().map(|s| s.abs_start).unwrap_or(0);
        for seg in &line.segs {
            if x < seg.x {
                break;
            }
            if x <= seg.x + seg.width {
                let local_x = x - seg.x;
                let ci = tm.hit_test_char_styled(&seg.text, seg.font_size, local_x, None);
                let byte = seg
                    .text
                    .char_indices()
                    .nth(ci)
                    .map(|(b, _)| b)
                    .unwrap_or(seg.text.len());
                return Some(CaretPos { block, offset: seg.abs_start + byte });
            }
            offset = seg.abs_end();
        }
        Some(CaretPos { block, offset })
    }

    /// Клик по гаттеру строки: чекбокс / шеврон toggle.
    fn gutter_hit(&self, p: Point) -> Option<(super::model::BlockId, GutterAction)> {
        let map = self.geom.lock().ok()?;
        let model = self.model();
        for (id, row) in map.iter() {
            if row.gutter <= 0.0 {
                continue;
            }
            let hit = p.x >= row.origin.x
                && p.x < row.origin.x + row.gutter
                && p.y >= row.origin.y
                && p.y < row.origin.y + row.line_h;
            if hit {
                let action =
                    edit::find_block(&model.blocks, *id).and_then(|b| gutter_action(&b.kind));
                if let Some(a) = action {
                    return Some((*id, a));
                }
            }
        }
        None
    }

    // ─── Навигация ──────────────────────────────────────────────────────────

    fn caret(&self) -> Option<CaretPos> {
        self.selection.map(|s| s.head)
    }

    fn set_caret(&mut self, pos: CaretPos, extend: bool) {
        self.selection = Some(match (self.selection, extend) {
            (Some(sel), true) => DocSelection { anchor: sel.anchor, head: pos },
            _ => DocSelection::caret(pos),
        });
        self.caret_on = true;
        self.blink_ms = 0.0;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn move_horizontal(&mut self, dir: i32, extend: bool) {
        let Some(pos) = self.caret() else { return };
        let model = self.model();
        let order = BlockOrder::of(&model);
        let text = edit::find_block(&model.blocks, pos.block)
            .and_then(|b| b.kind.text())
            .map(|t| t.text())
            .unwrap_or_default();
        let new = if dir < 0 {
            if pos.offset > 0 {
                CaretPos { block: pos.block, offset: edit::prev_char_boundary(&text, pos.offset) }
            } else if let Some(prev) = order.prev(pos.block) {
                CaretPos { block: prev, offset: edit::block_text_len(&model, prev) }
            } else {
                pos
            }
        } else if pos.offset < text.len() {
            CaretPos { block: pos.block, offset: edit::next_char_boundary(&text, pos.offset) }
        } else if let Some(next) = order.next(pos.block) {
            CaretPos { block: next, offset: 0 }
        } else {
            pos
        };
        drop(model);
        self.goal_x = None;
        self.set_caret(new, extend);
    }

    fn move_vertical(&mut self, dir: i32, extend: bool) {
        let Some(pos) = self.caret() else { return };
        let Some(rect) = self.caret_rect(pos) else { return };
        let goal_x = self.goal_x.unwrap_or(rect.origin.x);
        self.goal_x = Some(goal_x);

        let target = {
            let map = match self.geom.lock() {
                Ok(m) => m,
                Err(_) => return,
            };
            let model = self.model();
            let order = BlockOrder::of(&model);
            let row = map.get(&pos.block);
            let line_idx = row.map(|r| r.line_of_offset(pos.offset)).unwrap_or(0);
            let lines_n = row.map(|r| r.lines.len().max(1)).unwrap_or(1);

            if dir < 0 && line_idx > 0 {
                Some((pos.block, line_idx - 1))
            } else if dir > 0 && line_idx + 1 < lines_n {
                Some((pos.block, line_idx + 1))
            } else {
                let neighbor =
                    if dir < 0 { order.prev(pos.block) } else { order.next(pos.block) };
                neighbor.map(|nb| {
                    let n_lines = map.get(&nb).map(|r| r.lines.len().max(1)).unwrap_or(1);
                    (nb, if dir < 0 { n_lines - 1 } else { 0 })
                })
            }
        };
        let Some((block, line_idx)) = target else { return };
        // Смещение в целевой строке по goal_x.
        let new = {
            let map = match self.geom.lock() {
                Ok(m) => m,
                Err(_) => return,
            };
            let Some(row) = map.get(&block) else { return };
            let y = row.origin.y + row.lines.get(line_idx).map(|l| l.y).unwrap_or(0.0)
                + row.line_h / 2.0;
            Point::new(goal_x, y)
        };
        if let Some(new_pos) = self.hit_caret(new) {
            self.set_caret(new_pos, extend);
        }
    }

    fn move_line_edge(&mut self, end: bool, extend: bool) {
        let Some(pos) = self.caret() else { return };
        let map = match self.geom.lock() {
            Ok(m) => m,
            Err(_) => return,
        };
        let Some(row) = map.get(&pos.block) else { return };
        let li = row.line_of_offset(pos.offset);
        let Some(line) = row.lines.get(li) else { return };
        let offset = if end {
            line.segs.last().map(|s| s.abs_end()).unwrap_or(0)
        } else {
            line.segs.first().map(|s| s.abs_start).unwrap_or(0)
        };
        drop(map);
        self.goal_x = None;
        self.set_caret(CaretPos { block: pos.block, offset }, extend);
    }

    fn select_all(&mut self) {
        let model = self.model();
        let order = BlockOrder::of(&model);
        let (Some(first), Some(last)) = (order.ids.first(), order.ids.last()) else { return };
        let sel = DocSelection {
            anchor: CaretPos { block: *first, offset: 0 },
            head: CaretPos { block: *last, offset: edit::block_text_len(&model, *last) },
        };
        drop(model);
        self.selection = Some(sel);
        self.mark_dirty(DirtyFlags::RENDER);
    }

    /// Плоский текст выделения (для клипборда).
    fn selection_text(&self) -> String {
        let Some(sel) = self.selection else { return String::new() };
        if sel.is_caret() {
            return String::new();
        }
        let model = self.model();
        let order = BlockOrder::of(&model);
        let (start, end) = sel.ordered(&order);
        let (Some(si), Some(ei)) = (order.idx(start.block), order.idx(end.block)) else {
            return String::new();
        };
        let mut out = String::new();
        for i in si..=ei {
            let id = order.ids[i];
            let text = edit::find_block(&model.blocks, id)
                .and_then(|b| b.kind.text())
                .map(|t| t.text())
                .unwrap_or_default();
            let lo = if i == si { start.offset.min(text.len()) } else { 0 };
            let hi = if i == ei { end.offset.min(text.len()) } else { text.len() };
            if i > si {
                out.push('\n');
            }
            out.push_str(&text[lo..hi]);
        }
        out
    }

    // ─── Правки ─────────────────────────────────────────────────────────────

    fn delete_selection_if_any(&mut self) -> bool {
        let Some(sel) = self.selection else { return false };
        if sel.is_caret() {
            return false;
        }
        let mut model = self.model();
        let order = BlockOrder::of(&model);
        let caret = edit::delete_selection(&mut model, &order, sel);
        drop(model);
        self.selection = Some(DocSelection::caret(caret));
        self.after_edit();
        true
    }

    fn insert_str(&mut self, s: &str) {
        self.delete_selection_if_any();
        let Some(pos) = self.caret() else { return };
        let mut model = self.model();
        let new = edit::insert_text(&mut model, pos, s);
        drop(model);
        self.selection = Some(DocSelection::caret(new));
        self.after_edit();
    }

    fn paste(&mut self, text: &str) {
        let text = text.replace('\r', "");
        let mut lines = text.split('\n');
        if let Some(first) = lines.next() {
            self.insert_str(first);
        }
        for line in lines {
            self.enter(false);
            if !line.is_empty() {
                self.insert_str(line);
            }
        }
    }

    fn backspace(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let Some(pos) = self.caret() else { return };
        let mut model = self.model();
        let new = if pos.offset == 0 {
            let order = BlockOrder::of(&model);
            edit::backspace_at_start(&mut model, &order, pos)
        } else {
            let text = edit::find_block(&model.blocks, pos.block)
                .and_then(|b| b.kind.text())
                .map(|t| t.text())
                .unwrap_or_default();
            let start = edit::prev_char_boundary(&text, pos.offset);
            if let Some(t) =
                edit::find_block_mut(&mut model.blocks, pos.block).and_then(|b| b.kind.text_mut())
            {
                edit::text_delete(t, start, pos.offset);
            }
            CaretPos { block: pos.block, offset: start }
        };
        drop(model);
        self.selection = Some(DocSelection::caret(new));
        self.after_edit();
    }

    fn delete_forward(&mut self) {
        if self.delete_selection_if_any() {
            return;
        }
        let Some(pos) = self.caret() else { return };
        let mut model = self.model();
        let len = edit::block_text_len(&model, pos.block);
        let new = if pos.offset >= len {
            edit::delete_at_end(&mut model, pos)
        } else {
            let text = edit::find_block(&model.blocks, pos.block)
                .and_then(|b| b.kind.text())
                .map(|t| t.text())
                .unwrap_or_default();
            let end = edit::next_char_boundary(&text, pos.offset);
            if let Some(t) =
                edit::find_block_mut(&mut model.blocks, pos.block).and_then(|b| b.kind.text_mut())
            {
                edit::text_delete(t, pos.offset, end);
            }
            pos
        };
        drop(model);
        self.selection = Some(DocSelection::caret(new));
        self.after_edit();
    }

    fn enter(&mut self, shift: bool) {
        if shift {
            self.insert_str("\n");
            return;
        }
        self.delete_selection_if_any();
        let Some(pos) = self.caret() else { return };
        let mut model = self.model();
        let new = edit::split_block(&mut model, pos);
        drop(model);
        self.selection = Some(DocSelection::caret(new));
        self.after_edit();
    }

    // ─── Отрисовка выделения ────────────────────────────────────────────────

    fn draw_selection(&self, list: &mut DisplayList) {
        let Some(sel) = self.selection else { return };
        if sel.is_caret() {
            return;
        }
        let model = self.model();
        let order = BlockOrder::of(&model);
        drop(model);
        let (start, end) = sel.ordered(&order);
        let (Some(si), Some(ei)) = (order.idx(start.block), order.idx(end.block)) else { return };
        let Ok(map) = self.geom.lock() else { return };

        for i in si..=ei {
            let id = order.ids[i];
            let Some(row) = map.get(&id) else { continue };
            let block_start = if i == si { start.offset } else { 0 };
            let block_end = if i == ei {
                end.offset
            } else {
                row.lines.last().and_then(|l| l.segs.last()).map(|s| s.abs_end()).unwrap_or(0)
            };
            for (li, line) in row.lines.iter().enumerate() {
                let (Some(first), Some(last)) = (line.segs.first(), line.segs.last()) else {
                    continue;
                };
                let l_start = first.abs_start;
                let l_end = last.abs_end();
                let lo = block_start.max(l_start);
                let hi = block_end.min(l_end);
                if hi <= lo && !(block_start < l_start && block_end > l_end) {
                    continue;
                }
                let x1 = self.x_of_offset(row, li, lo);
                let x2 = self.x_of_offset(row, li, hi);
                if x2 <= x1 {
                    continue;
                }
                let rect = Rect::new(
                    Point::new(row.origin.x + x1, row.origin.y + line.y),
                    Size::new(x2 - x1, row.line_h),
                );
                list.push_rect(rect, self.style.selection_color, [2.0; 4]);
            }
        }
    }
}

fn dist(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo - v
    } else if v > hi {
        v - hi
    } else {
        0.0
    }
}

impl Element for DocumentEditorElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<DocumentEditor>() else { return };
        self.read_only = w.read_only;
        self.on_change = w.on_change.clone();
        if let Some(h) = &w.handle {
            if !Arc::ptr_eq(&self.model, &h.model) {
                self.model = h.model.clone();
                self.revision = Some(h.revision);
                self.rebuild = true;
            }
        }
        let fp = fingerprint(&w.source);
        if fp != self.source_fp {
            self.source_fp = fp;
            *lock(&self.model) = parse_document(&w.source);
            self.selection = None;
            self.rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            ctx.mark_layout_dirty();
        }
    }

    fn mount(&mut self, tree: &mut ElementTree) {
        self.tm = tree.text_measure.clone();
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        // Вызывается только для пустого документа (без детей).
        let width = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let height = self.style.doc_padding * 2.0 + self.style.line_h(self.style.text_size);
        self.bounds.size = Size::new(width, height);
        self.bounds.size
    }

    fn layout_hint(&self) -> LayoutHint {
        let s = &self.style;
        // Колонка как в Notion: листья сами ограничивают свою ширину
        // max_content_width (см. rows::clamp_width), а корень центрирует.
        LayoutHint::Column {
            gap: s.block_spacing,
            cross_align: CrossAxisAlignment::Center,
            main_align: MainAxisAlignment::Start,
            padding_left: s.doc_padding,
            padding_top: s.doc_padding,
            padding_right: s.doc_padding,
            padding_bottom: s.doc_padding,
            expand: false,
        }
    }

    fn manages_own_children(&self) -> bool {
        true
    }

    fn needs_rebuild(&self) -> bool {
        self.rebuild
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        let model = self.model();
        model.blocks.iter().map(|b| block_widget(b, &self.style, &self.geom)).collect()
    }

    fn clear_rebuild(&mut self) {
        self.rebuild = false;
        // Чистим геометрию исчезнувших блоков.
        let mut alive = HashSet::new();
        self.model().for_each(&mut |b| {
            alive.insert(b.id);
        });
        if let Ok(mut map) = self.geom.lock() {
            map.retain(|id, _| alive.contains(id));
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        self.draw_selection(list);
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.focused || self.read_only {
            return;
        }
        let Some(pos) = self.caret() else { return };
        let Some(rect) = self.caret_rect(pos) else { return };
        if self.caret_on {
            list.push_rect(rect, self.style.caret_color, [1.0; 4]);
        }
        // Preedit IME: текст с подчёркиванием у каретки.
        if let Some(pre) = &self.preedit {
            if !pre.is_empty() {
                let fs = self.style.text_size;
                let w = self
                    .tm
                    .as_deref()
                    .map(|tm| tm.measure_text_width(pre, fs, pre.chars().count()))
                    .unwrap_or(0.0);
                let text_rect = Rect::new(
                    Point::new(rect.origin.x + 3.0, rect.origin.y),
                    Size::new(w, 0.0),
                );
                list.push_rect(
                    Rect::new(
                        Point::new(rect.origin.x + 2.0, rect.origin.y - 1.0),
                        Size::new(w + 4.0, rect.size.height + 2.0),
                    ),
                    self.style.selection_color,
                    [2.0; 4],
                );
                list.push_text_aligned(
                    pre,
                    text_rect,
                    self.style.text_color,
                    fs,
                    TextAlign::DEFAULT,
                    TextDecoration::None,
                    400,
                );
                list.push_rect(
                    Rect::new(
                        Point::new(rect.origin.x + 3.0, rect.origin.y + rect.size.height - 1.0),
                        Size::new(w, 1.0),
                    ),
                    self.style.caret_color,
                    [0.0; 4],
                );
            }
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        // Метрика текста могла быть установлена в дерево после mount
        // (пересоздание рендера, headless-харнес) — подхватываем из контекста.
        if self.tm.is_none() {
            self.tm = ctx.text_measure.clone();
        }
        match event {
            Event::MouseDown { button: MouseButton::Left, position } => {
                if !self.bounds.contains(*position) {
                    if self.focused {
                        self.focused = false;
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    return EventResult::Ignored;
                }
                self.focused = true;
                ctx.set_focused_text(String::new());
                // Клик по гаттеру: чекбокс/шеврон.
                if !self.read_only {
                    if let Some((id, action)) = self.gutter_hit(*position) {
                        self.checkpoint(EditClass::Structure);
                        let mut model = self.model();
                        match action {
                            GutterAction::ToggleTodo => edit::toggle_todo(&mut model, id),
                            GutterAction::ToggleCollapse => edit::toggle_collapse(&mut model, id),
                        }
                        drop(model);
                        self.after_edit();
                        return EventResult::Handled;
                    }
                }
                if let Some(pos) = self.hit_caret(*position) {
                    self.goal_x = None;
                    self.set_caret(pos, ctx.modifiers.shift);
                    if !ctx.modifiers.shift {
                        self.mouse_selecting = true;
                        ctx.capture();
                    }
                }
                EventResult::Handled
            }
            Event::MouseMove(position) => {
                if self.mouse_selecting {
                    if let Some(pos) = self.hit_caret(*position) {
                        if let Some(sel) = &mut self.selection {
                            if sel.head != pos {
                                sel.head = pos;
                                self.mark_dirty(DirtyFlags::RENDER);
                            }
                        }
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button: MouseButton::Left, .. } => {
                if self.mouse_selecting {
                    self.mouse_selecting = false;
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::DoubleClick { button: MouseButton::Left, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                if let Some(pos) = self.hit_caret(*position) {
                    let model = self.model();
                    let text = edit::find_block(&model.blocks, pos.block)
                        .and_then(|b| b.kind.text())
                        .map(|t| t.text())
                        .unwrap_or_default();
                    drop(model);
                    let (s, e) = edit::word_bounds(&text, pos.offset);
                    self.selection = Some(DocSelection {
                        anchor: CaretPos { block: pos.block, offset: s },
                        head: CaretPos { block: pos.block, offset: e },
                    });
                    self.mark_dirty(DirtyFlags::RENDER);
                }
                EventResult::Handled
            }
            Event::KeyDown(key) => {
                if !self.focused {
                    return EventResult::Ignored;
                }
                let shift = ctx.modifiers.shift;
                let ctrl = ctx.modifiers.ctrl;
                let editable = !self.read_only;
                let handled = match key {
                    Key::A if ctrl => {
                        self.select_all();
                        true
                    }
                    Key::C if ctrl => {
                        let text = self.selection_text();
                        if !text.is_empty() {
                            ctx.copy_to_clipboard(&text);
                        }
                        true
                    }
                    Key::X if ctrl && editable => {
                        let text = self.selection_text();
                        if !text.is_empty() {
                            ctx.copy_to_clipboard(&text);
                            self.checkpoint(EditClass::Structure);
                            self.delete_selection_if_any();
                        }
                        true
                    }
                    Key::V if ctrl && editable => {
                        if let Some(text) = ctx.paste_from_clipboard() {
                            self.checkpoint(EditClass::Structure);
                            self.paste(&text);
                        }
                        true
                    }
                    Key::Z if ctrl && editable => {
                        if shift {
                            self.redo();
                        } else {
                            self.undo();
                        }
                        true
                    }
                    Key::Y if ctrl && editable => {
                        self.redo();
                        true
                    }
                    Key::Tab if editable => self.tab_indent_checkpointed(shift),
                    Key::Backspace if editable => {
                        self.checkpoint(EditClass::Deleting);
                        self.backspace();
                        true
                    }
                    Key::Delete if editable => {
                        self.checkpoint(EditClass::Deleting);
                        self.delete_forward();
                        true
                    }
                    Key::Enter if editable => {
                        self.checkpoint(EditClass::Structure);
                        self.enter(shift);
                        true
                    }
                    Key::Left => {
                        self.move_horizontal(-1, shift);
                        true
                    }
                    Key::Right => {
                        self.move_horizontal(1, shift);
                        true
                    }
                    Key::Up => {
                        self.move_vertical(-1, shift);
                        true
                    }
                    Key::Down => {
                        self.move_vertical(1, shift);
                        true
                    }
                    Key::Home => {
                        self.move_line_edge(false, shift);
                        true
                    }
                    Key::End => {
                        self.move_line_edge(true, shift);
                        true
                    }
                    Key::Escape => {
                        if let Some(sel) = self.selection {
                            self.selection = Some(DocSelection::caret(sel.head));
                            self.mark_dirty(DirtyFlags::RENDER);
                        }
                        true
                    }
                    _ => false,
                };
                if handled {
                    if let Some(rect) = self.caret().and_then(|p| self.caret_rect(p)) {
                        ctx.scroll_into_view(rect);
                    }
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::CharInput(c) => {
                if !self.focused || self.read_only || c.is_control() {
                    return EventResult::Ignored;
                }
                if self.caret().is_none() {
                    return EventResult::Ignored;
                }
                self.checkpoint(EditClass::Typing);
                let mut buf = [0u8; 4];
                self.insert_str(c.encode_utf8(&mut buf));
                EventResult::Handled
            }
            Event::ImePreedit { text, .. } => {
                if !self.focused {
                    return EventResult::Ignored;
                }
                self.preedit = if text.is_empty() { None } else { Some(text.clone()) };
                self.mark_dirty(DirtyFlags::RENDER);
                EventResult::Handled
            }
            Event::ImeCommit(text) => {
                if !self.focused || self.read_only {
                    return EventResult::Ignored;
                }
                self.preedit = None;
                self.checkpoint(EditClass::Typing);
                self.insert_str(text);
                EventResult::Handled
            }
            Event::FocusLost => {
                if self.focused {
                    self.focused = false;
                    self.mouse_selecting = false;
                    self.preedit = None;
                    self.mark_dirty(DirtyFlags::RENDER);
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if !self.focused || self.read_only || self.selection.is_none() {
            return false;
        }
        self.blink_ms += dt.as_secs_f32() * 1000.0;
        if self.blink_ms >= 530.0 {
            self.blink_ms = 0.0;
            self.caret_on = !self.caret_on;
            self.mark_dirty(DirtyFlags::RENDER);
        }
        true
    }

    fn wants_animate_tick(&self) -> bool {
        self.focused && !self.read_only && self.selection.is_some()
    }

    fn wants_tab(&self) -> bool {
        self.focused && !self.read_only
    }

    fn element_type_name(&self) -> &str {
        "document-editor"
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.apply_style(style);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }

    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }
    fn children(&self) -> &[ElementId] {
        &[]
    }
    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty |= flags;
    }
    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.remove(flags);
    }
    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty.contains(flags)
    }
}

impl StyledElement for DocumentEditorElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        let mut doc_style = DocStyle::default();
        doc_style.apply(style);
        // Дети перестраиваются только при реальном изменении стиля.
        if doc_style != *self.style {
            self.style = Arc::new(doc_style);
            self.rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }
}
