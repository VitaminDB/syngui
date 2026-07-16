use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;

pub struct ProgressBar {
    pub value: f32,
    pub indeterminate: bool,
    pub show_percentage: bool,
}

impl ProgressBar {
    pub fn new() -> Self {
        Self {
            value: 0.0,
            indeterminate: false,
            show_percentage: false,
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

    pub fn show_percentage(mut self) -> Self {
        self.show_percentage = true;
        self
    }

}

impl Default for ProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ProgressBar {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ProgressBarElement {
            id: ElementId::new(),
            value: self.value,
            indeterminate: self.indeterminate,
            show_percentage: self.show_percentage,
            bounds: Rect::zero(),
            animation_offset: 0.0,
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

pub struct ProgressBarElement {
    id: ElementId,
    value: f32,
    indeterminate: bool,
    show_percentage: bool,
    bounds: Rect,
    animation_offset: f32,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for ProgressBarElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(pb) = widget.as_any().downcast_ref::<ProgressBar>() {
            self.value = pb.value;
            self.indeterminate = pb.indeterminate;
            self.show_percentage = pb.show_percentage;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = self.mss.width.map(|d| d.resolve(constraints.max_width)).unwrap_or(constraints.max_width).min(constraints.max_width);
        let height = self.mss.height.map(|d| d.resolve(constraints.max_height)).unwrap_or(8.0).min(constraints.max_height);

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let track_color = self.mss.background_color.unwrap_or_else(|| Color::from_hex("#E5E7EB"));
        let fill_color = self.mss.accent_color
            .or(self.mss.background_color)
            .unwrap_or_else(|| Color::from_hex("#3B82F6"));
        let radius = self.bounds.size.height / 2.0;

        list.push_rect(self.bounds, track_color, [radius; 4]);

        if self.indeterminate {
            let strip_width = self.bounds.size.width * 0.3;
            let x = self.bounds.x() + self.animation_offset * self.bounds.size.width;
            let fill_rect = Rect::new(
                Point::new(x, self.bounds.y()),
                Size::new(strip_width, self.bounds.size.height),
            );
            list.push_rect(fill_rect, fill_color, [radius; 4]);
        } else if self.value > 0.0 {
            let fill_width = self.bounds.size.width * self.value;
            let fill_rect = Rect::new(
                self.bounds.origin,
                Size::new(fill_width, self.bounds.size.height),
            );
            list.push_rect(fill_rect, fill_color, [radius; 4]);
        }

        if self.show_percentage && !self.indeterminate {
            let percentage = format!("{}%", (self.value * 100.0) as i32);
            let text_rect = Rect::new(
                Point::new(
                    self.bounds.x() + (self.bounds.size.width - 40.0) / 2.0,
                    self.bounds.y() - 16.0,
                ),
                Size::new(40.0, 14.0),
            );
            let pct_color = self.mss.color.unwrap_or_else(|| Color::from_hex("#374151"));
            list.push_text_centered(&percentage, text_rect, pct_color, 12.0);
        }
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        if self.indeterminate {
            self.animation_offset += dt.as_secs_f32() * 0.5;
            if self.animation_offset > 1.0 {
                self.animation_offset = -0.3;
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

    fn element_type_name(&self) -> &str { "ProgressBar" }

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

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::ProgressBar,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                value: Some(format!("{:.0}%", self.value * 100.0)),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for ProgressBarElement {
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
