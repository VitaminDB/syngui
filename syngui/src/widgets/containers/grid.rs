use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{ChildHit, DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use super::IntoWidget;
use std::any::Any;

pub struct Grid {
    pub children: Vec<Box<dyn Widget>>,
    pub columns: usize,
    pub row_gap: f32,
    pub col_gap: f32,
    pub clip: bool,
    pub masonry: bool,
}

impl Grid {
    pub fn new(columns: usize) -> Self {
        Self {
            children: Vec::new(),
            columns: columns.max(1),
            row_gap: 0.0,
            col_gap: 0.0,
            clip: false,
            masonry: false,
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.children.push(child.into_widget());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Box<dyn Widget>>) -> Self {
        self.children.extend(children);
        self
    }

    pub fn row_gap(mut self, gap: f32) -> Self {
        self.row_gap = gap;
        self
    }

    pub fn col_gap(mut self, gap: f32) -> Self {
        self.col_gap = gap;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.row_gap = gap;
        self.col_gap = gap;
        self
    }

    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    pub fn masonry(mut self, masonry: bool) -> Self {
        self.masonry = masonry;
        self
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self::new(2)
    }
}

impl Widget for Grid {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(GridElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            columns: self.columns,
            row_gap: self.row_gap,
            col_gap: self.col_gap,
            clip: self.clip,
            masonry: self.masonry,
            child_ids: Vec::new(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            layout_cache: None,
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

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        for child in &self.children {
            let child_element = child.create_element();
            let child_id = tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }
}

pub struct GridElement {
    id: ElementId,
    bounds: Rect,
    columns: usize,
    row_gap: f32,
    col_gap: f32,
    clip: bool,
    masonry: bool,
    child_ids: Vec<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    pub(crate) layout_cache: Option<GridLayoutCache>,
}

#[derive(Debug, Clone)]
pub(crate) struct GridLayoutCache {
    pub cols: usize,
    pub col_width: f32,
    pub col_gap: f32,
    pub row_gap: f32,
    pub masonry: bool,
    pub row_y_offsets: Vec<f32>,
    pub columns_cells: Vec<Vec<MasonryCell>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MasonryCell {
    pub y_start: f32,
    pub y_end: f32,
    pub child_idx: usize,
}

impl Element for GridElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(grid) = widget.as_any().downcast_ref::<Grid>() {
            self.columns = grid.columns;
            self.row_gap = grid.row_gap;
            self.col_gap = grid.col_gap;
            self.clip = grid.clip;
            self.masonry = grid.masonry;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = if constraints.max_width.is_finite() { constraints.max_width } else { constraints.min_width.max(40.0) };
        let height = self.mss.height
            .map(|d| d.resolve(constraints.max_height))
            .unwrap_or(constraints.min_height);

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Grid {
            columns: self.columns,
            row_gap: self.row_gap,
            col_gap: self.col_gap,
            masonry: self.masonry,
        }
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn passthrough_hit_test(&self) -> bool { true }

    fn children(&self) -> &[ElementId] {
        &self.child_ids
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags |= flags;
    }

    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags.remove(flags);
    }

    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty_flags.contains(flags)
    }

    fn id(&self) -> ElementId {
        self.id
    }

    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str { "Grid" }

    fn clip_content(&self) -> bool { self.clip }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(gap) = self.mss.gap {
            self.row_gap = gap;
            self.col_gap = gap;
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }

    fn visible_child_indices(&self, cull: Rect, out: &mut Vec<usize>) -> bool {
        let cache = match &self.layout_cache {
            Some(c) => c,
            None => return false,
        };
        let min_x = cull.origin.x - self.bounds.origin.x;
        let max_x = min_x + cull.size.width;
        let min_y = cull.origin.y - self.bounds.origin.y;
        let max_y = min_y + cull.size.height;
        if max_x <= 0.0 || max_y <= 0.0 {
            return true;
        }

        const OVERSCAN: usize = 1;

        let col_step = cache.col_width + cache.col_gap;
        let col_start = if col_step > 0.0 { (min_x / col_step).floor().max(0.0) as usize } else { 0 };
        let col_end_excl = if col_step > 0.0 {
            (max_x / col_step).ceil().max(0.0) as usize
        } else {
            cache.cols
        };
        let col_start = col_start.saturating_sub(OVERSCAN).min(cache.cols);
        let col_end_excl = col_end_excl.saturating_add(OVERSCAN).min(cache.cols);
        if col_start >= col_end_excl {
            return true;
        }

        if cache.masonry {
            self.masonry_visible(cache, col_start, col_end_excl, min_y, max_y, OVERSCAN, out);
        } else {
            self.standard_visible(cache, col_start, col_end_excl, min_y, max_y, OVERSCAN, out);
        }
        true
    }

    fn child_at_position(&self, pos: Point) -> ChildHit {
        // ВАЖНО: Grid — passthrough-контейнер, поэтому `ChildHit::None`
        // приводит к тому, что диспетчер событий (`event.rs`) обрубает
        // путь и НЕ пробует детей поштучно. Геометрия строк здесь может
        // быть приблизительной (виртуализация скролла оценивает высоты
        // off-screen рядов в `measure_grid`), а у детей разная высота —
        // значит по оценённым `row_y_offsets` мы можем ошибочно решить
        // «мимо» и уронить клик по реальной карточке. Поэтому на любой
        // промах возвращаем `Unknown` (диспетчер сделает надёжный
        // fallback-перебор детей по их bounds), а быстрый `Index` — только
        // при уверенном попадании.
        let cache = match &self.layout_cache {
            Some(c) => c,
            None => return ChildHit::Unknown,
        };
        if !self.bounds.contains(pos) {
            return ChildHit::Unknown;
        }
        let local_x = pos.x - self.bounds.origin.x;
        let local_y = pos.y - self.bounds.origin.y;

        let col_step = cache.col_width + cache.col_gap;
        if col_step <= 0.0 {
            return ChildHit::Unknown;
        }
        let col = (local_x / col_step).floor() as i32;
        if col < 0 || col as usize >= cache.cols {
            return ChildHit::Unknown;
        }
        let col_local = local_x - col as f32 * col_step;
        if col_local > cache.col_width {
            return ChildHit::Unknown;
        }
        let col = col as usize;

        if cache.masonry {
            self.masonry_hit(cache, col, local_y)
        } else {
            self.standard_hit(cache, col, local_y)
        }
    }
}

impl GridElement {
    fn standard_visible(
        &self,
        cache: &GridLayoutCache,
        col_start: usize,
        col_end_excl: usize,
        min_y: f32,
        max_y: f32,
        overscan: usize,
        out: &mut Vec<usize>,
    ) {
        let offsets = &cache.row_y_offsets;
        if offsets.len() < 2 {
            return;
        }
        let rows = offsets.len() - 1;
        let row_start = offsets.partition_point(|&y| y <= min_y).saturating_sub(1);
        let row_end_excl = offsets[1..].partition_point(|&y| y - cache.row_gap < max_y).min(rows);
        let row_start = row_start.saturating_sub(overscan).min(rows);
        let row_end_excl = row_end_excl.saturating_add(overscan).min(rows);
        if row_start >= row_end_excl {
            return;
        }
        out.reserve((row_end_excl - row_start) * (col_end_excl - col_start));
        for row in row_start..row_end_excl {
            let base = row * cache.cols;
            for col in col_start..col_end_excl {
                out.push(base + col);
            }
        }
    }

    fn standard_hit(&self, cache: &GridLayoutCache, col: usize, local_y: f32) -> ChildHit {
        // `Unknown` (не `None`) на промахах — см. коммент в `child_at_position`:
        // row_y_offsets могут быть оценочными, а `None` у passthrough-Grid
        // роняет клик вместо fallback-перебора.
        let offsets = &cache.row_y_offsets;
        if offsets.len() < 2 {
            return ChildHit::Unknown;
        }
        let rows = offsets.len() - 1;
        let row = offsets.partition_point(|&y| y <= local_y).saturating_sub(1);
        if row >= rows {
            return ChildHit::Unknown;
        }
        let row_bottom = offsets[row + 1] - cache.row_gap;
        if local_y > row_bottom {
            return ChildHit::Unknown;
        }
        ChildHit::Index(row * cache.cols + col)
    }

    fn masonry_visible(
        &self,
        cache: &GridLayoutCache,
        col_start: usize,
        col_end_excl: usize,
        min_y: f32,
        max_y: f32,
        overscan: usize,
        out: &mut Vec<usize>,
    ) {
        for col in col_start..col_end_excl {
            let cells = match cache.columns_cells.get(col) {
                Some(v) => v,
                None => continue,
            };
            let first = cells.partition_point(|c| c.y_end <= min_y);
            let first = first.saturating_sub(overscan);
            let mut trailing = overscan;
            for cell in &cells[first..] {
                if cell.y_start >= max_y {
                    if trailing == 0 { break; }
                    trailing -= 1;
                }
                out.push(cell.child_idx);
            }
        }
    }

    fn masonry_hit(&self, cache: &GridLayoutCache, col: usize, local_y: f32) -> ChildHit {
        // `Unknown` на промахах — см. коммент в `child_at_position`.
        let cells = match cache.columns_cells.get(col) {
            Some(v) => v,
            None => return ChildHit::Unknown,
        };
        let idx = cells.partition_point(|c| c.y_start <= local_y).saturating_sub(1);
        if idx >= cells.len() {
            return ChildHit::Unknown;
        }
        let cell = cells[idx];
        if local_y >= cell.y_start && local_y < cell.y_end {
            return ChildHit::Index(cell.child_idx);
        }
        ChildHit::Unknown
    }
}

impl StyledElement for GridElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
