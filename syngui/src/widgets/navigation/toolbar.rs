use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult};
use crate::layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::basic::Text;
use crate::widget::context::EventContext;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use crate::widgets::containers::IntoWidget;
use std::any::Any;

pub struct Toolbar {
    pub children: Vec<Box<dyn Widget>>,
    pub title: Option<String>,
    pub height: Dimension,
}

impl Toolbar {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            title: None,
            height: Dimension::Px(48.0),
        }
    }

    pub fn with_title(title: impl Into<String>) -> Self {
        Self::new().title(title)
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.children.push(child.into_widget());
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Dimension::Px(height);
        self
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Toolbar {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ToolbarElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            height: self.height,
            child_ids: Vec::new(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        if let Some(ref title) = self.title {
            let title_widget = Text::new(title)
                .color(Color::from_hex("#1F2937"));
            let el = title_widget.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), title_widget.as_any().type_id());
            title_widget.mount(tree, id);
        }

        for child in &self.children {
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }
}

pub struct ToolbarElement {
    id: ElementId,
    bounds: Rect,
    height: Dimension,
    child_ids: Vec<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for ToolbarElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(toolbar) = widget.as_any().downcast_ref::<Toolbar>() {
            self.height = toolbar.height;
            self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = constraints.max_width;
        let height = self.mss.height.map(|d| d.resolve(constraints.max_height))
            .unwrap_or_else(|| self.height.resolve(constraints.max_height))
            .min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::from_hex("#FFFFFF"));
        list.push_rect(self.bounds, bg, [0.0; 4]);

        list.push_shadow(
            self.bounds,
            Color::new(0.0, 0.0, 0.0, 0.08),
            8.0,
            (0.0, 2.0),
            [0.0; 4],
        );

        let bottom_line = Rect::new(
            Point::new(self.bounds.x(), self.bounds.y() + self.bounds.size.height - 1.0),
            Size::new(self.bounds.size.width, 1.0),
        );
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
        list.push_rect(bottom_line, border_color, [0.0; 4]);
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] { &self.child_ids }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Row {
            gap: 4.0,
            offset_x: 8.0,
            cross_align: CrossAxisAlignment::Center,
            main_align: MainAxisAlignment::Start,
            padding_left: 0.0,
            padding_top: 0.0,
            padding_right: 0.0,
            padding_bottom: 0.0,
        }
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "Toolbar" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(h) = style.height() { self.height = h; }
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
}

impl StyledElement for ToolbarElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
