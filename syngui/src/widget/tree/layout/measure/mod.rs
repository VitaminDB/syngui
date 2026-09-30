mod helpers;

use super::{clamp_finite, clamp_finite_explicit};
use super::{ElementId, ElementTree};
use crate::core::Size;
use crate::layout::Constraints;
use crate::tree::LayoutCache;
use crate::widget::{DirtyFlags, LayoutHint};

macro_rules! layout_log {
    ($self:expr, $($arg:tt)*) => {
        if $self.layout_log_enabled {
            $self.log(format!($($arg)*));
        }
    };
}

impl ElementTree {
    pub fn layout(&mut self, root_id: ElementId, constraints: Constraints) -> Size {
        let size = self.layout_pass(root_id, constraints);
        // Позиции известны — отложенные прокрутки к именованным элементам.
        self.process_scroll_requests();
        size
    }

    fn layout_pass(&mut self, root_id: ElementId, constraints: Constraints) -> Size {
        if constraints.max_width.is_finite() && constraints.max_height.is_finite() {
            self.viewport_size = Size::new(constraints.max_width, constraints.max_height);
        }

        let constraints_hash = constraints.hash_key();

        let mut needs_layout = false;
        let mut dirty_ids: Vec<ElementId> = Vec::new();

        for (idx, node) in self.elements.iter_idx() {
            if node.element.is_dirty(DirtyFlags::LAYOUT) {
                dirty_ids.push(node.id);
                needs_layout = true;
            } else {
                let current_visible = node.element.is_visible();
                if let Some(cache) = self.layout_cache.get(idx as usize).copied() {
                    if !cache.is_empty_slot() && cache.visible != current_visible {
                        dirty_ids.push(node.id);
                        needs_layout = true;
                    }
                }
            }
        }

        if !needs_layout && constraints_hash == self.last_constraints_hash {
            if self.root_offset != self.last_root_offset {
                // Сдвинулся только корень (safe area, keyboard_pan): размеры
                // прежние, достаточно расставить позиции заново.
                self.last_root_offset = self.root_offset;
                self.position_recursive(root_id, self.root_offset);
            }
            self.sync_overlay_stack();
            return self
                .elements
                .resolve(root_id)
                .and_then(|i| self.layout_cache.get(i as usize).copied())
                .filter(|c| !c.is_empty_slot())
                .map(|c| c.size)
                .unwrap_or(Size::zero());
        }

        if !dirty_ids.is_empty() {
            let mut visited: std::collections::HashSet<ElementId> =
                std::collections::HashSet::with_capacity(dirty_ids.len() * 2);
            for id in dirty_ids {
                self.invalidate_cache_to_root(id, &mut visited);
            }
        } else {
            self.cache_clear();
        }

        let size = self.measure_recursive(root_id, constraints);

        self.position_recursive(root_id, self.root_offset);
        self.last_root_offset = self.root_offset;

        for node in self.elements.values_mut() {
            node.element.clear_dirty(DirtyFlags::LAYOUT);
        }

        let sync_ids: Vec<ElementId> = self.post_layout_sync_registry.iter().copied().collect();
        for id in sync_ids {
            if let Some(node) = self.elements.get(&id) {
                let wants_anim = node.element.needs_repaint() || node.element.wants_animate_tick();
                let wants_rebuild = node.element.needs_rebuild();
                if wants_anim {
                    self.animation_registry.insert(id);
                }
                if wants_rebuild {
                    self.rebuild_registry.insert(id);
                }
            }
        }

        self.last_constraints_hash = constraints_hash;

        self.sync_overlay_stack();

        size
    }

    pub(crate) fn sync_overlay_stack(&mut self) {
        let mut desired: Vec<(ElementId, crate::core::Rect, bool)> = Vec::new();
        for (id, node) in self.elements.iter() {
            if let Some((bounds, modal)) = node.element.overlay_request() {
                desired.push((*id, bounds, modal));
            }
        }
        let desired_set: std::collections::HashSet<ElementId> =
            desired.iter().map(|(id, _, _)| *id).collect();
        self.overlay_stack
            .retain(|e| !e.declarative || desired_set.contains(&e.element_id));
        for (id, local, modal) in desired {
            // Декларативные оверлеи (Dialog, Menu, Portal…) задают границы
            // сразу в координатах окна.
            let bounds = local;
            if let Some(entry) = self.overlay_stack.iter_mut().find(|e| e.element_id == id) {
                entry.bounds = bounds;
                entry.local = local;
                entry.modal = modal;
                entry.declarative = true;
            } else {
                self.overlay_stack.push(super::super::OverlayEntry {
                    element_id: id,
                    bounds,
                    local,
                    modal,
                    declarative: true,
                });
            }
        }
    }

