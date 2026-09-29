use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, UpdateContext, Widget,
};
use std::any::Any;

thread_local! {
    static SCROLL_REQUESTS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Прокрутить ближайшую прокрутку так, чтобы элемент `Named::new(name, ..)`
/// встал к её началу (плавно). Выполняется на следующем кадре — после
/// раскладки; удобно для алфавитных указателей («A», «Б»…) и якорей.
pub fn scroll_to_named(name: impl Into<String>) {
    SCROLL_REQUESTS.with(|r| r.borrow_mut().push(name.into()));
}

pub(crate) fn take_scroll_requests() -> Vec<String> {
    SCROLL_REQUESTS.with(|r| std::mem::take(&mut *r.borrow_mut()))
}

/// Имена, которых нет в этом дереве, ждут другого окна или следующего кадра
/// (не дольше пары кадров — чтобы не копились).
pub(crate) fn return_scroll_requests(mut left: Vec<String>) {
    if left.is_empty() {
        return;
    }
    SCROLL_REQUESTS.with(|r| {
        let mut r = r.borrow_mut();
        left.truncate(8);
        r.extend(left);
        let n = r.len();
        if n > 16 {
            r.drain(..n - 16);
        }
    });
}

pub struct Named {
    name: String,
    child: Box<dyn Widget>,
}

impl Named {
    pub fn new(name: impl Into<String>, child: impl Widget + 'static) -> Self {
        Self {
            name: name.into(),
            child: Box::new(child),
        }
    }
}

impl Widget for Named {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(NamedElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
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
        tree.set_debug_name(parent_id, self.name.clone());

        let element = self.child.create_element();
        let child_id =
            tree.insert_with_type_id(element, Some(parent_id), self.child.as_any().type_id());
        self.child.mount(tree, child_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![self.child.as_ref() as &dyn Widget]
    }
}

struct NamedElement {
    id: ElementId,
    bounds: Rect,
    dirty_flags: DirtyFlags,
}

impl Element for NamedElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(named) = widget.as_any().downcast_ref::<Named>() {
            let _ = named;
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        Size::new(constraints.max_width, constraints.max_height)
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn mount(&mut self, _tree: &mut crate::widget::ElementTree) {}

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
        "Named"
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHarness;
    use crate::widgets::{Column, ScrollView};

    #[test]
    fn scroll_to_named_glides_to_start() {
        let mut col = Column::new();
        for i in 0..20 {
            col = col.child(Named::new(format!("row{i}"), Column::new().height(100.0)));
        }
        let mut h = TestHarness::new(Box::new(ScrollView::new().vertical().child(col)));
        h.layout(300.0, 400.0);
        scroll_to_named("row10");
        h.layout(300.0, 400.0);
        for _ in 0..60 {
            h.animate(std::time::Duration::from_millis(16));
        }
        let sv = h.find_by_type_name("ScrollView")[0];
        let y = h.tree.get(sv).unwrap().scroll_offset().y;
        assert!((y - 1000.0).abs() < 1.0, "{y}");
    }
}
