use super::{ColumnAlign, ColumnWidth, SortDirection, SortKey, SortKeyFn, TableColumn, TableDataSource, TableView};
use crate::animation::transition::mss_color_to_core;
use crate::core::{Color, Point, Rect, RectExt, Size, Transform};
use crate::core::sync::Mutex;
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::{Constraints, CrossAxisAlignment};
use crate::mss::{ComputedStyle, Dimension, IconState, MssFields, TextAlign, TextDecoration};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use crate::widget::styled::WidgetExt;
use std::any::Any;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

const MI_ARROW_DROP_UP: &str = "\u{E5C7}";
const MI_ARROW_DROP_DOWN: &str = "\u{E5C5}";
const MI_UNFOLD_MORE: &str = "\u{E5D7}";
const MI_TUNE: &str = "\u{E429}";
const MI_CHECK: &str = "\u{E5CA}";

const SORT_ICON_SIZE: f32 = 18.0;
const SORT_ICON_PADDING: f32 = 4.0;
const SETTINGS_BUTTON_SIZE: f32 = 32.0;
const SETTINGS_BUTTON_INSET: f32 = 4.0;
const POPOVER_ITEM_HEIGHT: f32 = 32.0;
const POPOVER_PADDING: f32 = 8.0;
const POPOVER_MIN_WIDTH: f32 = 200.0;
const POPOVER_CHECK_SIZE: f32 = 18.0;
const POPOVER_GAP_AFTER_CHECK: f32 = 8.0;
const CONTEXT_MENU_WIDTH: f32 = 190.0;

