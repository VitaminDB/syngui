use crate::core::{Size, EdgeInsets};
use crate::layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
use crate::widget::LayoutHint;
use super::{ElementId, ElementTree};
use super::clamp_finite;
use crate::widget::tree::ScrollCullContext;

struct ChildProbe {
    idx: u32,
    id: ElementId,
    hint: LayoutHint,
    margin: EdgeInsets,
    mss_flex_grow: f32,
}

#[inline]
fn probe_flex(p: &ChildProbe) -> Option<f32> {
    (p.mss_flex_grow > 0.0).then_some(p.mss_flex_grow)
}

macro_rules! layout_log {
    ($self:expr, $($arg:tt)*) => {
        if $self.layout_log_enabled {
            $self.log(format!($($arg)*));
        }
    };
}

impl ElementTree {

    fn scroll_estimate(&self, child_id: ElementId, current_y: f32, _avg_height: f32) -> Option<Size> {
        if self.scroll_cull_stack.is_empty() {
            return None;
        }
        if self.force_full_measure {
            return None;
        }

        let ctx = self.scroll_cull_stack.last()?;
        let view_top = ctx.scroll_offset_y;
        let view_bottom = view_top + ctx.viewport_height;
        let buffer = ctx.viewport_height;

        if let Some(cache) = self.cache_get(&child_id) {
            if cache.visible {
                let child_bottom = current_y + cache.size.height;
                if child_bottom < (view_top - buffer) || current_y > (view_bottom + buffer) {
                    return Some(cache.size);
                }
            }
        }

        None
    }

    pub(super) fn measure_column(&mut self, children_idx: &[u32], constraints: Constraints, gap: f32, cross_align: CrossAxisAlignment, expand: bool, id: ElementId, pad_l: f32, pad_t: f32, pad_r: f32, pad_b: f32) -> Size {
        let gap = gap.max(0.0);
        let pad_h = pad_l + pad_r;
        let pad_v = pad_t + pad_b;

        let (explicit_w, explicit_h, min_w, max_w_dim, min_h, max_h_dim) = if let Some(node) = self.elements.get(&id) {
            let (ew, eh) = node.element.explicit_dimensions(constraints.containing_block.width, constraints.containing_block.height);
            let (minw, maxw, minh, maxh) = node.element.min_max_dimensions(constraints.containing_block.width, constraints.containing_block.height);
            (ew, eh, minw, maxw, minh, maxh)
        } else {
            (None, None, None, None, None, None)
        };

        let effective_max_width = explicit_w.unwrap_or(constraints.max_width) - pad_h;
        let raw_h = explicit_h.unwrap_or(constraints.max_height);
        let effective_max_height = if raw_h.is_infinite() { constraints.max_height } else { raw_h } - pad_v;

        let stretch = cross_align == CrossAxisAlignment::Stretch && effective_max_width.is_finite();
        let child_min_width = if stretch { effective_max_width } else { 0.0 };

        let child_cb = Size::new(
            if effective_max_width.is_finite() { effective_max_width } else { constraints.containing_block.width },
            if effective_max_height.is_finite() { effective_max_height } else { constraints.containing_block.height },
        );

        let non_expanded_constraints = Constraints {
            min_width: child_min_width,
            max_width: effective_max_width,
            min_height: 0.0,
            max_height: f32::INFINITY,
            containing_block: child_cb,
        };

        let child_probes: Vec<ChildProbe> = children_idx.iter().map(|&cidx| {
            if let Some(n) = self.elements.get_by_idx(cidx) {
                let hint = n.hint_cache.clone();
                let margin = if n.mss_margin_set { n.mss_margin } else { n.element.margin() };
                ChildProbe { idx: cidx, id: n.id, hint, margin, mss_flex_grow: n.mss_flex_grow }
            } else {
                ChildProbe { idx: cidx, id: ElementId::default(), hint: LayoutHint::default(), margin: EdgeInsets::default(), mss_flex_grow: 0.0 }
            }
        }).collect();

        let mut total_fixed_height = 0.0f32;
        let mut max_width = 0.0f32;
        let mut total_flex = 0.0f32;
        let mut expanded_idx: Vec<(u32, f32)> = Vec::new();

        let container_constraints = Constraints {
            min_width: child_min_width,
            max_width: effective_max_width,
            min_height: 0.0,
            max_height: effective_max_height,
            containing_block: child_cb,
        };

        let scroll_active = !self.scroll_cull_stack.is_empty() || self.force_full_measure;
        let mut measured_count = 0usize;
        let mut measured_height_sum = 0.0f32;

        // Gap считается только между детьми ненулевой высоты: скрытые
        // попапы/диалоги меряются в 0 и не должны раздвигать соседей.
        // Flex-дети — всегда участники: им ещё раздадут остаток.
        let mut gap_participants = 0usize;
        for probe in &child_probes {
            if let Some(flex) = probe_flex(probe) {
                total_flex += flex;
                expanded_idx.push((probe.idx, flex));
            } else {
                if scroll_active {
                    let avg_h = if measured_count > 0 { measured_height_sum / measured_count as f32 } else { 50.0 };
                    if let Some(est) = self.scroll_estimate(probe.id, total_fixed_height, avg_h) {
                        total_fixed_height += est.height;
                        if est.height > 0.0 {
                            gap_participants += 1;
                        }
                        max_width = max_width.max(est.width);
                        continue;
                    }
                }

                let c = if matches!(probe.hint, LayoutHint::Container { .. } | LayoutHint::Loose) {
                    container_constraints
                } else {
                    non_expanded_constraints
                };
                let child_size = self.measure_recursive_by_idx(probe.idx, c);
                let m = probe.margin;
                let h = child_size.height + m.top + m.bottom;
                total_fixed_height += h;
                if h > 0.0 {
                    gap_participants += 1;
                }
                max_width = max_width.max(child_size.width + m.left + m.right);
                measured_count += 1;
                measured_height_sum += h;
            }
        }
        gap_participants += expanded_idx.len();

        let gap_space = gap * gap_participants.saturating_sub(1) as f32;

        let total_height;
        if !expanded_idx.is_empty() && effective_max_height.is_finite() {
            let remaining = (effective_max_height - total_fixed_height - gap_space).max(0.0);

            for (cidx, flex) in &expanded_idx {
                let expanded_height = remaining * flex / total_flex;
                let expanded_constraints = Constraints {
                    min_width: 0.0,
                    max_width: effective_max_width,
                    min_height: expanded_height,
                    max_height: expanded_height,
                    containing_block: child_cb,
                };
                let child_size = self.measure_recursive_by_idx(*cidx, expanded_constraints);
                max_width = max_width.max(child_size.width);
            }

            total_height = effective_max_height;
        } else {
            for (cidx, _flex) in &expanded_idx {
                let child_size = self.measure_recursive_by_idx(*cidx, non_expanded_constraints);
                total_fixed_height += child_size.height;
                max_width = max_width.max(child_size.width);
            }
            total_height = total_fixed_height + gap_space;
        }

        let mut width = if let Some(ew) = explicit_w {
            ew.min(constraints.max_width)
        } else if expand && constraints.max_width.is_finite() {
            constraints.max_width
        } else if max_width.is_finite() {
            (max_width + pad_h).min(constraints.max_width)
        } else {
            (100.0f32 + pad_h).min(constraints.max_width)
        };
        let mut height = if let Some(eh) = explicit_h {
            eh.min(constraints.max_height)
        } else if total_height.is_finite() {
            (total_height + pad_v).min(constraints.max_height)
        } else {
            (100.0f32 + pad_v).min(constraints.max_height)
        };

        if let Some(min) = min_w { width = width.max(min); }
        if let Some(max) = max_w_dim { width = width.min(max); }
        if let Some(min) = min_h { height = height.max(min); }
        if let Some(max) = max_h_dim { height = height.min(max); }

        width = width.clamp(constraints.min_width.min(constraints.max_width), constraints.max_width);
        height = height.clamp(constraints.min_height.min(constraints.max_height), constraints.max_height);

        layout_log!(self,"Column: {} children ({}exp), size={:.1}x{:.1}",
            children_idx.len(), expanded_idx.len(), width, height);

        Size::new(width, height)
    }

