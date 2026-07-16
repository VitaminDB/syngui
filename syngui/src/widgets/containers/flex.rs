use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::layout::{MainAxisAlignment, CrossAxisAlignment, FlexDirection};
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use super::IntoWidget;
use std::any::Any;

pub struct Flex {
    pub children: Vec<Box<dyn Widget>>,
    pub direction: FlexDirection,
    pub gap: f32,
    pub wrap: bool,
    pub main_axis_alignment: MainAxisAlignment,
    pub cross_axis_alignment: CrossAxisAlignment,
}

impl Flex {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            direction: FlexDirection::default(),
            gap: 0.0,
            wrap: false,
            main_axis_alignment: MainAxisAlignment::default(),
            cross_axis_alignment: CrossAxisAlignment::default(),
        }
    }

    pub fn row() -> Self {
        Self::new().direction(FlexDirection::Row)
    }

    pub fn column() -> Self {
        Self::new().direction(FlexDirection::Column)
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.children.push(child.into_widget());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Box<dyn Widget>>) -> Self {
        self.children.extend(children);
        self
    }

    pub fn direction(mut self, direction: FlexDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }

    pub fn main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.main_axis_alignment = alignment;
        self
    }

    pub fn cross_axis_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
    }
}

impl Default for Flex {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Flex {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(FlexElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            direction: self.direction,
            gap: self.gap,
            wrap: self.wrap,
            main_axis_alignment: self.main_axis_alignment,
            cross_axis_alignment: self.cross_axis_alignment,
            child_ids: Vec::new(),
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
        for child in &self.children {
            let child_element = child.create_element();
            let child_id = tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }
}

pub struct FlexElement {
    id: ElementId,
    bounds: Rect,
    direction: FlexDirection,
    gap: f32,
    wrap: bool,
    main_axis_alignment: MainAxisAlignment,
    cross_axis_alignment: CrossAxisAlignment,
    child_ids: Vec<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for FlexElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(flex) = widget.as_any().downcast_ref::<Flex>() {
            self.direction = flex.direction;
            self.gap = flex.gap;
            self.wrap = flex.wrap;
            self.main_axis_alignment = flex.main_axis_alignment;
            self.cross_axis_alignment = flex.cross_axis_alignment;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = constraints.max_width;
        let height = constraints.min_height.max(40.0).min(constraints.max_height);

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn layout_hint(&self) -> LayoutHint {
        let pl = self.mss.padding_left.unwrap_or(0.0);
        let pt = self.mss.padding_top.unwrap_or(0.0);
        let pr = self.mss.padding_right.unwrap_or(0.0);
        let pb = self.mss.padding_bottom.unwrap_or(0.0);
        if self.wrap {
            return LayoutHint::Flex { col_gap: self.gap, row_gap: self.gap, justify: self.main_axis_alignment.clone(), align_items: self.cross_axis_alignment.clone() };
        }
        match self.direction {
            FlexDirection::Row => LayoutHint::Row { gap: self.gap, offset_x: 0.0, cross_align: CrossAxisAlignment::Start, main_align: MainAxisAlignment::Start, padding_left: pl, padding_top: pt, padding_right: pr, padding_bottom: pb },
            FlexDirection::Column => LayoutHint::Column { gap: self.gap, cross_align: self.cross_axis_alignment, main_align: self.main_axis_alignment, padding_left: pl, padding_top: pt, padding_right: pr, padding_bottom: pb, expand: false },
        }
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn passthrough_hit_test(&self) -> bool { true }

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

    fn element_type_name(&self) -> &str { "Flex" }

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
        if let Some(g) = self.mss.gap {
            self.gap = g;
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

impl StyledElement for FlexElement {
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
