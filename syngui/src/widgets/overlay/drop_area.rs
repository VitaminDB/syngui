use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult, DragData};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use crate::widgets::containers::IntoWidget;
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

pub struct DropInfo {
    pub data: DragData,
    pub position: Point,
    pub local_position: Point,
}

pub struct DropArea {
    pub child: Option<Box<dyn Widget>>,
    pub accept_types: Vec<String>,
    pub on_drop: Option<Arc<Mutex<dyn FnMut(DragData) + Send>>>,
    pub on_drop_positioned: Option<Arc<Mutex<dyn FnMut(DropInfo) + Send>>>,
    pub on_drag_enter: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    pub on_drag_leave: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    pub placeholder: String,
}

impl DropArea {
    pub fn new() -> Self {
        Self {
            child: None,
            accept_types: Vec::new(),
            on_drop: None,
            on_drop_positioned: None,
            on_drag_enter: None,
            on_drag_leave: None,
            placeholder: "Drop here".to_string(),
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn accept_types(mut self, types: Vec<String>) -> Self {
        self.accept_types = types;
        self
    }

    pub fn on_drop(mut self, callback: impl FnMut(DragData) + Send + 'static) -> Self {
        self.on_drop = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn on_drop_positioned(mut self, callback: impl FnMut(DropInfo) + Send + 'static) -> Self {
        self.on_drop_positioned = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn on_drag_enter(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_drag_enter = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn on_drag_leave(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_drag_leave = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

}

impl Default for DropArea {
    fn default() -> Self { Self::new() }
}

impl Widget for DropArea {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(DropAreaElement {
            id: ElementId::new(),
            accept_types: self.accept_types.clone(),
            on_drop: self.on_drop.clone(),
            on_drop_positioned: self.on_drop_positioned.clone(),
            on_drag_enter: self.on_drag_enter.clone(),
            on_drag_leave: self.on_drag_leave.clone(),
            placeholder: self.placeholder.clone(),
            bounds: Rect::zero(),
            drag_over: false,
            dropped_items: Vec::new(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            has_child: self.child.is_some(),
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
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.as_ref().map(|c| vec![c.as_ref() as &dyn Widget]).unwrap_or_default()
    }
}

struct DropAreaElement {
    id: ElementId,
    accept_types: Vec<String>,
    on_drop: Option<Arc<Mutex<dyn FnMut(DragData) + Send>>>,
    on_drop_positioned: Option<Arc<Mutex<dyn FnMut(DropInfo) + Send>>>,
    on_drag_enter: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    on_drag_leave: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    placeholder: String,
    bounds: Rect,
    drag_over: bool,
    dropped_items: Vec<String>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    has_child: bool,
}

impl DropAreaElement {
    fn accepts(&self, drag_type: &str) -> bool {
        self.accept_types.is_empty() || self.accept_types.iter().any(|t| t == drag_type)
    }
}

impl Element for DropAreaElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(da) = widget.as_any().downcast_ref::<DropArea>() {
            self.accept_types = da.accept_types.clone();
            self.on_drop = da.on_drop.clone();
            self.on_drop_positioned = da.on_drop_positioned.clone();
            self.on_drag_enter = da.on_drag_enter.clone();
            self.on_drag_leave = da.on_drag_leave.clone();
            self.placeholder = da.placeholder.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 200.0 };
        let h = if self.has_child { 0.0 } else { 80.0_f32.min(constraints.max_height) };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.has_child {
            return;
        }

        let placeholder_color = self.mss.color.unwrap_or(Color::from_hex("#9CA3AF"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let primary_bg = self.mss.background_color.unwrap_or(Color::from_hex("#EFF6FF"));
        let item_text_color = self.mss.color.map(|c| c.darken(0.1)).unwrap_or(Color::from_hex("#374151"));

        if self.drag_over {
            list.push_rect_bordered(
                self.bounds,
                primary_bg,
                [8.0; 4],
                Border { width: 2.0, color: primary },
            );
        } else {
            let idle_border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
            list.push_rect_bordered(
                self.bounds,
                Color::new(0.0, 0.0, 0.0, 0.0),
                [8.0; 4],
                Border { width: 1.0, color: idle_border },
            );
        }

        if self.dropped_items.is_empty() {
            let text_rect = Rect::new(
                Point::new(self.bounds.x(), self.bounds.y()),
                self.bounds.size,
            );
            list.push_text_centered(&self.placeholder, text_rect, placeholder_color, 14.0);
        } else {
            let mut y = self.bounds.y() + 8.0;
            for item in &self.dropped_items {
                let item_rect = Rect::new(
                    Point::new(self.bounds.x() + 8.0, y),
                    Size::new(self.bounds.size.width - 16.0, 28.0),
                );
                list.push_rect(item_rect, primary.with_alpha(0.1), [4.0; 4]);
                let text_rect = Rect::new(
                    Point::new(item_rect.x() + 8.0, item_rect.y()),
                    Size::new(item_rect.size.width - 16.0, item_rect.size.height),
                );
                list.push_text(item, text_rect, item_text_color, 13.0);
                y += 32.0;
            }
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::DragEnter { data, .. } => {
                if self.accepts(&data.drag_type) {
                    self.drag_over = true;
                    if let Some(ref cb) = self.on_drag_enter {
                        if let Ok(mut f) = cb.lock() { f(); }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::DragLeave => {
                if self.drag_over {
                    self.drag_over = false;
                    if let Some(ref cb) = self.on_drag_leave {
                        if let Ok(mut f) = cb.lock() { f(); }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::Drop { position, data } => {
                if self.bounds.contains(*position) && self.accepts(&data.drag_type) {
                    self.drag_over = false;
                    if let Some(ref cb) = self.on_drag_leave {
                        if let Ok(mut f) = cb.lock() { f(); }
                    }
                    if let Some(ref cb) = self.on_drop_positioned {
                        let info = DropInfo {
                            data: data.clone(),
                            position: *position,
                            local_position: Point::new(
                                position.x - self.bounds.origin.x,
                                position.y - self.bounds.origin.y,
                            ),
                        };
                        if let Ok(mut f) = cb.lock() { f(info); }
                    } else if let Some(ref cb) = self.on_drop {
                        if let Ok(mut f) = cb.lock() { f(data.clone()); }
                    } else {
                        self.dropped_items.push(data.payload.clone());
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::DragMove { position, data } => {
                if self.bounds.contains(*position) && self.accepts(&data.drag_type) {
                    if !self.drag_over {
                        self.drag_over = true;
                        if let Some(ref cb) = self.on_drag_enter {
                            if let Ok(mut f) = cb.lock() { f(); }
                        }
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                } else if self.drag_over {
                    self.drag_over = false;
                    if let Some(ref cb) = self.on_drag_leave {
                        if let Ok(mut f) = cb.lock() { f(); }
                    }
                    ctx.request_paint();
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn set_content_size(&mut self, size: Size) { self.bounds = Rect::new(self.bounds.origin, size); }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, tree: &mut ElementTree) {
        tree.register_drop_target(self.id);
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }

    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "DropArea" }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
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

impl StyledElement for DropAreaElement {
    fn apply_style(&mut self, _style: &ComputedStyle) { self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT); }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
}
