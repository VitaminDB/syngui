use super::IntoWidget;
use crate::animation::transition::mss_color_to_core;
use crate::core::{Color, Point, Rect, RectExt, Size, Transform};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::signal::RwSignal;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::f32::consts::PI;
use std::time::Duration;

const DEFAULT_HANDLE_SIZE: f32 = 16.0;
const DEFAULT_BORDER_WIDTH: f32 = 1.5;
const DEFAULT_MIN_SIZE: f32 = 20.0;
const ROTATION_HANDLE_DISTANCE: f32 = 24.0;

fn default_border_color() -> Color {
    Color::from_hex("#3B82F6")
}
fn default_handle_color() -> Color {
    Color::WHITE
}
fn default_handle_border_color() -> Color {
    Color::from_hex("#3B82F6")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandleId {
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
    Rotate,
    Body,
}

#[derive(Clone, Copy, Debug)]
pub struct TransformState {
    pub position: Point,
    pub size: Size,
    pub rotation_deg: f32,
}

pub struct TransformBox {
    child: Option<Box<dyn Widget>>,
    resizable: bool,
    rotatable: bool,
    moveable: bool,
    active: Option<RwSignal<bool>>,
    position: Option<RwSignal<Point>>,
    size_override: Option<RwSignal<Size>>,
    rotation: Option<RwSignal<f32>>,
    initial_width: Option<f32>,
    initial_height: Option<f32>,
    min_width: f32,
    min_height: f32,
}

impl TransformBox {
    pub fn new() -> Self {
        Self {
            child: None,
            resizable: true,
            rotatable: true,
            moveable: true,
            active: None,
            position: None,
            size_override: None,
            rotation: None,
            initial_width: None,
            initial_height: None,
            min_width: DEFAULT_MIN_SIZE,
            min_height: DEFAULT_MIN_SIZE,
        }
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn resizable(mut self, v: bool) -> Self {
        self.resizable = v;
        self
    }
    pub fn rotatable(mut self, v: bool) -> Self {
        self.rotatable = v;
        self
    }
    pub fn moveable(mut self, v: bool) -> Self {
        self.moveable = v;
        self
    }

    pub fn active(mut self, signal: RwSignal<bool>) -> Self {
        self.active = Some(signal);
        self
    }

    pub fn position(mut self, signal: RwSignal<Point>) -> Self {
        self.position = Some(signal);
        self
    }

    pub fn size_signal(mut self, signal: RwSignal<Size>) -> Self {
        self.size_override = Some(signal);
        self
    }

    pub fn rotation(mut self, signal: RwSignal<f32>) -> Self {
        self.rotation = Some(signal);
        self
    }

    pub fn initial_size(mut self, w: f32, h: f32) -> Self {
        self.initial_width = Some(w);
        self.initial_height = Some(h);
        self
    }

    pub fn min_size(mut self, w: f32, h: f32) -> Self {
        self.min_width = w;
        self.min_height = h;
        self
    }
}

impl Default for TransformBox {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TransformBox {
    fn create_element(&self) -> Box<dyn Element> {
        let initial_size = Size::new(
            self.initial_width.unwrap_or(0.0),
            self.initial_height.unwrap_or(0.0),
        );
        Box::new(TransformBoxElement {
            id: ElementId::new(),
            child_id: None,
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            resizable: self.resizable,
            rotatable: self.rotatable,
            moveable: self.moveable,
            active: self.active,
            position_signal: self.position,
            size_signal: self.size_override,
            rotation_signal: self.rotation,
            offset: Point::zero(),
            current_size: initial_size,
            initial_size_set: self.initial_width.is_some(),
            rotation_deg: 0.0,
            min_width: self.min_width,
            min_height: self.min_height,
            drag_mode: None,
            drag_start_mouse_screen: Point::zero(),
            drag_start_mouse_local: Point::zero(),
            drag_start_offset: Point::zero(),
            drag_start_size: Size::zero(),
            hovered_handle: None,
            mss_border_color: None,
            mss_border_width: None,
            mss_handle_size: None,
            mss_handle_color: None,
            mss_handle_border_color: None,
            needs_transform: false,
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

struct TransformBoxElement {
    id: ElementId,
    child_id: Option<ElementId>,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,

    resizable: bool,
    rotatable: bool,
    moveable: bool,

    active: Option<RwSignal<bool>>,
    position_signal: Option<RwSignal<Point>>,
    size_signal: Option<RwSignal<Size>>,
    rotation_signal: Option<RwSignal<f32>>,

    offset: Point,
    current_size: Size,
    initial_size_set: bool,
    rotation_deg: f32,

    min_width: f32,
    min_height: f32,

    drag_mode: Option<HandleId>,
    drag_start_mouse_screen: Point,
    drag_start_mouse_local: Point,
    drag_start_offset: Point,
    drag_start_size: Size,

    hovered_handle: Option<HandleId>,

    mss_border_color: Option<Color>,
    mss_border_width: Option<f32>,
    mss_handle_size: Option<f32>,
    mss_handle_color: Option<Color>,
    mss_handle_border_color: Option<Color>,

    needs_transform: bool,
}

impl TransformBoxElement {
    fn is_active(&self) -> bool {
        self.active.map(|s| s.get_untracked()).unwrap_or(false)
    }

    fn handle_size(&self) -> f32 {
        self.mss_handle_size.unwrap_or(DEFAULT_HANDLE_SIZE)
    }

    fn border_color(&self) -> Color {
        self.mss_border_color.unwrap_or_else(default_border_color)
    }

    fn border_width(&self) -> f32 {
        self.mss_border_width.unwrap_or(DEFAULT_BORDER_WIDTH)
    }

    fn handle_color(&self) -> Color {
        self.mss_handle_color.unwrap_or_else(default_handle_color)
    }

    fn handle_border_color(&self) -> Color {
        self.mss_handle_border_color
            .unwrap_or_else(default_handle_border_color)
    }

    fn local_center(&self) -> Point {
        Point::new(
            self.bounds.origin.x + self.current_size.width * 0.5,
            self.bounds.origin.y + self.current_size.height * 0.5,
        )
    }

    fn visual_center(&self) -> Point {
        let c = self.local_center();
        Point::new(c.x + self.offset.x, c.y + self.offset.y)
    }

    fn screen_to_local(&self, screen_pos: Point) -> Point {
        let cx = self.bounds.origin.x + self.current_size.width * 0.5;
        let cy = self.bounds.origin.y + self.current_size.height * 0.5;
        let dx = screen_pos.x - (cx + self.offset.x);
        let dy = screen_pos.y - (cy + self.offset.y);
        let rad = -self.rotation_deg * PI / 180.0;
        let cos = rad.cos();
        let sin = rad.sin();
        let rx = dx * cos - dy * sin;
        let ry = dx * sin + dy * cos;
        Point::new(cx + rx, cy + ry)
    }

    fn handle_rect_at(&self, center: Point) -> Rect {
        let hs = self.handle_size();
        let half = hs * 0.5;
        Rect::new(
            Point::new(center.x - half, center.y - half),
            Size::new(hs, hs),
        )
    }

    fn handle_centers(&self) -> [(HandleId, Point); 9] {
        let gap = 3.0_f32;
        let x0 = self.bounds.origin.x - gap;
        let y0 = self.bounds.origin.y - gap;
        let x1 = x0 + self.current_size.width + gap * 2.0;
        let y1 = y0 + self.current_size.height + gap * 2.0;
        let mx = (x0 + x1) * 0.5;
        let my = (y0 + y1) * 0.5;

        [
            (HandleId::TopLeft, Point::new(x0, y0)),
            (HandleId::Top, Point::new(mx, y0)),
            (HandleId::TopRight, Point::new(x1, y0)),
            (HandleId::Left, Point::new(x0, my)),
            (HandleId::Right, Point::new(x1, my)),
            (HandleId::BottomLeft, Point::new(x0, y1)),
            (HandleId::Bottom, Point::new(mx, y1)),
            (HandleId::BottomRight, Point::new(x1, y1)),
            (
                HandleId::Rotate,
                Point::new(mx, y0 - ROTATION_HANDLE_DISTANCE),
            ),
        ]
    }

    fn hit_test(&self, screen_pos: Point) -> Option<HandleId> {
        let local = self.screen_to_local(screen_pos);

        if self.rotatable {
            let handles = self.handle_centers();
            let (_, center) = handles[8];
            if self.handle_rect_at(center).contains(local) {
                return Some(HandleId::Rotate);
            }
        }

        if self.resizable {
            let handles = self.handle_centers();
            for &(id, center) in &handles[..8] {
                if self.handle_rect_at(center).contains(local) {
                    return Some(id);
                }
            }
        }

        if self.moveable && self.bounds.contains(local) {
            return Some(HandleId::Body);
        }

        None
    }

    fn cursor_for_handle(&self, handle: HandleId) -> CursorIcon {
        match handle {
            HandleId::Body => CursorIcon::Move,
            HandleId::Rotate => CursorIcon::Crosshair,
            _ => {
                let base_angle = match handle {
                    HandleId::Top => 0.0,
                    HandleId::TopRight => 45.0,
                    HandleId::Right => 90.0,
                    HandleId::BottomRight => 135.0,
                    HandleId::Bottom => 180.0,
                    HandleId::BottomLeft => 225.0,
                    HandleId::Left => 270.0,
                    HandleId::TopLeft => 315.0,
                    _ => 0.0,
                };
                let angle = (base_angle + self.rotation_deg).rem_euclid(360.0);
                let sector = ((angle + 22.5) / 45.0) as u32 % 8;
                match sector {
                    0 => CursorIcon::NResize,
                    1 => CursorIcon::NeResize,
                    2 => CursorIcon::EResize,
                    3 => CursorIcon::SeResize,
                    4 => CursorIcon::SResize,
                    5 => CursorIcon::SwResize,
                    6 => CursorIcon::WResize,
                    7 => CursorIcon::NwResize,
                    _ => CursorIcon::Default,
                }
            }
        }
    }

    fn update_needs_transform(&mut self) {
        self.needs_transform = self.rotation_deg.abs() > 0.001
            || self.offset.x.abs() > 0.001
            || self.offset.y.abs() > 0.001;
    }

    fn sync_to_signals(&self) {
        if let Some(s) = self.position_signal {
            s.set(self.offset);
        }
        if let Some(s) = self.size_signal {
            s.set(self.current_size);
        }
        if let Some(s) = self.rotation_signal {
            s.set(self.rotation_deg);
        }
    }

    fn sync_from_signals(&mut self) {
        if self.drag_mode.is_some() {
            return;
        }
        if let Some(s) = self.position_signal {
            self.offset = s.get_untracked();
        }
        if let Some(s) = self.size_signal {
            self.current_size = s.get_untracked();
        }
        if let Some(s) = self.rotation_signal {
            self.rotation_deg = s.get_untracked();
        }
        self.update_needs_transform();
    }

    fn apply_resize(&mut self, handle: HandleId, mouse_local: Point) {
        let dx = mouse_local.x - self.drag_start_mouse_local.x;
        let dy = mouse_local.y - self.drag_start_mouse_local.y;
        let sw = self.drag_start_size.width;
        let sh = self.drag_start_size.height;

        let (new_w, new_h, affects_left, affects_top) = match handle {
            HandleId::BottomRight => (sw + dx, sh + dy, false, false),
            HandleId::BottomLeft => (sw - dx, sh + dy, true, false),
            HandleId::TopRight => (sw + dx, sh - dy, false, true),
            HandleId::TopLeft => (sw - dx, sh - dy, true, true),
            HandleId::Right => (sw + dx, sh, false, false),
            HandleId::Left => (sw - dx, sh, true, false),
            HandleId::Bottom => (sw, sh + dy, false, false),
            HandleId::Top => (sw, sh - dy, false, true),
            _ => return,
        };

        let clamped_w = new_w.max(self.min_width);
        let clamped_h = new_h.max(self.min_height);

        let local_dx = if affects_left { sw - clamped_w } else { 0.0 };
        let local_dy = if affects_top { sh - clamped_h } else { 0.0 };

        if local_dx.abs() > 0.001 || local_dy.abs() > 0.001 {
            let rad = self.rotation_deg * PI / 180.0;
            let cos = rad.cos();
            let sin = rad.sin();
            let screen_dx = local_dx * cos - local_dy * sin;
            let screen_dy = local_dx * sin + local_dy * cos;
            self.offset = Point::new(
                self.drag_start_offset.x + screen_dx,
                self.drag_start_offset.y + screen_dy,
            );
        } else {
            self.offset = self.drag_start_offset;
        }

        self.current_size = Size::new(clamped_w, clamped_h);
        self.update_needs_transform();
    }

    fn build_transform(&self) -> Transform {
        let cx = self.bounds.origin.x + self.current_size.width * 0.5;
        let cy = self.bounds.origin.y + self.current_size.height * 0.5;
        let rad = self.rotation_deg * PI / 180.0;
        let mut t = Transform::identity();
        t = t.then(&Transform::translation(-cx, -cy));
        if rad.abs() > 0.0001 {
            t = t.then_rotate(euclid::Angle::radians(rad));
        }
        t = t.then_translate(euclid::Vector2D::new(
            cx + self.offset.x,
            cy + self.offset.y,
        ));
        t
    }
}

impl Element for TransformBoxElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(tb) = widget.as_any().downcast_ref::<TransformBox>() {
            self.resizable = tb.resizable;
            self.rotatable = tb.rotatable;
            self.moveable = tb.moveable;
            self.active = tb.active;
            self.position_signal = tb.position;
            self.size_signal = tb.size_override;
            self.rotation_signal = tb.rotation;
            self.min_width = tb.min_width;
            self.min_height = tb.min_height;
            self.sync_from_signals();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        if !self.initial_size_set {
            if self.current_size.width < 1.0 && constraints.max_width.is_finite() {
                self.current_size.width = constraints.max_width;
            }
            if self.current_size.height < 1.0 && constraints.max_height.is_finite() {
                self.current_size.height = constraints.max_height;
            }
            self.initial_size_set = true;
        }

        let w = self.current_size.width.max(self.min_width);
        let h = self.current_size.height.max(self.min_height);
        self.current_size = Size::new(w, h);
        self.bounds = Rect::new(Point::zero(), self.current_size);
        self.current_size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.needs_transform {
            list.push_transform(self.build_transform());
        }
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.is_active() {
            let bc = self.border_color();
            let bw = self.border_width();
            let gap = 3.0_f32;

            let border_rect = Rect::new(
                Point::new(self.bounds.origin.x - gap, self.bounds.origin.y - gap),
                Size::new(
                    self.current_size.width + gap * 2.0,
                    self.current_size.height + gap * 2.0,
                ),
            );
            list.push_rect_bordered(
                border_rect,
                Color::TRANSPARENT,
                [0.0; 4],
                Border::new(bw, bc),
            );

            if self.rotatable {
                let top_center_x = self.bounds.origin.x + self.current_size.width * 0.5;
                let top_center_y = self.bounds.origin.y - gap;
                let rot_y = top_center_y - ROTATION_HANDLE_DISTANCE;

                list.push_line_strip(
                    vec![[top_center_x, top_center_y], [top_center_x, rot_y]],
                    bc,
                    bw,
                );

                let hs = self.handle_size();
                let rot_rect = Rect::new(
                    Point::new(top_center_x - hs * 0.5, rot_y - hs * 0.5),
                    Size::new(hs, hs),
                );
                list.push_rect_bordered(
                    rot_rect,
                    self.handle_color(),
                    [hs * 0.5; 4],
                    Border::new(bw, self.handle_border_color()),
                );
            }

            if self.resizable {
                let x0 = border_rect.origin.x;
                let y0 = border_rect.origin.y;
                let x1 = x0 + border_rect.size.width;
                let y1 = y0 + border_rect.size.height;
                let mx = (x0 + x1) * 0.5;
                let my = (y0 + y1) * 0.5;
                let handle_positions = [
                    Point::new(x0, y0),
                    Point::new(mx, y0),
                    Point::new(x1, y0),
                    Point::new(x0, my),
                    Point::new(x1, my),
                    Point::new(x0, y1),
                    Point::new(mx, y1),
                    Point::new(x1, y1),
                ];
                let hs = self.handle_size();
                let hc = self.handle_color();
                let hbc = self.handle_border_color();

                for center in &handle_positions {
                    let rect = Rect::new(
                        Point::new(center.x - hs * 0.5, center.y - hs * 0.5),
                        Size::new(hs, hs),
                    );
                    list.push_rect_bordered(rect, hc, [1.0; 4], Border::new(bw, hbc));
                }
            }
        }

        if self.needs_transform {
            list.pop_transform();
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if !self.is_active() {
            return EventResult::Ignored;
        }

        match event {
            Event::MouseDown {
                button: MouseButton::Left,
                position,
            } => {
                if let Some(handle) = self.hit_test(*position) {
                    self.drag_mode = Some(handle);
                    self.drag_start_mouse_screen = *position;
                    self.drag_start_mouse_local = self.screen_to_local(*position);
                    self.drag_start_offset = self.offset;
                    self.drag_start_size = self.current_size;
                    ctx.set_cursor(self.cursor_for_handle(handle));
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }

            Event::MouseMove(pos) => {
                if let Some(handle) = self.drag_mode {
                    match handle {
                        HandleId::Body => {
                            let dx = pos.x - self.drag_start_mouse_screen.x;
                            let dy = pos.y - self.drag_start_mouse_screen.y;
                            self.offset = Point::new(
                                self.drag_start_offset.x + dx,
                                self.drag_start_offset.y + dy,
                            );
                            self.update_needs_transform();
                            self.sync_to_signals();
                            ctx.set_cursor(CursorIcon::Move);
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        HandleId::Rotate => {
                            let vc = self.visual_center();
                            let dx = pos.x - vc.x;
                            let dy = pos.y - vc.y;
                            self.rotation_deg = dx.atan2(-dy) * 180.0 / PI;
                            self.update_needs_transform();
                            self.sync_to_signals();
                            ctx.set_cursor(CursorIcon::Crosshair);
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        _ => {
                            let mouse_local = self.screen_to_local(*pos);
                            self.apply_resize(handle, mouse_local);
                            self.sync_to_signals();
                            ctx.set_cursor(self.cursor_for_handle(handle));
                            ctx.request_layout();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                }

                let prev = self.hovered_handle;
                self.hovered_handle = self.hit_test(*pos);
                if let Some(h) = self.hovered_handle {
                    ctx.set_cursor(self.cursor_for_handle(h));
                    if prev != self.hovered_handle {
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }
                if prev.is_some() {
                    ctx.request_paint();
                }
                EventResult::Ignored
            }

            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if self.drag_mode.is_some() {
                    self.drag_mode = None;
                    self.sync_to_signals();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }

            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        false
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
        "TransformBox"
    }

    fn intercepts_child_events(&self) -> bool {
        self.is_active()
    }

    fn hit_test(&self, point: Point) -> bool {
        let local = self.screen_to_local(point);
        if self.is_active() {
            let gap = ROTATION_HANDLE_DISTANCE + self.handle_size();
            let inflated = Rect::new(
                Point::new(self.bounds.x() - gap, self.bounds.y() - gap),
                Size::new(
                    self.bounds.size.width + 2.0 * gap,
                    self.bounds.size.height + 2.0 * gap,
                ),
            );
            inflated.contains(local)
        } else {
            self.bounds.contains(local)
        }
    }

    fn scroll_offset(&self) -> Point {
        Point::new(-self.offset.x, -self.offset.y)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Container {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        (
            Some(self.current_size.width),
            Some(self.current_size.height),
        )
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
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.apply_style(style);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        _active: Option<&ComputedStyle>,
        _focus: Option<&ComputedStyle>,
        _selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss.apply_transitions(base, hover, None, None, None);
    }
}

impl StyledElement for TransformBoxElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);

        self.mss_border_color = style
            .get("--tb-border-color")
            .and_then(|v| v.as_color())
            .map(mss_color_to_core);
        self.mss_border_width = style.get("--tb-border-width").and_then(|v| v.as_px());
        self.mss_handle_size = style.get("--tb-handle-size").and_then(|v| v.as_px());
        self.mss_handle_color = style
            .get("--tb-handle-color")
            .and_then(|v| v.as_color())
            .map(mss_color_to_core);
        self.mss_handle_border_color = style
            .get("--tb-handle-border-color")
            .and_then(|v| v.as_color())
            .map(mss_color_to_core);

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
