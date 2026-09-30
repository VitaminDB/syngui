//! Бокс с заданными пропорциями: ширина — всё, что дал родитель, высота —
//! по пропорции; не влезает по высоте (ограничение родителя или
//! [`AspectRatio::max_height`]) — уменьшается целиком, сохраняя пропорции.
//! Ребёнок получает ровно этот размер.
//!
//! Нужен для рамок предпросмотра (экран телефона или монитора, кадр обоев,
//! видео 16:9): размер области заранее не известен, а пропорции — известны.

use super::IntoWidget;
use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use std::any::Any;

pub struct AspectRatio {
    child: Option<Box<dyn Widget>>,
    ratio: f32,
    max_height: f32,
    classes: Vec<String>,
}

impl AspectRatio {
    /// `ratio` — ширина / высота (16:9 → `16.0 / 9.0`).
    pub fn new(ratio: f32) -> Self {
        Self {
            child: None,
            ratio: sane(ratio),
            max_height: 0.0,
            classes: Vec::new(),
        }
    }

    /// Предел высоты сверх ограничения родителя (0 — нет): высокий кадр
    /// (экран телефона) в широкой колонке иначе вырос бы на весь экран.
    pub fn max_height(mut self, h: f32) -> Self {
        self.max_height = h.max(0.0);
        self
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, class.into());
        self
    }
}

fn sane(ratio: f32) -> f32 {
    if ratio.is_finite() && ratio > 0.01 {
        ratio
    } else {
        1.0
    }
}

/// Размер бокса с пропорцией `ratio` в ограничениях `c` и под пределом
/// высоты `max_h` (0 — нет).
pub fn aspect_size(ratio: f32, max_h: f32, c: Constraints) -> Size {
    let r = sane(ratio);
    let mut limit_h = c.max_height;
    if max_h > 0.0 {
        limit_h = limit_h.min(max_h);
    }
    let mut w = if c.max_width.is_finite() {
        c.max_width
    } else if limit_h.is_finite() {
        limit_h * r
    } else {
        320.0
    };
    let mut h = w / r;
    if h > limit_h {
        h = limit_h;
        w = h * r;
    }
    Size::new(w.max(0.0), h.max(0.0))
}

impl Widget for AspectRatio {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(AspectRatioElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            ratio: self.ratio,
            max_height: self.max_height,
            classes: self.classes.clone(),
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
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child
            .as_ref()
            .map(|c| vec![c.as_ref() as &dyn Widget])
            .unwrap_or_default()
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct AspectRatioElement {
    id: ElementId,
    bounds: Rect,
    ratio: f32,
    max_height: f32,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for AspectRatioElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<AspectRatio>() {
            if self.ratio != w.ratio || self.max_height != w.max_height {
                self.ratio = w.ratio;
                self.max_height = w.max_height;
                self.mark_dirty(DirtyFlags::LAYOUT);
            }
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let size = aspect_size(self.ratio, self.max_height, constraints);
        self.bounds = Rect::new(self.bounds.origin, size);
        size
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::AspectRatio {
            ratio: self.ratio,
            max_height: self.max_height,
        }
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(
        &mut self,
        _event: &Event,
        _ctx: &mut crate::widget::context::EventContext,
    ) -> EventResult {
        EventResult::Ignored
    }

    fn passthrough_hit_test(&self) -> bool {
        true
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

    fn element_type_name(&self) -> &str {
        "AspectRatio"
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
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

impl StyledElement for AspectRatioElement {
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

    fn c(w: f32, h: f32) -> Constraints {
        Constraints {
            min_width: 0.0,
            max_width: w,
            min_height: 0.0,
            max_height: h,
            containing_block: Size::new(w, h),
        }
    }

    #[test]
    fn width_bound() {
        assert_eq!(aspect_size(2.0, 0.0, c(400.0, f32::INFINITY)), Size::new(400.0, 200.0));
    }

    #[test]
    fn height_bound_shrinks_width() {
        assert_eq!(aspect_size(0.5, 300.0, c(400.0, f32::INFINITY)), Size::new(150.0, 300.0));
        assert_eq!(aspect_size(1.0, 0.0, c(400.0, 100.0)), Size::new(100.0, 100.0));
    }
}
