use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::IntoWidget;
use std::any::Any;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowControlAction {
    Close,
    Minimize,
    ToggleMaximize,
}

pub struct WindowControl {
    pub action: WindowControlAction,
    pub child: Option<Box<dyn Widget>>,
}

impl WindowControl {
    pub fn new(action: WindowControlAction) -> Self {
        Self { action, child: None }
    }

    pub fn close() -> Self { Self::new(WindowControlAction::Close) }
    pub fn minimize() -> Self { Self::new(WindowControlAction::Minimize) }
    pub fn toggle_maximize() -> Self { Self::new(WindowControlAction::ToggleMaximize) }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }
}

impl Widget for WindowControl {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(WindowControlElement {
            id: ElementId::new(),
            action: self.action,
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        if let Some(child) = &self.child {
            let child_element = child.create_element();
            let child_id = tree.insert_with_type_id(
                child_element,
                Some(parent_id),
                child.as_any().type_id(),
            );
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.as_ref().map(|c| vec![c.as_ref() as &dyn Widget]).unwrap_or_default()
    }
}

struct WindowControlElement {
    id: ElementId,
    action: WindowControlAction,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for WindowControlElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<WindowControl>() {
            self.action = w.action;
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if let Event::MouseDown { button, position } = event {
            if *button == MouseButton::Left && self.bounds.contains(*position) {
                match self.action {
                    WindowControlAction::Close => ctx.close_window(),
                    WindowControlAction::Minimize => ctx.minimize_window(),
                    WindowControlAction::ToggleMaximize => ctx.toggle_maximize_window(),
                }
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }

    fn animate(&mut self, _dt: Duration) -> bool { false }
    fn needs_repaint(&self) -> bool { false }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn set_content_size(&mut self, size: Size) {
        self.bounds = Rect::new(self.bounds.origin, size);
    }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str { "WindowControl" }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }

    fn passthrough_hit_test(&self) -> bool { false }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }
}

impl StyledElement for WindowControlElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
