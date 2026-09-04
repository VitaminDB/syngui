use crate::core::{Color, Point, Rect, Size};
use crate::core::canvas::CanvasContext;
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;

type DrawFn = Arc<dyn Fn(&mut CanvasContext, f32) + Send + Sync>;

pub struct Canvas {
    draw: DrawFn,
    width: Option<Dimension>,
    height: Option<Dimension>,
    animated: bool,
    background: Option<Color>,
}

impl Canvas {
    pub fn new(draw: impl Fn(&mut CanvasContext, f32) + Send + Sync + 'static) -> Self {
        Self {
            draw: Arc::new(draw),
            width: None,
            height: None,
            animated: false,
            background: None,
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

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self.height = Some(Dimension::Px(h));
        self
    }

    pub fn animated(mut self, a: bool) -> Self {
        self.animated = a;
        self
    }

    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }
}

impl Widget for Canvas {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(CanvasElement {
            id: ElementId::new(),
            draw: self.draw.clone(),
            width: self.width,
            height: self.height,
            animated: self.animated,
            background: self.background,
            bounds: Rect::zero(),
            elapsed: 0.0,
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

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct CanvasElement {
    id: ElementId,
    draw: DrawFn,
    width: Option<Dimension>,
    height: Option<Dimension>,
    animated: bool,
    background: Option<Color>,
    bounds: Rect,
    elapsed: f32,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for CanvasElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(c) = widget.as_any().downcast_ref::<Canvas>() {
            self.draw = c.draw.clone();
            self.width = c.width;
            self.height = c.height;
            self.animated = c.animated;
            self.background = c.background;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = self.width.or(self.mss.width).map(|d| d.resolve(constraints.max_width)).unwrap_or(constraints.max_width).min(constraints.max_width);
        let height = self.height.or(self.mss.height).map(|d| d.resolve(constraints.max_height)).unwrap_or(200.0).min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.background.or(self.mss.background_color);
        if let Some(bg) = bg {
            list.push_rect(self.bounds, bg, [0.0; 4]);
        }

        let mut ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);
        ctx.set_mss_colors(self.mss.color, self.mss.background_color, self.mss.accent_color);
        (self.draw)(&mut ctx, self.elapsed);

        ctx.flush(list);
    }

    /// Кадры нужны только анимированному холсту.
    fn wants_animate_tick(&self) -> bool {
        self.animated
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        if self.animated {
            self.elapsed += dt.as_secs_f32();
            return true;
        }
        false
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
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

    fn clip_content(&self) -> bool {
        false
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
        if let Some(d) = self.mss.width { self.width = Some(d); }
        if let Some(d) = self.mss.height { self.height = Some(d); }
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

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Image,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some("Canvas".to_string()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for CanvasElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
