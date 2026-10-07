//! Плавный сдвиг при смене места в раскладке.
//!
//! Когда сосед в колонке исчезает или встаёт выше, остальные строки
//! прыгают на новое место в один кадр. `AnimatedPosition` запоминает, где
//! элемент стоял, и после перераскладки везёт его от старого места к новому
//! на пружине (FLIP: раскладка уже новая, а на экране — догоняющий сдвиг).
//! Ввод при этом идёт по новой раскладке.
//!
//! ```rust,ignore
//! Column::new().children(items.iter().map(|it| {
//!     Keyed::new(it.id, it.version, move || Box::new(AnimatedPosition::new(row(it))))
//! }))
//! ```
//!
//! Первое появление не анимируется (для него — [`Presence`](super::Presence)).

use crate::animation::Spring;
use crate::core::{Point, Rect, Size, Transform};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::time::Duration;

pub struct AnimatedPosition {
    child: Box<dyn Widget>,
    stiffness: f32,
    damping: f32,
    classes: Vec<String>,
}

impl AnimatedPosition {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self { child: Box::new(child), stiffness: 420.0, damping: 40.0, classes: Vec::new() }
    }

    /// Параметры пружины (по умолчанию 420 / 40 — быстро и без перелёта).
    pub fn spring(mut self, stiffness: f32, damping: f32) -> Self {
        self.stiffness = stiffness;
        self.damping = damping;
        self
    }

    pub fn class(mut self, class: &str) -> Self {
        crate::widget::push_classes(&mut self.classes, class.to_string());
        self
    }
}

impl AnimatedPosition {
    fn element(&self) -> AnimatedPositionElement {
        AnimatedPositionElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            last_pos: None,
            offset: (0.0, 0.0),
            velocity: (0.0, 0.0),
            active: false,
            spring: Spring::new().with_stiffness(self.stiffness).with_damping(self.damping),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        }
    }
}

impl Widget for AnimatedPosition {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(self.element())
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
        let child_id =
            tree.insert_with_type_id(child_element, Some(parent_id), self.child.as_any().type_id());
        self.child.mount(tree, child_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![self.child.as_ref() as &dyn Widget]
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct AnimatedPositionElement {
    id: ElementId,
    bounds: Rect,
    last_pos: Option<Point>,
    /// Текущий визуальный сдвиг от места в раскладке (стремится к нулю).
    offset: (f32, f32),
    velocity: (f32, f32),
    active: bool,
    spring: Spring,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for AnimatedPositionElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<AnimatedPosition>() {
            self.spring = Spring::new().with_stiffness(w.stiffness).with_damping(w.damping);
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, _constraints: Constraints) -> Size {
        Size::zero()
    }

    fn set_content_size(&mut self, size: Size) {
        self.bounds.size = size;
    }

    fn set_position(&mut self, pos: Point) {
        if let Some(last) = self.last_pos {
            let dx = last.x - pos.x;
            let dy = last.y - pos.y;
            if dx.abs() > 0.5 || dy.abs() > 0.5 {
                self.offset.0 += dx;
                self.offset.1 += dy;
                self.active = true;
                self.mark_dirty(DirtyFlags::RENDER);
            }
        }
        self.last_pos = Some(pos);
        self.bounds.origin = pos;
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if !self.active {
            return false;
        }
        // анимации выключены глобально — сразу на место (пружина шагает не больше 0,25 с за кадр)
        if !crate::animation::enabled() {
            self.offset = (0.0, 0.0);
            self.velocity = (0.0, 0.0);
            self.active = false;
            self.mark_dirty(DirtyFlags::RENDER);
            return true;
        }
        let dt = dt.as_secs_f32();
        let (x, vx) = self.spring.update(self.offset.0, 0.0, self.velocity.0, dt);
        let (y, vy) = self.spring.update(self.offset.1, 0.0, self.velocity.1, dt);
        self.offset = (x, y);
        self.velocity = (vx, vy);
        if self.spring.is_at_rest(x.abs().max(y.abs()), vx.abs().max(vy.abs())) {
            self.offset = (0.0, 0.0);
            self.velocity = (0.0, 0.0);
            self.active = false;
        }
        self.mark_dirty(DirtyFlags::RENDER);
        self.active
    }

    fn needs_repaint(&self) -> bool {
        self.active
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.active {
            list.push_transform(Transform::translation(self.offset.0, self.offset.1));
        }
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.active {
            list.pop_transform();
        }
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
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

    fn element_type_name(&self) -> &str {
        "AnimatedPosition"
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
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

    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }

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

impl StyledElement for AnimatedPositionElement {
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
    use crate::widget::Text;

    fn element() -> AnimatedPositionElement {
        AnimatedPosition::new(Text::new("x")).element()
    }

    #[test]
    fn first_placement_is_silent() {
        let mut el = element();
        el.set_position(Point::new(10.0, 20.0));
        assert!(!el.active);
        assert!(!el.animate(Duration::from_millis(16)));
    }

    #[test]
    fn move_starts_from_old_place_and_settles() {
        let mut el = element();
        el.set_position(Point::new(0.0, 100.0));
        el.set_position(Point::new(0.0, 40.0));
        assert!(el.active);
        assert!((el.offset.1 - 60.0).abs() < 1e-3);
        for _ in 0..200 {
            if !el.animate(Duration::from_millis(16)) {
                break;
            }
        }
        assert!(!el.active);
        assert_eq!(el.offset, (0.0, 0.0));
    }

    #[test]
    fn repeated_layout_at_same_place_does_not_animate() {
        let mut el = element();
        el.set_position(Point::new(5.0, 5.0));
        el.set_position(Point::new(5.0, 5.0));
        assert!(!el.active);
    }
}
