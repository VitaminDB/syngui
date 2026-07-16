use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, UpdateContext, Widget};
use crate::widget::context::EventContext;
use std::any::Any;

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
        let child_id = tree.insert_with_type_id(element, Some(parent_id), self.child.as_any().type_id());
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

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn mount(&mut self, _tree: &mut crate::widget::ElementTree) {
    }

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
