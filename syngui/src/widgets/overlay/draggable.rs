use crate::core::{Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton, DragData};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use crate::widgets::containers::IntoWidget;
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;
use std::time::Duration;

pub struct Draggable {
    pub child: Option<Box<dyn Widget>>,
    pub drag_type: String,
    pub payload: String,
    pub threshold: f32,
    pub on_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    pub on_double_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    /// Перенос начался (порог пройден) — с границами источника.
    pub on_drag_start: Option<Arc<Mutex<dyn FnMut(Rect) + Send>>>,
    pub label: Option<String>,
}

impl Draggable {
    pub fn new(drag_type: impl Into<String>, payload: impl Into<String>) -> Self {
        Self {
            child: None,
            drag_type: drag_type.into(),
            payload: payload.into(),
            threshold: 5.0,
            on_click: None,
            on_double_click: None,
            on_drag_start: None,
            label: None,
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    pub fn on_click(mut self, cb: impl FnMut() + Send + 'static) -> Self {
        self.on_click = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_double_click(mut self, cb: impl FnMut() + Send + 'static) -> Self {
        self.on_double_click = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Перенос начался: порог пройден, `bounds` — границы источника.
    pub fn on_drag_start(mut self, cb: impl FnMut(Rect) + Send + 'static) -> Self {
        self.on_drag_start = Some(Arc::new(Mutex::new(cb)));
        self
    }
}

impl Widget for Draggable {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(DraggableElement {
            id: ElementId::new(),
            drag_type: self.drag_type.clone(),
            payload: self.payload.clone(),
            threshold: self.threshold,
            on_click: self.on_click.clone(),
            on_double_click: self.on_double_click.clone(),
            on_drag_start: self.on_drag_start.clone(),
            label: self.label.clone(),
            bounds: Rect::zero(),
            mouse_down_pos: None,
            drag_started: false,
            pending_click_time: None,
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
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.as_ref().map(|c| vec![c.as_ref() as &dyn Widget]).unwrap_or_default()
    }
}

struct DraggableElement {
    id: ElementId,
    drag_type: String,
    payload: String,
    threshold: f32,
    on_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    on_double_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    on_drag_start: Option<Arc<Mutex<dyn FnMut(Rect) + Send>>>,
    label: Option<String>,
    bounds: Rect,
    mouse_down_pos: Option<Point>,
    drag_started: bool,
    pending_click_time: Option<f32>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for DraggableElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(d) = widget.as_any().downcast_ref::<Draggable>() {
            self.drag_type = d.drag_type.clone();
            self.payload = d.payload.clone();
            self.threshold = d.threshold;
            self.on_click = d.on_click.clone();
            self.on_double_click = d.on_double_click.clone();
            self.on_drag_start = d.on_drag_start.clone();
            self.label = d.label.clone();
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
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    self.mouse_down_pos = Some(*position);
                    self.drag_started = false;
                    ctx.set_cursor(CursorIcon::Grabbing);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseMove(pos) => {
                if self.bounds.contains(*pos) && self.mouse_down_pos.is_none() {
                    ctx.set_cursor(CursorIcon::Grab);
                }
                if let Some(down_pos) = self.mouse_down_pos {
                    if !self.drag_started {
                        let dx = pos.x - down_pos.x;
                        let dy = pos.y - down_pos.y;
                        let distance = (dx * dx + dy * dy).sqrt();
                        if distance > self.threshold {
                            self.drag_started = true;
                            ctx.cursor_position = *pos;
                            let mut data = DragData::new(
                                self.drag_type.clone(),
                                self.payload.clone(),
                                self.id.0,
                            );
                            data.label = self.label.clone();
                            ctx.start_drag(data);
                            if let Some(ref cb) = self.on_drag_start {
                                if let Ok(mut f) = cb.lock() { f(self.bounds); }
                            }
                            return EventResult::Handled;
                        }
                    }
                }
                EventResult::Ignored
            }
            Event::MouseUp { .. } => {
                if self.mouse_down_pos.is_some() {
                    let was_dragging = self.drag_started;
                    self.mouse_down_pos = None;
                    self.drag_started = false;
                    if !was_dragging {
                        if self.on_double_click.is_some() {
                            self.pending_click_time = Some(crate::input::DOUBLE_CLICK_INTERVAL.as_secs_f32());
                        } else {
                            if let Some(ref cb) = self.on_click {
                                if let Ok(mut f) = cb.lock() { f(); }
                            }
                        }
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::DoubleClick { position, .. } => {
                if self.bounds.contains(*position) {
                    self.pending_click_time = None;
                    if let Some(ref cb) = self.on_double_click {
                        if let Ok(mut f) = cb.lock() { f(); }
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::DragEnd { .. } => {
                self.mouse_down_pos = None;
                self.drag_started = false;
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if let Some(ref mut time) = self.pending_click_time {
            *time -= dt.as_secs_f32();
            if *time <= 0.0 {
                self.pending_click_time = None;
                if let Some(ref cb) = self.on_click {
                    if let Ok(mut f) = cb.lock() { f(); }
                }
                return false;
            }
            return true;
        }
        false
    }

    fn needs_repaint(&self) -> bool {
        self.pending_click_time.is_some()
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
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str { "Draggable" }

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

impl StyledElement for DraggableElement {
    fn apply_style(&mut self, _style: &ComputedStyle) { self.mark_dirty(DirtyFlags::RENDER); }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
}
