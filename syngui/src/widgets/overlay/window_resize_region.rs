//! Зона захвата для изменения размера frameless-окна.
//!
//! Окно без системной рамки композитор не даёт растягивать за края — у него
//! просто нет рамки. Виджет оборачивает корень окна: полоса шириной
//! [`inset`](WindowResizeRegion::inset) вдоль каждого края становится зоной
//! захвата с курсором-стрелкой, а нажатие в ней просит оконную систему начать
//! интерактивный ресайз ([`EventContext::start_window_resize`]).
//!
//! Обычно `inset` совпадает с прозрачным «воздухом» вокруг скруглённого шелла
//! (`.window-backdrop { padding }`): тогда зона захвата лежит в отступе и не
//! перекрывает содержимое. Дочерние элементы обрабатывают события первыми, так
//! что кнопки на самом краю продолжают работать.

use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult, MouseButton, ResizeDirection};
use crate::layout::Constraints;
use crate::mss::window_flags;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::IntoWidget;
use std::any::Any;
use std::time::Duration;

/// Ширина зоны захвата по умолчанию (логические пиксели).
const DEFAULT_INSET: f32 = 8.0;
/// Длина углового захвата: в пределах этого расстояния от угла ресайз идёт по
/// диагонали, а не по одной стороне.
const CORNER: f32 = 24.0;

pub struct WindowResizeRegion {
    pub child: Option<Box<dyn Widget>>,
    inset: f32,
    enabled: bool,
}

impl Default for WindowResizeRegion {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowResizeRegion {
    pub fn new() -> Self {
        Self { child: None, inset: DEFAULT_INSET, enabled: true }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    /// Ширина зоны захвата вдоль каждого края.
    pub fn inset(mut self, inset: f32) -> Self {
        self.inset = inset.max(0.0);
        self
    }

    /// Выключает захват — например, в развёрнутом или полноэкранном окне,
    /// где менять размер нечему.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Widget for WindowResizeRegion {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(WindowResizeRegionElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            inset: self.inset,
            enabled: self.enabled,
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
            let child_id = tree.insert_with_type_id(
                child_element,
                Some(parent_id),
                child.as_any().type_id(),
            );
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child
            .as_ref()
            .map(|c| vec![c.as_ref() as &dyn Widget])
            .unwrap_or_default()
    }
}

struct WindowResizeRegionElement {
    id: ElementId,
    bounds: Rect,
    inset: f32,
    enabled: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl WindowResizeRegionElement {
    /// Направление ресайза для точки, если она попадает в зону захвата.
    fn direction_at(&self, pos: Point) -> Option<ResizeDirection> {
        if !self.enabled || self.inset <= 0.0 || !self.bounds.contains(pos) {
            return None;
        }
        let b = self.bounds;
        let left = pos.x - b.origin.x;
        let top = pos.y - b.origin.y;
        let right = b.origin.x + b.size.width - pos.x;
        let bottom = b.origin.y + b.size.height - pos.y;

        let near_left = left < self.inset;
        let near_right = right < self.inset;
        let near_top = top < self.inset;
        let near_bottom = bottom < self.inset;
        if !(near_left || near_right || near_top || near_bottom) {
            return None;
        }

        // Углы: в пределах CORNER от угла по обеим осям — диагональ.
        let corner_left = left < CORNER;
        let corner_right = right < CORNER;
        let corner_top = top < CORNER;
        let corner_bottom = bottom < CORNER;

        Some(match (near_left, near_right, near_top, near_bottom) {
            (true, _, true, _) | (true, _, _, _) if corner_top => ResizeDirection::NorthWest,
            (true, _, _, true) | (true, _, _, _) if corner_bottom => ResizeDirection::SouthWest,
            (_, true, true, _) | (_, true, _, _) if corner_top => ResizeDirection::NorthEast,
            (_, true, _, true) | (_, true, _, _) if corner_bottom => ResizeDirection::SouthEast,
            (_, _, true, _) if corner_left => ResizeDirection::NorthWest,
            (_, _, true, _) if corner_right => ResizeDirection::NorthEast,
            (_, _, _, true) if corner_left => ResizeDirection::SouthWest,
            (_, _, _, true) if corner_right => ResizeDirection::SouthEast,
            (true, _, _, _) => ResizeDirection::West,
            (_, true, _, _) => ResizeDirection::East,
            (_, _, true, _) => ResizeDirection::North,
            _ => ResizeDirection::South,
        })
    }
}

impl Element for WindowResizeRegionElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<WindowResizeRegion>() {
            self.inset = w.inset;
            self.enabled = w.enabled;
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            0.0
        };
        let h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            0.0
        };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        // В развёрнутом и полноэкранном окне менять размер нечему — зона
        // захвата легла бы прямо на содержимое у края экрана.
        let flags = ctx.window_flags();
        if flags & (window_flags::MAXIMIZED | window_flags::FULLSCREEN) != 0 {
            return EventResult::Ignored;
        }
        match event {
            Event::MouseMove(pos) => {
                if let Some(direction) = self.direction_at(*pos) {
                    ctx.set_cursor(direction.cursor());
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if let Some(direction) = self.direction_at(*position) {
                    ctx.start_window_resize(direction);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        false
    }
    fn needs_repaint(&self) -> bool {
        false
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
    fn set_content_size(&mut self, size: Size) {
        self.bounds = Rect::new(self.bounds.origin, size);
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
        "WindowResizeRegion"
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }

    fn passthrough_hit_test(&self) -> bool {
        false
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
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }
}

impl StyledElement for WindowResizeRegionElement {
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

    fn region(w: f32, h: f32, inset: f32) -> WindowResizeRegionElement {
        WindowResizeRegionElement {
            id: ElementId::new(),
            bounds: Rect::new(Point::zero(), Size::new(w, h)),
            inset,
            enabled: true,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::empty(),
            mss: MssFields::new(),
        }
    }

    #[test]
    fn edges_and_corners() {
        let r = region(800.0, 600.0, 8.0);
        assert_eq!(r.direction_at(Point::new(400.0, 300.0)), None);
        assert_eq!(r.direction_at(Point::new(2.0, 300.0)), Some(ResizeDirection::West));
        assert_eq!(r.direction_at(Point::new(797.0, 300.0)), Some(ResizeDirection::East));
        assert_eq!(r.direction_at(Point::new(400.0, 3.0)), Some(ResizeDirection::North));
        assert_eq!(r.direction_at(Point::new(400.0, 597.0)), Some(ResizeDirection::South));
        assert_eq!(r.direction_at(Point::new(2.0, 2.0)), Some(ResizeDirection::NorthWest));
        assert_eq!(r.direction_at(Point::new(797.0, 597.0)), Some(ResizeDirection::SouthEast));
        // Полоса у края рядом с углом — диагональ, даже если по второй оси
        // точка вне полосы захвата.
        assert_eq!(r.direction_at(Point::new(2.0, 590.0)), Some(ResizeDirection::SouthWest));
        assert_eq!(r.direction_at(Point::new(790.0, 2.0)), Some(ResizeDirection::NorthEast));
    }

    #[test]
    fn disabled_region_never_matches() {
        let mut r = region(800.0, 600.0, 8.0);
        r.enabled = false;
        assert_eq!(r.direction_at(Point::new(2.0, 2.0)), None);
    }
}
