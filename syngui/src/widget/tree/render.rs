use crate::core::{Color, Point, Rect, Size};
use crate::render::{Border, DisplayList};
use crate::widget::LayoutHint;
use super::{ElementId, ElementTree};

impl ElementTree {
    pub fn build_display_list(&self, root_id: ElementId, list: &mut DisplayList, clip: Rect) {
        let Some(idx) = self.elements.resolve(root_id) else { return };
        if let Some(node) = self.elements.get_by_idx(idx) {
            let do_clip = node.element.clip_content();
            node.element.build_display_list(list, clip);

            let child_clip = if do_clip {
                let corner_radius = node.element.clip_corner_radius();
                if corner_radius == [0.0; 4] {
                    list.push_clip(node.element.bounds());
                } else {
                    list.push_clip_rounded(node.element.bounds(), corner_radius);
                }
                clip.intersection(&node.element.bounds()).unwrap_or(Rect::zero())
            } else {
                clip
            };

            for &child_idx in &node.children_idx {
                self.build_display_list_recursive_by_idx(child_idx, list, child_clip, child_clip);
            }

            if do_clip {
                list.pop_clip();
            }

            node.element.post_build_display_list(list, clip);
        }
    }

    fn build_display_list_recursive_by_idx(&self, idx: u32, list: &mut DisplayList, clip: Rect, cull_clip: Rect) {
        crate::perf::incr(crate::perf::Counter::DlVisit);
        if let Some(node) = self.elements.get_by_idx(idx) {
            if !node.element.is_visible() {
                crate::perf::incr(crate::perf::Counter::DlInvisibleSkip);
                return;
            }

            let pushed_transform = node.element.mss()
                .and_then(|m| m.compute_active_transform(node.element.bounds()));
            if let Some(t) = pushed_transform {
                list.push_transform(t);
            }

            let do_clip = node.element.clip_content();
            let hint = &node.hint_cache;
            let is_tooltip = matches!(hint, LayoutHint::Tooltip { .. });
            let is_portal = matches!(hint, LayoutHint::Portal { .. });
            if !is_tooltip {
                node.element.build_display_list(list, clip);
            }

            let child_clip = if do_clip {
                let corner_radius = node.element.clip_corner_radius();
                if corner_radius == [0.0; 4] {
                    list.push_clip(node.element.bounds());
                } else {
                    list.push_clip_rounded(node.element.bounds(), corner_radius);
                }
                clip.intersection(&node.element.bounds()).unwrap_or(Rect::zero())
            } else {
                clip
            };

            let scroll = node.element.scroll_offset();
            let child_cull = if do_clip {
                let base = cull_clip.intersection(&node.element.bounds()).unwrap_or(Rect::zero());
                if scroll.x != 0.0 || scroll.y != 0.0 {
                    Rect::new(
                        Point::new(base.origin.x + scroll.x, base.origin.y + scroll.y),
                        base.size,
                    )
                } else {
                    base
                }
            } else if scroll.x != 0.0 || scroll.y != 0.0 {
                Rect::new(
                    Point::new(cull_clip.origin.x + scroll.x, cull_clip.origin.y + scroll.y),
                    cull_clip.size,
                )
            } else {
                cull_clip
            };

            let is_stack = matches!(hint, LayoutHint::Stack);
            let active_count = node.element.active_child_count();

            let mut visible_buf: Vec<usize> = Vec::new();
            let can_use_filter = !is_tooltip && !is_portal && !is_stack
                && child_cull.size.width > 0.0 && child_cull.size.height > 0.0;
            let uses_filter = can_use_filter && node.element.visible_child_indices(child_cull, &mut visible_buf);

            if uses_filter {
                let visible_count = visible_buf.len();
                let children_total = node.children_idx.len();
                let effective = active_count.min(children_total);
                crate::perf::add(crate::perf::Counter::DlCulled, effective.saturating_sub(visible_count) as u64);
                for &i in &visible_buf {
                    if i >= effective { continue; }
                    let Some(&child_idx) = node.children_idx.get(i) else { continue };
                    self.build_display_list_recursive_by_idx(child_idx, list, child_clip, child_cull);
                }
            } else {
                let mut child_i = 0usize;
                for &child_idx in node.children_idx.iter().take(active_count) {
                    let is_tooltip_content = is_tooltip && child_i >= 1;
                    if !is_tooltip_content && !is_portal && child_cull.size.width > 0.0 && child_cull.size.height > 0.0 {
                        if let Some(child_node) = self.elements.get_by_idx(child_idx) {
                            let cb = child_node.element.bounds();
                            if cb.size.width > 0.0 && cb.size.height > 0.0 && !cb.intersects(&child_cull) {
                                crate::perf::incr(crate::perf::Counter::DlCulled);
                                child_i += 1;
                                continue;
                            }
                        }
                    }
                    if is_stack && child_i > 0 {
                        list.push_z_barrier();
                    }
                    if child_i == 1 && is_tooltip {
                        node.element.build_display_list(list, clip);
                    }
                    if is_tooltip_content {
                        if let Some(child_node) = self.elements.get_by_idx(child_idx) {
                            let content_clip = child_node.element.bounds();
                            self.build_display_list_recursive_by_idx(child_idx, list, content_clip, content_clip);
                        }
                    } else if is_portal {
                        if let Some(child_node) = self.elements.get_by_idx(child_idx) {
                            let content_clip = child_node.element.bounds();
                            self.build_display_list_recursive_by_idx(child_idx, list, content_clip, content_clip);
                        }
                    } else {
                        self.build_display_list_recursive_by_idx(child_idx, list, child_clip, child_cull);
                    }
                    child_i += 1;
                }
            }

            if do_clip {
                list.pop_clip();
            }

            node.element.post_build_display_list(list, clip);

            if pushed_transform.is_some() {
                list.pop_transform();
            }
        }
    }

