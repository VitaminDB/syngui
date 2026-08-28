use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::signal::{RwSignal, use_signal};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use crate::widgets::containers::IntoWidget;
use std::any::Any;
use std::cell::Cell;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Clone, Copy, Debug)]
pub enum PortalAnchor {
    Center,
    BottomEnd { margin_bottom: f32, margin_right: f32 },
    TopEnd { margin_top: f32, margin_right: f32 },
    BottomStart { margin_bottom: f32, margin_left: f32 },
}

impl Default for PortalAnchor {
    fn default() -> Self { Self::Center }
}

pub struct Portal {
    children: Vec<Box<dyn Widget>>,
    is_open: RwSignal<bool>,
    modal: bool,
    close_on_outside_click: bool,
    backdrop: bool,
    backdrop_color: Color,
    width: Option<Dimension>,
    anchor: PortalAnchor,
    on_close: Option<Arc<Mutex<dyn FnMut() + Send>>>,
}

impl Portal {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            is_open: use_signal(false),
            modal: true,
            close_on_outside_click: true,
            backdrop: true,
            backdrop_color: Color::new(0.0, 0.0, 0.0, 0.4),
            width: None,
            anchor: PortalAnchor::Center,
            on_close: None,
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

    pub fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    /// Закрывать ли модал кликом мимо содержимого. По умолчанию да.
    ///
    /// Выключается там, где случайный клик мимо стоит дорого: длинная форма
    /// с уже сделанными настройками, идущая операция. Escape при этом
    /// продолжает работать — это осознанный жест, а не промах.
    pub fn close_on_outside_click(mut self, close: bool) -> Self {
        self.close_on_outside_click = close;
        self
    }

    pub fn backdrop(mut self, backdrop: bool) -> Self {
        self.backdrop = backdrop;
        self
    }

    pub fn backdrop_color(mut self, color: Color) -> Self {
        self.backdrop_color = color;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(Dimension::Px(width));
        self
    }