impl Widget for TableView {
    fn create_element(&self) -> Box<dyn Element> {
        let data = match &self.data {
            TableDataSource::Eager(rows) => TableDataSource::Eager(rows.clone()),
            TableDataSource::Virtual { row_count, row_builder } => TableDataSource::Virtual {
                row_count: *row_count,
                row_builder: row_builder.clone(),
            },
        };
        let compositional = self.columns.iter().any(|c| {
            c.cell_renderer.is_some() || c.cell_renderer_with_row.is_some()
        });
        let initial_visibility = self
            .column_visibility_state
            .as_ref()
            .and_then(|s| s.lock().ok().map(|g| g.clone()))
            .filter(|v| v.len() == self.columns.len())
            .unwrap_or_else(|| self.columns.iter().map(|c| c.visible).collect());
        let el = TableViewElement {
            id: ElementId::new(),
            columns: self.columns.clone(),
            data,
            sortable: self.sortable,
            row_height: self.row_height,
            header_height: self.header_height,
            striped: self.striped,
            buffer_size: self.buffer_size,
            sort_column: None,
            sort_direction: SortDirection::None,
            sorted_indices: None,
            on_sort: self.on_sort.clone(),
            on_row_click: self.on_row_click.clone(),
            on_selection_change: self.on_selection_change.clone(),
            selected_rows: self.selected_rows.clone(),
            width: self.width,
            height: self.height,
            scroll_offset: self.scroll_state.as_ref()
                .map(|s| *s.lock().unwrap())
                .unwrap_or(0.0),
            scroll_state: self.scroll_state.clone(),
            scroll_offset_x: 0.0,
            h_scrollbar_dragging: false,
            h_scrollbar_drag_offset: 0.0,
            h_scrollbar_hovered: false,
            velocity: 0.0,
            hovered_row: None,
            hovered_header_col: None,
            settings_button_hovered: false,
            popover_open: false,
            popover_hovered_index: None,
            column_widths: Vec::new(),
            bounds: Rect::zero(),
            scrollbar_dragging: false,
            scrollbar_drag_offset: 0.0,
            scrollbar_hovered: false,
            scrollbar_fader: crate::widgets::scroll::ScrollbarFader::default(),
            row_cache: HashMap::new(),
            cache_range: 0..0,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            compositional,
            needs_child_rebuild: compositional,
            actual_content_height: 0.0,
            header_bg_custom: self.custom_header_bg,
            header_color_custom: self.custom_header_color,
            header_font_size: self.custom_header_font_size.unwrap_or(12.0),
            header_padding: 12.0,
            row_hover_bg: self.custom_row_hover_bg,
            row_selected_bg: self.custom_row_selected_bg,
            row_striped_bg: None,
            row_padding: self.custom_row_padding.unwrap_or([0.0; 4]),
            cell_font_size: self.custom_cell_font_size.unwrap_or(13.0),
            cell_padding: self.custom_cell_padding.unwrap_or(12.0),
            cell_min_width: self.custom_cell_min_width.unwrap_or(50.0),
            cell_max_width: self.custom_cell_max_width.unwrap_or(f32::INFINITY),
            col_widths: Vec::new(),
            col_min_widths: Vec::new(),
            col_max_widths: Vec::new(),
            mss_applied: false,
            row_bounds: Vec::new(),
            comp_visible_first: 0,
            comp_visible_last: 0,
            user_col_widths: self
                .column_widths_state
                .as_ref()
                .and_then(|s| s.lock().ok().map(|g| g.clone()))
                .unwrap_or_else(|| vec![None; self.columns.len()]),
            column_widths_state: self.column_widths_state.clone(),
            column_visibility: initial_visibility,
            column_visibility_state: self.column_visibility_state.clone(),
            on_column_visibility_change: self.on_column_visibility_change.clone(),
            resize_state: None,
            on_column_resize: self.on_column_resize.clone(),
            keyboard_nav: self.keyboard_nav,
            editable: self.editable,
            cursor_cell: None,
            focused: false,
            edit_state: None,
            on_cell_select: self.on_cell_select.clone(),
            on_cell_edit: self.on_cell_edit.clone(),
            on_row_double_click: self.on_row_double_click.clone(),
            on_cell_double_click: self.on_cell_double_click.clone(),
            grid_alpha: 0.4,
            sort_warned_virtual: false,
            cell_cursor: self.cell_cursor,
            text_selection: self.text_selection,
            text_measure: None,
            text_sel: None,
            text_selecting: false,
            context_menu: None,
        };
        Box::new(el)
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct TableViewElement {
    id: ElementId,
    columns: Vec<TableColumn>,
    data: TableDataSource,
    sortable: bool,
    row_height: f32,
    header_height: f32,
    striped: bool,
    buffer_size: usize,
    sort_column: Option<usize>,
    sort_direction: SortDirection,
    sorted_indices: Option<Vec<usize>>,
    on_sort: Option<Arc<Mutex<dyn FnMut(usize, SortDirection) + Send>>>,
    on_row_click: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    /// Выбор изменился: список выделенных строк целиком.
    on_selection_change: Option<Arc<Mutex<dyn FnMut(Vec<usize>) + Send>>>,
    selected_rows: Vec<usize>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    scroll_offset: f32,
    scroll_state: Option<Arc<Mutex<f32>>>,
    // Горизонтальная прокрутка — когда сумма ширин колонок шире области.
    scroll_offset_x: f32,
    h_scrollbar_dragging: bool,
    h_scrollbar_drag_offset: f32,
    h_scrollbar_hovered: bool,
    velocity: f32,
    hovered_row: Option<usize>,
    hovered_header_col: Option<usize>,
    settings_button_hovered: bool,
    popover_open: bool,
    popover_hovered_index: Option<usize>,
    column_widths: Vec<f32>,
    bounds: Rect,
    scrollbar_dragging: bool,
    scrollbar_drag_offset: f32,
    scrollbar_hovered: bool,
    scrollbar_fader: crate::widgets::scroll::ScrollbarFader,
    row_cache: HashMap<usize, Vec<String>>,
    cache_range: Range<usize>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    compositional: bool,
    needs_child_rebuild: bool,
    actual_content_height: f32,
    header_bg_custom: Option<Color>,
    header_color_custom: Option<Color>,
    header_font_size: f32,
    header_padding: f32,
    row_hover_bg: Option<Color>,
    row_selected_bg: Option<Color>,
    row_striped_bg: Option<Color>,
    row_padding: [f32; 4],
    cell_font_size: f32,
    cell_padding: f32,
    cell_min_width: f32,
    cell_max_width: f32,
    col_widths: Vec<Option<Dimension>>,
    col_min_widths: Vec<Option<Dimension>>,
    col_max_widths: Vec<Option<Dimension>>,
    mss_applied: bool,
    row_bounds: Vec<(f32, f32)>,
    comp_visible_first: usize,
    comp_visible_last: usize,
    user_col_widths: Vec<Option<f32>>,
    column_widths_state: Option<Arc<Mutex<Vec<Option<f32>>>>>,
    column_visibility: Vec<bool>,
    column_visibility_state: Option<Arc<Mutex<Vec<bool>>>>,
    on_column_visibility_change: Option<Arc<Mutex<dyn FnMut(usize, bool) + Send>>>,
    resize_state: Option<ColumnResizeState>,
    on_column_resize: Option<Arc<Mutex<dyn FnMut(usize, f32) + Send>>>,
    keyboard_nav: bool,
    editable: bool,
    cursor_cell: Option<(usize, usize)>,
    focused: bool,
    edit_state: Option<CellEditState>,
    on_cell_select: Option<Arc<Mutex<dyn FnMut(usize, usize) + Send>>>,
    on_cell_edit: Option<Arc<Mutex<dyn FnMut(usize, usize, String, String) + Send>>>,
    on_row_double_click: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    on_cell_double_click: Option<Arc<Mutex<dyn FnMut(usize, usize) + Send>>>,
    grid_alpha: f32,
    sort_warned_virtual: bool,
    cell_cursor: bool,
    text_selection: bool,
    /// Обмер текста из дерева — по нему считается ширина текста ячейки и
    /// позиция символа под курсором.
    text_measure: Option<Arc<dyn crate::widget::context::TextMeasure>>,
    /// Выделенный фрагмент текста ячейки: физические строка и колонка,
    /// якорь и подвижный конец в байтах исходной строки.
    text_sel: Option<CellTextSelection>,
    /// Мышь тянет выделение текста прямо сейчас.
    text_selecting: bool,
    /// Контекстное меню ячейки: точка вызова и подсвеченный пункт.
    context_menu: Option<CellContextMenu>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CellTextSelection {
    row: usize,
    col: usize,
    anchor: usize,
    head: usize,
}

impl CellTextSelection {
    fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }

    fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

#[derive(Debug, Clone)]
struct CellContextMenu {
    origin: Point,
    row: usize,
    col: usize,
    hovered: Option<usize>,
}

#[derive(Debug, Clone)]
struct CellEditState {
    row: usize,
    col: usize,
    original: String,
    buffer: String,
}

#[derive(Debug, Clone, Copy)]
struct ColumnResizeState {
    col: usize,
    start_x: f32,
    start_width: f32,
}

const RESIZE_HANDLE_WIDTH: f32 = 4.0;

impl TableViewElement {

    fn has_hideable_columns(&self) -> bool {
        self.columns.iter().any(|c| c.hideable)
    }

    fn is_col_visible(&self, col_idx: usize) -> bool {
        self.column_visibility.get(col_idx).copied().unwrap_or(true)
    }

    fn visible_columns(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.columns.len()).filter(|i| self.is_col_visible(*i))
    }

    fn settings_reserved_width(&self) -> f32 {
        if self.has_hideable_columns() {
            SETTINGS_BUTTON_SIZE + SETTINGS_BUTTON_INSET * 2.0
        } else {
            0.0
        }
    }

    fn settings_button_rect(&self) -> Option<Rect> {
        if !self.has_hideable_columns() {
            return None;
        }
        let x = self.bounds.x() + self.bounds.size.width
            - SETTINGS_BUTTON_INSET - SETTINGS_BUTTON_SIZE;
        let y = self.bounds.y() + (self.header_height - SETTINGS_BUTTON_SIZE) / 2.0;
        Some(Rect::new(
            Point::new(x, y),
            Size::new(SETTINGS_BUTTON_SIZE, SETTINGS_BUTTON_SIZE),
        ))
    }

    fn popover_rect(&self) -> Option<Rect> {
        if !self.popover_open { return None; }
        let btn = self.settings_button_rect()?;
        let hideable: Vec<_> = (0..self.columns.len())
            .filter(|i| self.columns[*i].hideable)
            .collect();
        if hideable.is_empty() { return None; }
        let h = POPOVER_PADDING * 2.0 + hideable.len() as f32 * POPOVER_ITEM_HEIGHT;
        let w = POPOVER_MIN_WIDTH;
        let x = (btn.x() + btn.size.width - w).max(self.bounds.x() + 4.0);
        let y = btn.y() + btn.size.height + 4.0;
        Some(Rect::new(Point::new(x, y), Size::new(w, h)))
    }

    fn popover_item_rect(&self, index: usize) -> Option<Rect> {
        let pop = self.popover_rect()?;
        let y = pop.y() + POPOVER_PADDING + index as f32 * POPOVER_ITEM_HEIGHT;
        Some(Rect::new(
            Point::new(pop.x() + POPOVER_PADDING, y),
            Size::new(pop.size.width - POPOVER_PADDING * 2.0, POPOVER_ITEM_HEIGHT),
        ))
    }

    fn hideable_columns(&self) -> Vec<usize> {
        (0..self.columns.len())
            .filter(|i| self.columns[*i].hideable)
            .collect()
    }

    fn popover_hit_test(&self, pos: Point) -> Option<usize> {
        if !self.popover_open { return None; }
        let hideable = self.hideable_columns();
        for (idx, _) in hideable.iter().enumerate() {
            if let Some(r) = self.popover_item_rect(idx) {
                if r.contains(pos) {
                    return Some(idx);
                }
            }
        }
        None
    }

    fn row_count(&self) -> usize {
        self.data.row_count()
    }

    fn physical_row(&self, visible_idx: usize) -> usize {
        match &self.sorted_indices {
            Some(perm) => perm.get(visible_idx).copied().unwrap_or(visible_idx),
            None => visible_idx,
        }
    }

    fn visible_row(&self, physical_idx: usize) -> usize {
        match &self.sorted_indices {
            Some(perm) => perm.iter().position(|&i| i == physical_idx).unwrap_or(physical_idx),
            None => physical_idx,
        }
    }

    fn compute_sort_perm(&self, col_idx: usize, dir: SortDirection) -> Option<Vec<usize>> {
        let TableDataSource::Eager(rows) = &self.data else { return None; };
        if rows.is_empty() { return Some(Vec::new()); }
        let col = self.columns.get(col_idx)?;
        let extractor: SortKeyFn = col
            .sort_key
            .clone()
            .unwrap_or_else(|| Arc::new(SortKey::from_cell));
        let keys: Vec<SortKey> = rows
            .iter()
            .map(|r| {
                let text = r.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                (extractor)(text)
            })
            .collect();
        let mut perm: Vec<usize> = (0..rows.len()).collect();
        perm.sort_by(|a, b| {
            let ord = keys[*a].cmp(&keys[*b]);
            match dir {
                SortDirection::Descending => ord.reverse(),
                _ => ord,
            }
        });
        Some(perm)
    }

    fn refresh_sorted_indices(&mut self) {
        if !self.sortable
            || self.sort_direction == SortDirection::None
            || self.sort_column.is_none()
            || self.on_sort.is_some()
        {
            self.sorted_indices = None;
            return;
        }
        if !matches!(self.data, TableDataSource::Eager(_)) {
            self.sorted_indices = None;
            if !self.sort_warned_virtual {
                log::warn!(
                    "TableView: sortable=true on a Virtual data source without on_sort \
                     — built-in sort is disabled. Provide on_sort or switch to Eager."
                );
                self.sort_warned_virtual = true;
            }
            return;
        }
        let col = self.sort_column.unwrap();
        let dir = self.sort_direction;
        self.sorted_indices = self.compute_sort_perm(col, dir);
    }

    fn compute_column_widths(&mut self, available: f32) {
        if self.user_col_widths.len() != self.columns.len() {
            self.user_col_widths.resize(self.columns.len(), None);
        }
        if self.column_visibility.len() != self.columns.len() {
            self.column_visibility.resize(self.columns.len(), true);
        }

        let available_for_columns = (available - self.settings_reserved_width()).max(0.0);

        let mut fixed_total = 0.0;
        let mut flex_total = 0.0;
        for (i, col) in self.columns.iter().enumerate() {
            if !self.is_col_visible(i) { continue; }
            if let Some(user_w) = self.user_col_widths.get(i).and_then(|w| *w) {
                fixed_total += user_w;
            } else if let Some(dim) = self.col_widths.get(i).and_then(|d| *d) {
                fixed_total += dim.resolve(available_for_columns);
            } else {
                match col.width {
                    ColumnWidth::Fixed(w) => fixed_total += w,
                    ColumnWidth::Flex(f) => flex_total += f,
                }
            }
        }
        let remaining = (available_for_columns - fixed_total).max(0.0);
        self.column_widths = self.columns.iter().enumerate().map(|(i, col)| {
            if !self.is_col_visible(i) {
                return 0.0;
            }
            let base_w = if let Some(user_w) = self.user_col_widths.get(i).and_then(|w| *w) {
                user_w
            } else if let Some(dim) = self.col_widths.get(i).and_then(|d| *d) {
                dim.resolve(available_for_columns)
            } else {
                match col.width {
                    ColumnWidth::Fixed(w) => w.max(col.min_width),
                    ColumnWidth::Flex(f) => {
                        let w = if flex_total > 0.0 { remaining * f / flex_total } else { remaining };
                        w.max(col.min_width)
                    }
                }
            };
            let min_w = self.col_min_widths.get(i).and_then(|d| *d)
                .map(|d| d.resolve(available_for_columns))
                .unwrap_or(self.cell_min_width)
                .max(col.min_width);
            let max_w_mss = self.col_max_widths.get(i).and_then(|d| *d)
                .map(|d| d.resolve(available_for_columns))
                .unwrap_or(self.cell_max_width);
            let max_w = max_w_mss.min(col.max_width);
            base_w.clamp(min_w.min(max_w), max_w)
        }).collect();
    }

    fn hit_resize_handle(&self, pos: Point) -> Option<usize> {
        if pos.y < self.bounds.y() || pos.y > self.bounds.y() + self.header_height {
            return None;
        }
        let visible: Vec<usize> = self.visible_columns().collect();
        let mut cx = self.bounds.x() - self.scroll_offset_x;
        for (vis_pos, phys_i) in visible.iter().copied().enumerate() {
            let w = self.column_widths.get(phys_i).copied().unwrap_or(0.0);
            cx += w;
            if vis_pos + 1 >= visible.len() { break; }
            if !self.columns[phys_i].resizable { continue; }
            if (pos.x - cx).abs() <= RESIZE_HANDLE_WIDTH {
                return Some(phys_i);
            }
        }
        None
    }

    fn persist_column_widths(&self) {
        if let Some(ref state) = self.column_widths_state {
            if let Ok(mut g) = state.lock() {
                *g = self.user_col_widths.clone();
            }
        }
    }

    fn persist_column_visibility(&self) {
        if let Some(ref state) = self.column_visibility_state {
            if let Ok(mut g) = state.lock() {
                *g = self.column_visibility.clone();
            }
        }
    }

    fn last_col(&self) -> usize {
        self.columns.len().saturating_sub(1)
    }

    fn last_row(&self) -> usize {
        self.row_count().saturating_sub(1)
    }

    fn visible_row_count(&self) -> usize {
        let body_h = self.body_rect().size.height.max(1.0);
        ((body_h / self.row_height).floor() as usize).max(1)
    }

    fn scroll_to_visible_row(&mut self, visible_idx: usize) {
        let rh = self.row_height.max(1.0);
        let top = visible_idx as f32 * rh;
        let bottom = top + rh;
        let body_h = self.body_rect().size.height;
        let max_s = self.max_scroll();
        if top < self.scroll_offset {
            self.set_scroll_offset(top.clamp(0.0, max_s));
        } else if bottom > self.scroll_offset + body_h {
            self.set_scroll_offset((bottom - body_h).clamp(0.0, max_s));
        }
        if self.compositional {
            self.check_comp_rebuild();
        } else {
            self.ensure_cached_for_viewport();
        }
    }

    fn move_cursor(&mut self, physical_row: usize, col: usize) {
        let r = physical_row.min(self.last_row());
        let c = col.min(self.last_col());
        let new_cursor = Some((r, c));
        if new_cursor != self.cursor_cell {
            self.cursor_cell = new_cursor;
            if let Some(ref cb) = self.on_cell_select {
                if let Ok(mut f) = cb.lock() { f(r, c); }
            }
            self.scroll_to_visible_row(self.visible_row(r));
        }
    }

    fn cell_text(&mut self, physical_row: usize, col: usize) -> String {
        self.get_physical_row(physical_row)
            .and_then(|r| r.get(col).cloned())
            .unwrap_or_default()
    }

    fn begin_edit(&mut self, row: usize, col: usize) {
        let text = self.cell_text(row, col);
        self.edit_state = Some(CellEditState {
            row,
            col,
            original: text.clone(),
            buffer: text,
        });
    }

    fn commit_edit(&mut self) {
        if let Some(edit) = self.edit_state.take() {
            if edit.buffer != edit.original {
                if let Some(ref cb) = self.on_cell_edit {
                    if let Ok(mut f) = cb.lock() {
                        f(edit.row, edit.col, edit.original, edit.buffer);
                    }
                }
            }
        }
    }

    fn cancel_edit(&mut self) {
        self.edit_state = None;
    }

    fn handle_key_nav(&mut self, key: Key, ctx: &mut EventContext) -> EventResult {
        if self.edit_state.is_some() {
            match key {
                Key::Escape => {
                    self.cancel_edit();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                Key::Enter => {
                    self.commit_edit();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                Key::Tab => {
                    self.commit_edit();
                    if let Some((r, c)) = self.cursor_cell {
                        let nc = self.next_visible_col(c, 1).unwrap_or(c);
                        self.move_cursor(r, nc);
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                Key::Backspace => {
                    if let Some(ref mut e) = self.edit_state {
                        e.buffer.pop();
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                _ => return EventResult::Handled,
            }
        }

        if self.popover_open && key == Key::Escape {
            self.popover_open = false;
            ctx.request_paint();
            return EventResult::Handled;
        }

        if self.editable {
            match key {
                Key::F2 | Key::Enter => {
                    if let Some((r, c)) = self.cursor_cell {
                        self.begin_edit(r, c);
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }
                _ => {}
            }
        }
        let Some((r, c)) = self.cursor_cell else {
            if self.row_count() > 0 && !self.columns.is_empty() {
                let first_col = self.visible_columns().next().unwrap_or(0);
                self.move_cursor(0, first_col);
                ctx.request_paint();
                return EventResult::Handled;
            }
            return EventResult::Ignored;
        };
        let page = self.visible_row_count();
        let vis_r = self.visible_row(r);
        let last_vis = self.row_count().saturating_sub(1);
        match key {
            Key::Left => {
                if let Some(prev) = self.next_visible_col(c, -1) {
                    self.move_cursor(r, prev);
                }
            }
            Key::Right => {
                if let Some(next) = self.next_visible_col(c, 1) {
                    self.move_cursor(r, next);
                }
            }
            Key::Up => {
                let new_vis = vis_r.saturating_sub(1);
                self.move_cursor(self.physical_row(new_vis), c);
            }
            Key::Down => {
                let new_vis = (vis_r + 1).min(last_vis);
                self.move_cursor(self.physical_row(new_vis), c);
            }
            Key::Home => {
                let first_col = self.visible_columns().next().unwrap_or(c);
                self.move_cursor(r, first_col);
            }
            Key::End => {
                let last_col = self.visible_columns().last().unwrap_or(c);
                self.move_cursor(r, last_col);
            }
            Key::PageUp => {
                let new_vis = vis_r.saturating_sub(page);
                self.move_cursor(self.physical_row(new_vis), c);
            }
            Key::PageDown => {
                let new_vis = (vis_r + page).min(last_vis);
                self.move_cursor(self.physical_row(new_vis), c);
            }
            Key::Tab => {
                if let Some(next) = self.next_visible_col(c, 1) {
                    self.move_cursor(r, next);
                }
            }
            _ => return EventResult::Ignored,
        }
        ctx.request_paint();
        EventResult::Handled
    }

    fn next_visible_col(&self, from_col: usize, dir: i32) -> Option<usize> {
        let visible: Vec<usize> = self.visible_columns().collect();
        let pos = visible.iter().position(|&i| i == from_col)?;
        if dir > 0 {
            visible.get(pos + 1).copied()
        } else {
            pos.checked_sub(1).and_then(|p| visible.get(p).copied())
        }
    }

    fn body_rect(&self) -> Rect {
        Rect::new(
            Point::new(self.bounds.x(), self.bounds.y() + self.header_height),
            Size::new(self.bounds.size.width, self.bounds.size.height - self.header_height),
        )
    }

    fn content_height(&self) -> f32 {
        if self.compositional && self.actual_content_height > 0.0 {
            self.actual_content_height
        } else {
            self.row_count() as f32 * self.row_height
        }
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height() - self.body_rect().size.height).max(0.0)
    }

    fn set_scroll_offset(&mut self, offset: f32) {
        self.scroll_offset = offset;
        if let Some(ref state) = self.scroll_state {
            if let Ok(mut s) = state.lock() {
                *s = offset;
            }
        }
    }

    /// Полная ширина всех видимых колонок (может быть больше области — тогда
    /// включается горизонтальная прокрутка).
    fn total_columns_width(&self) -> f32 {
        self.visible_columns()
            .map(|i| self.column_widths.get(i).copied().unwrap_or(0.0))
            .sum()
    }

    /// Предел горизонтальной прокрутки.
    fn max_scroll_x(&self) -> f32 {
        (self.total_columns_width() - self.body_rect().size.width).max(0.0)
    }

    fn set_scroll_offset_x(&mut self, offset: f32) {
        self.scroll_offset_x = offset.clamp(0.0, self.max_scroll_x());
    }

    /// Прямоугольники дорожки и ползунка горизонтальной полосы (если нужна).
    fn h_scrollbar_rects(&self) -> Option<(Rect, Rect)> {
        let body = self.body_rect();
        let content_w = self.total_columns_width();
        if content_w <= body.size.width {
            return None;
        }
        let style = self.compose_scrollbar_style();
        let track = crate::widgets::scroll::horizontal_track_rect(body, &style);
        let thumb = crate::widgets::scroll::horizontal_thumb_rect(
            body,
            content_w,
            self.scroll_offset_x,
            &style,
        );
        Some((track, thumb))
    }

    fn draw_h_scrollbar(&self, list: &mut DisplayList) {
        let body = self.body_rect();
        let content_w = self.total_columns_width();
        if content_w <= body.size.width + 0.5 {
            return;
        }
        // Полосу рисуем всегда, пока контент шире области: иначе не видно,
        // что таблицу вообще можно прокрутить вбок. Через общий fader она бы
        // не появилась — он гасит прозрачность до нуля, пока нет наведения.
        let style = self.compose_scrollbar_style();
        let Some((track, thumb)) = self.h_scrollbar_rects() else {
            return;
        };
        let radius = [style.corner_radius; 4];
        let base = self
            .mss
            .color
            .or(self.mss.border_color)
            .unwrap_or(Color::from_hex("#9CA3AF"));
        list.push_rect(track, base.with_alpha(0.14), radius);
        let thumb_alpha = if self.h_scrollbar_dragging || self.h_scrollbar_hovered {
            0.85
        } else {
            0.5
        };
        list.push_rect(thumb, base.with_alpha(thumb_alpha), radius);
    }

    fn check_comp_rebuild(&mut self) {
        if !self.compositional { return; }
        self.ensure_cached_for_viewport();
        let (f, l) = self.comp_visible_range();
        if f != self.comp_visible_first || l != self.comp_visible_last {
            self.needs_child_rebuild = true;
        }
    }

    fn comp_visible_range(&self) -> (usize, usize) {
        let count = self.row_count();
        if count == 0 { return (0, 0); }
        let body_h = self.body_rect().size.height;
        if body_h <= 0.0 {
            if self.comp_visible_first > 0 || self.comp_visible_last > 0 {
                return (self.comp_visible_first, self.comp_visible_last.min(count));
            }
            return (0, count.min(self.buffer_size * 2 + 10));
        }
        let rh = if self.actual_content_height > 0.0 && !self.row_bounds.is_empty() {
            self.actual_content_height / count as f32
        } else {
            self.row_height
        };
        let vis_first = (self.scroll_offset / rh) as usize;
        let vis_count = (body_h / rh) as usize + 2;
        let vis_last = (vis_first + vis_count).min(count);
        let buf = self.buffer_size;
        let first = vis_first.saturating_sub(buf);
        let last = (vis_last + buf).min(count);
        (first, last)
    }

    fn row_at_y(&self, y: f32) -> Option<usize> {
        let body = self.body_rect();
        if y < body.y() || y > body.y() + body.size.height { return None; }
        let local_y = y - body.y() + self.scroll_offset;

        if self.compositional && !self.row_bounds.is_empty() {
            let body_top = body.y();
            let spacer_offset = if self.comp_visible_first > 0 { 1 } else { 0 };
            for (i, &(row_y, row_h)) in self.row_bounds.iter().enumerate().skip(spacer_offset) {
                let rel_y = row_y - body_top;
                if local_y >= rel_y && local_y < rel_y + row_h {
                    let vis_row = (i - spacer_offset) + self.comp_visible_first;
                    if vis_row < self.row_count() {
                        return Some(self.physical_row(vis_row));
                    }
                }
            }
            return None;
        }

        let effective_rh = self.effective_row_height();
        if effective_rh <= 0.0 { return None; }
        let vis_row = (local_y / effective_rh) as usize;
        if vis_row < self.row_count() {
            Some(self.physical_row(vis_row))
        } else {
            None
        }
    }

    fn effective_row_height(&self) -> f32 {
        if self.compositional && self.actual_content_height > 0.0 {
            let count = self.row_count();
            if count > 0 { self.actual_content_height / count as f32 } else { self.row_height }
        } else {
            self.row_height
        }
    }

    fn col_at_x(&self, x: f32) -> Option<usize> {
        let mut cx = self.bounds.x() - self.scroll_offset_x;
        for phys_i in self.visible_columns() {
            let w = self.column_widths.get(phys_i).copied().unwrap_or(0.0);
            if x >= cx && x < cx + w { return Some(phys_i); }
            cx += w;
        }
        None
    }

    /// Левый край и ширина колонки на экране.
    fn col_x_screen(&self, phys_col: usize) -> Option<(f32, f32)> {
        let mut cx = self.bounds.x() - self.scroll_offset_x;
        for phys_i in self.visible_columns() {
            let w = self.column_widths.get(phys_i).copied().unwrap_or(0.0);
            if phys_i == phys_col { return Some((cx, w)); }
            cx += w;
        }
        None
    }

    fn measured_text_width(&self, text: &str) -> f32 {
        let chars = text.chars().count();
        match self.text_measure.as_ref() {
            Some(tm) => tm.measure_text_width(text, self.cell_font_size, chars),
            None => chars as f32 * self.cell_font_size * 0.55,
        }
    }

    /// Левый край и ширина нарисованного текста ячейки с учётом
    /// выравнивания колонки: по этой рамке ставится I-beam и рисуется
    /// подсветка выделения.
    fn cell_text_box(&self, phys_col: usize, text: &str) -> Option<(f32, f32)> {
        let (col_x, col_w) = self.col_x_screen(phys_col)?;
        let cp = self.cell_padding;
        let avail = (col_w - cp * 2.0).max(0.0);
        if avail <= 0.0 { return None; }
        let w = self.measured_text_width(text).min(avail);
        let left = col_x + self.row_padding[0] + cp;
        let x = match self.columns.get(phys_col).map(|c| c.align).unwrap_or_default() {
            ColumnAlign::Right => left + (avail - w),
            ColumnAlign::Center => left + (avail - w) / 2.0,
            ColumnAlign::Left => left,
        };
        Some((x, w))
    }

    /// Текст ячейки — только у обычных колонок: у колонки со своим
    /// рисователем содержимое живёт в дочернем виджете.
    fn plain_cell_text(&self, phys_row: usize, phys_col: usize) -> Option<String> {
        let col = self.columns.get(phys_col)?;
        if col.cell_renderer.is_some() || col.cell_renderer_with_row.is_some() {
            return None;
        }
        let row = self.get_physical_row(phys_row)?;
        let text = row.get(phys_col)?.clone();
        if text.is_empty() { None } else { Some(text) }
    }

    fn byte_at_x(&self, text: &str, x_local: f32) -> usize {
        let idx = match self.text_measure.as_ref() {
            Some(tm) => tm.hit_test_char_styled(text, self.cell_font_size, x_local.max(0.0), None),
            None => (x_local.max(0.0) / (self.cell_font_size * 0.55).max(1.0)) as usize,
        };
        text.char_indices().nth(idx).map(|(b, _)| b).unwrap_or(text.len())
    }

    /// Ячейка и позиция в её тексте под курсором — `None`, если курсор мимо
    /// самого текста (пустое место ячейки текстом не считается).
    fn cell_text_hit(&self, pos: Point) -> Option<(usize, usize, usize)> {
        if !self.text_selection { return None; }
        let phys_row = self.row_at_y(pos.y)?;
        let phys_col = self.col_at_x(pos.x)?;
        let text = self.plain_cell_text(phys_row, phys_col)?;
        let (text_x, text_w) = self.cell_text_box(phys_col, &text)?;
        if pos.x < text_x || pos.x > text_x + text_w { return None; }
        Some((phys_row, phys_col, self.byte_at_x(&text, pos.x - text_x)))
    }

    /// Позиция в тексте уже выбранной ячейки — для протягивания выделения,
    /// когда курсор ушёл за край текста.
    fn byte_in_cell(&self, phys_row: usize, phys_col: usize, x: f32) -> Option<usize> {
        let text = self.plain_cell_text(phys_row, phys_col)?;
        let (text_x, text_w) = self.cell_text_box(phys_col, &text)?;
        if x <= text_x { return Some(0); }
        if x >= text_x + text_w { return Some(text.len()); }
        Some(self.byte_at_x(&text, x - text_x))
    }

    fn selected_cell_text(&self) -> Option<String> {
        let sel = self.text_sel?;
        if sel.is_empty() { return None; }
        let text = self.plain_cell_text(sel.row, sel.col)?;
        let (start, end) = sel.range();
        text.get(start..end).map(|s| s.to_string())
    }

    /// Выделенные строки целиком: видимые колонки через табуляцию — в таком
    /// виде вставка попадает по столбцам в таблицах и редакторах.
    fn selected_rows_text(&self) -> Option<String> {
        if self.selected_rows.is_empty() { return None; }
        let mut rows: Vec<usize> = self.selected_rows.clone();
        rows.sort_by_key(|r| self.visible_row(*r));
        let cols: Vec<usize> = self.visible_columns().collect();
        let mut out = String::new();
        for (i, phys_row) in rows.iter().enumerate() {
            let Some(data) = self.get_physical_row(*phys_row) else { continue };
            if i > 0 { out.push('\n'); }
            for (j, phys_col) in cols.iter().enumerate() {
                if j > 0 { out.push('\t'); }
                if let Some(v) = data.get(*phys_col) { out.push_str(v); }
            }
        }
        if out.is_empty() { None } else { Some(out) }
    }

    /// Что скопирует «Копировать»: выделенный фрагмент, иначе текст ячейки
    /// под меню, иначе выделенные строки.
    fn copy_payload(&self) -> Option<String> {
        if let Some(t) = self.selected_cell_text() { return Some(t); }
        if let Some(menu) = self.context_menu.as_ref() {
            if let Some(t) = self.plain_cell_text(menu.row, menu.col) { return Some(t); }
        }
        self.selected_rows_text()
    }

    fn context_menu_items(&self) -> [String; 2] {
        [
            crate::i18n::builtin("table.copy", "Copy"),
            crate::i18n::builtin("table.copy_row", "Copy row"),
        ]
    }

    fn context_menu_rect(&self) -> Option<Rect> {
        let menu = self.context_menu.as_ref()?;
        let w = CONTEXT_MENU_WIDTH;
        let h = POPOVER_PADDING * 2.0 + 2.0 * POPOVER_ITEM_HEIGHT;
        // У правого и нижнего края таблицы меню разворачивается внутрь,
        // иначе пункты уехали бы за её границу.
        let max_x = self.bounds.x() + self.bounds.size.width;
        let max_y = self.bounds.y() + self.bounds.size.height;
        let x = if menu.origin.x + w > max_x { (menu.origin.x - w).max(self.bounds.x()) } else { menu.origin.x };
        let y = if menu.origin.y + h > max_y { (menu.origin.y - h).max(self.bounds.y()) } else { menu.origin.y };
        Some(Rect::new(Point::new(x, y), Size::new(w, h)))
    }

    fn context_menu_item_rect(&self, index: usize) -> Option<Rect> {
        let menu = self.context_menu_rect()?;
        let y = menu.y() + POPOVER_PADDING + index as f32 * POPOVER_ITEM_HEIGHT;
        Some(Rect::new(
            Point::new(menu.x() + POPOVER_PADDING, y),
            Size::new(menu.size.width - POPOVER_PADDING * 2.0, POPOVER_ITEM_HEIGHT),
        ))
    }

    fn context_menu_hit_test(&self, pos: Point) -> Option<usize> {
        if self.context_menu.is_none() { return None; }
        (0..2).find(|i| {
            self.context_menu_item_rect(*i).map_or(false, |r| r.contains(pos))
        })
    }

    /// Подсветка выделенного фрагмента под текстом ячейки.
    fn draw_text_selection(
        &self,
        list: &mut DisplayList,
        phys_row: usize,
        phys_col: usize,
        text: &str,
        text_y: f32,
    ) {
        let Some(sel) = self.text_sel else { return };
        if sel.row != phys_row || sel.col != phys_col || sel.is_empty() { return; }
        let Some((text_x, _)) = self.cell_text_box(phys_col, text) else { return };
        let (start, end) = sel.range();
        let color = self
            .mss
            .selection_color
            .unwrap_or_else(|| {
                self.mss
                    .accent_color
                    .unwrap_or(Color::from_hex("#3B82F6"))
                    .with_alpha(0.35)
            });
        list.push_text_selection(
            text,
            start.min(text.len()),
            end.min(text.len()),
            text_x,
            text_y,
            self.cell_font_size + 2.0,
            self.cell_font_size,
            color,
        );
    }

    fn draw_context_menu(&self, list: &mut DisplayList) {
        let Some(rect) = self.context_menu_rect() else { return; };
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#CBD5E1"));
        let fg = self.mss.color.unwrap_or(Color::from_hex("#1E293B"));
        let hover_bg = self.row_hover_bg.unwrap_or_else(|| bg.darken(0.06));

        let shadow = Rect::new(Point::new(rect.x() + 2.0, rect.y() + 4.0), rect.size);
        list.push_rect(shadow, Color::from_hex("#000000").with_alpha(0.18), [8.0; 4]);
        list.push_rect_bordered(rect, bg, [8.0; 4], Border::new(1.0, border_color));

        let hovered = self.context_menu.as_ref().and_then(|m| m.hovered);
        for (idx, label) in self.context_menu_items().iter().enumerate() {
            let Some(item) = self.context_menu_item_rect(idx) else { continue };
            if hovered == Some(idx) {
                list.push_rect(item, hover_bg, [4.0; 4]);
            }
            let text_rect = Rect::new(
                Point::new(item.x() + 8.0, item.y() + (item.size.height - self.cell_font_size) / 2.0),
                Size::new(item.size.width - 16.0, self.cell_font_size + 2.0),
            );
            list.push_text(label, text_rect, fg, self.cell_font_size);
        }
    }

    /// Копирует то, что просит пункт меню, и закрывает меню.
    fn run_context_menu_item(&mut self, index: usize, ctx: &mut EventContext) {
        let text = if index == 0 {
            self.copy_payload()
        } else {
            let row = self.context_menu.as_ref().map(|m| m.row);
            row.and_then(|r| {
                let cols: Vec<usize> = self.visible_columns().collect();
                let data = self.get_physical_row(r)?;
                let mut out = String::new();
                for (j, c) in cols.iter().enumerate() {
                    if j > 0 { out.push('\t'); }
                    if let Some(v) = data.get(*c) { out.push_str(v); }
                }
                if out.is_empty() { None } else { Some(out) }
            })
        };
        if let Some(text) = text {
            ctx.copy_to_clipboard(&text);
        }
        self.context_menu = None;
        ctx.request_paint();
    }

    fn scrollbar_rects(&self) -> Option<(Rect, Rect)> {
        let body = self.body_rect();
        if self.content_height() <= body.size.height {
            return None;
        }
        let style = self.compose_scrollbar_style();
        let track = crate::widgets::scroll::vertical_track_rect(body, &style);
        let thumb = crate::widgets::scroll::vertical_thumb_rect(
            body,
            self.content_height(),
            self.scroll_offset,
            &style,
        );
        Some((track, thumb))
    }

    fn compose_scrollbar_style(&self) -> crate::widgets::scroll::ScrollbarStyle {
        let fg = self.mss.color
            .or(self.mss.border_color)
            .unwrap_or(Color::from_hex("#9CA3AF"));
        self.mss.scrollbar_style(fg)
    }

    fn get_physical_row(&self, physical_idx: usize) -> Option<&Vec<String>> {
        match &self.data {
            TableDataSource::Eager(rows) => rows.get(physical_idx),
            TableDataSource::Virtual { .. } => self.row_cache.get(&physical_idx),
        }
    }

    fn get_visible_row(&self, visible_idx: usize) -> Option<&Vec<String>> {
        self.get_physical_row(self.physical_row(visible_idx))
    }

    fn ensure_cached_for_viewport(&mut self) {
        let (row_count, row_builder) = match &self.data {
            TableDataSource::Virtual { row_count, row_builder } => {
                (*row_count, row_builder.clone())
            }
            TableDataSource::Eager(_) => return,
        };

        let (fetch_start, fetch_end) = if self.compositional {
            self.comp_visible_range()
        } else {
            let body_h = (self.bounds.size.height - self.header_height).max(0.0);
            let viewport_top = self.scroll_offset;
            let viewport_bottom = viewport_top + body_h;
            let vis_first = (viewport_top / self.row_height) as usize;
            let vis_last =
                ((viewport_bottom / self.row_height) as usize + 1).min(row_count);
            (
                vis_first.saturating_sub(self.buffer_size),
                (vis_last + self.buffer_size).min(row_count),
            )
        };
        let fetch_range = fetch_start..fetch_end;

        if fetch_range.start >= self.cache_range.start
            && fetch_range.end <= self.cache_range.end
        {
            return;
        }

        let retain_start = fetch_start.saturating_sub(self.buffer_size);
        let retain_end = (fetch_end + self.buffer_size).min(row_count);
        self.row_cache.retain(|k, _| *k >= retain_start && *k < retain_end);

        for i in fetch_range {
            if !self.row_cache.contains_key(&i) {
                self.row_cache.insert(i, row_builder(i));
            }
        }
        self.cache_range = fetch_start..fetch_end;
    }

    fn draw_scrollbar(&self, list: &mut DisplayList) {
        let body = self.body_rect();
        if self.content_height() <= body.size.height { return; }
        let style = self.compose_scrollbar_style();
        let opacity = crate::widgets::scroll::effective_opacity(&self.scrollbar_fader, &style);
        if opacity <= 0.0 { return; }
        let mut fader = self.scrollbar_fader;
        fader.dragging = self.scrollbar_dragging;
        fader.hovered = self.scrollbar_hovered || fader.hovered;
        crate::widgets::scroll::render_vertical(
            list,
            body,
            self.content_height(),
            self.scroll_offset,
            &style,
            &fader,
            opacity,
        );
    }

    fn draw_header(&self, list: &mut DisplayList) {
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#E2E8F0"));
        let fg = self.mss.color.unwrap_or(Color::from_hex("#334155"));
        let header_bg = self.header_bg_custom.unwrap_or_else(|| bg.darken(0.04));
        let header_fg = self.header_color_custom.unwrap_or(fg);
        let h_font_size = self.header_font_size;
        let h_padding = self.header_padding;

        let header_rect = Rect::new(
            self.bounds.origin,
            Size::new(self.bounds.size.width, self.header_height),
        );
        list.push_rect(header_rect, header_bg, [8.0, 8.0, 0.0, 0.0]);

        let hb_rect = Rect::new(
            Point::new(self.bounds.x(), self.bounds.y() + self.header_height - 1.0),
            Size::new(self.bounds.size.width, 1.0),
        );
        list.push_rect(hb_rect, border_color, [0.0; 4]);

        let mut cx = self.bounds.x() - self.scroll_offset_x;
        let visible: Vec<usize> = self.visible_columns().collect();
        let last_vis_pos = visible.len().saturating_sub(1);
        for (vis_pos, phys_i) in visible.iter().copied().enumerate() {
            let col = &self.columns[phys_i];
            let w = self.column_widths.get(phys_i).copied().unwrap_or(0.0);
            if w <= 0.0 { continue; }

            let is_sorted = self.sortable && col.sortable && self.sort_column == Some(phys_i);
            let is_sortable = self.sortable && col.sortable;
            let icon_w = if is_sorted || is_sortable {
                SORT_ICON_SIZE + SORT_ICON_PADDING
            } else {
                0.0
            };

            let text_rect = Rect::new(
                Point::new(cx + h_padding, self.bounds.y() + (self.header_height - h_font_size) / 2.0),
                Size::new((w - h_padding * 2.0 - icon_w).max(0.0), h_font_size + 2.0),
            );

            let cell_fg = if is_sorted {
                self.mss.icon_color(IconState::Selected, header_fg)
            } else {
                header_fg
            };
            list.push_clip(Rect::new(
                Point::new(cx, self.bounds.y()),
                Size::new(w, self.header_height),
            ));
            list.push_text_singleline(
                &col.header,
                text_rect,
                cell_fg,
                h_font_size,
                col.align.to_text_align(),
                600,
            );
            list.pop_clip();

            if icon_w > 0.0 {
                let icon_x = cx + w - h_padding - SORT_ICON_SIZE;
                let icon_y = self.bounds.y() + (self.header_height - SORT_ICON_SIZE) / 2.0;
                let icon_rect = Rect::new(
                    Point::new(icon_x, icon_y),
                    Size::new(SORT_ICON_SIZE, SORT_ICON_SIZE),
                );
                let (glyph, color) = match (is_sorted, self.sort_direction) {
                    (true, SortDirection::Ascending) => (MI_ARROW_DROP_UP, cell_fg),
                    (true, SortDirection::Descending) => (MI_ARROW_DROP_DOWN, cell_fg),
                    _ => {
                        let hover = self.hovered_header_col == Some(phys_i);
                        let alpha = if hover { 0.55 } else { 0.25 };
                        (MI_UNFOLD_MORE, header_fg.with_alpha(alpha))
                    }
                };
                list.push_text_aligned(
                    glyph,
                    icon_rect,
                    color,
                    SORT_ICON_SIZE,
                    TextAlign::CENTER,
                    TextDecoration::None,
                    400,
                );
            }

            if vis_pos < last_vis_pos {
                let div_rect = Rect::new(
                    Point::new(cx + w - 0.5, self.bounds.y() + 6.0),
                    Size::new(1.0, self.header_height - 12.0),
                );
                list.push_rect(div_rect, border_color.with_alpha(0.5), [0.0; 4]);
            }
            cx += w;
        }

        if let Some(btn_rect) = self.settings_button_rect() {
            let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
            let bg_color = if self.popover_open {
                primary.with_alpha(0.18)
            } else if self.settings_button_hovered {
                header_fg.with_alpha(0.10)
            } else {
                Color::TRANSPARENT
            };
            if bg_color != Color::TRANSPARENT {
                list.push_rect(btn_rect, bg_color, [6.0; 4]);
            }
            let icon_color = if self.popover_open {
                primary
            } else {
                header_fg
            };
            list.push_text_aligned(
                MI_TUNE,
                btn_rect,
                icon_color,
                SORT_ICON_SIZE,
                TextAlign::CENTER,
                TextDecoration::None,
                400,
            );
        }
    }

    fn draw_popover(&self, list: &mut DisplayList) {
        let Some(pop_rect) = self.popover_rect() else { return; };
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#CBD5E1"));
        let fg = self.mss.color.unwrap_or(Color::from_hex("#1E293B"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let hover_bg = self.row_hover_bg.unwrap_or_else(|| bg.darken(0.06));

        let shadow_rect = Rect::new(
            Point::new(pop_rect.x() + 2.0, pop_rect.y() + 4.0),
            pop_rect.size,
        );
        list.push_rect(shadow_rect, Color::from_hex("#000000").with_alpha(0.18), [8.0; 4]);

        list.push_rect_bordered(pop_rect, bg, [8.0; 4], Border::new(1.0, border_color));

        let hideable = self.hideable_columns();
        for (idx, &phys_i) in hideable.iter().enumerate() {
            let Some(item_rect) = self.popover_item_rect(idx) else { continue; };
            let is_hover = self.popover_hovered_index == Some(idx);
            if is_hover {
                list.push_rect(item_rect, hover_bg, [4.0; 4]);
            }

            let check_rect = Rect::new(
                Point::new(
                    item_rect.x() + 4.0,
                    item_rect.y() + (item_rect.size.height - POPOVER_CHECK_SIZE) / 2.0,
                ),
                Size::new(POPOVER_CHECK_SIZE, POPOVER_CHECK_SIZE),
            );
            let checked = self.is_col_visible(phys_i);
            let check_border = if checked { primary } else { border_color };
            let check_fill = if checked { primary } else { Color::TRANSPARENT };
            list.push_rect_bordered(check_rect, check_fill, [3.0; 4], Border::new(1.5, check_border));
            if checked {
                list.push_text_aligned(
                    MI_CHECK,
                    check_rect,
                    Color::WHITE,
                    POPOVER_CHECK_SIZE - 2.0,
                    TextAlign::CENTER,
                    TextDecoration::None,
                    700,
                );
            }

            let label_rect = Rect::new(
                Point::new(
                    check_rect.x() + check_rect.size.width + POPOVER_GAP_AFTER_CHECK,
                    item_rect.y(),
                ),
                Size::new(
                    (item_rect.size.width
                        - (check_rect.size.width + POPOVER_GAP_AFTER_CHECK + 4.0))
                        .max(0.0),
                    item_rect.size.height,
                ),
            );
            list.push_text_aligned(
                &self.columns[phys_i].header,
                label_rect,
                fg,
                13.0,
                TextAlign::LEFT | TextAlign::VCENTER,
                TextDecoration::None,
                500,
            );
        }
    }
}

impl Element for TableViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(tv) = widget.as_any().downcast_ref::<TableView>() {
            let mut data_changed = false;
            let columns_changed = self.columns.len() != tv.columns.len()
                || self
                    .columns
                    .iter()
                    .zip(tv.columns.iter())
                    .any(|(a, b)| a.header != b.header);
            self.columns = tv.columns.clone();

            if columns_changed {
                // Кэш строк собран под прежний набор колонок: значения в нём
                // лежат по старым индексам, и после смены набора показывали бы
                // данные не из тех столбцов.
                self.row_cache.clear();
                self.cache_range = 0..0;
                self.column_visibility.resize(self.columns.len(), true);
                if let Some(ref state) = self.column_visibility_state {
                    if let Ok(g) = state.lock() {
                        if g.len() == self.columns.len() {
                            self.column_visibility = g.clone();
                        }
                    }
                }
                self.user_col_widths.resize(self.columns.len(), None);
            }

            match &tv.data {
                TableDataSource::Eager(rows) => {
                    if let TableDataSource::Eager(old_rows) = &self.data {
                        data_changed = old_rows != rows;
                    } else {
                        data_changed = true;
                    }
                    if data_changed {
                        self.data = TableDataSource::Eager(rows.clone());
                        self.row_cache.clear();
                        self.cache_range = 0..0;
                    }
                }
                TableDataSource::Virtual { row_count, row_builder } => {
                    let old_count = self.row_count();
                    self.data = TableDataSource::Virtual {
                        row_count: *row_count,
                        row_builder: row_builder.clone(),
                    };
                    if *row_count != old_count {
                        data_changed = true;
                        self.row_cache.clear();
                        self.cache_range = 0..0;
                    }
                }
            }
            self.sortable = tv.sortable;
            self.row_height = tv.row_height;
            self.header_height = tv.header_height;
            self.striped = tv.striped;
            self.buffer_size = tv.buffer_size;
            self.on_sort = tv.on_sort.clone();
            self.on_row_click = tv.on_row_click.clone();
            self.on_selection_change = tv.on_selection_change.clone();
            self.on_cell_select = tv.on_cell_select.clone();
            self.on_cell_edit = tv.on_cell_edit.clone();
            self.on_row_double_click = tv.on_row_double_click.clone();
            self.on_cell_double_click = tv.on_cell_double_click.clone();
            self.on_column_resize = tv.on_column_resize.clone();
            self.column_visibility_state = tv.column_visibility_state.clone();
            self.on_column_visibility_change = tv.on_column_visibility_change.clone();
            let old_visibility = self.column_visibility.clone();
            if let Some(ref state) = self.column_visibility_state {
                if let Ok(g) = state.lock() {
                    if g.len() == self.columns.len() {
                        self.column_visibility = g.clone();
                    }
                }
            }
            if old_visibility != self.column_visibility {
                // Скрытая колонка могла не считаться в row_builder — после
                // её включения кэш содержал бы пустые ячейки до ближайшей
                // прокрутки.
                self.row_cache.clear();
                self.cache_range = 0..0;
                self.needs_child_rebuild = self.compositional;
            }
            self.selected_rows = tv.selected_rows.clone();
            self.cell_cursor = tv.cell_cursor;
            self.text_selection = tv.text_selection;
            if !self.cell_cursor {
                self.cursor_cell = None;
            }
            if !self.text_selection {
                self.text_sel = None;
                self.text_selecting = false;
                self.context_menu = None;
            }
            self.width = tv.width;
            self.height = tv.height;
            self.compositional = tv.columns.iter().any(|c| {
                c.cell_renderer.is_some() || c.cell_renderer_with_row.is_some()
            });
            if (self.compositional && data_changed) || columns_changed {
                self.needs_child_rebuild = true;
            }
            if let Some(v) = tv.custom_header_bg { self.header_bg_custom = Some(v); }
            if let Some(v) = tv.custom_header_color { self.header_color_custom = Some(v); }
            if let Some(v) = tv.custom_header_font_size { self.header_font_size = v; }
            if let Some(v) = tv.custom_cell_font_size { self.cell_font_size = v; }
            if let Some(v) = tv.custom_cell_padding { self.cell_padding = v; }
            if let Some(v) = tv.custom_cell_min_width { self.cell_min_width = v; }
            if let Some(v) = tv.custom_cell_max_width { self.cell_max_width = v; }
            if let Some(v) = tv.custom_row_hover_bg { self.row_hover_bg = Some(v); }
            if let Some(v) = tv.custom_row_selected_bg { self.row_selected_bg = Some(v); }
            if let Some(v) = tv.custom_row_padding { self.row_padding = v; }
            self.compute_column_widths(self.bounds.size.width);
            self.ensure_cached_for_viewport();
            if data_changed || columns_changed {
                self.refresh_sorted_indices();
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self.width.map(|d| d.resolve(constraints.max_width)).unwrap_or(constraints.max_width).min(constraints.max_width);
        let h = self.height.map(|d| d.resolve(constraints.max_height)).unwrap_or(constraints.max_height).min(constraints.max_height);
        let h = if h.is_infinite() { 300.0 } else { h };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        let old_widths = self.column_widths.clone();
        self.compute_column_widths(w);
        if self.compositional && self.column_widths != old_widths {
            self.needs_child_rebuild = true;
        }
        // Не даём горизонтальной прокрутке выйти за предел при изменении
        // размеров окна или скрытии колонок.
        let max_x = self.max_scroll_x();
        if self.scroll_offset_x > max_x {
            self.scroll_offset_x = max_x;
        }
        self.ensure_cached_for_viewport();
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#E2E8F0"));
        let fg = self.mss.color.unwrap_or(Color::from_hex("#334155"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let radius_ref = self.bounds.size.width.min(self.bounds.size.height);
        let radii = self.mss.border_radius_resolved(radius_ref, 8.0);

        list.push_rect_bordered(self.bounds, bg, radii, Border::new(1.0, border_color));
        list.push_clip(self.bounds);
        self.draw_header(list);

        if self.compositional {
            let body = self.body_rect();
            list.push_clip(body);
            let ty = -self.scroll_offset;
            list.push_transform(Transform::translation(0.0, ty));

            let spacer_offset = if self.comp_visible_first > 0 { 1 } else { 0 };
            for (i, &(row_y, row_h)) in self.row_bounds.iter().enumerate().skip(spacer_offset) {
                let vis_row = (i - spacer_offset) + self.comp_visible_first;
                if vis_row >= self.row_count() { break; }
                let bottom_spacer_count = if self.comp_visible_last < self.row_count() { 1 } else { 0 };
                if i >= self.row_bounds.len() - bottom_spacer_count { break; }

                let phys_row = self.physical_row(vis_row);
                let is_hovered = self.hovered_row == Some(phys_row);
                let is_selected = self.selected_rows.contains(&phys_row);

                let row_bg = if is_selected {
                    self.row_selected_bg.unwrap_or_else(|| primary.with_alpha(0.15))
                } else if is_hovered {
                    self.row_hover_bg.unwrap_or_else(|| bg.darken(0.08))
                } else if self.striped && vis_row % 2 == 1 {
                    self.row_striped_bg.unwrap_or_else(|| bg.darken(0.015))
                } else {
                    continue;
                };

                let row_rect = Rect::new(
                    Point::new(self.bounds.x(), row_y),
                    Size::new(self.bounds.size.width, row_h),
                );
                list.push_rect(row_rect, row_bg, [0.0; 4]);
            }

            for (i, &(row_y, row_h)) in self.row_bounds.iter().enumerate().skip(spacer_offset) {
                let vis_row = (i - spacer_offset) + self.comp_visible_first;
                if vis_row >= self.row_count() { break; }
                let bottom_spacer_count = if self.comp_visible_last < self.row_count() { 1 } else { 0 };
                if i >= self.row_bounds.len() - bottom_spacer_count { break; }

                let hb = Rect::new(
                    Point::new(self.bounds.x(), row_y + row_h - 1.0),
                    Size::new(self.bounds.size.width, 1.0),
                );
                list.push_rect(hb, border_color.with_alpha(self.grid_alpha), [0.0; 4]);

                let visible: Vec<usize> = self.visible_columns().collect();
                let mut cx = self.bounds.x() - self.scroll_offset_x;
                let last_vis_pos = visible.len().saturating_sub(1);
                for (vis_pos, phys_j) in visible.iter().copied().enumerate() {
                    let w = self.column_widths.get(phys_j).copied().unwrap_or(0.0);
                    cx += w;
                    if vis_pos >= last_vis_pos { break; }
                    let vb = Rect::new(Point::new(cx - 0.5, row_y), Size::new(1.0, row_h));
                    list.push_rect(vb, border_color.with_alpha(self.grid_alpha), [0.0; 4]);
                }
            }

            // Текст обычных ячеек. В дереве виджетов их нет (см.
            // `build_children`), поэтому рисуем сами — по тем же границам
            // строк и колонок, что и сетка выше.
            let cp = self.cell_padding;
            let cfs = self.cell_font_size;
            let rp = self.row_padding;
            let bottom_spacer_count = if self.comp_visible_last < self.row_count() { 1 } else { 0 };
            for (i, &(row_y, row_h)) in self.row_bounds.iter().enumerate().skip(spacer_offset) {
                let vis_row = (i - spacer_offset) + self.comp_visible_first;
                if vis_row >= self.row_count() { break; }
                if i >= self.row_bounds.len() - bottom_spacer_count { break; }
                let phys_row = self.physical_row(vis_row);
                let Some(row_data) = self.get_physical_row(phys_row) else { continue };

                let mut cell_x = self.bounds.x() - self.scroll_offset_x + rp[0];
                for phys_col in self.visible_columns() {
                    let col_w = self.column_widths.get(phys_col).copied().unwrap_or(0.0);
                    let Some(col) = self.columns.get(phys_col) else { continue };
                    if col.cell_renderer.is_some() || col.cell_renderer_with_row.is_some() {
                        cell_x += col_w;
                        continue;
                    }
                    let editing = self
                        .edit_state
                        .as_ref()
                        .map(|e| e.row == phys_row && e.col == phys_col)
                        .unwrap_or(false);
                    let buf_text;
                    let text = if editing {
                        buf_text = self
                            .edit_state
                            .as_ref()
                            .map(|e| format!("{}|", e.buffer))
                            .unwrap_or_default();
                        buf_text.as_str()
                    } else {
                        row_data.get(phys_col).map(|s| s.as_str()).unwrap_or("")
                    };
                    if !text.is_empty() && col_w > 0.0 {
                        let avail_w = (col_w - cp * 2.0).max(0.0);
                        let cell_rect = Rect::new(
                            Point::new(
                                cell_x + cp,
                                row_y + rp[1] + (row_h - rp[1] - rp[3] - cfs) / 2.0,
                            ),
                            Size::new(avail_w, cfs + 2.0),
                        );
                        // Длинный текст переносится по словам и вылезает за
                        // строку — подрезаем его по клетке. Оценка ширины
                        // грубая (точный обмер стоит дороже самой отрисовки),
                        // лишний clip у пограничных ячеек безвреден, а на
                        // коротких значениях его нет вовсе — иначе батчинг
                        // ломался бы на каждой ячейке.
                        let may_wrap = text.chars().count() as f32 * cfs * 0.55 > avail_w;
                        if may_wrap {
                            list.push_clip(Rect::new(
                                Point::new(cell_x, row_y),
                                Size::new(col_w, row_h),
                            ));
                        }
                        self.draw_text_selection(list, phys_row, phys_col, text, cell_rect.y());
                        list.push_text_singleline(
                            text,
                            cell_rect,
                            fg,
                            cfs,
                            col.align.to_text_align(),
                            400,
                        );
                        if may_wrap {
                            list.pop_clip();
                        }
                    }
                    cell_x += col_w;
                }
            }

            if let Some((phys_r, c)) = self.cursor_cell.filter(|_| self.cell_cursor) {
                let vis_r = self.visible_row(phys_r);
                if vis_r >= self.comp_visible_first && vis_r < self.comp_visible_last {
                    let local = vis_r - self.comp_visible_first + spacer_offset;
                    if let Some((row_y, row_h)) = self.row_bounds.get(local).copied() {
                        let mut cx = self.bounds.x() - self.scroll_offset_x;
                        for phys_j in self.visible_columns() {
                            let w = self.column_widths.get(phys_j).copied().unwrap_or(0.0);
                            if phys_j == c {
                                let rect = Rect::new(Point::new(cx, row_y), Size::new(w, row_h));
                                list.push_rect_bordered(
                                    rect,
                                    primary.with_alpha(0.10),
                                    [2.0; 4],
                                    Border::new(2.0, primary),
                                );
                                break;
                            }
                            cx += w;
                        }
                    }
                }
            }
            return;
        }

        let body = self.body_rect();
        list.push_clip(body);

        let viewport_top = self.scroll_offset;
        let viewport_bottom = viewport_top + body.size.height;
        let count = self.row_count();
        let vis_first = (viewport_top / self.row_height) as usize;
        let vis_last = ((viewport_bottom / self.row_height) as usize + 1).min(count);
        let render_first = vis_first.saturating_sub(self.buffer_size);
        let render_last = (vis_last + self.buffer_size).min(count);

        let visible_cols: Vec<usize> = self.visible_columns().collect();
        let last_vis_pos = visible_cols.len().saturating_sub(1);
        for vis_row in render_first..render_last {
            let phys_row = self.physical_row(vis_row);
            let y = body.y() + (vis_row as f32 * self.row_height) - self.scroll_offset;
            let row_rect = Rect::new(Point::new(self.bounds.x(), y), Size::new(self.bounds.size.width, self.row_height));

            let is_selected = self.selected_rows.contains(&phys_row);
            let is_hovered = self.hovered_row == Some(phys_row);
            let row_bg = if is_selected {
                self.row_selected_bg.unwrap_or_else(|| primary.with_alpha(0.15))
            } else if is_hovered {
                self.row_hover_bg.unwrap_or_else(|| bg.darken(0.08))
            } else if self.striped && vis_row % 2 == 1 {
                self.row_striped_bg.unwrap_or_else(|| bg.darken(0.015))
            } else {
                Color::TRANSPARENT
            };
            if row_bg != Color::TRANSPARENT {
                list.push_rect(row_rect, row_bg, [0.0; 4]);
            }

            let rb = Rect::new(Point::new(self.bounds.x(), y + self.row_height - 1.0), Size::new(self.bounds.size.width, 1.0));
            list.push_rect(rb, border_color.with_alpha(self.grid_alpha), [0.0; 4]);

            let mut vx = self.bounds.x() - self.scroll_offset_x;
            for (vis_pos, phys_j) in visible_cols.iter().copied().enumerate() {
                let w = self.column_widths.get(phys_j).copied().unwrap_or(0.0);
                vx += w;
                if vis_pos >= last_vis_pos { break; }
                let vb = Rect::new(Point::new(vx - 0.5, y), Size::new(1.0, self.row_height));
                list.push_rect(vb, border_color.with_alpha(self.grid_alpha), [0.0; 4]);
            }

            let cp = self.cell_padding;
            let cfs = self.cell_font_size;
            let rp = self.row_padding;
            if let Some(row_data) = self.get_visible_row(vis_row) {
                let mut cell_x = self.bounds.x() - self.scroll_offset_x + rp[0];
                for phys_col in visible_cols.iter().copied() {
                    let col_w = self.column_widths.get(phys_col).copied().unwrap_or(0.0);
                    let editing = self
                        .edit_state
                        .as_ref()
                        .map(|e| e.row == phys_row && e.col == phys_col)
                        .unwrap_or(false);
                    let cell_cursor_here =
                        self.cell_cursor && self.cursor_cell == Some((phys_row, phys_col));
                    if cell_cursor_here || editing {
                        let cell_bg_rect = Rect::new(
                            Point::new(cell_x, y),
                            Size::new(col_w, self.row_height),
                        );
                        let (fill, border_w) = if editing {
                            (bg, 2.0)
                        } else {
                            (primary.with_alpha(0.10), 2.0)
                        };
                        list.push_rect_bordered(
                            cell_bg_rect,
                            fill,
                            [2.0; 4],
                            Border::new(border_w, primary),
                        );
                    }
                    let fallback = row_data.get(phys_col).map(|s| s.as_str()).unwrap_or("");
                    let buf_text;
                    let text = if editing {
                        buf_text = self
                            .edit_state
                            .as_ref()
                            .map(|e| format!("{}|", e.buffer))
                            .unwrap_or_default();
                        buf_text.as_str()
                    } else {
                        fallback
                    };
                    let cell_rect = Rect::new(
                        Point::new(cell_x + cp, y + rp[1] + (self.row_height - rp[1] - rp[3] - cfs) / 2.0),
                        Size::new((col_w - cp * 2.0).max(0.0), cfs + 2.0),
                    );
                    let align = self.columns.get(phys_col).map(|c| c.align).unwrap_or_default();
                    if !text.is_empty() && col_w > 0.0 {
                        list.push_clip(Rect::new(
                            Point::new(cell_x, y),
                            Size::new(col_w, self.row_height),
                        ));
                        self.draw_text_selection(list, phys_row, phys_col, text, cell_rect.y());
                        list.push_text_singleline(text, cell_rect, fg, cfs, align.to_text_align(), 400);
                        list.pop_clip();
                    }
                    cell_x += col_w;
                }
            }
        }

        self.draw_scrollbar(list);
        self.draw_h_scrollbar(list);
        list.pop_clip();
        list.pop_clip();
        list.push_rect_bordered(self.bounds, Color::TRANSPARENT, radii, Border::new(1.0, border_color));
        if self.popover_open {
            self.draw_popover(list);
        }
        if self.context_menu.is_some() {
            self.draw_context_menu(list);
        }
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.compositional { return; }
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#E2E8F0"));
        let radius_ref = self.bounds.size.width.min(self.bounds.size.height);
        let radii = self.mss.border_radius_resolved(radius_ref, 8.0);
        list.pop_transform();
        list.pop_clip();
        list.push_clip(self.bounds);
        self.draw_scrollbar(list);
        self.draw_h_scrollbar(list);
        list.pop_clip();
        list.pop_clip();
        list.push_rect_bordered(self.bounds, Color::TRANSPARENT, radii, Border::new(1.0, border_color));
        if self.popover_open {
            self.draw_popover(list);
        }
        if self.context_menu.is_some() {
            self.draw_context_menu(list);
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let mut needs_repaint = false;
        if self.velocity.abs() > 0.5 {
            self.velocity *= 0.92f32.powf(dt.as_secs_f32() * 60.0);
            let new_offset = (self.scroll_offset + self.velocity * dt.as_secs_f32())
                .clamp(0.0, self.max_scroll());
            self.set_scroll_offset(new_offset);
            if self.velocity.abs() < 0.5 {
                self.velocity = 0.0;
            }
            if !self.compositional {
                self.ensure_cached_for_viewport();
            }
            self.scrollbar_fader.flash();
            needs_repaint = true;
        }
        let style = self.compose_scrollbar_style();
        self.scrollbar_fader.dragging = self.scrollbar_dragging;
        self.scrollbar_fader.hovered = self.scrollbar_hovered || self.scrollbar_fader.hovered;
        if self.scrollbar_fader.tick(dt.as_secs_f32(), &style) {
            needs_repaint = true;
        }
        needs_repaint
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if self.context_menu.is_some() {
                    let hit = self.context_menu_hit_test(*pos);
                    if let Some(menu) = self.context_menu.as_mut() {
                        if menu.hovered != hit {
                            menu.hovered = hit;
                            ctx.request_paint();
                        }
                    }
                    if hit.is_some() {
                        ctx.set_cursor(CursorIcon::Pointer);
                        return EventResult::Handled;
                    }
                }

                // Протягивание выделения текста ведём даже за краем ячейки:
                // курсор ушёл вбок — выделение доходит до конца строки.
                if self.text_selecting {
                    if let Some(sel) = self.text_sel {
                        if let Some(byte) = self.byte_in_cell(sel.row, sel.col, pos.x) {
                            if byte != sel.head {
                                self.text_sel = Some(CellTextSelection { head: byte, ..sel });
                                ctx.request_paint();
                            }
                        }
                    }
                    ctx.set_cursor(CursorIcon::Text);
                    return EventResult::Handled;
                }

                if self.popover_open {
                    let new_idx = self.popover_hit_test(*pos);
                    if new_idx != self.popover_hovered_index {
                        self.popover_hovered_index = new_idx;
                        ctx.request_paint();
                    }
                    if new_idx.is_some() {
                        ctx.set_cursor(CursorIcon::Pointer);
                        return EventResult::Handled;
                    }
                }

                if let Some(rs) = self.resize_state {
                    let delta = pos.x - rs.start_x;
                    let col = &self.columns[rs.col];
                    let min_w = col.min_width.max(self.cell_min_width);
                    let max_w = col.max_width.min(self.cell_max_width);
                    let new_w = (rs.start_width + delta).clamp(min_w, max_w);
                    if self.user_col_widths.len() != self.columns.len() {
                        self.user_col_widths.resize(self.columns.len(), None);
                    }
                    self.user_col_widths[rs.col] = Some(new_w);
                    self.persist_column_widths();
                    self.dirty_flags |= DirtyFlags::LAYOUT | DirtyFlags::RENDER;
                    self.needs_child_rebuild = self.compositional;
                    ctx.set_cursor(CursorIcon::ColResize);
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if let Some(_col) = self.hit_resize_handle(*pos) {
                    ctx.set_cursor(CursorIcon::ColResize);
                    return EventResult::Handled;
                }

                if self.h_scrollbar_dragging {
                    let body = self.body_rect();
                    let content_w = self.total_columns_width();
                    let thumb_w = (body.size.width / content_w * body.size.width).max(20.0);
                    let max_x = self.max_scroll_x();
                    let rel = pos.x - body.x() - self.h_scrollbar_drag_offset;
                    let ratio = rel / (body.size.width - thumb_w).max(1.0);
                    self.set_scroll_offset_x((ratio * max_x).clamp(0.0, max_x));
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.scrollbar_dragging {
                    let body = self.body_rect();
                    let thumb_h = (body.size.height / self.content_height() * body.size.height).max(20.0);
                    let max_s = self.max_scroll();
                    let relative_y = pos.y - body.y() - self.scrollbar_drag_offset;
                    let ratio = relative_y / (body.size.height - thumb_h);
                    self.set_scroll_offset((ratio * max_s).clamp(0.0, max_s));
                    self.velocity = 0.0;
                    if self.compositional {
                        self.check_comp_rebuild();
                    } else {
                        self.ensure_cached_for_viewport();
                    }
                    ctx.set_cursor(CursorIcon::Default);
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if !self.bounds.contains(*pos) {
                    let changed = self.hovered_row.is_some()
                        || self.scrollbar_hovered
                        || self.hovered_header_col.is_some()
                        || self.settings_button_hovered;
                    self.hovered_row = None;
                    self.scrollbar_hovered = false;
                    self.hovered_header_col = None;
                    self.settings_button_hovered = false;
                    if changed { ctx.request_paint(); }
                    return EventResult::Ignored;
                }

                if pos.y < self.bounds.y() + self.header_height {
                    let mut painted = false;
                    let new_btn_hover = self
                        .settings_button_rect()
                        .map_or(false, |r| r.contains(*pos));
                    if new_btn_hover != self.settings_button_hovered {
                        self.settings_button_hovered = new_btn_hover;
                        painted = true;
                    }
                    let new_header_hover = if !new_btn_hover {
                        self.col_at_x(pos.x).filter(|i| {
                            self.sortable && self.columns.get(*i).map_or(false, |c| c.sortable)
                        })
                    } else {
                        None
                    };
                    if new_header_hover != self.hovered_header_col {
                        self.hovered_header_col = new_header_hover;
                        painted = true;
                    }
                    let sortable_hover = new_header_hover.is_some();
                    if new_btn_hover || sortable_hover {
                        ctx.set_cursor(CursorIcon::Pointer);
                    }
                    let body_was = self.hovered_row.is_some() || self.scrollbar_hovered;
                    self.hovered_row = None;
                    self.scrollbar_hovered = false;
                    if body_was { painted = true; }
                    if painted { ctx.request_paint(); }
                    return EventResult::Handled;
                }

                if self.hovered_header_col.is_some() || self.settings_button_hovered {
                    self.hovered_header_col = None;
                    self.settings_button_hovered = false;
                    ctx.request_paint();
                }

                let sb_hovered = self.scrollbar_rects()
                    .map_or(false, |(track, _)| track.contains(*pos));
                if sb_hovered != self.scrollbar_hovered {
                    self.scrollbar_hovered = sb_hovered;
                    ctx.request_paint();
                }
                if sb_hovered {
                    ctx.set_cursor(CursorIcon::Default);
                    self.hovered_row = None;
                    return EventResult::Handled;
                }

                let new_hover = self.row_at_y(pos.y);
                if new_hover != self.hovered_row {
                    self.hovered_row = new_hover;
                    ctx.request_paint();
                }
                if new_hover.is_some() {
                    if self.text_selection {
                        // Текст выделяется мышью: I-beam там, где он есть,
                        // обычная стрелка на пустом месте строки.
                        if self.cell_text_hit(*pos).is_some() {
                            ctx.set_cursor(CursorIcon::Text);
                        } else {
                            ctx.set_cursor(CursorIcon::Default);
                        }
                    } else {
                        ctx.set_cursor(CursorIcon::Pointer);
                    }
                }
                EventResult::Handled
            }
            Event::MouseDown { button, position } if *button == MouseButton::Right => {
                if !self.text_selection || !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                if position.y < self.bounds.y() + self.header_height {
                    return EventResult::Ignored;
                }
                let Some(phys_row) = self.row_at_y(position.y) else {
                    return EventResult::Ignored;
                };
                let col = self.col_at_x(position.x).unwrap_or(0);
                // Меню относится к ячейке под курсором: щелчок вне текущего
                // выделения переносит его туда, внутри — сохраняет.
                let keep = self
                    .text_sel
                    .map(|sel| sel.row == phys_row && sel.col == col && !sel.is_empty())
                    .unwrap_or(false);
                if !keep {
                    self.text_sel = None;
                }
                self.text_selecting = false;
                self.focused = true;
                self.context_menu = Some(CellContextMenu {
                    origin: *position,
                    row: phys_row,
                    col,
                    hovered: None,
                });
                ctx.request_paint();
                EventResult::Handled
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.context_menu.is_some() {
                    if let Some(idx) = self.context_menu_hit_test(*position) {
                        self.run_context_menu_item(idx, ctx);
                        return EventResult::Handled;
                    }
                    self.context_menu = None;
                    ctx.request_paint();
                }

                if self.popover_open {
                    if let Some(idx) = self.popover_hit_test(*position) {
                        let hideable = self.hideable_columns();
                        if let Some(&phys_i) = hideable.get(idx) {
                            let current = self.is_col_visible(phys_i);
                            let new_visible = !current;
                            if self.column_visibility.len() != self.columns.len() {
                                self.column_visibility.resize(self.columns.len(), true);
                            }
                            self.column_visibility[phys_i] = new_visible;
                            self.persist_column_visibility();
                            if let Some(ref cb) = self.on_column_visibility_change {
                                if let Ok(mut f) = cb.lock() { f(phys_i, new_visible); }
                            }
                            self.compute_column_widths(self.bounds.size.width);
                            self.needs_child_rebuild = self.compositional;
                            self.dirty_flags |= DirtyFlags::LAYOUT | DirtyFlags::RENDER;
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                    let inside = self.popover_rect().map_or(false, |r| r.contains(*position));
                    let on_button = self
                        .settings_button_rect()
                        .map_or(false, |r| r.contains(*position));
                    if !inside && !on_button {
                        self.popover_open = false;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                if !self.bounds.contains(*position) { return EventResult::Ignored; }

                if let Some(btn) = self.settings_button_rect() {
                    if btn.contains(*position) {
                        self.popover_open = !self.popover_open;
                        self.popover_hovered_index = None;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                if let Some(col) = self.hit_resize_handle(*position) {
                    let cur_w = self.column_widths.get(col).copied().unwrap_or(0.0);
                    self.resize_state = Some(ColumnResizeState {
                        col,
                        start_x: position.x,
                        start_width: cur_w,
                    });
                    ctx.set_cursor(CursorIcon::ColResize);
                    return EventResult::Handled;
                }

                if position.y < self.bounds.y() + self.header_height && self.sortable {
                    if let Some(col_idx) = self.col_at_x(position.x) {
                        let col = &self.columns[col_idx];
                        if col.sortable {
                            let new_dir = if self.sort_column == Some(col_idx) {
                                match self.sort_direction {
                                    SortDirection::None => SortDirection::Ascending,
                                    SortDirection::Ascending => SortDirection::Descending,
                                    SortDirection::Descending => SortDirection::None,
                                }
                            } else {
                                SortDirection::Ascending
                            };
                            self.sort_direction = new_dir;
                            self.sort_column = if new_dir == SortDirection::None {
                                None
                            } else {
                                Some(col_idx)
                            };
                            if let Some(ref cb) = self.on_sort {
                                if let Ok(mut f) = cb.lock() { f(col_idx, new_dir); }
                                self.sorted_indices = None;
                                self.row_cache.clear();
                                self.cache_range = 0..0;
                                self.ensure_cached_for_viewport();
                            } else {
                                self.refresh_sorted_indices();
                            }
                            self.needs_child_rebuild = self.compositional;
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                }

                if let Some((track, thumb)) = self.h_scrollbar_rects() {
                    if thumb.contains(*position) {
                        self.h_scrollbar_dragging = true;
                        self.h_scrollbar_drag_offset = position.x - thumb.x();
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    if track.contains(*position) {
                        let body = self.body_rect();
                        let thumb_w = thumb.size.width;
                        let max_x = self.max_scroll_x();
                        let rel = position.x - body.x() - thumb_w / 2.0;
                        let ratio = rel / (body.size.width - thumb_w).max(1.0);
                        self.set_scroll_offset_x((ratio * max_x).clamp(0.0, max_x));
                        self.h_scrollbar_dragging = true;
                        self.h_scrollbar_drag_offset = thumb_w / 2.0;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                if let Some((track, thumb)) = self.scrollbar_rects() {
                    if thumb.contains(*position) {
                        self.scrollbar_dragging = true;
                        self.scrollbar_drag_offset = position.y - thumb.y();
                        self.velocity = 0.0;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    if track.contains(*position) {
                        let body = self.body_rect();
                        let thumb_h = thumb.size.height;
                        let max_s = self.max_scroll();
                        let relative_y = position.y - body.y() - thumb_h / 2.0;
                        let ratio = relative_y / (body.size.height - thumb_h);
                        self.set_scroll_offset((ratio * max_s).clamp(0.0, max_s));
                        self.velocity = 0.0;
                        self.scrollbar_dragging = true;
                        self.scrollbar_drag_offset = thumb_h / 2.0;
                        if self.compositional {
                            self.check_comp_rebuild();
                        } else {
                            self.ensure_cached_for_viewport();
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                if let Some(phys_row) = self.row_at_y(position.y) {
                    let col_idx = self.col_at_x(position.x);
                    if let Some(ci) = col_idx {
                        let new_cursor = Some((phys_row, ci));
                        if new_cursor != self.cursor_cell {
                            if self.cell_cursor {
                                self.cursor_cell = new_cursor;
                            }
                            if let Some(ref cb) = self.on_cell_select {
                                if let Ok(mut f) = cb.lock() { f(phys_row, ci); }
                            }
                        }
                    }
                    if self.text_selection {
                        // Клик по тексту ставит каретку и начинает выделение;
                        // клик мимо текста снимает прежнее.
                        match self.cell_text_hit(*position) {
                            Some((row, col, byte)) => {
                                self.text_sel = Some(CellTextSelection {
                                    row,
                                    col,
                                    anchor: byte,
                                    head: byte,
                                });
                                self.text_selecting = true;
                            }
                            None => {
                                self.text_sel = None;
                                self.text_selecting = false;
                            }
                        }
                    }
                    self.focused = true;
                    // Ctrl добавляет строку к выбору, Shift выбирает
                    // промежуток от предыдущей — так ведут себя списки
                    // везде, и без этого нельзя править группу записей.
                    if ctx.modifiers.ctrl {
                        if let Some(pos) = self.selected_rows.iter().position(|&r| r == phys_row) {
                            self.selected_rows.remove(pos);
                        } else {
                            self.selected_rows.push(phys_row);
                        }
                    } else if ctx.modifiers.shift && !self.selected_rows.is_empty() {
                        let anchor = *self.selected_rows.last().unwrap();
                        let (from, to) = if anchor <= phys_row {
                            (anchor, phys_row)
                        } else {
                            (phys_row, anchor)
                        };
                        for row in from..=to {
                            if !self.selected_rows.contains(&row) {
                                self.selected_rows.push(row);
                            }
                        }
                    } else if let Some(pos) =
                        self.selected_rows.iter().position(|&r| r == phys_row)
                    {
                        if self.selected_rows.len() == 1 {
                            self.selected_rows.remove(pos);
                        } else {
                            self.selected_rows = vec![phys_row];
                        }
                    } else {
                        self.selected_rows = vec![phys_row];
                    }
                    if let Some(ref cb) = self.on_selection_change {
                        if let Ok(mut f) = cb.lock() {
                            f(self.selected_rows.clone());
                        }
                    }
                    if let Some(ref cb) = self.on_row_click {
                        if let Ok(mut f) = cb.lock() { f(phys_row); }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::C) if self.text_selection && self.focused && ctx.modifiers.ctrl => {
                match self.copy_payload() {
                    Some(text) => {
                        ctx.copy_to_clipboard(&text);
                        EventResult::Handled
                    }
                    None => EventResult::Ignored,
                }
            }
            Event::KeyDown(Key::Escape) if self.context_menu.is_some() => {
                self.context_menu = None;
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(key) if (self.keyboard_nav || self.editable || self.popover_open) && (self.focused || self.popover_open) => {
                self.handle_key_nav(*key, ctx)
            }
            Event::CharInput(ch) if self.edit_state.is_some() => {
                if !ch.is_control() {
                    if let Some(ref mut e) = self.edit_state {
                        e.buffer.push(*ch);
                    }
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::DoubleClick { button, position } if *button == MouseButton::Left => {
                if self.bounds.contains(*position) {
                    if let (Some(row), Some(col)) =
                        (self.row_at_y(position.y), self.col_at_x(position.x))
                    {
                        self.focused = true;
                        self.cursor_cell = Some((row, col));
                        if let Some(ref cb) = self.on_cell_double_click {
                            if let Ok(mut f) = cb.lock() { f(row, col); }
                        }
                        if let Some(ref cb) = self.on_row_double_click {
                            if let Ok(mut f) = cb.lock() { f(row); }
                        }
                        if self.editable {
                            self.begin_edit(row, col);
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::FocusLost => {
                self.focused = false;
                if self.edit_state.is_some() {
                    self.commit_edit();
                }
                if self.popover_open {
                    self.popover_open = false;
                }
                ctx.request_paint();
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } if *button == MouseButton::Left => {
                if self.text_selecting {
                    self.text_selecting = false;
                }
                if let Some(rs) = self.resize_state.take() {
                    let new_w = self
                        .user_col_widths
                        .get(rs.col)
                        .and_then(|w| *w)
                        .unwrap_or(rs.start_width);
                    if let Some(ref cb) = self.on_column_resize {
                        if let Ok(mut f) = cb.lock() {
                            f(rs.col, new_w);
                        }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.scrollbar_dragging {
                    self.scrollbar_dragging = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.h_scrollbar_dragging {
                    self.h_scrollbar_dragging = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseWheel { delta, delta_x, position } => {
                if !self.bounds.contains(*position) { return EventResult::Ignored; }
                let body = self.body_rect();
                if position.y < body.y() { return EventResult::Ignored; }

                // Горизонтальная прокрутка: собственная дельта тачпада/колеса,
                // либо вертикальная дельта с зажатым Shift (обычная мышь).
                let max_x = self.max_scroll_x();
                let h_delta = if delta_x.abs() > 0.01 {
                    *delta_x
                } else if ctx.modifiers.shift {
                    *delta
                } else {
                    0.0
                };
                if max_x > 0.0 && h_delta.abs() > 0.01 {
                    let new_x = (self.scroll_offset_x - h_delta).clamp(0.0, max_x);
                    if (new_x - self.scroll_offset_x).abs() > 0.01 {
                        self.scroll_offset_x = new_x;
                        self.scrollbar_fader.flash();
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                let new_offset = (self.scroll_offset - delta).clamp(0.0, self.max_scroll());
                if (new_offset - self.scroll_offset).abs() > 0.01 {
                    self.set_scroll_offset(new_offset);
                    self.velocity = 0.0;
                    if self.compositional {
                        self.check_comp_rebuild();
                    } else {
                        self.ensure_cached_for_viewport();
                    }
                    self.scrollbar_fader.flash();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> { Some(self) }
    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

    fn layout_hint(&self) -> LayoutHint {
        if self.compositional {
            LayoutHint::Scroll {
                left: 0.0, top: self.header_height, right: 0.0, bottom: 0.0,
                unbounded_width: false,
                unbounded_height: true,
            }
        } else {
            LayoutHint::Center
        }
    }

    fn clip_content(&self) -> bool { false }

    fn scroll_offset(&self) -> Point {
        if self.compositional {
            Point::new(0.0, self.scroll_offset)
        } else {
            Point::zero()
        }
    }

    fn set_content_size(&mut self, size: Size) {
        if self.compositional {
            self.actual_content_height = size.height;
        }
    }

    fn manages_own_children(&self) -> bool { self.compositional }
    fn needs_rebuild(&self) -> bool { self.compositional && self.needs_child_rebuild }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        if !self.compositional { return Vec::new(); }

        /// Схлопывает подряд идущие «обычные» колонки в одну распорку —
        /// место под текст, который рисуется мимо дерева виджетов.
        fn push_gap(
            row: &mut crate::widgets::containers::Row,
            px: &mut f32,
            flex: &mut f32,
        ) {
            if *px > 0.0 {
                let spacer = crate::widgets::containers::DecoratedBox::new()
                    .style("width", crate::mss::StyleValue::px(*px));
                row.children.push(Box::new(spacer));
                *px = 0.0;
            }
            if *flex > 0.0 {
                let spacer = crate::widgets::containers::DecoratedBox::new()
                    .style("flex-grow", crate::mss::StyleValue::Number(*flex));
                row.children.push(Box::new(spacer));
                *flex = 0.0;
            }
        }

        let count = self.row_count();
        let (virt_first, virt_last) = self.comp_visible_range();
        let mut column = crate::widgets::containers::Column::new()
            .cross_axis_alignment(CrossAxisAlignment::Stretch);


        let cp = self.cell_padding;
        let rp = self.row_padding;

        if virt_first > 0 {
            let top_h = virt_first as f32 * self.row_height;
            let spacer = crate::widgets::containers::DecoratedBox::new()
                .style("height", crate::mss::StyleValue::px(top_h));
            column.children.push(Box::new(spacer));
        }

        for vis_row in virt_first..virt_last.min(count) {
            let phys_row = self.physical_row(vis_row);
            let fallback_row;
            let row_data: Option<&Vec<String>> = match self.get_physical_row(phys_row) {
                Some(data) => Some(data),
                None => match &self.data {
                    TableDataSource::Virtual { row_builder, .. } => {
                        fallback_row = row_builder(phys_row);
                        Some(&fallback_row)
                    }
                    TableDataSource::Eager(_) => None,
                },
            };
            let mut row = crate::widgets::containers::Row::new()
                .gap(0.0)
                .cross_axis_alignment(CrossAxisAlignment::Center);

            // Виджеты строятся только для колонок с кастомным рендерером.
            // Обычные текстовые ячейки рисуются напрямую в display list
            // (см. `build_display_list`) — при десятках колонок разница
            // принципиальная: два виджета на строку вместо сотен, которые
            // пришлось бы пересобирать и переразмечать на каждый
            // прокрученный ряд.
            let mut pending_px = 0.0f32;
            let mut pending_flex = 0.0f32;

            for (col_idx, col) in self.columns.iter().enumerate() {
                if !self.is_col_visible(col_idx) { continue; }
                let computed_w = self.column_widths.get(col_idx).copied();

                let has_renderer =
                    col.cell_renderer.is_some() || col.cell_renderer_with_row.is_some();
                if !has_renderer {
                    match computed_w {
                        Some(w) => pending_px += w,
                        None => match col.width {
                            ColumnWidth::Flex(flex) => pending_flex += flex,
                            ColumnWidth::Fixed(w) => pending_px += w.max(col.min_width),
                        },
                    }
                    continue;
                }

                push_gap(&mut row, &mut pending_px, &mut pending_flex);

                let cell_text = row_data
                    .and_then(|r| r.get(col_idx))
                    .map(|s| s.as_str())
                    .unwrap_or("");

                let cross_align = match col.align {
                    ColumnAlign::Left => CrossAxisAlignment::Start,
                    ColumnAlign::Center => CrossAxisAlignment::Center,
                    ColumnAlign::Right => CrossAxisAlignment::End,
                };

                let cell_content: Box<dyn Widget> = {
                    let mut wrapper = crate::widgets::containers::Column::new()
                        .cross_axis_alignment(cross_align);
                    if let Some(ref renderer) = col.cell_renderer_with_row {
                        let row_slice: &[String] = row_data.map(|v| v.as_slice()).unwrap_or(&[]);
                        wrapper.children.push(renderer(phys_row, row_slice));
                    } else if let Some(ref renderer) = col.cell_renderer {
                        wrapper.children.push(renderer(phys_row, cell_text));
                    }
                    Box::new(
                        crate::widgets::containers::Padding::symmetric(cp, 4.0)
                            .child(wrapper)
                    )
                };

                let mut cell = crate::widgets::containers::DecoratedBox::new();
                cell.clip = true;
                cell.child = Some(cell_content);
                let cell = match computed_w {
                    Some(w) => cell.style("width", crate::mss::StyleValue::px(w)),
                    None => match col.width {
                        ColumnWidth::Flex(flex) => {
                            cell.style("flex-grow", crate::mss::StyleValue::Number(flex))
                        }
                        ColumnWidth::Fixed(w) => {
                            cell.style("width", crate::mss::StyleValue::px(w.max(col.min_width)))
                        }
                    },
                };
                row.children.push(Box::new(cell));
            }
            push_gap(&mut row, &mut pending_px, &mut pending_flex);

            let row_widget: Box<dyn Widget> = if rp[0] > 0.0 || rp[1] > 0.0 || rp[2] > 0.0 || rp[3] > 0.0 {
                Box::new(
                    crate::widgets::containers::Padding::only(rp[0], rp[1], rp[2], rp[3])
                        .child(row)
                )
            } else {
                Box::new(row)
            };
            let mut row_container = crate::widgets::containers::DecoratedBox::new();
            row_container.child = Some(row_widget);
            // Разделитель строки рисует сама таблица — цветом сетки с
            // `grid-alpha`. Рамка у контейнера строки давала вторую линию
            // поверх, причём чёрную: цвет из inline-стиля до неё не доходил.
            let row_container = row_container
                .style("height", crate::mss::StyleValue::px(self.row_height));
            column.children.push(Box::new(row_container));
        }

        let virt_last_actual = virt_last.min(count);
        if virt_last_actual < count {
            let bottom_h = (count - virt_last_actual) as f32 * self.row_height;
            let spacer = crate::widgets::containers::DecoratedBox::new()
                .style("height", crate::mss::StyleValue::px(bottom_h));
            column.children.push(Box::new(spacer));
        }

        vec![Box::new(column)]
    }

    fn clear_rebuild(&mut self) {
        self.needs_child_rebuild = false;
        let (f, l) = self.comp_visible_range();
        self.comp_visible_first = f;
        self.comp_visible_last = l;
    }

    fn set_row_bounds(&mut self, bounds: Vec<(f32, f32)>) {
        self.row_bounds = bounds;
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "TableView" }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        let old_color = self.mss.color;
        let old_border = self.mss.border_color;
        let old_bg = self.mss.background_color;
        self.mss.apply(style);
        if let Some(w) = style.width() { self.width = Some(w); }
        if let Some(h) = style.height() { self.height = Some(h); }

        if let Some(v) = style.get("header-bg").and_then(|v| v.as_color()) {
            self.header_bg_custom = Some(mss_color_to_core(v));
        }
        if let Some(v) = style.get("header-color").and_then(|v| v.as_color()) {
            self.header_color_custom = Some(mss_color_to_core(v));
        }
        if let Some(v) = style.get("header-font-size").and_then(|v| v.as_px()) {
            self.header_font_size = v;
        }
        if let Some(v) = style.get("header-padding").and_then(|v| v.as_px()) {
            self.header_padding = v;
        }
        if let Some(v) = style.get("row-hover-bg").and_then(|v| v.as_color()) {
            self.row_hover_bg = Some(mss_color_to_core(v));
        }
        if let Some(v) = style.get("row-selected-bg").and_then(|v| v.as_color()) {
            self.row_selected_bg = Some(mss_color_to_core(v));
        }
        if let Some(v) = style.get("row-striped-bg").and_then(|v| v.as_color()) {
            self.row_striped_bg = Some(mss_color_to_core(v));
        }
        if let Some(v) = style.get("row-padding").and_then(|v| v.as_px()) {
            self.row_padding = [v; 4];
        }
        if let Some(v) = style.get("row-padding-left").and_then(|v| v.as_px()) { self.row_padding[0] = v; }
        if let Some(v) = style.get("row-padding-top").and_then(|v| v.as_px()) { self.row_padding[1] = v; }
        if let Some(v) = style.get("row-padding-right").and_then(|v| v.as_px()) { self.row_padding[2] = v; }
        if let Some(v) = style.get("row-padding-bottom").and_then(|v| v.as_px()) { self.row_padding[3] = v; }
        if let Some(v) = style.get("cell-font-size").and_then(|v| v.as_px()) { self.cell_font_size = v; }
        if let Some(v) = style.get("cell-padding").and_then(|v| v.as_px()) { self.cell_padding = v; }
        if let Some(v) = style.get("cell-min-width").and_then(|v| v.as_px()) { self.cell_min_width = v; }
        if let Some(v) = style.get("cell-max-width").and_then(|v| v.as_px()) { self.cell_max_width = v; }
        if let Some(v) = style.get("grid-alpha").and_then(|v| v.as_px()) {
            self.grid_alpha = v.clamp(0.0, 1.0);
        }

        let col_count = self.columns.len();
        self.col_widths = vec![None; col_count];
        self.col_min_widths = vec![None; col_count];
        self.col_max_widths = vec![None; col_count];
        for i in 0..col_count {
            if let Some(v) = style.get(&format!("col-{}-width", i)).and_then(|v| v.as_dimension()) {
                self.col_widths[i] = Some(v);
            }
            if let Some(v) = style.get(&format!("col-{}-min-width", i)).and_then(|v| v.as_dimension()) {
                self.col_min_widths[i] = Some(v);
            }
            if let Some(v) = style.get(&format!("col-{}-max-width", i)).and_then(|v| v.as_dimension()) {
                self.col_max_widths[i] = Some(v);
            }
            if let Some(v) = style.get(&format!("col-{}-align", i)).and_then(|v| v.as_string()) {
                self.columns[i].align = match v {
                    "center" => ColumnAlign::Center,
                    "right" => ColumnAlign::Right,
                    _ => ColumnAlign::Left,
                };
            }
        }

        if self.compositional
            && (!self.mss_applied
                || self.mss.color != old_color
                || self.mss.border_color != old_border
                || self.mss.background_color != old_bg)
        {
            self.needs_child_rebuild = true;
            self.mss_applied = true;
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::ListBox,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(format!("Table with {} rows", self.row_count())),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for TableViewElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        if let Some(w) = style.width() { self.width = Some(w); }
        if let Some(h) = style.height() { self.height = Some(h); }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{TableColumn, TableView};
    use super::TableViewElement;
    use crate::core::types::RectExt;
    use crate::core::Point;
    use crate::input::{Event, MouseButton};
    use crate::testing::TestHarness;
    use std::sync::{Arc, Mutex};

    fn sample_table() -> TableView {
        TableView::new(
            vec![TableColumn::new("A"), TableColumn::new("B")],
            vec![
                vec!["a1".to_string(), "b1".to_string()],
                vec!["a2".to_string(), "b2".to_string()],
            ],
        )
        .row_height(20.0)
        .header_height(20.0)
    }

    /// Строки таблицы: клик по строке 0 приходится на y=30, по строке 1 — на y=50.
    fn click(h: &mut TestHarness, row: usize, ctrl: bool, shift: bool) {
        h.tree.modifiers = crate::input::Modifiers {
            ctrl,
            shift,
            alt: false,
            meta: false,
        };
        let y = 30.0 + row as f32 * 20.0;
        h.send_event(&Event::MouseDown {
            button: MouseButton::Left,
            position: Point::new(50.0, y),
        });
    }

    fn three_rows() -> TableView {
        TableView::new(
            vec![TableColumn::new("A")],
            (0..3).map(|n| vec![format!("строка {n}")]).collect(),
        )
        .row_height(20.0)
        .header_height(20.0)
    }

    #[test]
    fn plain_click_selects_one_row() {
        let seen = Arc::new(Mutex::new(Vec::<Vec<usize>>::new()));
        let sink = seen.clone();
        let table = three_rows().on_selection_change(move |rows| {
            sink.lock().unwrap().push(rows);
        });
        let mut h = TestHarness::new(Box::new(table));
        h.layout(400.0, 200.0);

        click(&mut h, 0, false, false);
        click(&mut h, 2, false, false);
        // Обычный клик заменяет выбор, а не копит его.
        assert_eq!(*seen.lock().unwrap(), vec![vec![0], vec![2]]);
    }

    #[test]
    fn ctrl_click_adds_and_removes_rows() {
        let seen = Arc::new(Mutex::new(Vec::<Vec<usize>>::new()));
        let sink = seen.clone();
        let table = three_rows().on_selection_change(move |rows| {
            sink.lock().unwrap().push(rows);
        });
        let mut h = TestHarness::new(Box::new(table));
        h.layout(400.0, 200.0);

        click(&mut h, 0, false, false);
        click(&mut h, 2, true, false);
        click(&mut h, 0, true, false);
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0], vec![0]);
        assert_eq!(seen[1], vec![0, 2], "Ctrl должен добавлять строку");
        assert_eq!(seen[2], vec![2], "повторный Ctrl снимает выбор со строки");
    }

    #[test]
    fn shift_click_selects_a_range() {
        let seen = Arc::new(Mutex::new(Vec::<Vec<usize>>::new()));
        let sink = seen.clone();
        let table = three_rows().on_selection_change(move |rows| {
            sink.lock().unwrap().push(rows);
        });
        let mut h = TestHarness::new(Box::new(table));
        h.layout(400.0, 200.0);

        click(&mut h, 0, false, false);
        click(&mut h, 2, false, true);
        let seen = seen.lock().unwrap();
        assert_eq!(seen[1], vec![0, 1, 2], "Shift должен выбрать промежуток");
    }

    #[test]
    fn double_click_fires_row_callback() {
        let hits = Arc::new(Mutex::new(Vec::<usize>::new()));
        let sink = hits.clone();
        let table = sample_table().on_row_double_click(move |row| {
            sink.lock().unwrap().push(row);
        });

        let mut h = TestHarness::new(Box::new(table));
        h.layout(400.0, 200.0);
        let pos = Point::new(50.0, 50.0);
        h.send_event(&Event::MouseDown { button: MouseButton::Left, position: pos });
        h.send_event(&Event::DoubleClick { button: MouseButton::Left, position: pos });

        assert_eq!(*hits.lock().unwrap(), vec![1]);
    }

    #[test]
    fn double_click_fires_cell_callback_with_column() {
        let hits = Arc::new(Mutex::new(Vec::<(usize, usize)>::new()));
        let sink = hits.clone();
        let table = sample_table().on_cell_double_click(move |row, col| {
            sink.lock().unwrap().push((row, col));
        });

        let mut h = TestHarness::new(Box::new(table));
        h.layout(400.0, 200.0);
        let pos = Point::new(300.0, 30.0);
        h.send_event(&Event::MouseDown { button: MouseButton::Left, position: pos });
        h.send_event(&Event::DoubleClick { button: MouseButton::Left, position: pos });

        assert_eq!(*hits.lock().unwrap(), vec![(0, 1)]);
    }

    #[test]
    fn double_click_reaches_table_with_custom_cell_widgets() {
        let hits = Arc::new(Mutex::new(Vec::<usize>::new()));
        let sink = hits.clone();
        let columns = vec![
            TableColumn::new("A").cell_renderer(|_, text| {
                Box::new(crate::widgets::Text::new(text)) as Box<dyn crate::widget::Widget>
            }),
            TableColumn::new("B"),
        ];
        let table = TableView::new(
            columns,
            vec![
                vec!["a1".to_string(), "b1".to_string()],
                vec!["a2".to_string(), "b2".to_string()],
            ],
        )
        .row_height(20.0)
        .header_height(20.0)
        .on_row_double_click(move |row| {
            sink.lock().unwrap().push(row);
        });

        let mut h = TestHarness::new(Box::new(table));
        h.layout(400.0, 200.0);
        h.rebuild();
        h.layout(400.0, 200.0);
        let pos = Point::new(50.0, 30.0);
        h.send_event(&Event::MouseDown { button: MouseButton::Left, position: pos });
        h.send_event(&Event::DoubleClick { button: MouseButton::Left, position: pos });

        assert_eq!(*hits.lock().unwrap(), vec![0]);
    }

    #[test]
    fn double_click_on_header_does_not_fire_row_callback() {
        let hits = Arc::new(Mutex::new(Vec::<usize>::new()));
        let sink = hits.clone();
        let table = sample_table().on_row_double_click(move |row| {
            sink.lock().unwrap().push(row);
        });

        let mut h = TestHarness::new(Box::new(table));
        h.layout(400.0, 200.0);
        let pos = Point::new(50.0, 10.0);
        h.send_event(&Event::MouseDown { button: MouseButton::Left, position: pos });
        h.send_event(&Event::DoubleClick { button: MouseButton::Left, position: pos });

        assert!(hits.lock().unwrap().is_empty());
    }
    #[test]
    fn horizontal_wheel_scrolls_wide_table() {
        // Таблица шире области: три фиксированные колонки по 300px в окне 400px.
        let table = TableView::new(
            vec![
                TableColumn::fixed("A", 300.0),
                TableColumn::fixed("B", 300.0),
                TableColumn::fixed("C", 300.0),
            ],
            vec![vec!["a".into(), "b".into(), "c".into()]],
        )
        .row_height(20.0)
        .header_height(20.0);

        let mut h = TestHarness::new(Box::new(table));
        h.layout(400.0, 200.0);

        // Прокрутка вправо (delta_x < 0 сдвигает содержимое влево, offset растёт).
        h.send_event(&Event::MouseWheel {
            delta: 0.0,
            delta_x: -120.0,
            position: Point::new(200.0, 100.0),
        });

        // 900px контента в 400px окне → max_scroll_x = 500; сместились на 120.
        let root = h.root_id;
        let el = h.tree.get_mut(root).unwrap()
            .as_any_mut().unwrap()
            .downcast_ref::<TableViewElement>().unwrap();
        assert!(
            el.scroll_offset_x > 0.0,
            "горизонтальная прокрутка не сработала: {}",
            el.scroll_offset_x
        );
        assert!(el.scroll_offset_x <= el.max_scroll_x() + 0.01);
    }

    /// Таблица с широкой колонкой: текст ячейки заведомо начинается в
    /// известной точке, поэтому по x можно целиться в конкретные символы.
    fn text_table() -> TableView {
        TableView::new(
            vec![TableColumn::fixed("A", 300.0)],
            vec![vec!["Костанай".to_string()]],
        )
        .row_height(20.0)
        .header_height(20.0)
        .text_selection(true)
    }

    fn element(h: &mut TestHarness) -> &mut TableViewElement {
        let root = h.root_id;
        h.tree
            .get_mut(root)
            .unwrap()
            .as_any_mut()
            .unwrap()
            .downcast_mut::<TableViewElement>()
            .unwrap()
    }

    #[test]
    fn cell_cursor_disabled_keeps_frame_off() {
        let table = three_rows().cell_cursor(false);
        let mut h = TestHarness::new(Box::new(table));
        h.layout(400.0, 200.0);

        click(&mut h, 1, false, false);

        let el = element(&mut h);
        assert_eq!(el.cursor_cell, None, "рамка ячейки должна быть выключена");
        assert_eq!(el.selected_rows, vec![1], "строка всё равно выделяется");
    }

    #[test]
    fn cell_cursor_enabled_by_default() {
        let mut h = TestHarness::new(Box::new(three_rows()));
        h.layout(400.0, 200.0);

        click(&mut h, 1, false, false);

        assert_eq!(element(&mut h).cursor_cell, Some((1, 0)));
    }

    #[test]
    fn drag_selects_cell_text() {
        let mut h = TestHarness::new(Box::new(text_table()));
        h.layout(400.0, 200.0);

        // Текст начинается с левого края ячейки плюс cell-padding; тянем от
        // самого начала строки вправо — выделение не должно быть пустым.
        let y = 30.0;
        h.send_event(&Event::MouseDown {
            button: MouseButton::Left,
            position: Point::new(14.0, y),
        });
        h.send_event(&Event::MouseMove(Point::new(200.0, y)));
        h.send_event(&Event::MouseUp {
            button: MouseButton::Left,
            position: Point::new(200.0, y),
        });

        let el = element(&mut h);
        let sel = el.text_sel.expect("выделение текста не началось");
        assert_eq!((sel.row, sel.col), (0, 0));
        assert!(!sel.is_empty(), "протягивание должно выделить фрагмент");
        assert!(!el.text_selecting, "после отпускания кнопки протягивание закончено");
        let text = el.selected_cell_text().expect("нет выделенного текста");
        assert!(
            "Костанай".starts_with(&text),
            "выделен фрагмент с начала строки, получено {text:?}"
        );
    }

    #[test]
    fn text_selection_off_keeps_cells_intact() {
        let mut h = TestHarness::new(Box::new(three_rows()));
        h.layout(400.0, 200.0);

        let y = 30.0;
        h.send_event(&Event::MouseDown {
            button: MouseButton::Left,
            position: Point::new(14.0, y),
        });
        h.send_event(&Event::MouseMove(Point::new(200.0, y)));

        assert!(element(&mut h).text_sel.is_none());
    }

    #[test]
    fn right_click_opens_and_closes_context_menu() {
        let mut h = TestHarness::new(Box::new(text_table()));
        h.layout(400.0, 200.0);

        h.send_event(&Event::MouseDown {
            button: MouseButton::Right,
            position: Point::new(60.0, 30.0),
        });
        {
            let el = element(&mut h);
            let menu = el.context_menu.as_ref().expect("меню не открылось");
            assert_eq!(menu.row, 0);
        }

        // Щелчок по первому пункту («Копировать») закрывает меню.
        let item = element(&mut h)
            .context_menu_item_rect(0)
            .expect("нет пункта меню");
        let center = Point::new(
            item.x() + item.size.width / 2.0,
            item.y() + item.size.height / 2.0,
        );
        h.send_event(&Event::MouseDown { button: MouseButton::Left, position: center });
        assert!(element(&mut h).context_menu.is_none(), "меню должно закрыться");
    }

    #[test]
    fn right_click_ignored_without_text_selection() {
        let mut h = TestHarness::new(Box::new(three_rows()));
        h.layout(400.0, 200.0);

        h.send_event(&Event::MouseDown {
            button: MouseButton::Right,
            position: Point::new(60.0, 30.0),
        });

        assert!(element(&mut h).context_menu.is_none());
    }

    #[test]
    fn selected_rows_text_joins_columns_with_tabs() {
        let mut h = TestHarness::new(Box::new(sample_table().text_selection(true)));
        h.layout(400.0, 200.0);

        click(&mut h, 0, false, false);
        click(&mut h, 1, true, false);

        let text = element(&mut h)
            .selected_rows_text()
            .expect("нет текста выделенных строк");
        assert_eq!(text, "a1\tb1\na2\tb2");
    }
}
