use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::render::DisplayList;
use crate::signal;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;

pub struct WidgetMarker;

pub struct ReactiveMarker;

pub trait IntoWidget<Marker> {
    fn into_widget(self) -> Box<dyn Widget>;
}

impl<W: Widget + 'static> IntoWidget<WidgetMarker> for W {
    fn into_widget(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl<F, W> IntoWidget<ReactiveMarker> for F
where
    F: Fn() -> W + Send + Sync + 'static,
    W: Widget + 'static,
{
    fn into_widget(self) -> Box<dyn Widget> {
        Box::new(Reactive::new(move || vec![Box::new(self()) as Box<dyn Widget>]))
    }
}

pub struct Reactive {
    builder: Arc<dyn Fn() -> Vec<Box<dyn Widget>> + Send + Sync>,
}

impl Reactive {
    pub fn new(builder: impl Fn() -> Vec<Box<dyn Widget>> + Send + Sync + 'static) -> Self {
        Self {
            builder: Arc::new(builder),
        }
    }
}

impl Widget for Reactive {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ReactiveElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            child_ids: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            builder: self.builder.clone(),
            mounted: false,
            needs_child_rebuild: false,
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
        let _ = (tree, parent_id);
    }
}

struct ReactiveElement {
    id: ElementId,
    bounds: Rect,
    child_ids: Vec<ElementId>,
    dirty_flags: DirtyFlags,
    builder: Arc<dyn Fn() -> Vec<Box<dyn Widget>> + Send + Sync>,
    mounted: bool,
    needs_child_rebuild: bool,
}

impl Element for ReactiveElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(reactive) = widget.as_any().downcast_ref::<Reactive>() {
            if !Arc::ptr_eq(&self.builder, &reactive.builder) {
                self.builder = reactive.builder.clone();
                self.needs_child_rebuild = true;
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = constraints.min_width.max(0.0);
        let h = constraints.min_height.max(0.0);
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Loose
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn passthrough_hit_test(&self) -> bool {
        true
    }

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

    fn element_type_name(&self) -> &str { "Reactive" }

    fn manages_own_children(&self) -> bool { true }

    fn needs_rebuild(&self) -> bool {
        !self.mounted || self.needs_child_rebuild || signal::is_element_dirty(self.id)
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        signal::begin_tracking(self.id);
        signal::begin_element_scope(self.id);
        let children = (self.builder)();
        signal::end_element_scope();
        signal::end_tracking();
        children
    }

    fn clear_rebuild(&mut self) {
        self.mounted = true;
        self.needs_child_rebuild = false;
        signal::clear_element_dirty(self.id);
    }
}

impl Drop for ReactiveElement {
    fn drop(&mut self) {
        signal::cleanup_element(self.id);
    }
}
