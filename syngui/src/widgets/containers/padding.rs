use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use super::IntoWidget;
use std::any::Any;

pub struct Padding {
    pub child: Option<Box<dyn Widget>>,
    pub padding: f32,
    pub left: Option<f32>,
    pub right: Option<f32>,
    pub top: Option<f32>,
    pub bottom: Option<f32>,
    pub clip: bool,
}

impl Padding {
    pub fn all(padding: f32) -> Self {
        Self {
            child: None,
            padding,
            left: None,
            right: None,
            top: None,
            bottom: None,
            clip: false,
        }
    }

    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    pub fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            child: None,
            padding: 0.0,
            left: Some(horizontal),
            right: Some(horizontal),
            top: Some(vertical),
            bottom: Some(vertical),
            clip: false,
        }
    }

    pub fn only(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            child: None,
            padding: 0.0,
            left: Some(left),
            top: Some(top),
            right: Some(right),
            bottom: Some(bottom),
            clip: false,
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    fn get_left(&self) -> f32 {
        self.left.unwrap_or(self.padding)
    }

    fn get_right(&self) -> f32 {
        self.right.unwrap_or(self.padding)
    }

    fn get_top(&self) -> f32 {
        self.top.unwrap_or(self.padding)
    }

    fn get_bottom(&self) -> f32 {
        self.bottom.unwrap_or(self.padding)
    }
}

impl Widget for Padding {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PaddingElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            left: self.get_left(),
            right: self.get_right(),
            top: self.get_top(),
            bottom: self.get_bottom(),
            clip: self.clip,
            child_id: None,
            classes: Vec::new(),
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
            let child_id = tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.as_ref().map(|c| vec![c.as_ref() as &dyn Widget]).unwrap_or_default()
    }
}

pub struct PaddingElement {
    id: ElementId,
    bounds: Rect,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    clip: bool,
    child_id: Option<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for PaddingElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(padding) = widget.as_any().downcast_ref::<Padding>() {
            self.left = padding.get_left();
            self.right = padding.get_right();
            self.top = padding.get_top();
            self.bottom = padding.get_bottom();
            self.clip = padding.clip;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = if constraints.max_width.is_finite() { constraints.max_width } else { self.left + self.right + 40.0 };
        let height = constraints.min_height.max(self.top + self.bottom + 20.0).min(if constraints.max_height.is_finite() { constraints.max_height } else { self.top + self.bottom + 40.0 });

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: self.left, top: self.top, right: self.right, bottom: self.bottom }
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn passthrough_hit_test(&self) -> bool { true }

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

    fn element_type_name(&self) -> &str { "Padding" }

    fn clip_content(&self) -> bool { self.clip }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(pl) = self.mss.padding_left { self.left = pl; }
        if let Some(pr) = self.mss.padding_right { self.right = pr; }
        if let Some(pt) = self.mss.padding_top { self.top = pt; }
        if let Some(pb) = self.mss.padding_bottom { self.bottom = pb; }
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

impl StyledElement for PaddingElement {
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
