//! Контейнер-обвязка структурных блоков (цитата, callout, toggle, дети
//! пунктов списка): Column-раскладка детей + собственная отрисовка фона,
//! скругления и левой цветной полосы. Фон рисуется до детей, поэтому
//! оказывается под ними.

use std::any::Any;
use std::time::Duration;

use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, UpdateContext};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, Widget};

pub struct Chrome {
    children: Vec<Box<dyn Widget>>,
    gap: f32,
    padding: [f32; 4], // l, t, r, b
    bg: Option<Color>,
    radius: f32,
    border_left: Option<(f32, Color)>,
}

impl Chrome {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            gap: 0.0,
            padding: [0.0; 4],
            bg: None,
            radius: 0.0,
            border_left: None,
        }
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn padding(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.padding = [left, top, right, bottom];
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }

    pub fn border_left(mut self, width: f32, color: Color) -> Self {
        self.border_left = Some((width, color));
        self
    }

    pub fn child(mut self, child: Box<dyn Widget>) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Box<dyn Widget>>) -> Self {
        self.children.extend(children);
        self
    }
}

impl Widget for Chrome {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ChromeElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            gap: self.gap,
            padding: self.padding,
            bg: self.bg,
            radius: self.radius,
            border_left: self.border_left,
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
        for child in &self.children {
            let element = child.create_element();
            let child_id =
                tree.insert_with_type_id(element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }
}

pub struct ChromeElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    gap: f32,
    padding: [f32; 4],
    bg: Option<Color>,
    radius: f32,
    border_left: Option<(f32, Color)>,
}

impl Element for ChromeElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<Chrome>() else { return };
        let layout_changed = self.gap != w.gap || self.padding != w.padding;
        self.gap = w.gap;
        self.padding = w.padding;
        self.bg = w.bg;
        self.radius = w.radius;
        self.border_left = w.border_left;
        self.mark_dirty(DirtyFlags::RENDER);
        if layout_changed {
            self.mark_dirty(DirtyFlags::LAYOUT);
            ctx.mark_layout_dirty();
        }
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn layout(&mut self, constraints: Constraints) -> Size {
        // Контейнер без детей — нулевая высота; с детьми размер считает
        // ElementTree по layout_hint.
        let width = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        self.bounds.size = Size::new(width, self.padding[1] + self.padding[3]);
        self.bounds.size
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Column {
            gap: self.gap,
            cross_align: CrossAxisAlignment::Stretch,
            main_align: MainAxisAlignment::Start,
            padding_left: self.padding[0],
            padding_top: self.padding[1],
            padding_right: self.padding[2],
            padding_bottom: self.padding[3],
            expand: false,
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg) = self.bg {
            let r = self.radius;
            list.push_rect(self.bounds, bg, [r, r, r, r]);
        }
        if let Some((width, color)) = self.border_left {
            let bar = Rect::new(
                self.bounds.origin,
                Size::new(width, self.bounds.size.height),
            );
            let r = (self.radius).min(width / 2.0);
            list.push_rect(bar, color, [r, r, r, r]);
        }
    }

    fn element_type_name(&self) -> &str {
        "doc-chrome"
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        false
    }

    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }
    fn children(&self) -> &[ElementId] {
        &[]
    }
    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty |= flags;
    }
    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.remove(flags);
    }
    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty.contains(flags)
    }
}