    pub(super) fn measure_row(&mut self, children_idx: &[u32], constraints: Constraints, gap: f32, offset_x: f32, cross_align: CrossAxisAlignment, main_align: MainAxisAlignment, id: ElementId, pad_l: f32, pad_t: f32, pad_r: f32, pad_b: f32) -> Size {
        let gap = gap.max(0.0);
        let pad_h = pad_l + pad_r;
        let pad_v = pad_t + pad_b;
        let (explicit_w, explicit_h, min_w, max_w_dim, min_h, max_h_dim) = if let Some(node) = self.elements.get(&id) {
            let (ew, eh) = node.element.explicit_dimensions(constraints.containing_block.width, constraints.containing_block.height);
            let (minw, maxw, minh, maxh) = node.element.min_max_dimensions(constraints.containing_block.width, constraints.containing_block.height);
            (ew, eh, minw, maxw, minh, maxh)
        } else {
            (None, None, None, None, None, None)
        };

        let effective_max_width = (explicit_w.unwrap_or(constraints.max_width) - pad_h).max(0.0);
        let effective_max_height = (explicit_h.unwrap_or(constraints.max_height) - pad_v).max(0.0);

        let child_cb = Size::new(
            if effective_max_width.is_finite() { effective_max_width } else { constraints.containing_block.width },
            if effective_max_height.is_finite() { effective_max_height } else { constraints.containing_block.height },
        );

        let non_expanded_constraints = Constraints {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: effective_max_height,
            containing_block: child_cb,
        };

        let child_probes: Vec<ChildProbe> = children_idx.iter().map(|&cidx| {
            if let Some(n) = self.elements.get_by_idx(cidx) {
                let hint = n.hint_cache.clone();
                let margin = if n.mss_margin_set { n.mss_margin } else { n.element.margin() };
                ChildProbe { idx: cidx, id: n.id, hint, margin, mss_flex_grow: n.mss_flex_grow }
            } else {
                ChildProbe { idx: cidx, id: ElementId::default(), hint: LayoutHint::default(), margin: EdgeInsets::default(), mss_flex_grow: 0.0 }
            }
        }).collect();

        let mut total_fixed_width = offset_x;
        let mut max_height = 0.0f32;
        let mut total_flex = 0.0f32;
        let mut expanded_idx: Vec<(u32, f32)> = Vec::new();
        let mut measured_widths: Vec<(u32, f32)> = Vec::with_capacity(children_idx.len());

        let container_constraints = Constraints {
            min_width: 0.0,
            max_width: effective_max_width,
            min_height: 0.0,
            max_height: effective_max_height,
            containing_block: child_cb,
        };

        // Gap считается только между детьми ненулевой ширины: скрытые
        // попапы/оверлеи меряются в 0 и не должны раздвигать соседей.
        // Flex-дети — всегда участники: им ещё раздадут остаток.
        let mut gap_participants = 0usize;
        for probe in &child_probes {
            if let Some(flex) = probe_flex(probe) {
                total_flex += flex;
                expanded_idx.push((probe.idx, flex));
            } else {
                let c = if matches!(probe.hint, LayoutHint::Container { .. }) {
                    container_constraints
                } else {
                    non_expanded_constraints
                };
                let child_size = self.measure_recursive_by_idx(probe.idx, c);
                let m = probe.margin;
                let extent = child_size.width + m.left + m.right;
                total_fixed_width += extent;
                if extent > 0.0 {
                    gap_participants += 1;
                }
                max_height = max_height.max(child_size.height + m.top + m.bottom);
                measured_widths.push((probe.idx, child_size.width));
            }
        }
        gap_participants += expanded_idx.len();

        let gap_space = gap * gap_participants.saturating_sub(1) as f32;

        let total_width;
        if !expanded_idx.is_empty() && effective_max_width.is_finite() {
            let remaining = (effective_max_width - total_fixed_width - gap_space).max(0.0);

            let stretch = cross_align == CrossAxisAlignment::Stretch && effective_max_height.is_finite();
            for (cidx, flex) in &expanded_idx {
                let expanded_width = remaining * flex / total_flex;
                let expanded_constraints = Constraints {
                    min_width: expanded_width,
                    max_width: expanded_width,
                    min_height: if stretch { effective_max_height } else { 0.0 },
                    max_height: effective_max_height,
                    containing_block: Size::new(expanded_width, if effective_max_height.is_finite() { effective_max_height } else { child_cb.height }),
                };
                let child_size = self.measure_recursive_by_idx(*cidx, expanded_constraints);
                max_height = max_height.max(child_size.height);
                measured_widths.push((*cidx, expanded_width));
            }

            total_width = effective_max_width;
        } else {
            for (cidx, _flex) in &expanded_idx {
                let child_size = self.measure_recursive_by_idx(*cidx, non_expanded_constraints);
                total_fixed_width += child_size.width;
                max_height = max_height.max(child_size.height);
                measured_widths.push((*cidx, child_size.width));
            }
            total_width = total_fixed_width + gap_space;
        }

        if cross_align == CrossAxisAlignment::Stretch
            && !effective_max_height.is_finite()
            && max_height.is_finite()
            && max_height > 0.0
        {
            for (cidx, width) in &measured_widths {
                let margin = child_probes
                    .iter()
                    .find(|p| p.idx == *cidx)
                    .map(|p| p.margin)
                    .unwrap_or_default();
                let target = (max_height - margin.top - margin.bottom).max(0.0);
                let stretch_constraints = Constraints {
                    min_width: *width,
                    max_width: *width,
                    min_height: target,
                    max_height: f32::INFINITY,
                    containing_block: Size::new(*width, target),
                };
                self.measure_recursive_by_idx(*cidx, stretch_constraints);
            }
        }

        if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(constraints);
        }

