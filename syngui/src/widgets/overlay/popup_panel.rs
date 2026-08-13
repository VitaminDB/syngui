use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{Border, DisplayList};
use crate::signal::{RwSignal, use_signal};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widgets::containers::IntoWidget;
use crate::widgets::overlay::menu::PopupAnchor;
use std::any::Any;
use std::cell::Cell;
use std::sync::Arc;
use crate::core::sync::Mutex;

pub struct PopupPanel {
    children: Vec<Box<dyn Widget>>,
    is_open: RwSignal<bool>,
    anchor_rect: RwSignal<Rect>,
    anchor: PopupAnchor,
    min_width: f32,
    max_width: Option<f32>,
    max_height: f32,
    on_close: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    classes: Vec<String>,
}

impl PopupPanel {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            is_open: use_signal(false),
            anchor_rect: use_signal(Rect::zero()),
            anchor: PopupAnchor::BottomEnd,
            min_width: 180.0,
            max_width: None,
            max_height: 600.0,
            on_close: None,
            classes: Vec::new(),
        }
    }

    pub fn child<M>(mut self, widget: impl IntoWidget<M>) -> Self {
        self.children.push(widget.into_widget());
        self
    }

    pub fn is_open(mut self, state: RwSignal<bool>) -> Self {
        self.is_open = state;
        self
    }

    pub fn anchor_rect(mut self, rect: RwSignal<Rect>) -> Self {
        self.anchor_rect = rect;
        self
    }

    pub fn anchor(mut self, anchor: PopupAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = Some(width);
        self
    }

    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = height;
        self
    }

    pub fn on_close(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_close = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn class(mut self, class: &str) -> Self {
        self.classes.push(class.to_string());
        self
    }
}

impl Default for PopupPanel {
    fn default() -> Self { Self::new() }
}

impl Widget for PopupPanel {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PopupPanelElement {
            id: ElementId::new(),
            is_open: self.is_open,
            anchor_rect: self.anchor_rect,
            anchor: self.anchor,
            min_width: self.min_width,
            max_width: self.max_width,
            max_height: self.max_height,
            on_close: self.on_close.clone(),
            child_ids: Vec::new(),
            bounds: Rect::zero(),
            viewport_size: Cell::new(Size::zero()),
            content_size: Cell::new(Size::zero()),
            placed_rect: Cell::new(Rect::zero()),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            overlay_registered: false,
            mss: MssFields::new(),
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        for child in &self.children {
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }

    fn widget_classes(&self) -> &[String] { &self.classes }
}

struct PopupPanelElement {
    id: ElementId,
    is_open: RwSignal<bool>,
    anchor_rect: RwSignal<Rect>,
    anchor: PopupAnchor,
    min_width: f32,
    max_width: Option<f32>,
    max_height: f32,
    on_close: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    child_ids: Vec<ElementId>,
    bounds: Rect,
    viewport_size: Cell<Size>,
    content_size: Cell<Size>,
    placed_rect: Cell<Rect>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    overlay_registered: bool,
    mss: MssFields,
}

impl PopupPanelElement {
    fn is_open(&self) -> bool {
        self.is_open.get_untracked()
    }

    fn close(&mut self, ctx: &mut EventContext) {
        self.is_open.set(false);
        if self.overlay_registered {
            ctx.unregister_overlay();
            self.overlay_registered = false;
        }
        if let Some(ref cb) = self.on_close {
            if let Ok(mut f) = cb.lock() { f(); }
        }
        ctx.request_paint();
    }

    fn border_radius(&self) -> [f32; 4] {
        self.mss.border_radius_resolved(0.0, 8.0)
    }

    fn panel_rect(&self) -> Rect {
        let viewport = self.viewport_size.get();
        let content = self.content_size.get();
        let ar = self.anchor_rect.get_untracked();

        let width = content
            .width
            .max(self.min_width)
            .min(self.content_width_limit());
        let natural_height = content.height;
        let height = natural_height.min(self.max_height);

        let (mut x, mut y) = match self.anchor {
            PopupAnchor::BottomStart => {
                (ar.origin.x, ar.origin.y + ar.size.height)
            }
            PopupAnchor::BottomEnd => {
                (ar.origin.x + ar.size.width - width, ar.origin.y + ar.size.height)
            }
            PopupAnchor::Position => {
                (ar.origin.x, ar.origin.y)
            }
        };

        if viewport.width > 0.0 {
            x = x.max(0.0).min((viewport.width - width).max(0.0));
        }

        if viewport.height > 0.0 && y + height > viewport.height {
            let flipped = ar.origin.y - height;
            if flipped >= 0.0 {
                y = flipped;
            }
        }
        y = y.max(0.0);

        let max_available = (viewport.height - y).max(0.0);
        let final_height = height.min(max_available);

        Rect::new(Point::new(x, y), Size::new(width, final_height))
    }

