use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::IntoWidget;
use std::any::Any;

pub struct Sidebar {
    header: Option<Box<dyn Widget>>,
    footer: Option<Box<dyn Widget>>,
    children: Vec<Box<dyn Widget>>,
    classes: Vec<String>,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            header: None,
            footer: None,
            children: Vec::new(),
            classes: Vec::new(),
        }
    }

    pub fn header<M>(mut self, widget: impl IntoWidget<M>) -> Self {
        self.header = Some(widget.into_widget());
        self
    }

    pub fn footer<M>(mut self, widget: impl IntoWidget<M>) -> Self {
        self.footer = Some(widget.into_widget());
        self
    }

    pub fn child<M>(mut self, widget: impl IntoWidget<M>) -> Self {
        self.children.push(widget.into_widget());
        self
    }

    pub fn children(mut self, widgets: Vec<Box<dyn Widget>>) -> Self {
        self.children = widgets;
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Sidebar {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(SidebarElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            child_ids: Vec::new(),
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
        if let Some(ref w) = self.header {
            let el = w.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), w.as_any().type_id());
            w.mount(tree, id);
        }
        for w in &self.children {
            let el = w.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), w.as_any().type_id());
            w.mount(tree, id);
        }
        if let Some(ref w) = self.footer {
            let el = w.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), w.as_any().type_id());
            w.mount(tree, id);
        }
    }

    fn widget_classes(&self) -> &[String] { &self.classes }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        let mut result: Vec<&dyn Widget> = Vec::new();
        if let Some(ref w) = self.header {
            result.push(w.as_ref());
        }
        for w in &self.children {
            result.push(w.as_ref());
        }
        if let Some(ref w) = self.footer {
            result.push(w.as_ref());
        }
        result
    }
}

struct SidebarElement {
    id: ElementId,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    child_ids: Vec<ElementId>,
    mss: MssFields,
}

impl Element for SidebarElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(_sidebar) = widget.as_any().downcast_ref::<Sidebar>() {
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = self
            .mss
            .width
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(constraints.max_width.min(240.0));
        let height = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            400.0
        };
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        let w = self.mss.width.map(|d| d.resolve(0.0));
        (w, Some(f32::INFINITY))
    }

    fn layout_hint(&self) -> LayoutHint {
        let pl = self.mss.padding_left.unwrap_or(0.0);
        let pt = self.mss.padding_top.unwrap_or(0.0);
        let pr = self.mss.padding_right.unwrap_or(0.0);
        let pb = self.mss.padding_bottom.unwrap_or(0.0);
        LayoutHint::Column {
            gap: self.mss.gap.unwrap_or(0.0),
            cross_align: crate::layout::CrossAxisAlignment::Stretch,
            main_align: crate::layout::MainAxisAlignment::Start,
            padding_left: pl,
            padding_top: pt,
            padding_right: pr,
            padding_bottom: pb,
            expand: false,
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg) = self.mss.background_color {
            list.push_rect(self.bounds, bg, [0.0; 4]);
        }

        if let Some(bc) = self.mss.border_color {
            let bw = self.mss.border_width.unwrap_or(1.0);
            let border_rect = Rect::new(
                Point::new(
                    self.bounds.origin.x + self.bounds.size.width - bw,
                    self.bounds.origin.y,
                ),
                Size::new(bw, self.bounds.size.height),
            );
            list.push_rect(border_rect, bc, [0.0; 4]);
        }

    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] {
        &self.child_ids
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
        if let Some(node) = tree.elements.get(&self.id) {
            self.child_ids = node.children.clone();
        }
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }
    fn get_classes(&self) -> &[String] {
        &self.classes
    }
    fn element_type_name(&self) -> &str {
        "Sidebar"
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(d) = style.width() {
            self.mss.width = Some(d);
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
}

impl StyledElement for SidebarElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        self.apply_computed_style(style);
    }
    fn classes(&self) -> &[String] {
        &self.classes
    }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }
}
