use crate::animation::transition::mss_color_to_core;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult};
use crate::layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
use crate::mss::{ComputedStyle, Dimension, StyleValue};
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use crate::widget::basic::Text;
use crate::widget::styled::WidgetExt;
use crate::widgets::containers::{DecoratedBox, IntoWidget};
use std::any::Any;

pub struct TopAppBar {
    title: String,
    leading: Option<Box<dyn Widget>>,
    actions: Vec<Box<dyn Widget>>,
    height: Dimension,
    gap: f32,
}

impl TopAppBar {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            leading: None,
            actions: Vec::new(),
            height: Dimension::Px(56.0),
            gap: 8.0,
        }
    }

    pub fn leading<M>(mut self, widget: impl IntoWidget<M>) -> Self {
        self.leading = Some(widget.into_widget());
        self
    }

    pub fn action<M>(mut self, widget: impl IntoWidget<M>) -> Self {
        self.actions.push(widget.into_widget());
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = Dimension::Px(h);
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

impl Widget for TopAppBar {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(TopAppBarElement {
            id: ElementId::new(),
            height: self.height,
            elevation: 4.0,
            bg_color: None,
            shadow_color: Color::new(0.0, 0.0, 0.0, 0.2),
            gap: self.gap,
            child_ids: Vec::new(),
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        if let Some(ref leading) = self.leading {
            let el = leading.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), leading.as_any().type_id());
            leading.mount(tree, id);
        }

        let title_widget = Text::new(&self.title).class("title");
        let title_el = title_widget.create_element();
        let title_id = tree.insert_with_type_id(title_el, Some(parent_id), title_widget.as_any().type_id());
        title_widget.mount(tree, title_id);

        let spacer = DecoratedBox::new().style("flex-grow", StyleValue::Number(1.0));
        let spacer_el = spacer.create_element();
        let spacer_id = tree.insert_with_type_id(spacer_el, Some(parent_id), spacer.as_any().type_id());
        spacer.mount(tree, spacer_id);

        for action in &self.actions {
            let el = action.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), action.as_any().type_id());
            action.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        let mut result: Vec<&dyn Widget> = Vec::new();
        if let Some(ref leading) = self.leading {
            result.push(leading.as_ref() as &dyn Widget);
        }
        for action in &self.actions {
            result.push(action.as_ref() as &dyn Widget);
        }
        result
    }
}

pub struct TopAppBarElement {
    id: ElementId,
    height: Dimension,
    elevation: f32,
    bg_color: Option<Color>,
    shadow_color: Color,
    gap: f32,
    child_ids: Vec<ElementId>,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for TopAppBarElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(bar) = widget.as_any().downcast_ref::<TopAppBar>() {
            self.height = bar.height;
            self.gap = bar.gap;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = constraints.max_width;
        let h = self.height.resolve(constraints.max_height).min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        (None, Some(self.height.resolve(f32::INFINITY)))
    }

    fn layout_hint(&self) -> LayoutHint {
        let [pl, pt, pr, pb] = self.mss.padding_ltrb([0.0, 0.0, 0.0, 0.0]);
        LayoutHint::Row {
            gap: self.mss.gap.unwrap_or(self.gap),
            offset_x: 0.0,
            cross_align: CrossAxisAlignment::Center,
            main_align: MainAxisAlignment::Start,
            padding_left: pl,
            padding_top: pt,
            padding_right: pr,
            padding_bottom: pb,
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.bg_color
            .or(self.mss.background_color)
            .unwrap_or(Color::from_hex("#1976D2"));
        list.push_rect(self.bounds, bg, [0.0; 4]);

        let border_width = self.mss.border_width_or(0.0);
        if border_width > 0.0 {
            let bc = self.mss.border_color.unwrap_or(Color::from_hex("#E0E0E0"));
            let b = self.bounds;
            list.push_rect(
                Rect::new(
                    Point::new(b.x(), b.y() + b.size.height - border_width),
                    Size::new(b.size.width, border_width),
                ),
                bc,
                [0.0; 4],
            );
        }

        if self.elevation > 0.0 {
            let blur = self.elevation * 2.0;
            let offset_y = self.elevation * 0.5;
            list.begin_overlay_absolute();
            let clip_rect = Rect::new(
                Point::new(self.bounds.origin.x, self.bounds.origin.y + self.bounds.size.height),
                Size::new(self.bounds.size.width, blur + offset_y),
            );
            list.push_clip(clip_rect);
            list.push_shadow(
                self.bounds,
                self.shadow_color,
                blur,
                (0.0, offset_y),
                [0.0; 4],
            );
            list.pop_clip();
            list.end_overlay();
        }
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

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "TopAppBar" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);

        if let Some(bg) = style.background_color() { self.bg_color = Some(mss_color_to_core(bg)); }
        if let Some(d) = style.height() { self.height = d; }
        if let Some(e) = style.get("elevation").and_then(|v| v.as_px()) { self.elevation = e; }
        if let Some(c) = style.get("shadow-color").and_then(|v| v.as_color()) {
            self.shadow_color = mss_color_to_core(c);
        }

        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }
}

impl StyledElement for TopAppBarElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        if let Some(bg) = style.background_color() { self.bg_color = Some(mss_color_to_core(bg)); }
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
