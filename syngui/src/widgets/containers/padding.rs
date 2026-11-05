use super::IntoWidget;
use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use std::any::Any;

pub struct Padding {
    pub child: Option<Box<dyn Widget>>,
    pub padding: f32,
    pub left: Option<f32>,
    pub right: Option<f32>,
    pub top: Option<f32>,
    pub bottom: Option<f32>,
    pub clip: bool,
    /// Края, к которым прибавляется safe area окна ([`crate::viewport::safe_area`]):
    /// слева, сверху, справа, снизу.
    pub safe: [bool; 4],
}

impl Padding {
    /// Содержимое — внутри safe area окна (вырез камеры, строка состояния, жестовая
    /// панель), а сам контейнер и фон под ним — во весь экран. Для режима
    /// [`AppBuilder::edge_to_edge`](crate::app::AppBuilder::edge_to_edge) и полноэкранного
    /// Android: фон корня уходит под вырез, интерактивное — нет. Отступ пересчитывается
    /// при каждой раскладке (поворот, скрытие панелей); края — [`safe_edges`](Self::safe_edges),
    /// дополнительный отступ — [`extra`](Self::extra).
    ///
    /// ```ignore
    /// Stack::new().child(background()).child(Padding::safe_area().child(content()))
    /// ```
    pub fn safe_area() -> Self {
        let mut p = Self::all(0.0);
        p.safe = [true; 4];
        p
    }

    /// Какие края отодвигать на safe area: слева, сверху, справа, снизу.
    pub fn safe_edges(mut self, left: bool, top: bool, right: bool, bottom: bool) -> Self {
        self.safe = [left, top, right, bottom];
        self
    }

    /// Дополнительный отступ поверх safe area (слева, сверху, справа, снизу).
    pub fn extra(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.left = Some(left);
        self.top = Some(top);
        self.right = Some(right);
        self.bottom = Some(bottom);
        self
    }

    pub fn all(padding: f32) -> Self {
        Self {
            child: None,
            padding,
            left: None,
            right: None,
            top: None,
            bottom: None,
            clip: false,
            safe: [false; 4],
        }
    }

    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    pub fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            child: None,
            padding: 0.0,
            left: Some(horizontal),
            right: Some(horizontal),
            top: Some(vertical),
            bottom: Some(vertical),
            clip: false,
            safe: [false; 4],
        }
    }

    pub fn only(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            child: None,
            padding: 0.0,
            left: Some(left),
            top: Some(top),
            right: Some(right),
            bottom: Some(bottom),
            clip: false,
            safe: [false; 4],
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    fn get_left(&self) -> f32 {
        self.left.unwrap_or(self.padding)
    }

    fn get_right(&self) -> f32 {
        self.right.unwrap_or(self.padding)
    }

    fn get_top(&self) -> f32 {
        self.top.unwrap_or(self.padding)
    }

    fn get_bottom(&self) -> f32 {
        self.bottom.unwrap_or(self.padding)
    }
}

impl Widget for Padding {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PaddingElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            left: self.get_left(),
            right: self.get_right(),
            top: self.get_top(),
            bottom: self.get_bottom(),
            clip: self.clip,
            safe: self.safe,
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
            let child_id =
                tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
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

impl PaddingElement {
    /// Отступы с учётом safe area окна (для `Padding::safe_area`).
    fn effective(&self) -> (f32, f32, f32, f32) {
        if self.safe == [false; 4] {
            return (self.left, self.top, self.right, self.bottom);
        }
        let sa = crate::viewport::safe_area();
        let add = |on: bool, v: f32| if on { v } else { 0.0 };
        (
            self.left + add(self.safe[0], sa.left),
            self.top + add(self.safe[1], sa.top),
            self.right + add(self.safe[2], sa.right),
            self.bottom + add(self.safe[3], sa.bottom),
        )
    }
}

pub struct PaddingElement {
    id: ElementId,
    bounds: Rect,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    clip: bool,
    safe: [bool; 4],
    child_id: Option<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for PaddingElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(padding) = widget.as_any().downcast_ref::<Padding>() {
            self.left = padding.get_left();
            self.right = padding.get_right();
            self.top = padding.get_top();
            self.bottom = padding.get_bottom();
            self.clip = padding.clip;
            self.safe = padding.safe;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let (left, top, right, bottom) = self.effective();
        let width = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            left + right + 40.0
        };
        let height = constraints
            .min_height
            .max(top + bottom + 20.0)
            .min(if constraints.max_height.is_finite() {
                constraints.max_height
            } else {
                top + bottom + 40.0
            });

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn layout_hint(&self) -> LayoutHint {
        let (l, t, r, b) = self.effective();
        LayoutHint::Padding { left: l, top: t, right: r, bottom: b }
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
        static EMPTY: &[ElementId] = &[];
        match self.child_id {
            Some(ref id) => std::slice::from_ref(id),
            None => EMPTY,
        }
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
        "Padding"
    }

    fn clip_content(&self) -> bool {
        self.clip
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
    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(pl) = self.mss.padding_left {
            self.left = pl;
        }
        if let Some(pr) = self.mss.padding_right {
            self.right = pr;
        }
        if let Some(pt) = self.mss.padding_top {
            self.top = pt;
        }
        if let Some(pb) = self.mss.padding_bottom {
            self.bottom = pb;
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
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }
}

impl StyledElement for PaddingElement {
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

#[cfg(all(test, feature = "testing"))]
mod tests {
    use crate::prelude::*;
    use crate::testing::TestHarness;

    #[test]
    fn safe_area_padding_follows_viewport_safe_area() {
        crate::viewport::publish_safe_area(crate::core::EdgeInsets::new(0.0, 37.0, 0.0, 20.0));
        let mut h = TestHarness::new(Box::new(
            Padding::safe_area()
                .safe_edges(false, true, false, true)
                .extra(0.0, 5.0, 0.0, 0.0)
                .child(DecoratedBox::new().class("content")),
        ));
        h.apply_mss(".content { width: 100%; height: 100%; }");
        h.layout(400.0, 800.0);
        let id = h.find_by_class("content")[0];
        let b = h.element_bounds(id);
        crate::viewport::publish_safe_area(crate::core::EdgeInsets::zero());
        assert_eq!(b.origin.y, 42.0, "сверху safe area 37 + extra 5: {b:?}");
        assert_eq!(b.size.height, 800.0 - 42.0 - 20.0, "{b:?}");
    }
}
