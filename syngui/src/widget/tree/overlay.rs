use super::{ElementId, ElementTree, OverlayEntry};
use crate::core::Rect;
use std::time::Duration;

impl ElementTree {
    /// Зарегистрировать overlay: `bounds` — в координатах элемента.
    pub fn register_overlay(&mut self, element_id: ElementId, bounds: Rect, modal: bool) {
        self.overlay_stack.retain(|e| e.element_id != element_id);
        self.overlay_stack.push(OverlayEntry {
            element_id,
            bounds: self.overlay_window_rect(element_id, bounds),
            local: bounds,
            modal,
            declarative: false,
        });
    }

    /// Пересчитать оконные границы оверлеев по текущей прокрутке предков.
    /// Без этого список, открытый в окне, которое потом прокрутили колесом,
    /// ловил клики на старом месте, а по видимым пунктам они уходили в
    /// виджеты под ним.
    pub(crate) fn refresh_overlay_bounds(&mut self) {
        for i in 0..self.overlay_stack.len() {
            if self.overlay_stack[i].declarative {
                continue;
            }
            let (id, local) = (self.overlay_stack[i].element_id, self.overlay_stack[i].local);
            self.overlay_stack[i].bounds = self.overlay_window_rect(id, local);
        }
    }

    /// Прямоугольник в координатах элемента → координаты окна.
    pub(crate) fn overlay_window_rect(&self, element_id: ElementId, local: Rect) -> Rect {
        let (s, k) = self.accumulated_event_transform(element_id);
        Rect::new(
            crate::core::Point::new(k * local.origin.x - s.x, k * local.origin.y - s.y),
            crate::core::Size::new(k * local.size.width, k * local.size.height),
        )
    }

    pub fn unregister_overlay(&mut self, element_id: ElementId) {
        self.overlay_stack.retain(|e| e.element_id != element_id);
    }

    /// Тикает анимации. Обход — только по реестру `animation_registry`:
    /// все точки старта анимаций регистрируют элемент (каскад стилей, диспетчер
    /// событий, измерение/позиционирование, scroll-into-view — см.
    /// `note_animation_started` / `sync_registries_for`), а закончившиеся
    /// элементы обход снимает сам. Раньше обходились ВСЕ элементы дерева.
    pub fn animate(&mut self, _root_id: ElementId, dt: Duration) -> bool {
        let _t = web_time::Instant::now();
        let ids: Vec<ElementId> = self.animation_registry.iter().copied().collect();
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
            }
            // Перерисовать нужно и когда анимация закончилась именно на этом
            // тике: последний кадр показывал промежуточное значение, а
            // конечное без кадра так и осталось бы ненарисованным.
            if animated || was_repainting {
                needs_repaint = true;
            }
            let needs_rebuild_now = self
                .elements
                .get(&id)
                .map(|n| n.element.needs_rebuild())
                .unwrap_or(false);
            if needs_rebuild_now {
                self.rebuild_registry.insert(id);
                needs_repaint = true;
            }
            let keep = self
                .elements
                .get(&id)
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