    fn content_width_limit(&self) -> f32 {
        let viewport = self.viewport_size.get();
        let limit = self.max_width.unwrap_or(self.min_width);
        if viewport.width > 0.0 {
            limit.min(viewport.width)
        } else {
            limit
        }
    }

    fn placed_rect(&self) -> Rect {
        let placed = self.placed_rect.get();
        if placed.size.width > 0.0 {
            placed
        } else {
            self.panel_rect()
        }
    }
}

impl Element for PopupPanelElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(p) = widget.as_any().downcast_ref::<PopupPanel>() {
            self.is_open = p.is_open;
            self.anchor_rect = p.anchor_rect;
            self.anchor = p.anchor;
            self.min_width = p.min_width;
            self.max_width = p.max_width;
            self.max_height = p.max_height;
            self.on_close = p.on_close.clone();
            self.is_open.subscribe_element(self.id);
            self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::zero()
    }

    fn is_relayout_boundary(&self) -> bool {
        true
    }

    fn intercepts_child_events(&self) -> bool {
        !self.is_open()
    }

    fn layout_hint(&self) -> LayoutHint {
        let panel = self.panel_rect();
        self.placed_rect.set(panel);
        LayoutHint::FloatingWindow {
            x: panel.origin.x,
            y: panel.origin.y,
        }
    }

    fn is_visible(&self) -> bool {
        self.is_open()
    }

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        (Some(self.content_width_limit()), Some(self.max_height))
    }

    fn set_content_size(&mut self, size: Size) {
        self.content_size.set(size);
    }

    fn set_viewport_size(&mut self, size: Size) {
        self.viewport_size.set(size);
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_open() {
            list.push_clip(Rect::zero());
            return;
        }

        list.begin_overlay_absolute();

        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
        let radii = self.border_radius();

        let panel = self.placed_rect();

        list.push_shadow(
            panel,
            Color::new(0.0, 0.0, 0.0, 0.15),
            16.0,
            (0.0, 4.0),
            radii,
        );

        list.push_rect_bordered(panel, bg, radii, Border { width: 1.0, color: border_color });

        list.push_clip(panel);

    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_open() {
            list.pop_clip();
            return;
        }
        list.pop_clip();
        list.end_overlay();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let is_open = self.is_open();

        if is_open && !self.overlay_registered {
            let overlay_bounds = Rect::new(Point::zero(), self.viewport_size.get());
            ctx.register_overlay(overlay_bounds, true);
            self.overlay_registered = true;
            ctx.request_layout();
            ctx.request_paint();
        } else if !is_open && self.overlay_registered {
            ctx.unregister_overlay();
            self.overlay_registered = false;
            ctx.request_layout();
            ctx.request_paint();
        }

        if !is_open {
            return EventResult::Ignored;
        }

        let panel = self.placed_rect();

        match event {
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && !panel.contains(*position) {
                    self.close(ctx);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(crate::input::Key::Escape) => {
                self.close(ctx);
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &self.child_ids }
    fn bounds(&self) -> Rect { self.bounds }

    fn hit_test(&self, _point: Point) -> bool {
        self.is_open()
    }

    fn overlay_request(&self) -> Option<(Rect, bool)> {
        if !self.is_open() { return None; }
        let viewport = self.viewport_size.get();
        if viewport.width <= 0.0 || viewport.height <= 0.0 { return None; }
        Some((Rect::new(Point::zero(), viewport), true))
    }

    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }

    fn mount(&mut self, _tree: &mut ElementTree) {
        self.is_open.subscribe_element(self.id);
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "PopupPanel" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
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

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Group,
            state: crate::a11y::NodeState {
                hidden: !self.is_open(),
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties::default(),
        })
    }
}

impl StyledElement for PopupPanelElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
