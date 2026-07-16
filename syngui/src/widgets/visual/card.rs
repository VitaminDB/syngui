use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use crate::widgets::containers::IntoWidget;
use std::any::Any;

pub struct Card {
    pub child: Option<Box<dyn Widget>>,
}

impl Card {
    pub fn new() -> Self {
        Self {
            child: None,
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Card {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(CardElement {
            id: ElementId::new(),
            elevation: 2.0,
            border_radius: 12.0,
            padding_left: 16.0,
            padding_right: 16.0,
            padding_top: 16.0,
            padding_bottom: 16.0,
            color: Color::WHITE,
            bounds: Rect::zero(),
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

pub struct CardElement {
    id: ElementId,
    elevation: f32,
    border_radius: f32,
    padding_left: f32,
    padding_right: f32,
    padding_top: f32,
    padding_bottom: f32,
    color: Color,
    bounds: Rect,
    child_id: Option<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for CardElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if widget.as_any().downcast_ref::<Card>().is_some() {
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let pad_h = self.padding_left + self.padding_right;
        let pad_v = self.padding_top + self.padding_bottom;
        let width = if constraints.max_width.is_finite() { constraints.max_width } else { pad_h + 100.0 };
        let height = constraints.min_height
            .max(pad_v + 20.0)
            .min(if constraints.max_height.is_finite() { constraints.max_height } else { pad_v + 100.0 });

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let ref_size = self.bounds.size.width.min(self.bounds.size.height);
        let radii = self.mss.border_radius_resolved(ref_size, self.border_radius);

        if self.elevation > 0.0 {
            let blur = self.elevation * 2.0;
            let offset_y = self.elevation * 0.5;
            let shadow_color = Color::BLACK.with_alpha(0.15);
            list.push_shadow(self.bounds, shadow_color, blur, (0.0, offset_y), radii);
        }

        let bg = self.mss.background_color.unwrap_or(self.color);
        list.push_rect(self.bounds, bg, radii);
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] {
        static EMPTY: &[ElementId] = &[];
        match self.child_id {
            Some(ref id) => std::slice::from_ref(id),
            None => EMPTY,
        }
    }

    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str { "Card" }

    fn layout_hint(&self) -> LayoutHint {
        let p = self.mss.padding_ltrb([self.padding_left, self.padding_top, self.padding_right, self.padding_bottom]);
        LayoutHint::Padding {
            left: p[0],
            top: p[1],
            right: p[2],
            bottom: p[3],
        }
    }

    fn clip_content(&self) -> bool { true }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(bg) = self.mss.background_color {
            self.color = bg;
        }
        if let Some(p) = self.mss.padding_left { self.padding_left = p; }
        if let Some(p) = self.mss.padding_right { self.padding_right = p; }
        if let Some(p) = self.mss.padding_top { self.padding_top = p; }
        if let Some(p) = self.mss.padding_bottom { self.padding_bottom = p; }
        if let Some(shadows) = &self.mss.box_shadow {
            if let Some(s) = shadows.0.first() {
                self.elevation = s.blur_radius / 2.0;
            }
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
            role: crate::a11y::Role::Group,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some("Card".to_string()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for CardElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
