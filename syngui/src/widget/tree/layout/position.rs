use crate::core::{Point, Size};
use super::{ElementId, ElementTree};

macro_rules! layout_log {
    ($self:expr, $($arg:tt)*) => {
        if $self.layout_log_enabled {
            $self.log(format!($($arg)*));
        }
    };
}

impl ElementTree {

    pub(crate) fn position_recursive(&mut self, id: ElementId, parent_pos: Point) {
        let Some(idx) = self.elements.resolve(id) else { return; };
        let own_size = self.cache_get_by_idx(idx).map(|c| c.size).unwrap_or(Size::zero());

        let parent_pos = self.snap_point(parent_pos);

        layout_log!(self,"[POSITION] Element {} at ({:.1}, {:.1})",
            id.0, parent_pos.x, parent_pos.y);
        self.indent_level += 1;

        let (children, hint) = {
            let node = self.elements.get_by_idx(idx);
            (
                node.map(|n| n.children.clone()).unwrap_or_default(),
                node.map(|n| n.hint_cache.clone()).unwrap_or_default(),
            )
        };

        if let Some(node) = self.elements.get_mut_by_idx(idx) {
            node.element.set_position(parent_pos);
            if !matches!(hint, crate::widget::LayoutHint::Scroll { .. } | crate::widget::LayoutHint::AnimatedSize | crate::widget::LayoutHint::Container { .. } | crate::widget::LayoutHint::Portal { .. } | crate::widget::LayoutHint::FloatingWindow { .. } | crate::widget::LayoutHint::Tooltip { .. }) {
                node.element.set_content_size(own_size);
            }
        }

        match hint {
            crate::widget::LayoutHint::Column { gap, cross_align, main_align, padding_left, padding_top, padding_right, padding_bottom, expand: _ } => {
                let padded_pos = Point::new(parent_pos.x + padding_left, parent_pos.y + padding_top);
                let padded_size = Size::new(
                    (own_size.width - padding_left - padding_right).max(0.0),
                    (own_size.height - padding_top - padding_bottom).max(0.0),
                );
                self.position_column_children(&children, padded_pos, padded_size, gap, cross_align, main_align);
            }
            crate::widget::LayoutHint::Row { gap, offset_x, cross_align, main_align, padding_left, padding_top, padding_right, padding_bottom } => {
                let padded_pos = Point::new(parent_pos.x + padding_left, parent_pos.y + padding_top);
                let padded_size = Size::new(
                    (own_size.width - padding_left - padding_right).max(0.0),
                    (own_size.height - padding_top - padding_bottom).max(0.0),
                );
                self.position_row_children(&children, padded_pos, padded_size, gap, offset_x, cross_align, main_align);
            }
            crate::widget::LayoutHint::TabBar { gap, .. } => {
                self.position_row_children(
                    &children, parent_pos, own_size, gap, 0.0,
                    crate::layout::CrossAxisAlignment::Stretch,
                    crate::layout::MainAxisAlignment::Start,
                );
            }
            crate::widget::LayoutHint::Padding { left, top, .. } => {
                self.position_padding_children(&children, parent_pos, left, top);
            }
            crate::widget::LayoutHint::Stack { .. } => {
                self.position_stack_children(&children, parent_pos);
            }
            crate::widget::LayoutHint::Center => {
                self.position_center_children(&children, parent_pos, own_size);
            }
            crate::widget::LayoutHint::Grid { columns, row_gap, col_gap, masonry } => {
                self.position_grid_children(id, &children, parent_pos, own_size, columns, row_gap, col_gap, masonry);
            }
            crate::widget::LayoutHint::Scroll { left, top, .. } => {
                self.position_padding_children(&children, parent_pos, left, top);
                let mut gc_bounds = Vec::new();
                for &child_id in &children {
                    if let Some(child_node) = self.elements.get(&child_id) {
                        let gc_ids: Vec<_> = child_node.children.clone();
                        for gc_id in gc_ids {
                            if let Some(gc_node) = self.elements.get(&gc_id) {
                                let b = gc_node.element.bounds();
                                gc_bounds.push((b.origin.y, b.size.height));
                            }
                        }
                    }
                }
                if !gc_bounds.is_empty() {
                    if let Some(node) = self.elements.get_mut(&id) {
                        node.element.set_row_bounds(gc_bounds);
                    }
                }
            }
            crate::widget::LayoutHint::HorizontalPages => {
                let page_width = own_size.width;
                let page_height = own_size.height;
                for (i, &child_id) in children.iter().enumerate() {
                    let child_size = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());
                    let x = parent_pos.x + i as f32 * page_width + (page_width - child_size.width) / 2.0;
                    let y = parent_pos.y + (page_height - child_size.height) / 2.0;
                    self.position_recursive(child_id, Point::new(x, y));
                }
            }
            crate::widget::LayoutHint::Split { horizontal, ratio, divider } => {
                if children.len() >= 2 {
                    if horizontal {
                        let avail = (own_size.width - divider).max(0.0);
                        let first_w = avail * ratio;
                        self.position_recursive(children[0], parent_pos);
                        self.position_recursive(children[1], Point::new(parent_pos.x + first_w + divider, parent_pos.y));
                    } else {
                        let avail = (own_size.height - divider).max(0.0);
                        let first_h = avail * ratio;
                        self.position_recursive(children[0], parent_pos);
                        self.position_recursive(children[1], Point::new(parent_pos.x, parent_pos.y + first_h + divider));
                    }
                }
            }
            crate::widget::LayoutHint::AnimatedSize => {
                self.position_stack_children(&children, parent_pos);
            }
            crate::widget::LayoutHint::Container { left, top, .. } => {
                self.position_padding_children(&children, parent_pos, left, top);
            }
            crate::widget::LayoutHint::Loose => {
                self.position_padding_children(&children, parent_pos, 0.0, 0.0);
            }
            crate::widget::LayoutHint::Portal { anchor, margin_a, margin_b } => {
                let viewport = self.viewport_size;
                let mut total_height = 0.0f32;
                let mut max_width = 0.0f32;
                for &child_id in &children {
                    if let Some(cache) = self.cache_get(&child_id) {
                        max_width = max_width.max(cache.size.width);
                        total_height += cache.size.height;
                    }
                }
                let (x, y) = match anchor {
                    1 => (
                        viewport.width - max_width - margin_b,
                        viewport.height - total_height - margin_a,
                    ),
                    2 => (
                        viewport.width - max_width - margin_b,
                        margin_a,
                    ),
                    3 => (
                        margin_b,
                        viewport.height - total_height - margin_a,
                    ),
                    _ => (
                        (viewport.width - max_width) / 2.0,
                        (viewport.height - total_height) / 2.0,
                    ),
                };
                let mut cy = y;
                for &child_id in &children {
                    let child_size = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());
                    self.position_recursive(child_id, Point::new(x, cy));
                    cy += child_size.height;
                }
            }
            crate::widget::LayoutHint::FloatingWindow { x, y } => {
                let mut cy = y;
                for &child_id in &children {
                    let child_size = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());
                    self.position_recursive(child_id, Point::new(x, cy));
                    cy += child_size.height;
                }
            }
            crate::widget::LayoutHint::Flex { col_gap, row_gap, justify, align_items } => {
                self.position_flex_children(&children, parent_pos, own_size.width, col_gap, row_gap, &justify, &align_items);
            }
            crate::widget::LayoutHint::Positioned { x, y } => {
                let child_pos = Point::new(parent_pos.x + x, parent_pos.y + y);
                for &child_id in &children {
                    self.position_recursive(child_id, child_pos);
                }
            }
            crate::widget::LayoutHint::PanZoom => {
                self.position_stack_children(&children, parent_pos);
            }
            crate::widget::LayoutHint::Tooltip { position, gap, padding_l, padding_t, padding_r, padding_b } => {
                if let Some(&target_id) = children.first() {
                    self.position_recursive(target_id, parent_pos);
                }
                let active_count = self.elements.get(&id)
                    .map_or(usize::MAX, |n| n.element.active_child_count());
                if children.len() > 1 && active_count > 1 {
                    let target_size = self.cache_get(children.first().unwrap())
                        .map(|c| c.size).unwrap_or(Size::zero());
                    let mut content_h = 0.0f32;
                    let mut content_w = 0.0f32;
                    for &child_id in &children[1..] {
                        let cs = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());
                        content_w = content_w.max(cs.width);
                        content_h += cs.height;
                    }
                    let bg_w = content_w + padding_l + padding_r;
                    let bg_h = content_h + padding_t + padding_b;
                    let (bg_x, bg_y) = match position {
                        1 => (
                            parent_pos.x,
                            parent_pos.y - bg_h - gap,
                        ),
                        2 => (
                            parent_pos.x - bg_w - gap,
                            parent_pos.y + (target_size.height - bg_h) / 2.0,
                        ),
                        3 => (
                            parent_pos.x + target_size.width + gap,
                            parent_pos.y + (target_size.height - bg_h) / 2.0,
                        ),
                        _ => (
                            parent_pos.x,
                            parent_pos.y + target_size.height + gap,
                        ),
                    };
                    let x = bg_x + padding_l;
                    let mut cy = bg_y + padding_t;
                    for &child_id in &children[1..] {
                        let cs = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());
                        self.position_recursive(child_id, Point::new(x, cy));
                        cy += cs.height;
                    }
                }
            }
        }

        self.indent_level -= 1;
    }

    fn position_column_children(&mut self, children: &[ElementId], parent_pos: Point, parent_size: Size, gap: f32, cross_align: crate::layout::CrossAxisAlignment, main_align: crate::layout::MainAxisAlignment) {
        let mut total_height = 0.0f32;
        // Gap начисляется только между детьми ненулевой высоты: скрытые
        // попапы/диалоги меряются в 0 и не должны раздвигать соседей.
        let mut participants = 0usize;
        let mut child_infos: Vec<(ElementId, Size, crate::core::EdgeInsets)> = Vec::new();
        for &child_id in children {
            let child_size = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());
            let m = self.elements.get(&child_id).map(|n| n.effective_margin()).unwrap_or_default();
            let extent = child_size.height + m.top + m.bottom;
            total_height += extent;
            if extent > 0.0 {
                participants += 1;
            }
            child_infos.push((child_id, child_size, m));
        }
        total_height += gap * participants.saturating_sub(1) as f32;

        let remaining = (parent_size.height - total_height).max(0.0);

        let (mut y, gap_extra) = match main_align {
            crate::layout::MainAxisAlignment::Start => (parent_pos.y, 0.0),
            crate::layout::MainAxisAlignment::Center => (parent_pos.y + remaining / 2.0, 0.0),
            crate::layout::MainAxisAlignment::End => (parent_pos.y + remaining, 0.0),
            crate::layout::MainAxisAlignment::SpaceBetween => {
                let extra = if participants > 1 {
                    remaining / (participants - 1) as f32
                } else { 0.0 };
                (parent_pos.y, extra)
            }
            crate::layout::MainAxisAlignment::SpaceEvenly => {
                let extra = remaining / (participants + 1) as f32;
                (parent_pos.y + extra, extra)
            }
            crate::layout::MainAxisAlignment::SpaceAround => {
                let extra = remaining / participants.max(1) as f32;
                (parent_pos.y + extra / 2.0, extra)
            }
        };

        let content_width = parent_size.width;

        for (child_id, child_size, m) in &child_infos {
            let x = match cross_align {
                crate::layout::CrossAxisAlignment::Start | crate::layout::CrossAxisAlignment::Stretch | crate::layout::CrossAxisAlignment::Baseline => parent_pos.x + m.left,
                crate::layout::CrossAxisAlignment::Center => parent_pos.x + (content_width - child_size.width) / 2.0,
                crate::layout::CrossAxisAlignment::End => parent_pos.x + content_width - child_size.width - m.right,
            };
            self.position_recursive(*child_id, Point::new(x, y + m.top));
            let extent = child_size.height + m.top + m.bottom;
            y += extent;
            if extent > 0.0 {
                y += gap + gap_extra;
            }
        }
    }

    fn position_row_children(&mut self, children: &[ElementId], parent_pos: Point, parent_size: Size, gap: f32, offset_x: f32, cross_align: crate::layout::CrossAxisAlignment, main_align: crate::layout::MainAxisAlignment) {
        let mut total_width = offset_x;
        // Gap начисляется только между детьми ненулевой ширины: скрытые
        // попапы/оверлеи меряются в 0 и не должны раздвигать соседей.
        let mut participants = 0usize;
        let mut child_infos: Vec<(ElementId, Size, crate::core::EdgeInsets)> = Vec::new();
        for &child_id in children {
            let child_size = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());
            let m = self.elements.get(&child_id).map(|n| n.effective_margin()).unwrap_or_default();
            let extent = child_size.width + m.left + m.right;
            total_width += extent;
            if extent > 0.0 {
                participants += 1;
            }
            child_infos.push((child_id, child_size, m));
        }
        total_width += gap * participants.saturating_sub(1) as f32;

        let remaining = (parent_size.width - total_width).max(0.0);
        let (start_offset, effective_gap) = match main_align {
            crate::layout::MainAxisAlignment::Start => (0.0, gap),
            crate::layout::MainAxisAlignment::End => (remaining, gap),
            crate::layout::MainAxisAlignment::Center => (remaining / 2.0, gap),
            crate::layout::MainAxisAlignment::SpaceBetween => {
                if participants > 1 {
                    (0.0, gap + remaining / (participants - 1) as f32)
                } else {
                    (remaining / 2.0, gap)
                }
            }
            crate::layout::MainAxisAlignment::SpaceAround => {
                let n = participants.max(1) as f32;
                let space = remaining / n;
                (space / 2.0, gap + space)
            }
            crate::layout::MainAxisAlignment::SpaceEvenly => {
                let n = participants as f32 + 1.0;
                let space = remaining / n;
                (space, gap + space)
            }
        };

        let mut x = parent_pos.x + offset_x + start_offset;
        for (child_id, child_size, m) in &child_infos {
            let y = match cross_align {
                crate::layout::CrossAxisAlignment::Start => parent_pos.y + m.top,
                crate::layout::CrossAxisAlignment::Center => parent_pos.y + (parent_size.height - child_size.height) / 2.0 + m.top,
                crate::layout::CrossAxisAlignment::End => parent_pos.y + parent_size.height - child_size.height - m.bottom,
                crate::layout::CrossAxisAlignment::Stretch | crate::layout::CrossAxisAlignment::Baseline => parent_pos.y + m.top,
            };
            self.position_recursive(*child_id, Point::new(x + m.left, y));
            let extent = child_size.width + m.left + m.right;
            x += extent;
            if extent > 0.0 {
                x += effective_gap;
            }
        }
    }

    fn position_padding_children(&mut self, children: &[ElementId], parent_pos: Point, left: f32, top: f32) {
        for &child_id in children {
            self.position_recursive(child_id, Point::new(parent_pos.x + left, parent_pos.y + top));
        }
    }

    fn position_stack_children(&mut self, children: &[ElementId], parent_pos: Point) {
        for &child_id in children {
            self.position_recursive(child_id, parent_pos);
        }
    }

    fn position_center_children(&mut self, children: &[ElementId], parent_pos: Point, parent_size: Size) {
        for &child_id in children {
            let child_size = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());
            let x = parent_pos.x + (parent_size.width - child_size.width) / 2.0;
            let y = parent_pos.y + (parent_size.height - child_size.height) / 2.0;
            self.position_recursive(child_id, Point::new(x, y));
        }
    }

    fn position_flex_children(&mut self, children: &[ElementId], parent_pos: Point, max_width: f32, col_gap: f32, row_gap: f32, justify: &crate::layout::MainAxisAlignment, align_items: &crate::layout::CrossAxisAlignment) {
        struct Line {
            start: usize,
            end: usize,
            width: f32,
            height: f32,
        }

        let mut lines: Vec<Line> = Vec::new();
        let mut line_start = 0;
        let mut line_w = 0.0f32;
        let mut line_h = 0.0f32;
        let mut line_count = 0usize;

        for (i, &child_id) in children.iter().enumerate() {
            let size = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());
            let needed = if line_count > 0 { col_gap + size.width } else { size.width };
            if line_count > 0 && line_w + needed > max_width {
                lines.push(Line { start: line_start, end: i, width: line_w, height: line_h });
                line_start = i;
                line_w = size.width;
                line_h = size.height;
                line_count = 1;
            } else {
                line_w += needed;
                line_h = line_h.max(size.height);
                line_count += 1;
            }
        }
        if line_count > 0 {
            lines.push(Line { start: line_start, end: children.len(), width: line_w, height: line_h });
        }

        let mut y = parent_pos.y;
        for line in &lines {
            let item_count = line.end - line.start;
            let free_space = (max_width - line.width).max(0.0);

            let (mut x, extra_gap) = match justify {
                crate::layout::MainAxisAlignment::Start => (parent_pos.x, 0.0),
                crate::layout::MainAxisAlignment::End => (parent_pos.x + free_space, 0.0),
                crate::layout::MainAxisAlignment::Center => (parent_pos.x + free_space / 2.0, 0.0),
                crate::layout::MainAxisAlignment::SpaceBetween => {
                    if item_count > 1 {
                        (parent_pos.x, free_space / (item_count - 1) as f32)
                    } else {
                        (parent_pos.x, 0.0)
                    }
                }
                crate::layout::MainAxisAlignment::SpaceAround => {
                    let gap_each = free_space / item_count as f32;
                    (parent_pos.x + gap_each / 2.0, gap_each)
                }
                crate::layout::MainAxisAlignment::SpaceEvenly => {
                    let gap_each = free_space / (item_count + 1) as f32;
                    (parent_pos.x + gap_each, gap_each)
                }
            };

            for i in line.start..line.end {
                let child_id = children[i];
                let size = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());

                let child_y = match align_items {
                    crate::layout::CrossAxisAlignment::Start | crate::layout::CrossAxisAlignment::Baseline => y,
                    crate::layout::CrossAxisAlignment::End => y + line.height - size.height,
                    crate::layout::CrossAxisAlignment::Center => y + (line.height - size.height) / 2.0,
                    crate::layout::CrossAxisAlignment::Stretch => y,
                };

                self.position_recursive(child_id, Point::new(x, child_y));
                x += size.width + col_gap + extra_gap;
            }
            y += line.height + row_gap;
        }
    }

    fn position_grid_children(&mut self, id: ElementId, children: &[ElementId], parent_pos: Point, grid_size: Size, columns: usize, row_gap: f32, col_gap: f32, masonry: bool) {
        use crate::widgets::containers::grid::{GridElement, GridLayoutCache, MasonryCell};

        let cols = columns.max(1);
        let total_col_gap = if cols > 1 { col_gap * (cols - 1) as f32 } else { 0.0 };
        let col_width = (grid_size.width - total_col_gap) / cols as f32;

        let (row_y_offsets, columns_cells) = if masonry {
            let mut col_y = vec![0.0f32; cols];
            let mut columns_cells: Vec<Vec<MasonryCell>> = vec![Vec::new(); cols];
            for (i, &child_id) in children.iter().enumerate() {
                let col = i % cols;
                let y_start = col_y[col];
                let x = parent_pos.x + col as f32 * (col_width + col_gap);
                let y = parent_pos.y + y_start;
                self.position_recursive(child_id, Point::new(x, y));
                let child_h = self.cache_get(&child_id).map(|c| c.size.height).unwrap_or(0.0);
                let y_end = y_start + child_h;
                columns_cells[col].push(MasonryCell { y_start, y_end, child_idx: i });
                col_y[col] = y_end + row_gap;
            }
            (Vec::new(), columns_cells)
        } else {
            let mut row_heights: Vec<f32> = Vec::new();
            for (i, &child_id) in children.iter().enumerate() {
                let child_size = self.cache_get(&child_id).map(|c| c.size).unwrap_or(Size::zero());
                let row = i / cols;
                if row >= row_heights.len() {
                    row_heights.push(child_size.height);
                } else {
                    row_heights[row] = row_heights[row].max(child_size.height);
                }
            }

            let mut row_y_offsets: Vec<f32> = Vec::with_capacity(row_heights.len() + 1);
            let mut y = 0.0;
            for h in &row_heights {
                row_y_offsets.push(y);
                y += h + row_gap;
            }
            row_y_offsets.push(y);

            for (i, &child_id) in children.iter().enumerate() {
                let row = i / cols;
                let col = i % cols;
                let x = parent_pos.x + col as f32 * (col_width + col_gap);
                let cy = parent_pos.y + row_y_offsets.get(row).copied().unwrap_or(0.0);
                self.position_recursive(child_id, Point::new(x, cy));
            }
            (row_y_offsets, Vec::new())
        };

        if let Some(node) = self.elements.get_mut(&id) {
            if let Some(grid) = node.element.as_any_mut().and_then(|a| a.downcast_mut::<GridElement>()) {
                grid.layout_cache = Some(GridLayoutCache {
                    cols,
                    col_width,
                    col_gap,
                    row_gap,
                    masonry,
                    row_y_offsets,
                    columns_cells,
                });
            }
        }
    }
}
