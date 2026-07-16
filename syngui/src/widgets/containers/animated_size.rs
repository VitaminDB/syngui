use crate::animation::{Animation, Easing};
use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationAxis {
    Width,
    Height,
    Both,
}

impl Default for AnimationAxis {
    fn default() -> Self {
        AnimationAxis::Both
    }
}

pub struct AnimatedSize {
    child: Box<dyn Widget>,
    duration_ms: u32,
    easing: Easing,
    clip: bool,
    axis: AnimationAxis,
}

impl AnimatedSize {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            duration_ms: 300,
            easing: Easing::EaseOutCubic,
            clip: true,
            axis: AnimationAxis::Both,
        }
    }

    pub fn duration_ms(mut self, ms: u32) -> Self {
        self.duration_ms = ms;
        self
    }

    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    pub fn axis(mut self, axis: AnimationAxis) -> Self {
        self.axis = axis;
        self
    }
}

impl Widget for AnimatedSize {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(AnimatedSizeElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            target_size: Size::zero(),
            current_width: 0.0,
            current_height: 0.0,
            width_anim: None,
            height_anim: None,
            duration_ms: self.duration_ms,
            easing: self.easing,
            clip: self.clip,
            axis: self.axis,
            initialized: false,
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
        let child_element = self.child.create_element();
        let child_id = tree.insert_with_type_id(child_element, Some(parent_id), self.child.as_any().type_id());
        self.child.mount(tree, child_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![self.child.as_ref() as &dyn Widget]
    }
}

pub struct AnimatedSizeElement {
    id: ElementId,
    bounds: Rect,
    target_size: Size,
    current_width: f32,
    current_height: f32,
    width_anim: Option<Animation>,
    height_anim: Option<Animation>,
    duration_ms: u32,
    easing: Easing,
    clip: bool,
    axis: AnimationAxis,
    initialized: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for AnimatedSizeElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<AnimatedSize>() {
            self.duration_ms = w.duration_ms;
            self.easing = w.easing;
            self.clip = w.clip;
            self.axis = w.axis;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self.current_width.clamp(constraints.min_width.min(constraints.max_width), constraints.max_width);
        let h = self.current_height.clamp(constraints.min_height.min(constraints.max_height), constraints.max_height);
        self.bounds.size = Size::new(w, h);
        Size::new(w, h)
    }

    fn set_content_size(&mut self, size: Size) {
        let old_target = self.target_size;
        self.target_size = size;

        if !self.initialized {
            self.current_width = size.width;
            self.current_height = size.height;
            self.initialized = true;
            return;
        }

        let width_changed = (size.width - old_target.width).abs() > 0.5;
        let height_changed = (size.height - old_target.height).abs() > 0.5;

        if width_changed {
            if matches!(self.axis, AnimationAxis::Width | AnimationAxis::Both) {
                self.width_anim = Some(
                    Animation::tween(self.easing)
                        .from(self.current_width)
                        .to(size.width)
                        .duration_ms(self.duration_ms)
                        .build(),
                );
            } else {
                self.current_width = size.width;
            }
        }

        if height_changed {
            if matches!(self.axis, AnimationAxis::Height | AnimationAxis::Both) {
                self.height_anim = Some(
                    Animation::tween(self.easing)
                        .from(self.current_height)
                        .to(size.height)
                        .duration_ms(self.duration_ms)
                        .build(),
                );
            } else {
                self.current_height = size.height;
            }
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let mut running = false;

        if let Some(ref mut anim) = self.width_anim {
            if anim.tick(dt) {
                self.current_width = anim.current_value();
                running = true;
            } else {
                self.current_width = anim.current_value();
                self.width_anim = None;
            }
        }

        if let Some(ref mut anim) = self.height_anim {
            if anim.tick(dt) {
                self.current_height = anim.current_value();
                running = true;
            } else {
                self.current_height = anim.current_value();
                self.height_anim = None;
            }
        }

        if running {
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
        running
    }

    fn needs_repaint(&self) -> bool {
        self.width_anim.is_some() || self.height_anim.is_some()
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
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

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::AnimatedSize
    }

    fn clip_content(&self) -> bool {
        self.clip
    }

    fn element_type_name(&self) -> &str {
        "AnimatedSize"
    }

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
}

impl StyledElement for AnimatedSizeElement {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_snap_no_animation() {
        let mut el = AnimatedSizeElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            target_size: Size::zero(),
            current_width: 0.0,
            current_height: 0.0,
            width_anim: None,
            height_anim: None,
            duration_ms: 300,
            easing: Easing::EaseOutCubic,
            clip: true,
            axis: AnimationAxis::Both,
            initialized: false,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::empty(),
            mss: MssFields::new(),
        };

        el.set_content_size(Size::new(100.0, 50.0));
        assert!(el.initialized);
        assert_eq!(el.current_width, 100.0);
        assert_eq!(el.current_height, 50.0);
        assert!(el.width_anim.is_none());
        assert!(el.height_anim.is_none());
    }

    #[test]
    fn size_change_starts_animation() {
        let mut el = AnimatedSizeElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            target_size: Size::zero(),
            current_width: 0.0,
            current_height: 0.0,
            width_anim: None,
            height_anim: None,
            duration_ms: 300,
            easing: Easing::Linear,
            clip: true,
            axis: AnimationAxis::Both,
            initialized: false,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::empty(),
            mss: MssFields::new(),
        };

        el.set_content_size(Size::new(100.0, 50.0));

        el.set_content_size(Size::new(200.0, 100.0));
        assert!(el.width_anim.is_some());
        assert!(el.height_anim.is_some());
        assert_eq!(el.current_width, 100.0);
    }

    #[test]
    fn axis_height_only() {
        let mut el = AnimatedSizeElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            target_size: Size::zero(),
            current_width: 0.0,
            current_height: 0.0,
            width_anim: None,
            height_anim: None,
            duration_ms: 300,
            easing: Easing::Linear,
            clip: true,
            axis: AnimationAxis::Height,
            initialized: false,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::empty(),
            mss: MssFields::new(),
        };

        el.set_content_size(Size::new(100.0, 50.0));
        el.set_content_size(Size::new(200.0, 100.0));

        assert!(el.width_anim.is_none());
        assert_eq!(el.current_width, 200.0);
        assert!(el.height_anim.is_some());
        assert_eq!(el.current_height, 50.0);
    }

    #[test]
    fn animate_completes() {
        let mut el = AnimatedSizeElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            target_size: Size::zero(),
            current_width: 0.0,
            current_height: 0.0,
            width_anim: None,
            height_anim: None,
            duration_ms: 100,
            easing: Easing::Linear,
            clip: true,
            axis: AnimationAxis::Both,
            initialized: false,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::empty(),
            mss: MssFields::new(),
        };

        el.set_content_size(Size::new(100.0, 50.0));
        el.set_content_size(Size::new(200.0, 100.0));

        let running = el.animate(Duration::from_millis(200));
        assert!(!running);
        assert!((el.current_width - 200.0).abs() < 1.0);
        assert!((el.current_height - 100.0).abs() < 1.0);
        assert!(el.width_anim.is_none());
        assert!(el.height_anim.is_none());
    }
}