    fn invalidate_cache_to_root(
        &mut self,
        id: ElementId,
        visited: &mut std::collections::HashSet<ElementId>,
    ) {
        let mut current = id;
        loop {
            if !visited.insert(current) {
                break;
            }
            self.cache_remove(&current);
            if let Some(parent_id) = self.elements.get(&current).and_then(|n| n.parent) {
                current = parent_id;
            } else {
                break;
            }
        }
    }

    pub(crate) fn measure_recursive(&mut self, id: ElementId, constraints: Constraints) -> Size {
        let Some(idx) = self.elements.resolve(id) else {
            return Size::zero();
        };
        self.measure_recursive_by_idx(idx, constraints)
    }

    pub(crate) fn measure_recursive_by_idx(&mut self, idx: u32, constraints: Constraints) -> Size {
        crate::perf::incr(crate::perf::Counter::MeasureVisit);
        let id = self
            .elements
            .get_by_idx(idx)
            .map(|n| n.id)
            .unwrap_or_default();

        let (visible, is_boundary) = {
            let node = self.elements.get_by_idx(idx);
            (
                node.map_or(true, |n| n.element.is_visible()),
                node.map_or(false, |n| n.element.is_relayout_boundary()),
            )
        };
        if !visible {
            if is_boundary {
                let vp = self.viewport_size;
                if let Some(node) = self.elements.get_mut_by_idx(idx) {
                    node.element.set_viewport_size(vp);
                    node.element.layout(constraints);
                }
            }
            let constraints_hash = constraints.hash_key();
            self.cache_set_by_idx(
                idx,
                LayoutCache {
                    size: Size::zero(),
                    constraints_hash,
                    visible: false,
                },
            );
            return Size::zero();
        }

        let constraints_hash = constraints.hash_key();
        if let Some(cache) = self.cache_get_by_idx(idx) {
            if cache.constraints_hash == constraints_hash && cache.visible {
                crate::perf::incr(crate::perf::Counter::MeasureCacheHit);
                return cache.size;
            }
        }

        let hint = self
            .elements
            .get_by_idx(idx)
            .map(|n| n.hint_cache.clone())
            .unwrap_or_default();
        let is_col_or_row = matches!(
            &hint,
            LayoutHint::Column { .. } | LayoutHint::Row { .. } | LayoutHint::TabBar { .. }
        );
        let (children, children_idx): (Vec<ElementId>, Vec<u32>) = {
            let node = self.elements.get_by_idx(idx);
            if is_col_or_row {
                (
                    Vec::new(),
                    node.map(|n| n.children_idx.clone()).unwrap_or_default(),
                )
            } else {
                (
                    node.map(|n| n.children.clone()).unwrap_or_default(),
                    Vec::new(),
                )
            }
        };
        let children_empty = if is_col_or_row {
            children_idx.is_empty()
        } else {
            children.is_empty()
        };

        layout_log!(self, "[MEASURE] Element {} - hint: {:?}", id.0, hint);
        self.indent_level += 1;
        layout_log!(
            self,
            "constraints: min={:.1}x{:.1} max={:.1}x{:.1}",
            constraints.min_width,
            constraints.min_height,
            constraints.max_width,
            constraints.max_height
        );

        let own_size = if children_empty {
            let (size, explicit) = if let Some(node) = self.elements.get_mut_by_idx(idx) {
                (
                    node.element.layout(constraints),
                    node.element.explicit_dimensions(
                        constraints.containing_block.width,
                        constraints.containing_block.height,
                    ),
                )
            } else {
                (Size::zero(), (None, None))
            };
            // Пустые Column/Row — натурального размера (только поля), а не
            // на все ограничения, как отвечает их `layout`: пустой список
            // (уведомлений и т. п.) в свободных ограничениях иначе занимал
            // всю высоту и выталкивал соседей за край. Явные width/height и
            // `Column::expand` — как раньше.
            // Только сами Column/Row: свои элементы с той же подсказкой
            // раскладки (холст редактора документов) считают размер в `layout`.
            let plain = self
                .elements
                .get_by_idx(idx)
                .is_some_and(|n| matches!(n.element.element_type_name(), "Column" | "Row"));
            let pad = match &hint {
                _ if !plain => None,
                LayoutHint::Column { padding_left, padding_top, padding_right, padding_bottom, expand: false, .. }
                | LayoutHint::Row { padding_left, padding_top, padding_right, padding_bottom, .. } => {
                    Some(Size::new(padding_left + padding_right, padding_top + padding_bottom))
                }
                _ => None,
            };
            let size = match pad {
                Some(p) => Size::new(
                    if explicit.0.is_some() { size.width } else { p.width },
                    if explicit.1.is_some() { size.height } else { p.height },
                ),
                None => size,
            };
            let clamped = clamp_finite_explicit(size, constraints, explicit);
            if pad.is_some() {
                // Границы элемента — по итоговому размеру (фон, попадания).
                if let Some(node) = self.elements.get_mut_by_idx(idx) {
                    node.element.layout(Constraints {
                        min_width: clamped.width,
                        max_width: clamped.width,
                        min_height: clamped.height,
                        max_height: clamped.height,
                        containing_block: constraints.containing_block,
                    });
                }
            }
            layout_log!(
                self,
                "leaf: {:.1}x{:.1} -> clamped: {:.1}x{:.1}",
                size.width,
                size.height,
                clamped.width,
                clamped.height
            );
            clamped
        } else {
            match &hint {
                LayoutHint::Column {
                    gap,
                    cross_align,
                    main_align: _,
                    padding_left,
                    padding_top,
                    padding_right,
                    padding_bottom,
                    expand,
                } => self.measure_column(
                    &children_idx,
                    constraints,
                    *gap,
                    *cross_align,
                    *expand,
                    id,
                    *padding_left,
                    *padding_top,
                    *padding_right,
                    *padding_bottom,
                ),
                LayoutHint::Row {
                    gap,
                    offset_x,
                    cross_align,
                    main_align,
                    padding_left,
                    padding_top,
                    padding_right,
                    padding_bottom,
                } => self.measure_row(
                    &children_idx,
                    constraints,
                    *gap,
                    *offset_x,
                    *cross_align,
                    *main_align,
                    id,
                    *padding_left,
                    *padding_top,
                    *padding_right,
                    *padding_bottom,
                ),
                LayoutHint::Padding {
                    left,
                    top,
                    right,
                    bottom,
                } => self.measure_padding(&children, constraints, *left, *top, *right, *bottom, id),
                LayoutHint::Stack { expand } => {
                    self.measure_stack(&children, constraints, id, *expand)
                }
                LayoutHint::Center => self.measure_center(&children, constraints, id),
                LayoutHint::AspectRatio { ratio, max_height } => {
                    self.measure_aspect_ratio(&children, constraints, *ratio, *max_height, id)
                }
                LayoutHint::Grid {
                    columns,
                    row_gap,
                    col_gap,
                    masonry,
                } => self.measure_grid(
                    &children,
                    constraints,
                    *columns,
                    *row_gap,
                    *col_gap,
                    *masonry,
                    id,
                ),
                LayoutHint::HorizontalPages => {
                    self.measure_horizontal_pages(&children, constraints, id)
                }
                LayoutHint::Split {
                    horizontal,
                    ratio,
                    divider,
                } => self.measure_split(&children, constraints, *horizontal, *ratio, *divider, id),
                LayoutHint::Scroll {
                    left,
                    top,
                    right,
                    bottom,
                    unbounded_width,
                    unbounded_height,
                } => self.measure_scroll(
                    &children,
                    constraints,
                    *left,
                    *top,
                    *right,
                    *bottom,
                    *unbounded_width,
                    *unbounded_height,
                    id,
                ),
                LayoutHint::AnimatedSize => self.measure_animated_size(&children, constraints, id),
                LayoutHint::Switcher => self.measure_switcher(&children, constraints, id),
                LayoutHint::Container {
                    left,
                    top,
                    right,
                    bottom,
                } => {
                    self.measure_container(&children, constraints, *left, *top, *right, *bottom, id)
                }
                LayoutHint::Aligned {
                    left,
                    top,
                    right,
                    bottom,
                    ..
                } => self.measure_aligned(&children, constraints, *left, *top, *right, *bottom, id),
                LayoutHint::Portal { .. } => self.measure_portal(&children, constraints, id),
                LayoutHint::FloatingWindow { .. } => {
                    self.measure_floating_window(&children, constraints, id)
                }
                LayoutHint::Loose => self.measure_loose(&children, constraints, id),
                LayoutHint::Flex {
                    col_gap,
                    row_gap,
                    justify: _,
                    align_items: _,
                } => self.measure_flex(&children, constraints, *col_gap, *row_gap, id),
                LayoutHint::Tooltip { .. } => self.measure_tooltip(&children, constraints, id),
                LayoutHint::TabBar { equal_width, gap } => {
                    self.measure_tab_bar(&children_idx, constraints, *equal_width, *gap, id)
                }
                LayoutHint::Positioned { .. } => {
                    self.measure_positioned(&children, constraints, id)
                }
                LayoutHint::PanZoom => self.measure_pan_zoom(&children, constraints, id),
            }
        };

        layout_log!(
            self,
            "-> measured: {:.1}x{:.1}",
            own_size.width,
            own_size.height
        );

        if !children_empty
            && !matches!(
                hint,
                LayoutHint::AnimatedSize
                    | LayoutHint::Switcher
                    | LayoutHint::Container { .. }
                    | LayoutHint::Aligned { .. }
                    | LayoutHint::Portal { .. }
                    | LayoutHint::FloatingWindow { .. }
                    | LayoutHint::Tooltip { .. }
            )
        {
            let tight = Constraints {
                min_width: own_size.width,
                max_width: own_size.width,
                min_height: own_size.height,
                max_height: own_size.height,
                containing_block: own_size,
            };
            if let Some(node) = self.elements.get_mut_by_idx(idx) {
                node.element.layout(tight);
            }
        }

        self.cache_set_by_idx(
            idx,
            LayoutCache {
                size: own_size,
                constraints_hash,
                visible: true,
            },
        );

        // Измерение могло запустить анимацию (AnimatedSize, оценки скролла).
        self.note_animation_started(id);

        self.indent_level -= 1;
        own_size
    }
}