    pub fn build_drag_overlay(&self, list: &mut DisplayList) {
        let drag = match &self.drag_state {
            Some(d) => d,
            None => return,
        };

        list.begin_overlay_absolute();

        let display_text = drag.data.label.as_deref().unwrap_or(&drag.data.payload);
        let font_size = 14.0_f32;

        let text_width = self.text_measure.as_ref()
            .map(|tm| tm.measure_text_width(display_text, font_size, display_text.chars().count()))
            .unwrap_or(display_text.len() as f32 * font_size * 0.6);
        let min_width = (text_width + 24.0).max(drag.source_bounds.size.width);
        let size = Size::new(min_width, drag.source_bounds.size.height.max(32.0));

        let ghost_origin = Point::new(
            drag.current_pos.x - drag.drag_offset.x,
            drag.current_pos.y - drag.drag_offset.y,
        );
        let ghost_rect = Rect::new(ghost_origin, size);

        let bg = Color::from_hex("#2B2D31");
        let border_color = Color::from_hex("#00B4D8");
        let text_color = Color::from_hex("#F2F3F5");
        let shadow_color = Color::new(0.0, 0.0, 0.0, 0.4);

        list.push_opacity(0.85);

        let shadow_rect = Rect::new(
            Point::new(ghost_origin.x + 2.0, ghost_origin.y + 2.0),
            size,
        );
        list.push_rect(shadow_rect, shadow_color, [8.0; 4]);

        list.push_rect_bordered(ghost_rect, bg, [8.0; 4], Border { width: 2.0, color: border_color });

        let text_rect = Rect::new(
            Point::new(ghost_rect.origin.x + 12.0, ghost_rect.origin.y),
            Size::new(ghost_rect.size.width - 24.0, ghost_rect.size.height),
        );
        list.push_text(display_text, text_rect, text_color, font_size);

        list.pop_opacity();
        list.end_overlay();
    }
}
