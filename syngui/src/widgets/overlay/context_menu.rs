use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use crate::widgets::containers::IntoWidget;
use super::menu::{PopupMenu, MenuItem};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;
use crate::signal::{RwSignal, use_signal};

pub struct ContextMenu {
    pub child: Option<Box<dyn Widget>>,
    pub menu_items: Vec<MenuItem>,
    pub on_select: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    menu_open: RwSignal<bool>,
    menu_pos: RwSignal<Point>,
    popup: PopupMenu,
}

impl ContextMenu {
    pub fn new() -> Self {
        let menu_open = use_signal(false);
        let menu_pos = use_signal(Point::zero());
        let popup = PopupMenu::new()
            .is_open(menu_open)
            .position(menu_pos);
        Self {
            child: None,
            menu_items: Vec::new(),
            on_select: None,
            menu_open,
            menu_pos,
            popup,
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn items(mut self, items: Vec<MenuItem>) -> Self {
        self.menu_items = items.clone();
        self.popup = self.popup.items(items);
        self
    }

    pub fn on_select(mut self, callback: impl FnMut(&str) + Send + 'static) -> Self {
        let cb = Arc::new(Mutex::new(callback));
        self.on_select = Some(cb.clone());
        self.popup = self.popup.on_select(move |id| {
            if let Ok(mut f) = cb.lock() { f(id); }
        });
        self
    }
}

impl Default for ContextMenu {
    fn default() -> Self { Self::new() }
}

impl Widget for ContextMenu {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ContextMenuElement {
            id: ElementId::new(),
            menu_items: self.menu_items.clone(),
            on_select: self.on_select.clone(),
            menu_open: self.menu_open,
            menu_pos: self.menu_pos,
            bounds: Rect::zero(),
            child_ids: Vec::new(),
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
            let child_id = tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
        let menu_element = self.popup.create_element();
        let menu_id = tree.insert_with_type_id(menu_element, Some(parent_id), self.popup.as_any().type_id());
        self.popup.mount(tree, menu_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        let mut out: Vec<&dyn Widget> = Vec::new();
        if let Some(c) = &self.child {
            out.push(c.as_ref());
        }
        out.push(&self.popup);
        out
    }
}

struct ContextMenuElement {
    id: ElementId,
    menu_items: Vec<MenuItem>,
    on_select: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    menu_open: RwSignal<bool>,
    menu_pos: RwSignal<Point>,
    bounds: Rect,
    child_ids: Vec<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for ContextMenuElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(cm) = widget.as_any().downcast_ref::<ContextMenu>() {
            self.menu_items = cm.menu_items.clone();
            self.on_select = cm.on_select.clone();
            self.menu_open = cm.menu_open;
            self.menu_pos = cm.menu_pos;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Right && self.bounds.contains(*position) {
                    self.menu_pos.set(*position);
                    self.menu_open.set(true);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &self.child_ids }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str { "ContextMenu" }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }

    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
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

impl StyledElement for ContextMenuElement {
    fn apply_style(&mut self, _style: &ComputedStyle) { self.mark_dirty(DirtyFlags::RENDER); }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
}
