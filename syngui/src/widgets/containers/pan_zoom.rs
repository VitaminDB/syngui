use std::any::Any;
use std::sync::Arc;

use crate::core::{Color, Point, Rect, Size, Transform};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::signal::RwSignal;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use crate::widget::context::EventContext;

use super::IntoWidget;

pub type BackgroundCallback = Arc<dyn Fn(Point, Point) + Send + Sync>;

pub type PanFilter = Arc<dyn Fn(Point) -> bool + Send + Sync>;

pub struct PanZoomViewport {
    pub child: Option<Box<dyn Widget>>,
    pub pan: Option<RwSignal<Point>>,
    pub zoom: Option<RwSignal<f32>>,
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub zoom_speed: f32,
    pub grid: bool,
    pub grid_step: f32,
    pub pan_button: MouseButton,
    pub on_background_click: Option<BackgroundCallback>,
    pub on_background_context_menu: Option<BackgroundCallback>,
    pub pan_filter: Option<PanFilter>,
    pub classes: Vec<String>,
}

impl PanZoomViewport {
    pub fn new() -> Self {
        Self {
            child: None,
            pan: None,
            zoom: None,
            min_zoom: 0.25,
            max_zoom: 4.0,
            zoom_speed: 0.0015,
            grid: false,
            grid_step: 50.0,
            pan_button: MouseButton::Middle,
            on_background_click: None,
            on_background_context_menu: None,
            pan_filter: None,
            classes: Vec::new(),
        }
    }

