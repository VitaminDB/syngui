use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widget::context::TextMeasure;
use std::any::Any;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default)]
pub enum BadgeSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl BadgeSize {
    fn font_size(&self) -> f32 {
        match self {
            BadgeSize::Small => 10.0,
            BadgeSize::Medium => 12.0,
            BadgeSize::Large => 14.0,
        }
    }
}

pub struct Badge {
    pub text: String,
    pub size: BadgeSize,
    pub is_dot: bool,
}

impl Badge {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            size: BadgeSize::default(),
            is_dot: false,
        }
    }

    pub fn dot() -> Self {
        Self {
            text: String::new(),
            size: BadgeSize::Small,
            is_dot: true,
        }
    }

    pub fn size(mut self, size: BadgeSize) -> Self {
        self.size = size;
        self
    }

    pub fn small(self) -> Self {
        self.size(BadgeSize::Small)
    }

    pub fn medium(self) -> Self {
        self.size(BadgeSize::Medium)
    }

    pub fn large(self) -> Self {
        self.size(BadgeSize::Large)
    }
}

impl Widget for Badge {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(BadgeElement {
            id: ElementId::new(),
            text: self.text.clone(),
            size: self.size,
            is_dot: self.is_dot,
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
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

pub struct BadgeElement {
    id: ElementId,
    text: String,
    size: BadgeSize,
    is_dot: bool,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<Arc<dyn TextMeasure>>,
}

impl Element for BadgeElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(badge) = widget.as_any().downcast_ref::<Badge>() {
            self.text = badge.text.clone();
            self.size = badge.size;
            self.is_dot = badge.is_dot;
            self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let pad = self.mss.padding_left.unwrap_or(8.0);
        let font_h = self.size.font_size();
        let height = if self.is_dot { 8.0 } else { font_h + pad * 2.0 };

        let width = if self.is_dot {
            8.0
        } else {
            let text_width = self.text_measure.as_ref()
                .map(|tm| tm.measure_text_width(&self.text, font_h, self.text.chars().count()))
                .unwrap_or_else(|| self.text.chars().count() as f32 * font_h * 0.65);
            (text_width + pad * 2.0).max(height)
        };

        let width = width.min(constraints.max_width);
        let height = height.min(constraints.max_height);

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let radius = self.bounds.size.height / 2.0;
        let cr = [radius; 4];
        let bg = self.mss.background_color.unwrap_or_else(|| Color::from_hex("#EF4444"));
        let text_color = self.mss.color.unwrap_or(Color::WHITE);
        let bw = self.mss.border_width_or(1.5);

        if bw > 0.0 {
            let bc = self.mss.border_color
                .unwrap_or_else(|| bg.darken(0.15));
            list.push_rect_bordered(self.bounds, bg, cr, Border::new(bw, bc));
        } else {
            list.push_rect(self.bounds, bg, cr);
        }

        if !self.is_dot && !self.text.is_empty() {
            let font_size = self.mss.font_size.unwrap_or_else(|| self.size.font_size());
            list.push_text_centered(&self.text, self.bounds, text_color, font_size);
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

    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

    fn element_type_name(&self) -> &str { "Badge" }

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
            role: crate::a11y::Role::StaticText,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(self.text.clone()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for BadgeElement {
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
