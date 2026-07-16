use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;

pub struct CircularProgress {
    pub value: f32,
    pub indeterminate: bool,
    pub size: f32,
    pub stroke_width: f32,
}

impl CircularProgress {
    pub fn new() -> Self {
        Self {
            value: 0.0,
            indeterminate: false,
            size: 40.0,
            stroke_width: 4.0,
        }
    }

    pub fn with_value(value: f32) -> Self {
        Self::new().value(value)
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value.clamp(0.0, 1.0);
        self
    }

    pub fn indeterminate(mut self) -> Self {
        self.indeterminate = true;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }

}

impl Default for CircularProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for CircularProgress {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(CircularProgressElement {
            id: ElementId::new(),
            value: self.value,
            indeterminate: self.indeterminate,
            size: self.size,
            stroke_width: self.stroke_width,
            bounds: Rect::zero(),
            rotation_angle: 0.0,
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

pub struct CircularProgressElement {
    id: ElementId,
    value: f32,
    indeterminate: bool,
    size: f32,
    stroke_width: f32,
    bounds: Rect,
    rotation_angle: f32,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for CircularProgressElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(cp) = widget.as_any().downcast_ref::<CircularProgress>() {
            self.value = cp.value;
            self.indeterminate = cp.indeterminate;
            self.size = cp.size;
            self.stroke_width = cp.stroke_width;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let s = self.size.min(constraints.max_width).min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(s, s));
        Size::new(s, s)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let cx = self.size / 2.0;
        let cy = self.size / 2.0;
        let radius = (self.size - self.stroke_width) / 2.0;

        let mut ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);

        let track_color = self.mss.border_color.unwrap_or_else(|| Color::from_hex("#E5E7EB"));
        ctx.set_color(track_color);
        ctx.set_stroke_width(self.stroke_width);
        ctx.stroke_circle(cx, cy, radius);

        let fill_color = self.mss.accent_color.unwrap_or_else(|| Color::from_hex("#3B82F6"));
        ctx.set_color(fill_color);
        ctx.set_stroke_width(self.stroke_width);

        if self.indeterminate {
            let start = self.rotation_angle;
            let sweep = std::f32::consts::PI * 1.5;
            ctx.draw_arc(cx, cy, radius, start, start + sweep);
        } else if self.value > 0.0 {
            let start = -std::f32::consts::FRAC_PI_2;
            let end = start + self.value * std::f32::consts::TAU;
            ctx.draw_arc(cx, cy, radius, start, end);
        }

        ctx.flush(list);
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        if self.indeterminate {
            self.rotation_angle += dt.as_secs_f32() * std::f32::consts::TAU * 0.8;
            if self.rotation_angle > std::f32::consts::TAU {
                self.rotation_angle -= std::f32::consts::TAU;
            }
            return true;
        }
        false
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

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str { "CircularProgress" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(d) = self.mss.width {
            self.size = d.resolve(self.size);
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

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::ProgressBar,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                value: if self.indeterminate {
                    Some("Indeterminate".to_string())
                } else {
                    Some(format!("{:.0}%", self.value * 100.0))
                },
                ..Default::default()
            },
        })
    }
}

impl StyledElement for CircularProgressElement {
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
