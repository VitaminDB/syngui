use crate::core::{Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use super::IntoWidget;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext,
    Widget,
};
use crate::widget::context::{EventContext, EventContextExt};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

type ClickCb = Arc<Mutex<dyn FnMut() + Send>>;
type ClickAtCb = Arc<Mutex<dyn FnMut(Point) + Send>>;
type ClickBoundsCb = Arc<Mutex<dyn FnMut(Point, Rect) + Send>>;
type HoverCb = Arc<Mutex<dyn FnMut(bool) + Send>>;
type MouseBtnCb = Arc<Mutex<dyn FnMut(Point) + Send>>;
type BackCb = Arc<Mutex<dyn FnMut() -> bool + Send>>;

pub struct GestureDetector {
    child: Option<Box<dyn Widget>>,
    on_click: Option<ClickCb>,
    on_click_at: Option<ClickAtCb>,
    on_click_with_bounds: Option<ClickBoundsCb>,
    on_double_click: Option<ClickCb>,
    on_hover_change: Option<HoverCb>,
    on_mouse_down: Option<MouseBtnCb>,
    on_mouse_up: Option<MouseBtnCb>,
    on_back: Option<BackCb>,
    cursor: CursorIcon,
    classes: Vec<String>,
}

impl GestureDetector {
    pub fn new() -> Self {
        Self {
            child: None,
            on_click: None,
            on_click_at: None,
            on_click_with_bounds: None,
            on_double_click: None,
            on_hover_change: None,
            on_mouse_down: None,
            on_mouse_up: None,
            on_back: None,
            cursor: CursorIcon::Pointer,
            classes: Vec::new(),
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn on_click(mut self, cb: impl FnMut() + Send + 'static) -> Self {
        self.on_click = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_click_at(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_click_at = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_click_with_bounds(mut self, cb: impl FnMut(Point, Rect) + Send + 'static) -> Self {
        self.on_click_with_bounds = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_double_click(mut self, cb: impl FnMut() + Send + 'static) -> Self {
        self.on_double_click = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_hover_change(mut self, cb: impl FnMut(bool) + Send + 'static) -> Self {
        self.on_hover_change = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_mouse_down(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_mouse_down = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_mouse_up(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_mouse_up = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_back(mut self, cb: impl FnMut() -> bool + Send + 'static) -> Self {
        self.on_back = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn cursor(mut self, cursor: CursorIcon) -> Self {
        self.cursor = cursor;
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Default for GestureDetector {
    fn default() -> Self { Self::new() }
}

impl Widget for GestureDetector {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(GestureDetectorElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            hovered: false,
            pressed: false,
            on_click: self.on_click.clone(),
            on_click_at: self.on_click_at.clone(),
            on_click_with_bounds: self.on_click_with_bounds.clone(),
            on_double_click: self.on_double_click.clone(),
            on_hover_change: self.on_hover_change.clone(),
            on_mouse_down: self.on_mouse_down.clone(),
            on_mouse_up: self.on_mouse_up.clone(),
            on_back: self.on_back.clone(),
            cursor: self.cursor,
            child_id: None,
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        if let Some(child) = &self.child {
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.as_ref().map(|c| vec![c.as_ref() as &dyn Widget]).unwrap_or_default()
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct GestureDetectorElement {
    id: ElementId,
    bounds: Rect,
    hovered: bool,
    pressed: bool,
    on_click: Option<ClickCb>,
    on_click_at: Option<ClickAtCb>,
    on_click_with_bounds: Option<ClickBoundsCb>,
    on_double_click: Option<ClickCb>,
    on_hover_change: Option<HoverCb>,
    on_mouse_down: Option<MouseBtnCb>,
    on_mouse_up: Option<MouseBtnCb>,
    on_back: Option<BackCb>,
    cursor: CursorIcon,
    child_id: Option<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for GestureDetectorElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(gd) = widget.as_any().downcast_ref::<GestureDetector>() {
            self.on_click = gd.on_click.clone();
            self.on_click_at = gd.on_click_at.clone();
            self.on_click_with_bounds = gd.on_click_with_bounds.clone();
            self.on_double_click = gd.on_double_click.clone();
            self.on_hover_change = gd.on_hover_change.clone();
            self.on_mouse_down = gd.on_mouse_down.clone();
            self.on_mouse_up = gd.on_mouse_up.clone();
            self.on_back = gd.on_back.clone();
            self.cursor = gd.cursor;
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 };
        let size = Size::new(w, h);
        self.bounds = Rect::new(Point::zero(), size);
        size
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                let inside = self.bounds.contains(*pos);
                if inside != self.hovered {
                    self.hovered = inside;
                    if let Some(ref cb) = self.on_hover_change {
                        if let Ok(mut f) = cb.lock() { f(inside); }
                    }
                    ctx.request_paint();
                }
                if inside {
                    ctx.set_cursor(self.cursor);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    self.pressed = true;
                    if let Some(ref cb) = self.on_mouse_down {
                        if let Ok(mut f) = cb.lock() { f(*position); }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, position } => {
                if *button == MouseButton::Left && self.pressed {
                    self.pressed = false;
                    if let Some(ref cb) = self.on_mouse_up {
                        if let Ok(mut f) = cb.lock() { f(*position); }
                    }
                    if self.bounds.contains(*position) {
                        if let Some(ref cb) = self.on_click {
                            if let Ok(mut f) = cb.lock() { f(); }
                        }
                        if let Some(ref cb) = self.on_click_at {
                            if let Ok(mut f) = cb.lock() { f(*position); }
                        }
                        if let Some(ref cb) = self.on_click_with_bounds {
                            if let Ok(mut f) = cb.lock() { f(*position, self.bounds); }
                        }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::DoubleClick { position, .. } => {
                if self.bounds.contains(*position) {
                    if let Some(ref cb) = self.on_double_click {
                        if let Ok(mut f) = cb.lock() { f(); }
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::BackPressed => {
                if let Some(ref cb) = self.on_back {
                    if let Ok(mut f) = cb.lock() {
                        if f() {
                            return EventResult::Handled;
                        }
                    }
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] {
        match self.child_id {
            Some(ref id) => std::slice::from_ref(id),
            None => &[],
        }
    }

    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }

    fn mount(&mut self, tree: &mut ElementTree) {
        if let Some(node) = tree.elements.get(&self.id) {
            if let Some(first) = node.children.first() {
                self.child_id = Some(*first);
            }
        }
    }

    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "GestureDetector" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(c) = self.mss.cursor {
            self.cursor = c;
        }
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

impl StyledElement for GestureDetectorElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {}
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; }
}
