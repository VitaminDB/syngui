use std::any::Any;

use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::signal::RwSignal;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};

use super::IntoWidget;

pub struct Positioned {
    pub child: Option<Box<dyn Widget>>,
    pub x: f32,
    pub y: f32,
    pub offset_signal: Option<RwSignal<Point>>,
    pub size: Option<Size>,
    pub classes: Vec<String>,
}

impl Positioned {
    pub fn new<M>(child: impl IntoWidget<M>) -> Self {
        Self {
            child: Some(child.into_widget()),
            x: 0.0,
            y: 0.0,
            offset_signal: None,
            size: None,
            classes: Vec::new(),
        }
    }

    pub fn x(mut self, x: f32) -> Self {
        self.x = x;
        self
    }

    pub fn y(mut self, y: f32) -> Self {
        self.y = y;
        self
    }

    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    pub fn offset_signal(mut self, signal: RwSignal<Point>) -> Self {
        self.offset_signal = Some(signal);
        let p = signal.get_untracked();
        self.x = p.x;
        self.y = p.y;
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = Some(size);
        self
    }

    pub fn dimensions(mut self, width: f32, height: f32) -> Self {
        self.size = Some(Size::new(width, height));
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Widget for Positioned {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PositionedElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            x: self.x,
            y: self.y,
            offset_signal: self.offset_signal,
            size: self.size,
            child_id: None,
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
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
        if let Some(child) = &self.child {
            let child_element = child.create_element();
            let child_id =
                tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child
            .as_ref()
            .map(|c| vec![c.as_ref() as &dyn Widget])
            .unwrap_or_default()
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct PositionedElement {
    id: ElementId,
    bounds: Rect,
    x: f32,
    y: f32,
    offset_signal: Option<RwSignal<Point>>,
    size: Option<Size>,
    child_id: Option<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for PositionedElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(p) = widget.as_any().downcast_ref::<Positioned>() {
            let changed_pos = self.x != p.x || self.y != p.y;
            let changed_size = self.size != p.size;
            self.x = p.x;
            self.y = p.y;
            self.offset_signal = p.offset_signal;
            self.size = p.size;
            if let Some(s) = self.offset_signal {
                s.subscribe_element(self.id);
            }
            if changed_pos || changed_size {
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        if let Some(s) = self.offset_signal {
            let p = s.get_untracked();
            if p.x != self.x || p.y != self.y {
                self.x = p.x;
                self.y = p.y;
            }
        }
        let size = self.size.unwrap_or_else(|| {
            Size::new(
                if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 },
                if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 },
            )
        });
        self.bounds = Rect::new(self.bounds.origin, size);
        size
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Positioned { x: self.x, y: self.y }
    }

    fn animate(&mut self, _dt: std::time::Duration) -> bool {
        if let Some(s) = self.offset_signal {
            let p = s.get_untracked();
            if p.x != self.x || p.y != self.y {
                self.x = p.x;
                self.y = p.y;
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
                return true;
            }
        }
        false
    }

    fn needs_repaint(&self) -> bool {
        if let Some(s) = self.offset_signal {
            let p = s.get_untracked();
            return p.x != self.x || p.y != self.y;
        }
        false
    }

    fn explicit_dimensions(&self, _parent_w: f32, _parent_h: f32) -> (Option<f32>, Option<f32>) {
        match self.size {
            Some(s) => (Some(s.width), Some(s.height)),
            None => (None, None),
        }
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(
        &mut self,
        _event: &Event,
        _ctx: &mut crate::widget::context::EventContext,
    ) -> EventResult {
        EventResult::Ignored
    }

    fn passthrough_hit_test(&self) -> bool {
        true
    }

    fn children(&self) -> &[ElementId] {
        static EMPTY: &[ElementId] = &[];
        match self.child_id {
            Some(ref id) => std::slice::from_ref(id),
            None => EMPTY,
        }
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = Point::new(pos.x + self.x, pos.y + self.y);
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

    fn mount(&mut self, _tree: &mut ElementTree) {
        if let Some(s) = self.offset_signal {
            s.subscribe_element(self.id);
        }
    }

    fn wants_animate_tick(&self) -> bool {
        self.offset_signal.is_some()
    }

    fn element_type_name(&self) -> &str {
        "Positioned"
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER);
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

impl StyledElement for PositionedElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