        let content_width = if total_width.is_finite() {
            (total_width + pad_h).min(constraints.max_width)
        } else {
            (100.0f32 + pad_h).min(constraints.max_width)
        };
        let mut width = if let Some(ew) = explicit_w {
            ew.min(constraints.max_width)
        } else if main_align != MainAxisAlignment::Start && constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            content_width
        };
        let children_height = if max_height.is_finite() {
            max_height + pad_v
        } else {
            100.0f32 + pad_v
        };
        let mut height = if let Some(eh) = explicit_h {
            eh.min(constraints.max_height)
        } else {
            children_height.min(constraints.max_height)
        };

        if let Some(min) = min_w { width = width.max(min); }
        if let Some(max) = max_w_dim { width = width.min(max); }
        if let Some(min) = min_h { height = height.max(min); }
        if let Some(max) = max_h_dim { height = height.min(max); }

        width = width.clamp(constraints.min_width.min(constraints.max_width), constraints.max_width);
        height = height.clamp(constraints.min_height.min(constraints.max_height), constraints.max_height);

        layout_log!(self,"Row: {} children ({}exp), size={:.1}x{:.1}",
            children_idx.len(), expanded_idx.len(), width, height);

        Size::new(width, height)
    }

    pub(super) fn measure_padding(&mut self, children: &[ElementId], constraints: Constraints, left: f32, top: f32, right: f32, bottom: f32, id: ElementId) -> Size {
        let (explicit_w, explicit_h) = if let Some(node) = self.elements.get(&id) {
            node.element.explicit_dimensions(
                constraints.containing_block.width,
                constraints.containing_block.height,
            )
        } else {
            (None, None)
        };

        let (child_min_w, child_max_w) = match explicit_w {
            Some(w) => {
                let cw = (w - left - right).max(0.0);
                (cw, cw)
            }
            None => {
                let min = (constraints.min_width - left - right).max(0.0);
                let max = (constraints.max_width - left - right).max(0.0);
                (min.min(max), max)
            }
        };
        let (child_min_h, child_max_h) = match explicit_h {
            Some(h) => {
                let ch = (h - top - bottom).max(0.0);
                (ch, ch)
            }
            None => {
                let min = (constraints.min_height - top - bottom).max(0.0);
                let max = (constraints.max_height - top - bottom).max(0.0);
                (min.min(max), max)
            }
        };
        let cb = Size::new(
            explicit_w
                .map(|w| (w - left - right).max(0.0))
                .unwrap_or_else(|| (constraints.containing_block.width - left - right).max(0.0)),
            explicit_h
                .map(|h| (h - top - bottom).max(0.0))
                .unwrap_or_else(|| (constraints.containing_block.height - top - bottom).max(0.0)),
        );
        let child_constraints = Constraints {
            min_width: child_min_w,
            max_width: child_max_w,
            min_height: child_min_h,
            max_height: child_max_h,
            containing_block: cb,
        };

        let mut child_size = Size::zero();
        for &child_id in children {
            let cs = self.measure_recursive(child_id, child_constraints);
            child_size.width = child_size.width.max(cs.width);
            child_size.height = child_size.height.max(cs.height);
        }

        let final_w = explicit_w.unwrap_or(child_size.width + left + right);
        let final_h = explicit_h.unwrap_or(child_size.height + top + bottom);

        layout_log!(self, "Padding: {:.1}x{:.1}", final_w, final_h);

        Size::new(
            final_w.min(constraints.max_width),
            final_h.min(constraints.max_height),
        )
    }

    pub(super) fn measure_loose(&mut self, children: &[ElementId], constraints: Constraints, id: ElementId) -> Size {
        let loose = Constraints {
            min_width: 0.0,
            max_width: constraints.max_width,
            min_height: 0.0,
            max_height: constraints.max_height,
            containing_block: constraints.containing_block,
        };

        let mut child_size = Size::zero();
        for &child_id in children {
            let cs = self.measure_recursive(child_id, loose);
            child_size.width = child_size.width.max(cs.width);
            child_size.height = child_size.height.max(cs.height);
        }

        if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(constraints);
        }

        // Ширину, навязанную родителем, соблюдаем: когда Row растягивает
        // flex-ребёнка, он задаёт min = max = доля остатка, и Loose обязан
        // вернуть эту долю, а не натуральную ширину содержимого. Иначе
        // `.grow` молча схлопывается — причём не сразу, а после первой же
        // перестройки реактивного поддерева, когда у узла появляется
        // ребёнок с собственной шириной.
        //
        // По высоте — наоборот, натуральный размер: `min_height` сюда
        // приходит от `Stack(StackFit::Expand)` как «можешь занять всё», и
        // если его исполнить, содержимое перестаёт центрироваться по
        // поперечной оси Row — заголовок панели уезжает к верхнему краю.
        Size::new(
            child_size
                .width
                .max(constraints.min_width)
                .min(constraints.max_width.max(constraints.min_width)),
            child_size.height.min(constraints.max_height),
        )
    }

    pub(super) fn measure_tab_bar(&mut self, children_idx: &[u32], constraints: Constraints, equal_width: bool, gap: f32, id: ElementId) -> Size {
        use crate::layout::{CrossAxisAlignment, MainAxisAlignment};
        if !equal_width || children_idx.is_empty() {
            return self.measure_row(children_idx, constraints, gap, 0.0,
                CrossAxisAlignment::Stretch, MainAxisAlignment::Start,
                id, 0.0, 0.0, 0.0, 0.0);
        }

        let (explicit_w, explicit_h) = if let Some(node) = self.elements.get(&id) {
            node.element.explicit_dimensions(constraints.containing_block.width, constraints.containing_block.height)
        } else {
            (None, None)
        };
        let avail_w = explicit_w.unwrap_or(constraints.max_width);
        let n = children_idx.len() as f32;
        let gap_total = gap * (n - 1.0).max(0.0);
        let per_child = if avail_w.is_finite() {
            ((avail_w - gap_total) / n).max(0.0)
        } else {
            return self.measure_row(children_idx, constraints, gap, 0.0,
                CrossAxisAlignment::Stretch, MainAxisAlignment::Start,
                id, 0.0, 0.0, 0.0, 0.0);
        };

        let effective_max_h = explicit_h.unwrap_or(constraints.max_height);
        let child_cb = Size::new(
            if per_child.is_finite() { per_child } else { constraints.containing_block.width },
            if effective_max_h.is_finite() { effective_max_h } else { constraints.containing_block.height },
        );
        let child_constraints = Constraints {
            min_width: per_child,
            max_width: per_child,
            min_height: 0.0,
            max_height: effective_max_h,
            containing_block: child_cb,
        };

        let mut max_height = 0.0f32;
        for &cidx in children_idx {
            let child_size = self.measure_recursive_by_idx(cidx, child_constraints);
            max_height = max_height.max(child_size.height);
        }

        if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(constraints);
        }

        let width = avail_w.min(constraints.max_width);
        let height = explicit_h
            .map(|eh| eh.min(constraints.max_height))
            .unwrap_or_else(|| max_height.min(constraints.max_height));
        Size::new(width, height)
    }

    pub(super) fn measure_stack(&mut self, children: &[ElementId], constraints: Constraints, id: ElementId, expand: bool) -> Size {
        let mut max_width = 0.0f32;
        let mut max_height = 0.0f32;

        let expanded = Constraints {
            min_width: if constraints.max_width.is_finite() { constraints.max_width } else { constraints.min_width },
            max_width: constraints.max_width,
            min_height: if constraints.max_height.is_finite() { constraints.max_height } else { constraints.min_height },
            max_height: constraints.max_height,
            containing_block: constraints.containing_block,
        };

        for &child_id in children {
            let child_constraints = if expand && !self.child_floats_over_stack(child_id) {
                expanded
            } else {
                constraints
            };
            let child_size = self.measure_recursive(child_id, child_constraints);
            max_width = max_width.max(child_size.width);
            max_height = max_height.max(child_size.height);
        }

        let _ = if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(constraints)
        } else {
            Size::zero()
        };

        Size::new(
            max_width.min(constraints.max_width),
            max_height.min(constraints.max_height)
        )
    }

    fn child_floats_over_stack(&self, child_id: ElementId) -> bool {
        self.elements
            .get(&child_id)
            .map(|node| {
                matches!(
                    node.element.layout_hint(),
                    crate::widget::LayoutHint::Portal { .. }
                        | crate::widget::LayoutHint::FloatingWindow { .. }
                        | crate::widget::LayoutHint::Positioned { .. }
                        | crate::widget::LayoutHint::Tooltip { .. }
                )
            })
            .unwrap_or(false)
    }

    pub(super) fn measure_center(&mut self, children: &[ElementId], constraints: Constraints, id: ElementId) -> Size {
        let element_size = if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(constraints)
        } else {
            Size::zero()
        };

        let child_max_w = if element_size.width > 0.0 { element_size.width } else { constraints.max_width };
        let child_max_h = if element_size.height > 0.0 { element_size.height } else { constraints.max_height };
        let child_constraints = Constraints {
            min_width: 0.0,
            max_width: child_max_w,
            min_height: 0.0,
            max_height: child_max_h,
            containing_block: Size::new(
                if child_max_w.is_finite() { child_max_w } else { constraints.containing_block.width },
                if child_max_h.is_finite() { child_max_h } else { constraints.containing_block.height },
            ),
        };

        let mut max_width = 0.0f32;
        let mut max_height = 0.0f32;

        for &child_id in children {
            let child_size = self.measure_recursive(child_id, child_constraints);
            max_width = max_width.max(child_size.width);
            max_height = max_height.max(child_size.height);
        }

        let width = element_size.width.max(max_width);
        let height = element_size.height.max(max_height);

        layout_log!(self,"Center: element={:.1}x{:.1} child={:.1}x{:.1}",
            element_size.width, element_size.height, max_width, max_height);

        Size::new(
            width.min(constraints.max_width),
            height.min(constraints.max_height)
        )
    }

    pub(super) fn measure_container(&mut self, children: &[ElementId], constraints: Constraints, left: f32, top: f32, right: f32, bottom: f32, id: ElementId) -> Size {
        let pad_h = left + right;
        let pad_v = top + bottom;

        let (explicit_w, explicit_h, min_w, max_w, min_h, max_h) = if let Some(node) = self.elements.get(&id) {
            let (ew, eh) = node.element.explicit_dimensions(constraints.containing_block.width, constraints.containing_block.height);
            let (minw, maxw, minh, maxh) = node.element.min_max_dimensions(constraints.containing_block.width, constraints.containing_block.height);
            (ew, eh, minw, maxw, minh, maxh)
        } else {
            (None, None, None, None, None, None)
        };

        let parent_size = if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(constraints)
        } else {
            Size::zero()
        };

        let parent_imposed_w = constraints.max_width.is_finite()
            && constraints.min_width >= constraints.max_width;
        let parent_imposed_h = constraints.max_height.is_finite()
            && constraints.min_height >= constraints.max_height;
        let (mss_intrinsic_w, mss_intrinsic_h) = if let Some(node) = self.elements.get(&id) {
            let mss = node.element.mss();
            let iw = mss.and_then(|m| m.width).map(|d| d.is_intrinsic()).unwrap_or(false);
            let ih = mss.and_then(|m| m.height).map(|d| d.is_intrinsic()).unwrap_or(false);
            (iw, ih)
        } else {
            (false, false)
        };
        let shrink_w = mss_intrinsic_w || (explicit_w.is_none() && !parent_imposed_w);
        let shrink_h = mss_intrinsic_h || (explicit_h.is_none() && !parent_imposed_h);

        let child_max_h = if shrink_h {
            if constraints.max_height.is_finite() {
                (constraints.max_height - pad_v).max(0.0)
            } else {
                f32::INFINITY
            }
        } else {
            (parent_size.height - pad_v).max(0.0)
        };

        let self_cb_w = if let Some(m) = max_w { m } else if let Some(ew) = explicit_w { ew } else { constraints.containing_block.width };
        let self_cb_h = if let Some(m) = max_h { m } else if let Some(eh) = explicit_h { eh } else { constraints.containing_block.height };
        let probe_cb = Size::new(
            (self_cb_w - pad_h).max(0.0),
            (self_cb_h - pad_v).max(0.0),
        );

        let needs_intrinsic_pass =
            shrink_w && (min_w.is_some() || max_w.is_some() || mss_intrinsic_w || mss_intrinsic_h);
        let (mut width, mut height, content) = if needs_intrinsic_pass {
            let child_min_h = if !shrink_h { child_max_h } else { 0.0 };
            let probe_constraints = Constraints {
                min_width: 0.0, max_width: f32::INFINITY,
                min_height: child_min_h, max_height: child_max_h,
                containing_block: probe_cb,
            };
            let mut probe_content = Size::zero();
            for &child_id in children {
                let cs = self.measure_recursive(child_id, probe_constraints);
                probe_content.width = probe_content.width.max(cs.width);
                probe_content.height += cs.height;
            }

            let mut w = probe_content.width + pad_h;
            if let Some(min) = min_w { w = w.max(min); }
            if let Some(max) = max_w { w = w.min(max); }
            let final_max_w = (w - pad_h).max(0.0);
            let final_min_h = if !shrink_h { child_max_h } else { 0.0 };
            let final_cb = Size::new(final_max_w, probe_cb.height);
            let final_constraints = Constraints {
                min_width: 0.0, max_width: final_max_w,
                min_height: final_min_h, max_height: child_max_h,
                containing_block: final_cb,
            };
            let mut content = Size::zero();
            for &child_id in children {
                let cs = self.measure_recursive(child_id, final_constraints);
                content.width = content.width.max(cs.width);
                content.height += cs.height;
            }
            let h = if shrink_h { content.height + pad_v } else { parent_size.height };
            (w, h, content)
        } else {
            let child_max_w = if !shrink_w {
                (parent_size.width - pad_h).max(0.0)
            } else {
                (constraints.max_width - pad_h).max(0.0)
            };
            let child_min_w = if !shrink_w { child_max_w } else { 0.0 };
            let child_min_h = if !shrink_h { child_max_h } else { 0.0 };
            let single_cb = Size::new(
                if child_max_w.is_finite() { child_max_w } else { probe_cb.width },
                if child_max_h.is_finite() { child_max_h } else { probe_cb.height },
            );
            let child_constraints = Constraints {
                min_width: child_min_w, max_width: child_max_w,
                min_height: child_min_h, max_height: child_max_h,
                containing_block: single_cb,
            };
            let mut content = Size::zero();
            for &child_id in children {
                let cs = self.measure_recursive(child_id, child_constraints);
                content.width = content.width.max(cs.width);
                content.height += cs.height;
            }
            let w = if shrink_w { content.width + pad_h } else { parent_size.width };
            let h = if shrink_h { content.height + pad_v } else { parent_size.height };
            (w, h, content)
        };

        if let Some(min) = min_w { width = width.max(min); }
        if let Some(max) = max_w { width = width.min(max); }
        if let Some(min) = min_h { height = height.max(min); }
        if let Some(max) = max_h { height = height.min(max); }
        width = width.clamp(constraints.min_width.min(constraints.max_width), constraints.max_width);
        height = height.clamp(constraints.min_height.min(constraints.max_height), constraints.max_height);

        if width != parent_size.width || height != parent_size.height {
            let tight = Constraints {
                min_width: width, max_width: width,
                min_height: height, max_height: height,
                containing_block: constraints.containing_block,
            };
            if let Some(node) = self.elements.get_mut(&id) {
                node.element.layout(tight);
            }
        }

        if let Some(node) = self.elements.get_mut(&id) {
            node.element.set_content_size(content);
        }

        layout_log!(self,"Container: {:.1}x{:.1} children={:.1}x{:.1} shrink=({},{})",
            width, height, content.width, content.height, shrink_w, shrink_h);
        Size::new(width, height)
    }

    pub(super) fn measure_portal(&mut self, children: &[ElementId], constraints: Constraints, id: ElementId) -> Size {
        if let Some(node) = self.elements.get_mut(&id) {
            node.element.set_viewport_size(self.viewport_size);
            node.element.layout(constraints);
        }

        let viewport = self.viewport_size;
        let (explicit_w, _) = if let Some(node) = self.elements.get(&id) {
            node.element.explicit_dimensions(constraints.containing_block.width, constraints.containing_block.height)
        } else {
            (None, None)
        };
        let child_constraints = Constraints {
            min_width: 0.0,
            max_width: explicit_w.unwrap_or(viewport.width),
            min_height: 0.0,
            max_height: viewport.height,
            containing_block: viewport,
        };
        let mut content_size = Size::zero();
        for &child_id in children {
            let cs = self.measure_recursive(child_id, child_constraints);
            content_size.width = content_size.width.max(cs.width);
            content_size.height += cs.height;
        }

        if let Some(node) = self.elements.get_mut(&id) {
            node.element.set_content_size(content_size);
        }

        layout_log!(self,"Portal: children={:.1}x{:.1}, takes zero space",
            content_size.width, content_size.height);
        Size::zero()
    }

    pub(super) fn measure_floating_window(&mut self, children: &[ElementId], constraints: Constraints, id: ElementId) -> Size {
        if let Some(node) = self.elements.get_mut(&id) {
            node.element.set_viewport_size(self.viewport_size);
            node.element.layout(constraints);
        }

        let (explicit_w, explicit_h) = if let Some(node) = self.elements.get(&id) {
            node.element.explicit_dimensions(constraints.containing_block.width, constraints.containing_block.height)
        } else {
            (None, None)
        };
        let child_constraints = Constraints {
            min_width: 0.0,
            max_width: explicit_w.unwrap_or(self.viewport_size.width),
            min_height: 0.0,
            max_height: explicit_h.unwrap_or(f32::INFINITY),
            containing_block: Size::new(
                explicit_w.unwrap_or(self.viewport_size.width),
                explicit_h.unwrap_or(self.viewport_size.height),
            ),
        };
        let mut content_size = Size::zero();
        for &child_id in children {
            let cs = self.measure_recursive(child_id, child_constraints);
            content_size.width = content_size.width.max(cs.width);
            content_size.height += cs.height;
        }

        if let Some(node) = self.elements.get_mut(&id) {
            node.element.set_content_size(content_size);
            node.refresh_hint_cache();
        }

        layout_log!(self,"FloatingWindow: children={:.1}x{:.1}, takes zero space",
            content_size.width, content_size.height);
        Size::zero()
    }

    pub(super) fn measure_tooltip(&mut self, children: &[ElementId], constraints: Constraints, id: ElementId) -> Size {
        let target_size = if let Some(&child_id) = children.first() {
            self.measure_recursive(child_id, constraints)
        } else {
            Size::zero()
        };

        let tight = Constraints {
            min_width: target_size.width, max_width: target_size.width,
            min_height: target_size.height, max_height: target_size.height,
            containing_block: target_size,
        };
        if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(tight);
        }

        let active_count = self.elements.get(&id)
            .map_or(usize::MAX, |n| n.element.active_child_count());

        if children.len() > 1 && active_count > 1 {
            let (explicit_w, explicit_h) = if let Some(node) = self.elements.get(&id) {
                node.element.explicit_dimensions(constraints.containing_block.width, constraints.containing_block.height)
            } else {
                (None, None)
            };
            let content_constraints = Constraints {
                min_width: 0.0,
                max_width: explicit_w.unwrap_or(self.viewport_size.width.min(400.0)),
                min_height: 0.0,
                max_height: explicit_h.unwrap_or(self.viewport_size.height.min(300.0)),
                containing_block: Size::new(
                    explicit_w.unwrap_or(self.viewport_size.width.min(400.0)),
                    explicit_h.unwrap_or(self.viewport_size.height.min(300.0)),
                ),
            };
            let mut content_size = Size::zero();
            for &child_id in &children[1..] {
                let cs = self.measure_recursive(child_id, content_constraints);
                content_size.width = content_size.width.max(cs.width);
                content_size.height += cs.height;
            }

            if let Some(node) = self.elements.get_mut(&id) {
                node.element.set_content_size(content_size);
            }
        }

        layout_log!(self, "Tooltip: target={:.1}x{:.1}", target_size.width, target_size.height);
        target_size
    }

    pub(super) fn measure_flex(&mut self, children: &[ElementId], constraints: Constraints, col_gap: f32, row_gap: f32, _id: ElementId) -> Size {
        let col_gap = col_gap.max(0.0);
        let row_gap = row_gap.max(0.0);
        let max_width = if constraints.max_width.is_finite() { constraints.max_width } else { 800.0 };

        let flex_cb = Size::new(
            if max_width.is_finite() { max_width } else { constraints.containing_block.width },
            constraints.containing_block.height,
        );
        let child_constraints = Constraints {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
            containing_block: flex_cb,
        };

        let mut child_sizes: Vec<(ElementId, Size)> = Vec::with_capacity(children.len());
        for &child_id in children {
            let size = self.measure_recursive(child_id, child_constraints);
            child_sizes.push((child_id, size));
        }

        let mut lines: Vec<(f32, f32)> = Vec::new();
        let mut line_w = 0.0f32;
        let mut line_h = 0.0f32;
        let mut line_count = 0usize;

        for &(_, size) in &child_sizes {
            let needed = if line_count > 0 { col_gap + size.width } else { size.width };
            if line_count > 0 && line_w + needed > max_width {
                lines.push((line_w, line_h));
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
            lines.push((line_w, line_h));
        }

        let total_row_gap = if lines.len() > 1 { row_gap * (lines.len() - 1) as f32 } else { 0.0 };
        let total_height = lines.iter().map(|(_, h)| *h).sum::<f32>() + total_row_gap;
        let total_width = lines.iter().map(|(w, _)| *w).fold(0.0f32, f32::max).min(max_width);

        let result_width = if constraints.max_width.is_finite() {
            max_width
        } else {
            total_width
        };

        layout_log!(self,"Flex: {} children, {} lines, size={:.1}x{:.1}",
            children.len(), lines.len(), result_width, total_height);

        Size::new(
            constraints.constrain_width(result_width),
            constraints.constrain_height(total_height),
        )
    }

    pub(super) fn measure_grid(&mut self, children: &[ElementId], constraints: Constraints, columns: usize, row_gap: f32, col_gap: f32, masonry: bool, id: ElementId) -> Size {
        crate::perf::incr(crate::perf::Counter::MeasureGridCall);

        if let Some(node) = self.elements.get_mut(&id) {
            if let Some(grid) = node.element.as_any_mut().and_then(|a| a.downcast_mut::<crate::widgets::containers::grid::GridElement>()) {
                grid.layout_cache = None;
            }
        }

        let cols = columns.max(1);
        let total_col_gap = if cols > 1 { col_gap * (cols - 1) as f32 } else { 0.0 };

        let col_width = if constraints.max_width.is_finite() {
            (constraints.max_width - total_col_gap) / cols as f32
        } else {
            let probe = Constraints {
                min_width: 0.0,
                max_width: f32::INFINITY,
                min_height: 0.0,
                max_height: f32::INFINITY,
                containing_block: constraints.containing_block,
            };
            let mut max_natural = 0.0f32;
            let probe_count = cols.min(children.len());
            for &child_id in &children[..probe_count] {
                crate::perf::incr(crate::perf::Counter::MeasureGridProbe);
                let child_size = self.measure_recursive(child_id, probe);
                max_natural = max_natural.max(child_size.width);
            }
            max_natural.max(1.0)
        };

        let child_constraints = Constraints {
            min_width: col_width,
            max_width: col_width,
            min_height: 0.0,
            max_height: f32::INFINITY,
            containing_block: Size::new(col_width, constraints.containing_block.height),
        };

        let total_height = if masonry {
            let mut col_heights = vec![0.0f32; cols];
            for (i, &child_id) in children.iter().enumerate() {
                let col = i % cols;
                let child_size = self.measure_recursive(child_id, child_constraints);
                if col_heights[col] > 0.0 { col_heights[col] += row_gap; }
                col_heights[col] += child_size.height;
            }
            col_heights.iter().cloned().fold(0.0f32, f32::max)
        } else {
            let mut row_heights: Vec<f32> = Vec::new();
            let mut cumulative_y = 0.0f32;
            let mut measured_rows = 0usize;
            let mut measured_h_sum = 0.0f32;

            for (i, &child_id) in children.iter().enumerate() {
                crate::perf::incr(crate::perf::Counter::MeasureGridChild);
                let row = i / cols;
                let is_first_in_row = i % cols == 0;

                if is_first_in_row && row > 0 {
                    cumulative_y += row_heights[row - 1] + row_gap;
                }

                let avg_h = if measured_rows > 0 { measured_h_sum / measured_rows as f32 } else { 50.0 };
                if let Some(est) = self.scroll_estimate(child_id, cumulative_y, avg_h) {
                    crate::perf::incr(crate::perf::Counter::MeasureGridEstimated);
                    if row >= row_heights.len() {
                        row_heights.push(est.height);
                    } else {
                        row_heights[row] = row_heights[row].max(est.height);
                    }
                    continue;
                }

                let child_size = self.measure_recursive(child_id, child_constraints);
                if row >= row_heights.len() {
                    row_heights.push(child_size.height);
                    measured_rows += 1;
                    measured_h_sum += child_size.height;
                } else {
                    row_heights[row] = row_heights[row].max(child_size.height);
                }
            }

            let total_row_gap = if row_heights.len() > 1 { row_gap * (row_heights.len() - 1) as f32 } else { 0.0 };
            row_heights.iter().sum::<f32>() + total_row_gap
        };

        let grid_width = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            col_width * cols as f32 + total_col_gap
        };

        let final_size = Size::new(
            grid_width,
            total_height.min(constraints.max_height),
        );

        if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(Constraints {
                min_width: final_size.width,
                max_width: final_size.width,
                min_height: final_size.height,
                max_height: final_size.height,
                containing_block: final_size,
            });
        }

        layout_log!(self,"Grid: {}cols, {}masonry, size={:.1}x{:.1}",
            cols, if masonry { "masonry" } else { "standard" }, final_size.width, final_size.height);

        final_size
    }

    pub(super) fn measure_horizontal_pages(&mut self, children: &[ElementId], constraints: Constraints, id: ElementId) -> Size {
        let child_constraints = Constraints {
            min_width: 0.0,
            max_width: constraints.max_width,
            min_height: 0.0,
            max_height: constraints.max_height,
            containing_block: constraints.containing_block,
        };

        let mut max_height = 0.0f32;
        for &child_id in children {
            let child_size = self.measure_recursive(child_id, child_constraints);
            max_height = max_height.max(child_size.height);
        }

        let element_size = if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(constraints)
        } else {
            Size::zero()
        };

        let width = element_size.width.min(constraints.max_width);
        let height = element_size.height.max(max_height).min(constraints.max_height);

        layout_log!(self,"HorizontalPages: {} pages, size={:.1}x{:.1}",
            children.len(), width, height);

        Size::new(width, height)
    }

    pub(super) fn measure_split(&mut self, children: &[ElementId], constraints: Constraints, horizontal: bool, ratio: f32, divider: f32, id: ElementId) -> Size {
        let _ = if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(constraints)
        } else {
            Size::zero()
        };

        let total_w = if constraints.max_width.is_finite() { constraints.max_width } else { 400.0 };
        let total_h = if constraints.max_height.is_finite() { constraints.max_height } else { 300.0 };

        if children.len() >= 2 {
            if horizontal {
                let avail = (total_w - divider).max(0.0);
                let first_w = avail * ratio;
                let second_w = avail - first_w;
                let c1 = Constraints { min_width: first_w, max_width: first_w, min_height: total_h, max_height: total_h, containing_block: Size::new(first_w, total_h) };
                let c2 = Constraints { min_width: second_w, max_width: second_w, min_height: total_h, max_height: total_h, containing_block: Size::new(second_w, total_h) };
                self.measure_recursive(children[0], c1);
                self.measure_recursive(children[1], c2);
            } else {
                let avail = (total_h - divider).max(0.0);
                let first_h = avail * ratio;
                let second_h = avail - first_h;
                let c1 = Constraints { min_width: total_w, max_width: total_w, min_height: first_h, max_height: first_h, containing_block: Size::new(total_w, first_h) };
                let c2 = Constraints { min_width: total_w, max_width: total_w, min_height: second_h, max_height: second_h, containing_block: Size::new(total_w, second_h) };
                self.measure_recursive(children[0], c1);
                self.measure_recursive(children[1], c2);
            }
        }

        Size::new(total_w, total_h)
    }

    pub(super) fn measure_scroll(&mut self, children: &[ElementId], constraints: Constraints, left: f32, top: f32, right: f32, bottom: f32, unbounded_width: bool, unbounded_height: bool, id: ElementId) -> Size {
        let vp = self.viewport_size;
        if let Some(node) = self.elements.get_mut(&id) {
            node.element.set_viewport_size(vp);
        }

        let scroll_cb_w = if unbounded_width {
            constraints.containing_block.width
        } else {
            (constraints.max_width - left - right).max(0.0)
        };
        let scroll_cb_h = if unbounded_height {
            constraints.containing_block.height
        } else {
            (constraints.max_height - top - bottom).max(0.0)
        };
        let child_constraints = Constraints {
            min_width: 0.0,
            max_width: if unbounded_width {
                f32::INFINITY
            } else {
                (constraints.max_width - left - right).max(0.0)
            },
            min_height: 0.0,
            max_height: if unbounded_height {
                f32::INFINITY
            } else {
                (constraints.max_height - top - bottom).max(0.0)
            },
            containing_block: Size::new(scroll_cb_w, scroll_cb_h),
        };

        let viewport_h = constraints.max_height;
        let pushed_cull = if unbounded_height && viewport_h.is_finite() && viewport_h > 0.0 {
            let scroll_y = self.elements.get(&id)
                .map(|n| n.element.scroll_offset().y)
                .unwrap_or(0.0);
            self.scroll_cull_stack.push(ScrollCullContext {
                viewport_height: viewport_h,
                scroll_offset_y: scroll_y,
            });
            true
        } else {
            false
        };

        let mut content_width = 0.0f32;
        let mut content_height = 0.0f32;
        for &child_id in children {
            let cs = self.measure_recursive(child_id, child_constraints);
            content_width = content_width.max(cs.width);
            content_height = content_height.max(cs.height);
        }

        if pushed_cull {
            self.scroll_cull_stack.pop();
        }

        let _ = if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(constraints)
        } else {
            Size::zero()
        };

        let width = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            (content_width + left + right).max(constraints.min_width)
        };
        let height = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            (content_height + top + bottom).max(constraints.min_height)
        };

        if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(Constraints {
                min_width: width,
                max_width: width,
                min_height: height,
                max_height: height,
                containing_block: Size::new(width, height),
            });
            node.element.set_content_size(Size::new(content_width, content_height));
        }

        layout_log!(self,"Scroll: viewport={:.1}x{:.1} content={:.1}x{:.1}",
            width, height, content_width, content_height);

        Size::new(width, height)
    }

    pub(super) fn measure_positioned(&mut self, children: &[ElementId], constraints: Constraints, id: ElementId) -> Size {
        let (explicit_w, explicit_h) = if let Some(node) = self.elements.get(&id) {
            node.element.explicit_dimensions(
                constraints.containing_block.width,
                constraints.containing_block.height,
            )
        } else {
            (None, None)
        };

        let child_constraints = match (explicit_w, explicit_h) {
            (Some(w), Some(h)) => Constraints {
                min_width: w, max_width: w,
                min_height: h, max_height: h,
                containing_block: Size::new(w, h),
            },
            _ => Constraints {
                min_width: 0.0,
                max_width: explicit_w.unwrap_or(constraints.max_width),
                min_height: 0.0,
                max_height: explicit_h.unwrap_or(constraints.max_height),
                containing_block: Size::new(
                    explicit_w.unwrap_or(constraints.containing_block.width),
                    explicit_h.unwrap_or(constraints.containing_block.height),
                ),
            },
        };

        let mut child_size = Size::zero();
        for &cid in children {
            let cs = self.measure_recursive(cid, child_constraints);
            child_size.width = child_size.width.max(cs.width);
            child_size.height = child_size.height.max(cs.height);
        }

        let final_size = Size::new(
            explicit_w.unwrap_or(child_size.width),
            explicit_h.unwrap_or(child_size.height),
        );
        let tight = Constraints {
            min_width: final_size.width, max_width: final_size.width,
            min_height: final_size.height, max_height: final_size.height,
            containing_block: final_size,
        };
        if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(tight);
            node.refresh_hint_cache();
        }

        layout_log!(self, "Positioned: child={:.1}x{:.1}", child_size.width, child_size.height);
        Size::new(
            final_size.width.min(constraints.max_width),
            final_size.height.min(constraints.max_height),
        )
    }

    pub(super) fn measure_pan_zoom(&mut self, children: &[ElementId], constraints: Constraints, id: ElementId) -> Size {
        let viewport_w = if constraints.max_width.is_finite() { constraints.max_width } else { 800.0 };
        let viewport_h = if constraints.max_height.is_finite() { constraints.max_height } else { 600.0 };
        let viewport_size = Size::new(viewport_w, viewport_h);

        let child_constraints = Constraints {
            min_width: 0.0,
            max_width: viewport_w,
            min_height: 0.0,
            max_height: viewport_h,
            containing_block: viewport_size,
        };

        let mut content = Size::zero();
        for &cid in children {
            let cs = self.measure_recursive(cid, child_constraints);
            content.width = content.width.max(cs.width);
            content.height = content.height.max(cs.height);
        }

        let tight = Constraints {
            min_width: viewport_w, max_width: viewport_w,
            min_height: viewport_h, max_height: viewport_h,
            containing_block: viewport_size,
        };
        if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(tight);
            node.element.set_content_size(content);
        }

        layout_log!(self, "PanZoom: viewport={:.1}x{:.1} content={:.1}x{:.1}",
            viewport_w, viewport_h, content.width, content.height);
        viewport_size
    }

    pub(super) fn measure_animated_size(&mut self, children: &[ElementId], constraints: Constraints, id: ElementId) -> Size {
        let mut child_size = Size::zero();
        for &child_id in children {
            let cs = self.measure_recursive(child_id, constraints);
            child_size.width = child_size.width.max(cs.width);
            child_size.height = child_size.height.max(cs.height);
        }

        if let Some(node) = self.elements.get_mut(&id) {
            node.element.set_content_size(child_size);
        }

        let animated_size = if let Some(node) = self.elements.get_mut(&id) {
            node.element.layout(constraints)
        } else {
            child_size
        };

        let result = clamp_finite(animated_size, constraints);
        layout_log!(self,"AnimatedSize: target={:.1}x{:.1} animated={:.1}x{:.1}",
            child_size.width, child_size.height, result.width, result.height);
        result
    }
}