    pub fn pan_filter(mut self, filter: impl Fn(Point) -> bool + Send + Sync + 'static) -> Self {
        self.pan_filter = Some(Arc::new(filter));
        self
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn pan(mut self, signal: RwSignal<Point>) -> Self {
        self.pan = Some(signal);
        self
    }

    pub fn zoom(mut self, signal: RwSignal<f32>) -> Self {
        self.zoom = Some(signal);
        self
    }

    pub fn zoom_range(mut self, min: f32, max: f32) -> Self {
        self.min_zoom = min;
        self.max_zoom = max;
        self
    }

    pub fn zoom_speed(mut self, speed: f32) -> Self {
        self.zoom_speed = speed;
        self
    }

    pub fn grid(mut self, on: bool) -> Self {
        self.grid = on;
        self
    }

    pub fn grid_step(mut self, step: f32) -> Self {
        self.grid_step = step.max(1.0);
        self
    }

    pub fn pan_button(mut self, button: MouseButton) -> Self {
        self.pan_button = button;
        self
    }

    pub fn on_background_click(mut self, cb: impl Fn(Point, Point) + Send + Sync + 'static) -> Self {
        self.on_background_click = Some(Arc::new(cb));
        self
    }

    pub fn on_background_context_menu(mut self, cb: impl Fn(Point, Point) + Send + Sync + 'static) -> Self {
        self.on_background_context_menu = Some(Arc::new(cb));
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Default for PanZoomViewport {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for PanZoomViewport {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PanZoomViewportElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            child_id: None,
            pan: self.pan,
            zoom: self.zoom,
            min_zoom: self.min_zoom,
            max_zoom: self.max_zoom,
            zoom_speed: self.zoom_speed,
            grid: self.grid,
            grid_step: self.grid_step,
            pan_button: self.pan_button,
            on_background_click: self.on_background_click.clone(),
            on_background_context_menu: self.on_background_context_menu.clone(),
            pan_filter: self.pan_filter.clone(),
            pan_drag_start: None,
            lmb_pan_pending: None,
            lmb_pan_drag: None,
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            current_pan: Point::zero(),
            current_zoom: 1.0,
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

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct PanZoomViewportElement {
    id: ElementId,
    bounds: Rect,
    child_id: Option<ElementId>,
    pan: Option<RwSignal<Point>>,
    zoom: Option<RwSignal<f32>>,
    min_zoom: f32,
    max_zoom: f32,
    zoom_speed: f32,
    grid: bool,
    grid_step: f32,
    pan_button: MouseButton,
    on_background_click: Option<BackgroundCallback>,
    on_background_context_menu: Option<BackgroundCallback>,
    pan_filter: Option<PanFilter>,
    pan_drag_start: Option<(Point, Point)>,
    lmb_pan_pending: Option<(Point, Point)>,
    lmb_pan_drag: Option<(Point, Point)>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    current_pan: Point,
    current_zoom: f32,
}

impl PanZoomViewportElement {
    fn refresh_state(&mut self) {
        if let Some(s) = self.pan {
            self.current_pan = s.get_untracked();
        }
        if let Some(s) = self.zoom {
            self.current_zoom = s.get_untracked().clamp(self.min_zoom, self.max_zoom);
        } else {
            self.current_zoom = 1.0;
        }
    }

    fn screen_to_world(&self, screen: Point) -> Point {
        let local = Point::new(screen.x - self.bounds.origin.x, screen.y - self.bounds.origin.y);
        Point::new(
            (local.x - self.current_pan.x) / self.current_zoom,
            (local.y - self.current_pan.y) / self.current_zoom,
        )
    }

    fn apply_zoom(&mut self, delta: f32, around_screen: Point) {
        let Some(zoom_sig) = self.zoom else { return; };
        let old_zoom = self.current_zoom.max(0.0001);
        let new_zoom = (old_zoom * (delta * self.zoom_speed).exp())
            .clamp(self.min_zoom, self.max_zoom);
        if (new_zoom - old_zoom).abs() < 1e-5 { return; }

        let cursor_local = Point::new(
            around_screen.x - self.bounds.origin.x,
            around_screen.y - self.bounds.origin.y,
        );
        let factor = new_zoom / old_zoom;
        let new_pan = Point::new(
            cursor_local.x - (cursor_local.x - self.current_pan.x) * factor,
            cursor_local.y - (cursor_local.y - self.current_pan.y) * factor,
        );

        if let Some(pan_sig) = self.pan {
            pan_sig.set(new_pan);
        }
        zoom_sig.set(new_zoom);
        self.current_pan = new_pan;
        self.current_zoom = new_zoom;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

impl Element for PanZoomViewportElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<PanZoomViewport>() {
            self.pan = w.pan;
            self.zoom = w.zoom;
            self.min_zoom = w.min_zoom;
            self.max_zoom = w.max_zoom;
            self.zoom_speed = w.zoom_speed;
            self.grid = w.grid;
            self.grid_step = w.grid_step;
            self.pan_button = w.pan_button;
            self.pan_filter = w.pan_filter.clone();
            self.on_background_click = w.on_background_click.clone();
            self.on_background_context_menu = w.on_background_context_menu.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
        if let Some(s) = self.pan {
            s.subscribe_element(self.id);
        }
        if let Some(s) = self.zoom {
            s.subscribe_element(self.id);
        }
        self.refresh_state();
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 };
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        self.refresh_state();
        Size::new(w, h)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::PanZoom
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::TRANSPARENT);
        if bg.a > 0.0 {
            list.push_rect(self.bounds, bg, [0.0; 4]);
        }

        if self.grid {
            self.draw_grid(list);
        }

        list.push_clip(self.bounds);

        let t = Transform::scale(self.current_zoom, self.current_zoom)
            .then(&Transform::translation(
                self.current_pan.x,
                self.current_pan.y,
            ));
        list.push_transform(t);
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        list.pop_transform();
        list.pop_clip();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        const LMB_PAN_THRESHOLD: f32 = 4.0;

        match event {
            Event::MouseWheel { delta, position, .. } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                self.apply_zoom(*delta, *position);
                ctx.capture();
                EventResult::Handled
            }
            Event::MouseDown { button, position } if *button == self.pan_button && self.bounds.contains(*position) => {
                self.pan_drag_start = Some((*position, self.current_pan));
                ctx.set_cursor(CursorIcon::Grabbing);
                ctx.capture();
                EventResult::Handled
            }
            Event::MouseMove(pos) => {
                if let Some((start_screen, start_pan)) = self.pan_drag_start {
                    let dx = pos.x - start_screen.x;
                    let dy = pos.y - start_screen.y;
                    let new_pan = Point::new(start_pan.x + dx, start_pan.y + dy);
                    if let Some(s) = self.pan {
                        s.set(new_pan);
                    }
                    self.current_pan = new_pan;
                    ctx.set_cursor(CursorIcon::Grabbing);
                    self.mark_dirty(DirtyFlags::RENDER);
                    return EventResult::Handled;
                }
                if let Some((start_screen, start_pan)) = self.lmb_pan_drag {
                    let dx = pos.x - start_screen.x;
                    let dy = pos.y - start_screen.y;
                    let new_pan = Point::new(start_pan.x + dx, start_pan.y + dy);
                    if let Some(s) = self.pan {
                        s.set(new_pan);
                    }
                    self.current_pan = new_pan;
                    ctx.set_cursor(CursorIcon::Grabbing);
                    self.mark_dirty(DirtyFlags::RENDER);
                    return EventResult::Handled;
                }
                if let Some((start_screen, start_pan)) = self.lmb_pan_pending {
                    let dx = pos.x - start_screen.x;
                    let dy = pos.y - start_screen.y;
                    if dx.abs() >= LMB_PAN_THRESHOLD || dy.abs() >= LMB_PAN_THRESHOLD {
                        self.lmb_pan_pending = None;
                        self.lmb_pan_drag = Some((start_screen, start_pan));
                        let new_pan = Point::new(start_pan.x + dx, start_pan.y + dy);
                        if let Some(s) = self.pan {
                            s.set(new_pan);
                        }
                        self.current_pan = new_pan;
                        ctx.set_cursor(CursorIcon::Grabbing);
                        self.mark_dirty(DirtyFlags::RENDER);
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } if *button == self.pan_button && self.pan_drag_start.is_some() => {
                self.pan_drag_start = None;
                ctx.set_cursor(CursorIcon::Default);
                EventResult::Handled
            }
            Event::MouseUp { button: MouseButton::Left, position } => {
                if self.lmb_pan_drag.take().is_some() {
                    ctx.set_cursor(CursorIcon::Default);
                    return EventResult::Handled;
                }
                if let Some((start_screen, _)) = self.lmb_pan_pending.take() {
                    let dx = position.x - start_screen.x;
                    let dy = position.y - start_screen.y;
                    if dx.abs() < LMB_PAN_THRESHOLD && dy.abs() < LMB_PAN_THRESHOLD {
                        if let Some(cb) = &self.on_background_click {
                            let world = self.screen_to_world(*position);
                            cb(world, *position);
                        }
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button: MouseButton::Left, position } if self.bounds.contains(*position) => {
                if let Some(filter) = &self.pan_filter {
                    let world = self.screen_to_world(*position);
                    if !filter(world) {
                        return EventResult::Ignored;
                    }
                }
                self.lmb_pan_pending = Some((*position, self.current_pan));
                ctx.capture();
                EventResult::Handled
            }
            Event::MouseDown { button: MouseButton::Right, position } if self.bounds.contains(*position) => {
                if let Some(cb) = &self.on_background_context_menu {
                    let world = self.screen_to_world(*position);
                    cb(world, *position);
                    ctx.capture();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
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

    fn mount(&mut self, _tree: &mut ElementTree) {
        if let Some(s) = self.pan {
            s.subscribe_element(self.id);
        }
        if let Some(s) = self.zoom {
            s.subscribe_element(self.id);
        }
        self.refresh_state();
    }

    fn animate(&mut self, _dt: std::time::Duration) -> bool {
        let prev_pan = self.current_pan;
        let prev_zoom = self.current_zoom;
        self.refresh_state();
        if self.current_pan != prev_pan || (self.current_zoom - prev_zoom).abs() > 1e-5 {
            self.mark_dirty(DirtyFlags::RENDER);
            true
        } else {
            false
        }
    }

    fn needs_repaint(&self) -> bool {
        let pan_diff = self.pan.map_or(false, |s| s.get_untracked() != self.current_pan);
        let zoom_diff = self.zoom.map_or(false, |s| {
            (s.get_untracked().clamp(self.min_zoom, self.max_zoom) - self.current_zoom).abs() > 1e-5
        });
        pan_diff || zoom_diff
    }

    fn scroll_offset(&self) -> Point {
        Point::new(-self.current_pan.x, -self.current_pan.y)
    }

    fn event_scale(&self) -> f32 {
        // EPSILON-floor — safety net против NaN, если кто-то задаст zoom=0
        self.current_zoom.max(f32::EPSILON)
    }

    fn clip_content(&self) -> bool {
        false
    }

    fn element_type_name(&self) -> &str {
        "PanZoomViewport"
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
        self.mark_dirty(DirtyFlags::RENDER);
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

impl PanZoomViewportElement {
    fn draw_grid(&self, list: &mut DisplayList) {
        let zoom = self.current_zoom.max(0.0001);
        let mut step = self.grid_step * zoom;
        let mut major_every = 1u32;
        while step < 8.0 {
            step *= 5.0;
            major_every *= 5;
        }
        let _ = major_every;

        let dot_color = self.mss.color
            .unwrap_or(Color::from_srgb(0xC0, 0xC8, 0xD4, 0.55));

        let origin_x = self.bounds.origin.x + self.current_pan.x;
        let origin_y = self.bounds.origin.y + self.current_pan.y;

        let bx0 = self.bounds.origin.x;
        let by0 = self.bounds.origin.y;
        let bx1 = bx0 + self.bounds.size.width;
        let by1 = by0 + self.bounds.size.height;

        let kx0 = ((bx0 - origin_x) / step).ceil();
        let ky0 = ((by0 - origin_y) / step).ceil();
        let kx1 = ((bx1 - origin_x) / step).floor();
        let ky1 = ((by1 - origin_y) / step).floor();

        let dot = 1.5_f32;
        let nx = ((kx1 - kx0) as i32).max(0);
        let ny = ((ky1 - ky0) as i32).max(0);
        if nx > 400 || ny > 400 {
            return;
        }

        let mut k_y = ky0;
        while k_y <= ky1 {
            let y = origin_y + k_y * step;
            let mut k_x = kx0;
            while k_x <= kx1 {
                let x = origin_x + k_x * step;
                let r = Rect::new(
                    Point::new(x - dot * 0.5, y - dot * 0.5),
                    Size::new(dot, dot),
                );
                list.push_rect(r, dot_color, [dot * 0.5; 4]);
                k_x += 1.0;
            }
            k_y += 1.0;
        }
    }
}

impl StyledElement for PanZoomViewportElement {
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