    pub fn anchor(mut self, anchor: PortalAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn on_close(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_close = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Widget for Portal {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PortalElement {
            id: ElementId::new(),
            is_open: self.is_open,
            modal: self.modal,
            close_on_outside_click: self.close_on_outside_click,
            backdrop: self.backdrop,
            backdrop_color: self.backdrop_color,
            width: self.width,
            anchor: self.anchor,
            on_close: self.on_close.clone(),
            child_ids: Vec::new(),
            bounds: Rect::zero(),
            viewport_size: Cell::new(Size::zero()),
            content_size: Cell::new(Size::zero()),
            classes: Vec::new(),
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
}

struct PortalElement {
    id: ElementId,
    is_open: RwSignal<bool>,
    modal: bool,
    close_on_outside_click: bool,
    backdrop: bool,
    backdrop_color: Color,
    width: Option<Dimension>,
    anchor: PortalAnchor,
    on_close: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    child_ids: Vec<ElementId>,
    bounds: Rect,
    viewport_size: Cell<Size>,
    content_size: Cell<Size>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    overlay_registered: bool,
    mss: MssFields,
}

impl PortalElement {
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

    fn content_rect(&self) -> Rect {
        let viewport = self.viewport_size.get();
        let content = self.content_size.get();
        let w = if let Some(ref dim) = self.width {
            dim.resolve(viewport.width)
        } else {
            content.width
        };
        let h = content.height;
        let (x, y) = match self.anchor {
            PortalAnchor::Center => (
                (viewport.width - w) / 2.0,
                (viewport.height - h) / 2.0,
            ),
            PortalAnchor::BottomEnd { margin_bottom, margin_right } => (
                viewport.width - w - margin_right,
                viewport.height - h - margin_bottom,
            ),
            PortalAnchor::TopEnd { margin_top, margin_right } => (
                viewport.width - w - margin_right,
                margin_top,
            ),
            PortalAnchor::BottomStart { margin_bottom, margin_left } => (
                margin_left,
                viewport.height - h - margin_bottom,
            ),
        };
        Rect::new(Point::new(x, y), Size::new(w, h))
    }
}

impl Element for PortalElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(p) = widget.as_any().downcast_ref::<Portal>() {
            self.is_open = p.is_open;
            self.modal = p.modal;
            self.close_on_outside_click = p.close_on_outside_click;
            self.backdrop = p.backdrop;
            self.backdrop_color = p.backdrop_color;
            self.width = p.width;
            self.anchor = p.anchor;
            self.on_close = p.on_close.clone();
            self.mark_dirty(DirtyFlags::RENDER);
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

    fn layout_hint(&self) -> LayoutHint {
        let (anchor, margin_a, margin_b) = match self.anchor {
            PortalAnchor::Center => (0, 0.0, 0.0),
            PortalAnchor::BottomEnd { margin_bottom, margin_right } => (1, margin_bottom, margin_right),
            PortalAnchor::TopEnd { margin_top, margin_right } => (2, margin_top, margin_right),
            PortalAnchor::BottomStart { margin_bottom, margin_left } => (3, margin_bottom, margin_left),
        };
        LayoutHint::Portal { anchor, margin_a, margin_b }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_open() {
            list.push_clip(Rect::zero());
            return;
        }

        list.begin_overlay_absolute();

        let viewport = list.surface_size();
        self.viewport_size.set(viewport);

        if self.backdrop {
            let backdrop_rect = Rect::new(Point::zero(), viewport);
            list.push_rect(backdrop_rect, self.backdrop_color, [0.0; 4]);
        }

    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_open() {
            list.pop_clip();
            return;
        }

        list.end_overlay();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let is_open = self.is_open();

        if is_open && !self.overlay_registered {
            let viewport = self.viewport_size.get();
            let overlay_bounds = if self.modal {
                Rect::new(Point::zero(), viewport)
            } else {
                self.content_rect()
            };
            ctx.register_overlay(overlay_bounds, self.modal);
            self.overlay_registered = true;
            ctx.request_layout();
            ctx.request_paint();
        } else if !is_open && self.overlay_registered {
            ctx.unregister_overlay();
            self.overlay_registered = false;
        }

        if !is_open {
            return EventResult::Ignored;
        }

        match event {
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.modal {
                    let content = self.content_rect();
                    if !content.contains(*position) {
                        if self.close_on_outside_click {
                            self.close(ctx);
                        }
                        // Клик по подложке модала не проходит вниз в любом
                        // случае: иначе он попал бы в страницу под диалогом.
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::KeyDown(crate::input::Key::Escape) => {
                self.close(ctx);
                EventResult::Handled
            }
            Event::MouseMove(_) => EventResult::Ignored,
            _ => EventResult::Ignored,
        }
    }

    fn active_child_count(&self) -> usize {
        if self.is_open() { usize::MAX } else { 0 }
    }

    fn children(&self) -> &[ElementId] { &self.child_ids }
    fn bounds(&self) -> Rect { self.bounds }

    fn hit_test(&self, _point: Point) -> bool {
        self.is_open()
    }

    fn overlay_request(&self) -> Option<(Rect, bool)> {
        if !self.is_open() { return None; }
        let bounds = if self.modal {
            Rect::new(Point::zero(), self.viewport_size.get())
        } else {
            self.content_rect()
        };
        Some((bounds, self.modal))
    }
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
    fn element_type_name(&self) -> &str { "Portal" }

    fn set_content_size(&mut self, size: Size) {
        self.content_size.set(size);
    }

    fn set_viewport_size(&mut self, size: Size) {
        self.viewport_size.set(size);
    }

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        let w = self.width.as_ref().map(|d| d.resolve(self.viewport_size.get().width));
        (w, None)
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(d) = self.mss.width { self.width = Some(d); }
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

impl StyledElement for PortalElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
