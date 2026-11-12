//! `ClickAway` — нажатие в любом месте окна мимо содержимого (шапка, вкладки, боковой список,
//! заголовок окна) вызывает обработчик: закрыть выпадающее меню или панель, нарисованную в общем
//! дереве (не через [`Portal`](crate::widgets::Portal)). Само нажатие идёт дальше как обычно.
//!
//! ```ignore
//! ClickAway::new(menu).on_away(move || menu_open.set(false))
//! ```

use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;

pub struct ClickAway {
    child: Box<dyn Widget>,
    on_away: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ClickAway {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self { child: Box::new(child), on_away: None }
    }

    /// Нажатие мимо содержимого.
    pub fn on_away(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_away = Some(Arc::new(f));
        self
    }
}

impl Widget for ClickAway {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ClickAwayElement { id: ElementId::new(), bounds: Rect::zero(), dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER })
    }

    /// Обработчик живёт в реестре дерева с монтирования: новый виджет — новый элемент.
    fn can_update(&self, _other: &dyn Any) -> bool {
        false
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        if let Some(f) = &self.on_away {
            tree.add_click_away(parent_id, f.clone());
        }
        let element = self.child.create_element();
        let child_id = tree.insert_with_type_id(element, Some(parent_id), self.child.as_any().type_id());
        self.child.mount(tree, child_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![self.child.as_ref() as &dyn Widget]
    }
}

struct ClickAwayElement {
    id: ElementId,
    bounds: Rect,
    dirty_flags: DirtyFlags,
}

impl Element for ClickAwayElement {
    fn update(&mut self, _widget: &dyn Widget, _ctx: &mut UpdateContext) {}

    fn layout(&mut self, constraints: Constraints) -> Size {
        Size::new(constraints.max_width, constraints.max_height)
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    /// Своих размеров обёртка не хранит — попадание решает содержимое.
    fn passthrough_hit_test(&self) -> bool {
        true
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, origin: Point) {
        self.bounds.origin = origin;
    }

    fn id(&self) -> ElementId {
        self.id
    }

    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }

    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags |= flags;
    }

    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags.remove(flags);
    }

    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty_flags.intersects(flags)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Loose
    }

    fn element_type_name(&self) -> &str {
        "ClickAway"
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHarness;
    use crate::widgets::{Button, Column};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn fires_only_outside_and_click_goes_on() {
        let away = Arc::new(AtomicUsize::new(0));
        let clicks = Arc::new(AtomicUsize::new(0));
        let (a2, c2) = (away.clone(), clicks.clone());
        let other = Button::new("tab").on_click(move || {
            c2.fetch_add(1, Ordering::SeqCst);
        });
        let menu = Button::new("item");
        let root = Column::new().child(other).child(ClickAway::new(menu).on_away(move || {
            a2.fetch_add(1, Ordering::SeqCst);
        }));
        let mut h = TestHarness::new(Box::new(root));
        h.layout(300.0, 300.0);
        let b = h.find_by_type_name("Button");
        let center = |r: Rect| Point::new(r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0);
        let item = h.tree.get(b[1]).unwrap().bounds();
        h.send_events(&crate::testing::click_at(center(item)));
        assert_eq!(away.load(Ordering::SeqCst), 0, "щелчок по меню — не мимо");
        let tab = h.tree.get(b[0]).unwrap().bounds();
        h.send_events(&crate::testing::click_at(center(tab)));
        assert_eq!(away.load(Ordering::SeqCst), 1, "щелчок мимо меню");
        assert_eq!(clicks.load(Ordering::SeqCst), 1, "щелчок дошёл до вкладки");
    }
}