#[cfg(test)]
mod root_offset_tests {
    use crate::core::Point;
    use crate::testing::TestHarness;
    use crate::widgets::{DecoratedBox, Padding};

    /// Смена `root_offset` при тех же ограничениях (safe area, keyboard_pan)
    /// переставляет позиции без полного layout — дерево не «грязное», но
    /// координаты обновляются.
    #[test]
    fn root_offset_change_repositions_tree() {
        let mut h = TestHarness::new(Box::new(Padding::all(10.0).child(DecoratedBox::new())));
        h.layout(200.0, 100.0);
        let inner = h.find_by_type_name("DecoratedBox")[0];
        assert_eq!(h.element_bounds(inner).origin, Point::new(10.0, 10.0));

        h.tree.root_offset = Point::new(0.0, -30.0);
        h.layout(200.0, 100.0);
        let bounds = h.element_bounds(inner);
        assert_eq!((bounds.origin.x, bounds.origin.y), (10.0, -20.0));
        assert_eq!(h.element_bounds(h.root_id).origin.y, -30.0);

        h.tree.root_offset = Point::zero();
        h.layout(200.0, 100.0);
        assert_eq!(h.element_bounds(inner).origin, Point::new(10.0, 10.0));
    }
}

#[cfg(test)]
mod row_stretch_tests {
    use crate::layout::CrossAxisAlignment;
    use crate::testing::TestHarness;
    use crate::widgets::{DecoratedBox, Padding, Row};

    /// Row со Stretch при конечной высоте тянет и не-flex детей до высоты
    /// строки — как `align-items: stretch` (боковая панель рядом со списком).
    #[test]
    fn stretch_non_flex_children_to_line_height() {
        let row = Row::new()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .child(Padding::all(40.0).child(DecoratedBox::new()))
            .child(DecoratedBox::new().child(Padding::all(5.0).child(DecoratedBox::new())));
        let mut h = TestHarness::new(Box::new(row));
        h.layout(300.0, 200.0);
        let boxes = h.find_by_type_name("DecoratedBox");
        let tallest = boxes.iter().map(|&b| h.element_bounds(b).size.height).fold(0.0f32, f32::max);
        let side = boxes.iter().map(|&b| h.element_bounds(b)).find(|b| b.origin.x >= 80.0 && b.size.width >= 10.0).expect("второй ребёнок");
        assert!(tallest <= 200.0);
        assert_eq!(side.size.height, 80.0, "второй ребёнок растянут до высоты первого");
    }
}
