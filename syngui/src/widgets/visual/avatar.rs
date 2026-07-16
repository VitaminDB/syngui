use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;

pub struct Avatar {
    pub text: Option<String>,
    pub size: f32,
}

impl Avatar {
    pub fn new() -> Self {
        Self {
            text: None,
            size: 40.0,
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl Default for Avatar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Avatar {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(AvatarElement {
            id: ElementId::new(),
            text: self.text.clone(),
            size: self.size,
            bounds: Rect::zero(),
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

pub struct AvatarElement {
    id: ElementId,
    text: Option<String>,
    size: f32,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for AvatarElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(avatar) = widget.as_any().downcast_ref::<Avatar>() {
            self.text = avatar.text.clone();
            self.size = avatar.size;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let s = self.mss.width
            .map(|d| d.resolve(self.size))
            .unwrap_or(self.size)
            .min(constraints.max_width)
            .min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(s, s));
        Size::new(s, s)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let radius = self.bounds.size.height / 2.0;
        let bg = self.mss.background_color
            .unwrap_or_else(|| Color::from_hex("#3B82F6"));

        list.push_rect(self.bounds, bg, [radius; 4]);

        if let Some(ref text) = self.text {
            let text_color = self.mss.color
                .unwrap_or(Color::WHITE);
            let font_size = self.mss.font_size.unwrap_or(self.bounds.size.height * 0.4);
            list.push_text_centered(text, self.bounds, text_color, font_size);
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

    fn element_type_name(&self) -> &str { "Avatar" }

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
            role: crate::a11y::Role::Image,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: self.text.clone().or_else(|| Some("Avatar".to_string())),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for AvatarElement {
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
