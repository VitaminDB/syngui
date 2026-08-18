use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::layout::{MainAxisAlignment, CrossAxisAlignment};
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use super::IntoWidget;
use std::any::Any;

pub struct Row {
    pub children: Vec<Box<dyn Widget>>,
    pub gap: f32,
    pub main_axis_alignment: MainAxisAlignment,
    pub cross_axis_alignment: CrossAxisAlignment,
    pub clip: bool,
    width: Option<Dimension>,
    height: Option<Dimension>,
    classes: Vec<String>,
}

impl Row {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            gap: 0.0,
            main_axis_alignment: MainAxisAlignment::default(),
            cross_axis_alignment: CrossAxisAlignment::default(),
            clip: false,
            width: None,
            height: None,
            classes: Vec::new(),
        }
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(Dimension::Px(h));
        self
    }

    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.children.push(child.into_widget());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Box<dyn Widget>>) -> Self {
        self.children.extend(children);
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.main_axis_alignment = alignment;
        self
    }

    pub fn cross_axis_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
    }

    pub fn center(self) -> Self {
        self.cross_axis_alignment(CrossAxisAlignment::Center)
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Row {
    fn create_element(&self) -> Box<dyn Element> {
        let mut mss = MssFields::new();
        mss.width = self.width;
        mss.height = self.height;
        Box::new(RowElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            gap: self.gap,
            main_axis_alignment: self.main_axis_alignment,
            cross_axis_alignment: self.cross_axis_alignment,
            clip: self.clip,
            child_ids: Vec::new(),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss,
            builder_width: self.width,
            builder_height: self.height,
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
            let child_element = child.create_element();
            let child_id = tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct RowElement {
    id: ElementId,
    bounds: Rect,
    gap: f32,
    main_axis_alignment: MainAxisAlignment,
    cross_axis_alignment: CrossAxisAlignment,
    clip: bool,
    child_ids: Vec<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    builder_width: Option<Dimension>,
    builder_height: Option<Dimension>,
}

impl Element for RowElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(row) = widget.as_any().downcast_ref::<Row>() {
            self.gap = row.gap;
            self.main_axis_alignment = row.main_axis_alignment;
            self.cross_axis_alignment = row.cross_axis_alignment;
            self.clip = row.clip;
            if row.width.is_some() {
                self.mss.width = row.width;
                self.builder_width = row.width;
            }
            if row.height.is_some() {
                self.mss.height = row.height;
                self.builder_height = row.height;
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = if let Some(ref d) = self.mss.width {
            d.resolve(constraints.max_width).min(constraints.max_width)
        } else if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            constraints.min_width.max(0.0)
        };
        let height = if let Some(ref d) = self.mss.height {
            d.resolve(constraints.max_height).min(constraints.max_height)
        } else if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            constraints.min_height.max(0.0)
        };

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn layout_hint(&self) -> LayoutHint {
        let pad = self.mss.padding_ltrb([0.0; 4]);
        LayoutHint::Row {
            gap: self.mss.gap.unwrap_or(self.gap),
            offset_x: 0.0,
            cross_align: self.cross_axis_alignment,
            main_align: self.main_axis_alignment,
            padding_left: pad[0],
            padding_top: pad[1],
            padding_right: pad[2],
            padding_bottom: pad[3],
        }
    }

    fn explicit_dimensions(&self, parent_width: f32, parent_height: f32) -> (Option<f32>, Option<f32>) {
        (
            self.mss.width.and_then(|d| d.resolve_opt(parent_width)),
            self.mss.height.and_then(|d| d.resolve_opt(parent_height)),
        )
    }

    fn min_max_dimensions(&self, parent_width: f32, parent_height: f32)
        -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>)
    {
        (
            self.mss.min_width.and_then(|d| d.resolve_opt(parent_width)),
            self.mss.max_width.and_then(|d| d.resolve_opt(parent_width)),
            self.mss.min_height.and_then(|d| d.resolve_opt(parent_height)),
            self.mss.max_height.and_then(|d| d.resolve_opt(parent_height)),
        )
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        self.mss.paint_background(list, self.bounds);
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        self.mss.paint_border(list, self.bounds);
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn passthrough_hit_test(&self) -> bool { true }

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

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn clip_content(&self) -> bool { self.clip }

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
        if self.mss.width.is_none() {
            self.mss.width = self.builder_width;
        }
        if self.mss.height.is_none() {
            self.mss.height = self.builder_height;
        }
        if let Some(g) = self.mss.gap {
            self.gap = g;
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn element_type_name(&self) -> &str { "Row" }
}

impl StyledElement for RowElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        self.apply_computed_style(style);
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
    use super::Row;
    use crate::core::Rect;
    use crate::render::display_list::{DisplayList, DrawCommand};
    use crate::testing::TestHarness;
    use crate::widget::{Widget, WidgetExt};
    use crate::widgets::Text;

    fn commands_for(mss: &str) -> Vec<DrawCommand> {
        let row = Row::new()
            .child(Text::new("cell"))
            .class("bar");
        let mut h = TestHarness::new(Box::new(row) as Box<dyn Widget>);
        let engine = h.apply_mss(mss);
        h.apply_styles(&engine);
        h.layout(200.0, 40.0);
        let mut list = DisplayList::new();
        h.tree.build_display_list(h.root_id, &mut list, Rect::new(crate::core::Point::zero(), crate::core::Size::new(200.0, 40.0)));
        list.commands()
    }

    #[test]
    fn row_paints_mss_background() {
        let cmds = commands_for(".bar { background: #ff0000; }");
        let painted = cmds.iter().any(|c| match c {
            DrawCommand::Rect { color, .. } => color.r > 0.9 && color.a > 0.9,
            _ => false,
        });
        assert!(painted, "Row не нарисовал фон из MSS: {:?}", cmds);
    }

    #[test]
    fn row_paints_bottom_border_from_shorthand() {
        let cmds = commands_for(".bar { border-bottom: 1px solid #00ff00; }");
        let painted = cmds.iter().any(|c| match c {
            DrawCommand::Rect { per_side_border: Some(ps), .. } => {
                ps.widths[3] > 0.0 && ps.widths[0] == 0.0 && ps.widths[1] == 0.0 && ps.widths[2] == 0.0
            }
            _ => false,
        });
        assert!(painted, "Row не нарисовал border-bottom: {:?}", cmds);
    }
}
