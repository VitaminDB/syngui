use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;

#[derive(Clone, Copy, Debug, Default)]
pub enum DividerDirection {
    #[default]
    Horizontal,
    Vertical,
}

pub struct Divider {
    pub direction: DividerDirection,
    pub length: Option<f32>,
    pub indent: f32,
}

impl Divider {
    pub fn horizontal() -> Self {
        Self {
            direction: DividerDirection::Horizontal,
            length: None,
            indent: 0.0,
        }
    }

    pub fn vertical() -> Self {
        Self {
            direction: DividerDirection::Vertical,
            length: None,
            indent: 0.0,
        }
    }

    pub fn length(mut self, length: f32) -> Self {
        self.length = Some(length);
        self
    }

    pub fn indent(mut self, indent: f32) -> Self {
        self.indent = indent;
        self
    }
}

impl Default for Divider {
    fn default() -> Self {
        Self::horizontal()
    }
}

impl Widget for Divider {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(DividerElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            direction: self.direction,
            length: self.length,
            indent: self.indent,
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

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct DividerElement {
    id: ElementId,
    bounds: Rect,
    direction: DividerDirection,
    length: Option<f32>,
    indent: f32,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for DividerElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(divider) = widget.as_any().downcast_ref::<Divider>() {
            self.direction = divider.direction;
            self.length = divider.length;
            self.indent = divider.indent;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let thickness = self.mss.border_width_or(1.0);
        let padding = self.mss.padding_left.unwrap_or(0.0);
        let pad2 = padding * 2.0;

        let mss_w = self.mss.width.map(|d| d.resolve(constraints.max_width));
        let mss_h = self.mss.height.map(|d| d.resolve(constraints.max_height));

        match self.direction {
            DividerDirection::Horizontal => {
                let width = mss_w
                    .or(self.length)
                    .unwrap_or(constraints.max_width)
                    .min(constraints.max_width);
                let height = mss_h
                    .unwrap_or(thickness + pad2)
                    .min(constraints.max_height);
                self.bounds = Rect::new(Point::zero(), Size::new(width, height));
                Size::new(width, height)
            }
            DividerDirection::Vertical => {
                let width = mss_w
                    .unwrap_or(thickness + pad2)
                    .min(constraints.max_width);
                let height = mss_h
                    .or(self.length)
                    .unwrap_or(constraints.max_height)
                    .min(constraints.max_height);
                self.bounds = Rect::new(Point::zero(), Size::new(width, height));
                Size::new(width, height)
            }
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let color = self.mss.color
            .unwrap_or_else(|| Color::from_hex("#E5E7EB"));
        let thickness = self.mss.border_width_or(1.0);
        let padding = self.mss.padding_left.unwrap_or(0.0);

        match self.direction {
            DividerDirection::Horizontal => {
                let line_y = self.bounds.y() + padding;
                let line_rect = Rect::new(
                    Point::new(self.bounds.x() + self.indent, line_y),
                    Size::new(self.bounds.size.width - self.indent * 2.0, thickness),
                );
                list.push_rect(line_rect, color, [0.0; 4]);
            }
            DividerDirection::Vertical => {
                let line_x = self.bounds.x() + padding;
                let line_rect = Rect::new(
                    Point::new(line_x, self.bounds.y() + self.indent),
                    Size::new(thickness, self.bounds.size.height - self.indent * 2.0),
                );
                list.push_rect(line_rect, color, [0.0; 4]);
            }
        }
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] {
        &[]
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

    fn element_type_name(&self) -> &str { "Divider" }

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

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Presentation,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties::default(),
        })
    }
}

impl StyledElement for DividerElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
