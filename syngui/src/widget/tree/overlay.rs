use crate::core::Rect;
use super::{ElementId, ElementTree, OverlayEntry};
use std::time::Duration;

impl ElementTree {
    pub fn register_overlay(&mut self, element_id: ElementId, bounds: Rect, modal: bool) {
        self.overlay_stack.retain(|e| e.element_id != element_id);
        self.overlay_stack.push(OverlayEntry { element_id, bounds, modal, declarative: false });
    }

    pub fn unregister_overlay(&mut self, element_id: ElementId) {
        self.overlay_stack.retain(|e| e.element_id != element_id);
    }

    pub fn animate(&mut self, _root_id: ElementId, dt: Duration) -> bool {
        let _t = web_time::Instant::now();
        let ids: Vec<ElementId> = self.elements.keys().copied().collect();
        let mut needs_repaint = false;
        let mut stale: Vec<ElementId> = Vec::new();

        for id in ids {
            crate::perf::incr(crate::perf::Counter::AnimateVisit);
            let node = match self.elements.get_mut(&id) {
                Some(n) => n,
                None => continue,
            };

            if !node.element.is_visible() {
                continue;
            }

            let was_repainting = node.element.needs_repaint();
            if was_repainting {
                crate::perf::incr(crate::perf::Counter::AnimateTicking);
            }
            let animated = node.element.animate(dt);
            if animated {
                crate::perf::incr(crate::perf::Counter::AnimateTrue);
                needs_repaint = true;
            }
            let needs_rebuild_now = self.elements.get(&id)
                .map(|n| n.element.needs_rebuild())
                .unwrap_or(false);
            if needs_rebuild_now {
                self.rebuild_registry.insert(id);
                needs_repaint = true;
            }
            let keep = self.elements.get(&id)
                .map(|n| n.element.needs_repaint() || n.element.wants_animate_tick())
                .unwrap_or(false);
            if keep {
                self.animation_registry.insert(id);
            } else if was_repainting || animated {
                stale.push(id);
            }
        }

        for id in stale {
            self.animation_registry.remove(&id);
        }

        crate::perf::add_time(crate::perf::TimeKind::Animate, _t.elapsed());
        needs_repaint
    }
}
