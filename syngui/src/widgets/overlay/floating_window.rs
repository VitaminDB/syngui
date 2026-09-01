use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use crate::signal::{RwSignal, use_signal};
use crate::widgets::containers::IntoWidget;
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

const TITLE_BAR_HEIGHT: f32 = 36.0;
const DEFAULT_PADDING: f32 = 12.0;
const DEFAULT_BORDER_RADIUS: f32 = 8.0;
const DEFAULT_TITLE_FONT_SIZE: f32 = 14.0;
const DEFAULT_CLOSE_ICON: &str = "\u{E5CD}";
const DEFAULT_MINIMIZE_ICON: &str = "\u{E931}";

const RESIZE_GRAB_ZONE: f32 = 5.0;
const RESIZE_CORNER_ZONE: f32 = 14.0;
const RESIZE_MIN_FALLBACK: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl ResizeEdge {
    fn cursor(self) -> CursorIcon {
        match self {
            ResizeEdge::Top => CursorIcon::NResize,
            ResizeEdge::Bottom => CursorIcon::SResize,
            ResizeEdge::Left => CursorIcon::WResize,
            ResizeEdge::Right => CursorIcon::EResize,
            ResizeEdge::TopLeft => CursorIcon::NwResize,
            ResizeEdge::TopRight => CursorIcon::NeResize,
            ResizeEdge::BottomLeft => CursorIcon::SwResize,
            ResizeEdge::BottomRight => CursorIcon::SeResize,
        }
    }
}

pub struct FloatingWindow {
    title: String,
    icon: Option<String>,
    is_open: RwSignal<bool>,
    position: RwSignal<Point>,
    size: Size,
    closable: bool,
    centered: bool,
    drag_on_body: bool,
    resizable: bool,
    modal: bool,
    minimizable: bool,
    is_minimized: RwSignal<bool>,
    size_signal: Option<RwSignal<Size>>,
    on_close: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    children: Vec<Box<dyn Widget>>,
}

impl FloatingWindow {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            icon: None,
            is_open: use_signal(false),
            position: use_signal(Point::new(100.0, 100.0)),
            size: Size::new(300.0, 200.0),
            closable: true,
            centered: false,
            drag_on_body: false,
            resizable: false,
            modal: false,
            minimizable: false,
            is_minimized: use_signal(false),
            size_signal: None,
            on_close: None,
            children: Vec::new(),
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn child<M>(mut self, widget: impl IntoWidget<M>) -> Self {
        self.children.push(widget.into_widget());
        self
    }

    pub fn is_open(mut self, state: RwSignal<bool>) -> Self {
        self.is_open = state;
        self
    }

    pub fn position(mut self, pos: RwSignal<Point>) -> Self {
        self.position = pos;
        self
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    pub fn drag_on_body(mut self, enabled: bool) -> Self {
        self.drag_on_body = enabled;
        self
    }

    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    pub fn size_signal(mut self, signal: RwSignal<Size>) -> Self {
        self.size_signal = Some(signal);
        self
    }

    pub fn centered(mut self) -> Self {
        self.centered = true;
        self
    }

    pub fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    pub fn minimizable(mut self, on: bool) -> Self {
        self.minimizable = on;
        self
    }

    pub fn is_minimized(mut self, sig: RwSignal<bool>) -> Self {
        self.is_minimized = sig;
        self
    }

    pub fn on_close(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_close = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Widget for FloatingWindow {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(FloatingWindowElement {
            id: ElementId::new(),
            title: self.title.clone(),
            icon: self.icon.clone(),
            is_open: self.is_open,
            position: self.position,
            base_size: self.size,
            closable: self.closable,
            drag_on_body: self.drag_on_body,
            resizable: self.resizable,
            modal: self.modal,
            minimizable: self.minimizable,
            is_minimized: self.is_minimized,
            size_signal: self.size_signal,
            on_close: self.on_close.clone(),
            child_ids: Vec::new(),
            bounds: Rect::zero(),
            viewport_size: Size::zero(),
            content_size: Size::zero(),
            dragging: false,
            drag_offset: Point::zero(),
            resizing: None,
            resize_start_mouse: Point::zero(),
            resize_start_size: Size::zero(),
            resize_start_pos: Point::zero(),
            user_resized: false,
            hover_close: false,
            hover_minimize: false,
            hover_title_bar: false,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            overlay_registered: false,
            centered: self.centered,
            needs_center: self.centered,
            was_open: false,
            mss: MssFields::new(),
            mss_title_font_size: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
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

struct FloatingWindowElement {
    id: ElementId,
    title: String,
    icon: Option<String>,
    is_open: RwSignal<bool>,
    position: RwSignal<Point>,
    base_size: Size,
    closable: bool,
    drag_on_body: bool,
    resizable: bool,
    modal: bool,
    minimizable: bool,
    is_minimized: RwSignal<bool>,
    size_signal: Option<RwSignal<Size>>,
    on_close: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    child_ids: Vec<ElementId>,
    bounds: Rect,
    viewport_size: Size,
    content_size: Size,
    centered: bool,
    needs_center: bool,
    was_open: bool,
    dragging: bool,
    drag_offset: Point,
    resizing: Option<ResizeEdge>,
    resize_start_mouse: Point,
    resize_start_size: Size,
    resize_start_pos: Point,
    user_resized: bool,
    hover_close: bool,
    hover_minimize: bool,
    hover_title_bar: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    overlay_registered: bool,
    mss: MssFields,
    mss_title_font_size: Option<f32>,
}

impl FloatingWindowElement {
    fn is_open(&self) -> bool {
        self.is_open.get_untracked()
    }

    fn is_minimized_state(&self) -> bool {
        self.minimizable && self.is_minimized.get_untracked()
    }

    fn is_visible_window(&self) -> bool {
        self.is_open() && !self.is_minimized_state()
    }

    fn padding(&self) -> f32 {
        self.mss.padding_left.unwrap_or(DEFAULT_PADDING)
    }

    fn border_radius(&self) -> f32 {
        self.mss.border_radius_uniform(100.0, DEFAULT_BORDER_RADIUS)
    }

    fn title_font_size(&self) -> f32 {
        self.mss_title_font_size.unwrap_or(DEFAULT_TITLE_FONT_SIZE)
    }

    fn resolved_width(&self) -> f32 {
        let vw = self.viewport_size.width;
        if let Some(d) = self.mss.width {
            return d.resolve(vw);
        }
        let max_w = self.mss.max_width.map(|d| d.resolve(vw)).unwrap_or(f32::INFINITY);
        if self.user_resized {
            let min_w = self.mss.min_width.map(|d| d.resolve(vw)).unwrap_or(RESIZE_MIN_FALLBACK);
            return self.base_size.width.clamp(min_w, max_w);
        }
        let min_w = self.mss.min_width.map(|d| d.resolve(vw)).unwrap_or(self.base_size.width);
        let pad = self.padding();
        let needed = self.content_size.width + 2.0 * pad;
        needed.clamp(min_w.min(max_w), max_w)
    }

    fn resolved_height(&self) -> f32 {
        let vh = self.viewport_size.height;
        if let Some(d) = self.mss.height {
            return d.resolve(vh);
        }
        if self.base_size.height > 0.0 {
            return self.base_size.height;
        }
        let pad = self.padding();
        let needed = TITLE_BAR_HEIGHT + 2.0 * pad + self.content_size.height;
        let min_h = self.mss.min_height.map(|d| d.resolve(vh)).unwrap_or(0.0);
        let max_h = self.mss.max_height.map(|d| d.resolve(vh)).unwrap_or(f32::INFINITY);
        needed.clamp(min_h.min(max_h), max_h)
    }

    fn window_rect(&self) -> Rect {
        let pos = self.position.get_untracked();
        Rect::new(pos, Size::new(self.resolved_width(), self.resolved_height()))
    }

    fn title_bar_rect(&self) -> Rect {
        let win = self.window_rect();
        Rect::new(win.origin, Size::new(win.size.width, TITLE_BAR_HEIGHT))
    }

    fn close_button_rect(&self) -> Rect {
        let tb = self.title_bar_rect();
        Rect::new(
            Point::new(tb.x() + tb.size.width - 32.0, tb.y() + 4.0),
            Size::new(28.0, 28.0),
        )
    }

    fn minimize_button_rect(&self) -> Rect {
        let tb = self.title_bar_rect();
        let dx = if self.closable { 64.0 } else { 32.0 };
        Rect::new(
            Point::new(tb.x() + tb.size.width - dx, tb.y() + 4.0),
            Size::new(28.0, 28.0),
        )
    }

    fn title_buttons_reserved(&self) -> f32 {
        let mut r = 0.0;
        if self.closable { r += 32.0; }
        if self.minimizable { r += 32.0; }
        if r > 0.0 { r += 16.0; }
        r
    }

    fn content_rect(&self) -> Rect {
        let win = self.window_rect();
        let pad = self.padding();
        Rect::new(
            Point::new(win.x() + pad, win.y() + TITLE_BAR_HEIGHT + pad),
            Size::new(
                (win.size.width - 2.0 * pad).max(0.0),
                (win.size.height - TITLE_BAR_HEIGHT - 2.0 * pad).max(0.0),
            ),
        )
    }

    fn resize_min_max(&self) -> (f32, f32, f32, f32) {
        let vw = self.viewport_size.width;
        let vh = self.viewport_size.height;
        let min_w = self.mss.min_width
            .map(|d| d.resolve(vw))
            .unwrap_or(RESIZE_MIN_FALLBACK);
        let max_w = self.mss.max_width
            .map(|d| d.resolve(vw))
            .unwrap_or(vw.max(RESIZE_MIN_FALLBACK));
        let min_h = self.mss.min_height
            .map(|d| d.resolve(vh))
            .unwrap_or(TITLE_BAR_HEIGHT + DEFAULT_PADDING * 2.0 + 20.0);
        let max_h = self.mss.max_height
            .map(|d| d.resolve(vh))
            .unwrap_or(vh.max(RESIZE_MIN_FALLBACK));
        (min_w, max_w, min_h, max_h)
    }

    fn hit_test_resize_edge(&self, pos: Point) -> Option<ResizeEdge> {
        if !self.resizable { return None; }
        let win = self.window_rect();
        let g = RESIZE_GRAB_ZONE;
        let outer = Rect::new(
            Point::new(win.x() - g, win.y() - g),
            Size::new(win.size.width + 2.0 * g, win.size.height + 2.0 * g),
        );
        if !outer.contains(pos) { return None; }
        let inner = Rect::new(
            Point::new(win.x() + g, win.y() + g),
            Size::new((win.size.width - 2.0 * g).max(0.0), (win.size.height - 2.0 * g).max(0.0)),
        );
        if inner.contains(pos) { return None; }

        let near_left = pos.x < win.x() + RESIZE_CORNER_ZONE;
        let near_right = pos.x > win.x() + win.size.width - RESIZE_CORNER_ZONE;
        let near_top = pos.y < win.y() + RESIZE_CORNER_ZONE;
        let near_bottom = pos.y > win.y() + win.size.height - RESIZE_CORNER_ZONE;

        match (near_top, near_bottom, near_left, near_right) {
            (true, _, true, _) => Some(ResizeEdge::TopLeft),
            (true, _, _, true) => Some(ResizeEdge::TopRight),
            (_, true, true, _) => Some(ResizeEdge::BottomLeft),
            (_, true, _, true) => Some(ResizeEdge::BottomRight),
            (true, _, _, _) => Some(ResizeEdge::Top),
            (_, true, _, _) => Some(ResizeEdge::Bottom),
            (_, _, true, _) => Some(ResizeEdge::Left),
            (_, _, _, true) => Some(ResizeEdge::Right),
            _ => {
                let dl = (pos.x - win.x()).abs();
                let dr = (pos.x - (win.x() + win.size.width)).abs();
                let dt = (pos.y - win.y()).abs();
                let db = (pos.y - (win.y() + win.size.height)).abs();
                let min_dist = dl.min(dr).min(dt).min(db);
                if min_dist == dt { Some(ResizeEdge::Top) }
                else if min_dist == db { Some(ResizeEdge::Bottom) }
                else if min_dist == dl { Some(ResizeEdge::Left) }
                else { Some(ResizeEdge::Right) }
            }
        }
    }

    fn apply_resize(&mut self, edge: ResizeEdge, mouse: Point) {
        let dx = mouse.x - self.resize_start_mouse.x;
        let dy = mouse.y - self.resize_start_mouse.y;
        let sw = self.resize_start_size.width;
        let sh = self.resize_start_size.height;

        let (new_w, new_h, affects_left, affects_top) = match edge {
            ResizeEdge::BottomRight => (sw + dx, sh + dy, false, false),
            ResizeEdge::BottomLeft  => (sw - dx, sh + dy, true,  false),
            ResizeEdge::TopRight    => (sw + dx, sh - dy, false, true),
            ResizeEdge::TopLeft     => (sw - dx, sh - dy, true,  true),
            ResizeEdge::Right       => (sw + dx, sh,      false, false),
            ResizeEdge::Left        => (sw - dx, sh,      true,  false),
            ResizeEdge::Bottom      => (sw,      sh + dy, false, false),
            ResizeEdge::Top         => (sw,      sh - dy, false, true),
        };

        let (min_w, max_w, min_h, max_h) = self.resize_min_max();
        let clamped_w = new_w.clamp(min_w, max_w);
        let clamped_h = new_h.clamp(min_h, max_h);

        let pos_dx = if affects_left { sw - clamped_w } else { 0.0 };
        let pos_dy = if affects_top { sh - clamped_h } else { 0.0 };
        let new_pos = Point::new(
            self.resize_start_pos.x + pos_dx,
            self.resize_start_pos.y + pos_dy,
        );

        self.base_size = Size::new(clamped_w, clamped_h);
        self.user_resized = true;
        self.position.set(new_pos);
        if let Some(sig) = self.size_signal {
            sig.set(Size::new(clamped_w, clamped_h));
        }
    }

    fn close_window(&mut self, ctx: &mut EventContext) {
        self.is_open.set(false);
        self.is_minimized.set(false);
        if self.overlay_registered {
            ctx.unregister_overlay();
            self.overlay_registered = false;
        }
        if let Some(ref cb) = self.on_close {
            if let Ok(mut f) = cb.lock() { f(); }
        }
        self.dragging = false;
        self.resizing = None;
        self.hover_close = false;
        self.hover_minimize = false;
        self.hover_title_bar = false;
        ctx.request_paint();
    }
}

impl Element for FloatingWindowElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<FloatingWindow>() {
            self.title = w.title.clone();
            self.icon = w.icon.clone();
            self.is_open = w.is_open;
            self.position = w.position;
            self.base_size = w.size;
            self.closable = w.closable;
            self.resizable = w.resizable;
            self.minimizable = w.minimizable;
            self.is_minimized = w.is_minimized;
            self.size_signal = w.size_signal;
            self.on_close = w.on_close.clone();
            self.is_open.subscribe_element(self.id);
            self.is_minimized.subscribe_element(self.id);
            if self.resizing.is_none() {
                if let Some(sig) = self.size_signal {
                    sig.subscribe_element(self.id);
                    let s = sig.get_untracked();
                    if s.width >= 0.0 && s.height >= 0.0 {
                        if self.base_size != s {
                            self.base_size = s;
                            self.needs_center = self.centered;
                        }
                    }
                }
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));

        if self.resizing.is_none() {
            if let Some(sig) = self.size_signal {
                let s = sig.get_untracked();
                if s.width >= 0.0 && s.height >= 0.0 && self.base_size != s {
                    self.base_size = s;
                    if self.centered {
                        self.needs_center = true;
                    }
                }
            }
        }

        let currently_open = self.is_open();
        if self.centered && currently_open && !self.was_open {
            self.needs_center = true;
        }
        self.was_open = currently_open;

        let vw = self.viewport_size.width;
        let vh = self.viewport_size.height;
        if self.needs_center && vw > 0.0 && vh > 0.0 {
            if self.base_size.height > 0.0 {
                self.needs_center = false;
                let cx = (vw - self.base_size.width) / 2.0;
                let cy = (vh - self.base_size.height) / 2.0;
                self.position.set(Point::new(cx.max(0.0), cy.max(0.0)));
            }
        }

        Size::zero()
    }

    fn set_viewport_size(&mut self, size: Size) {
        self.viewport_size = size;
    }

    fn is_visible(&self) -> bool {
        self.is_open()
    }

    fn is_relayout_boundary(&self) -> bool {
        true
    }

    fn layout_hint(&self) -> LayoutHint {
        let pos = self.position.get_untracked();
        let pad = self.padding();
        LayoutHint::FloatingWindow {
            x: pos.x + pad,
            y: pos.y + TITLE_BAR_HEIGHT + pad,
        }
    }

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        let pad = self.padding();
        let w = self.resolved_width();
        let content_h = if self.base_size.height > 0.0 {
            Some((self.base_size.height - TITLE_BAR_HEIGHT - 2.0 * pad).max(0.0))
        } else {
            None
        };
        (
            Some((w - 2.0 * pad).max(0.0)),
            content_h,
        )
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_visible_window() {
            list.push_clip(Rect::zero());
            return;
        }

        list.begin_overlay_absolute();

        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg = self.mss.color.unwrap_or(Color::from_hex("#111827"));
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
        let radius = self.border_radius();
        let title_fs = self.title_font_size();
        let gray_500 = fg.with_alpha(0.5);
        let red_500 = Color::from_hex("#EF4444");

        let win = self.window_rect();

        list.push_shadow(
            win,
            Color::new(0.0, 0.0, 0.0, 0.15),
            16.0,
            (0.0, 4.0),
            [radius; 4],
        );

        list.push_rect_bordered(win, bg, [radius; 4], Border { width: 1.0, color: border_color });

        let tb = self.title_bar_rect();
        list.push_rect(tb, bg.darken(0.08), [radius, radius, 0.0, 0.0]);

        list.push_rect(
            Rect::new(Point::new(tb.x(), tb.y() + tb.size.height - 1.0), Size::new(tb.size.width, 1.0)),
            border_color,
            [0.0; 4],
        );

        let title_x_offset = if let Some(ref icon) = self.icon {
            let icon_size = title_fs + 4.0;
            let icon_rect = Rect::new(
                Point::new(tb.x() + 10.0, tb.y() + (tb.size.height - icon_size) / 2.0),
                Size::new(icon_size, icon_size),
            );
            list.push_text_centered(icon, icon_rect, fg.with_alpha(0.7), icon_size);
            icon_size + 6.0
        } else {
            0.0
        };
        let reserved = self.title_buttons_reserved();
        let title_rect = Rect::new(
            Point::new(tb.x() + 12.0 + title_x_offset, tb.y()),
            Size::new((tb.size.width - reserved - title_x_offset - 12.0).max(0.0), tb.size.height),
        );
        list.push_text(&self.title, title_rect, fg, title_fs);

        if self.minimizable {
            let min_rect = self.minimize_button_rect();
            if self.hover_minimize {
                list.push_rect(min_rect, fg.with_alpha(0.08), [4.0; 4]);
            }
            list.push_text_centered(
                DEFAULT_MINIMIZE_ICON,
                min_rect,
                if self.hover_minimize { fg } else { gray_500 },
                title_fs,
            );
        }

        if self.closable {
            let close_rect = self.close_button_rect();
            if self.hover_close {
                list.push_rect(close_rect, red_500.with_alpha(0.1), [4.0; 4]);
            }
            list.push_text_centered(DEFAULT_CLOSE_ICON, close_rect, if self.hover_close { red_500 } else { gray_500 }, title_fs);
        }

        let content = self.content_rect();
        list.push_clip(content);

    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_visible_window() {
            list.pop_clip();
            return;
        }
        list.pop_clip();
        list.end_overlay();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let is_open = self.is_open();
        let visible = self.is_visible_window();

        if visible && !self.overlay_registered {
            let win = self.window_rect();
            ctx.register_overlay(win, false);
            self.overlay_registered = true;
            ctx.request_paint();
        } else if !visible && self.overlay_registered {
            ctx.unregister_overlay();
            self.overlay_registered = false;
            ctx.request_paint();
        }

        if !is_open || !visible {
            return EventResult::Ignored;
        }

        match event {
            Event::BackPressed => {
                if self.closable {
                    self.close_window(ctx);
                    return EventResult::Handled;
                }
                // Незакрываемое окно «съедает» жест: назад под модалкой
                // уводил бы навигацию, пока окно остаётся на экране.
                return EventResult::Handled;
            }
            Event::MouseMove(pos) => {
                if let Some(edge) = self.resizing {
                    self.apply_resize(edge, *pos);
                    let win = self.window_rect();
                    ctx.register_overlay(win, false);
                    ctx.set_cursor(edge.cursor());
                    ctx.request_layout();
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.dragging {
                    let new_pos = Point::new(
                        pos.x - self.drag_offset.x,
                        pos.y - self.drag_offset.y,
                    );
                    self.position.set(new_pos);
                    let win = self.window_rect();
                    ctx.register_overlay(win, false);
                    ctx.set_cursor(CursorIcon::Grabbing);
                    ctx.request_layout();
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if let Some(edge) = self.hit_test_resize_edge(*pos) {
                    ctx.set_cursor(edge.cursor());
                    return EventResult::Handled;
                }

                let was_hover_close = self.hover_close;
                let was_hover_min = self.hover_minimize;
                let was_hover_tb = self.hover_title_bar;
                self.hover_close = self.closable && self.close_button_rect().contains(*pos);
                self.hover_minimize = self.minimizable && self.minimize_button_rect().contains(*pos);
                self.hover_title_bar = self.title_bar_rect().contains(*pos);

                if self.hover_close != was_hover_close
                    || self.hover_minimize != was_hover_min
                    || self.hover_title_bar != was_hover_tb
                {
                    ctx.request_paint();
                }

                if self.hover_title_bar && !self.hover_close && !self.hover_minimize {
                    ctx.set_cursor(CursorIcon::Grab);
                    return EventResult::Handled;
                }

                if self.window_rect().contains(*pos) {
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left {
                    if self.closable && self.close_button_rect().contains(*position) {
                        self.close_window(ctx);
                        return EventResult::Handled;
                    }

                    if self.minimizable && self.minimize_button_rect().contains(*position) {
                        self.is_minimized.set(true);
                        self.dragging = false;
                        self.resizing = None;
                        if self.overlay_registered {
                            ctx.unregister_overlay();
                            self.overlay_registered = false;
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }

                    if let Some(edge) = self.hit_test_resize_edge(*position) {
                        self.resizing = Some(edge);
                        self.resize_start_mouse = *position;
                        self.resize_start_size = Size::new(self.resolved_width(), self.resolved_height());
                        self.resize_start_pos = self.position.get_untracked();
                        ctx.set_cursor(edge.cursor());
                        return EventResult::Handled;
                    }

                    let drag_area = if self.drag_on_body {
                        self.window_rect().contains(*position)
                    } else {
                        self.title_bar_rect().contains(*position)
                    };
                    if drag_area {
                        let win_pos = self.position.get_untracked();
                        self.dragging = true;
                        self.drag_offset = Point::new(
                            position.x - win_pos.x,
                            position.y - win_pos.y,
                        );
                        ctx.set_cursor(CursorIcon::Grabbing);
                        return EventResult::Handled;
                    }

                    if self.window_rect().contains(*position) {
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::MouseUp { .. } => {
                if self.resizing.is_some() {
                    self.resizing = None;
                    return EventResult::Handled;
                }
                if self.dragging {
                    self.dragging = false;
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::TouchStart { position, .. } => {
                if self.closable && self.close_button_rect().contains(*position) {
                    self.close_window(ctx);
                    return EventResult::Handled;
                }
                if self.minimizable && self.minimize_button_rect().contains(*position) {
                    self.is_minimized.set(true);
                    self.dragging = false;
                    self.resizing = None;
                    if self.overlay_registered {
                        ctx.unregister_overlay();
                        self.overlay_registered = false;
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if let Some(edge) = self.hit_test_resize_edge(*position) {
                    self.resizing = Some(edge);
                    self.resize_start_mouse = *position;
                    self.resize_start_size = Size::new(self.resolved_width(), self.resolved_height());
                    self.resize_start_pos = self.position.get_untracked();
                    return EventResult::Handled;
                }
                let drag_area = if self.drag_on_body {
                    self.window_rect().contains(*position)
                } else {
                    self.title_bar_rect().contains(*position)
                };
                if drag_area {
                    let win_pos = self.position.get_untracked();
                    self.dragging = true;
                    self.drag_offset = Point::new(
                        position.x - win_pos.x,
                        position.y - win_pos.y,
                    );
                    return EventResult::Handled;
                }
                if self.window_rect().contains(*position) {
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::TouchMove { position, .. } => {
                if let Some(edge) = self.resizing {
                    self.apply_resize(edge, *position);
                    let win = self.window_rect();
                    ctx.register_overlay(win, false);
                    ctx.request_layout();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.dragging {
                    let new_pos = Point::new(
                        position.x - self.drag_offset.x,
                        position.y - self.drag_offset.y,
                    );
                    self.position.set(new_pos);
                    let win = self.window_rect();
                    ctx.register_overlay(win, false);
                    ctx.request_layout();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::TouchEnd { .. } => {
                if self.resizing.is_some() {
                    self.resizing = None;
                    return EventResult::Handled;
                }
                if self.dragging {
                    self.dragging = false;
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &self.child_ids }
    fn bounds(&self) -> Rect { self.bounds }
    fn hit_test(&self, _point: Point) -> bool { self.is_visible_window() }
    fn overlay_request(&self) -> Option<(Rect, bool)> {
        if self.is_visible_window() {
            Some((self.window_rect(), self.modal))
        } else {
            None
        }
    }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {
        self.is_open.subscribe_element(self.id);
        self.is_minimized.subscribe_element(self.id);
        if let Some(sig) = self.size_signal {
            sig.subscribe_element(self.id);
        }
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "FloatingWindow" }

    fn set_content_size(&mut self, size: Size) {
        self.content_size = size;
        if self.needs_center && self.base_size.height <= 0.0 {
            let vw = self.viewport_size.width;
            let vh = self.viewport_size.height;
            if vw > 0.0 && vh > 0.0 {
                self.needs_center = false;
                let win_w = self.resolved_width();
                let win_h = self.resolved_height();
                let cx = (vw - win_w) / 2.0;
                let cy = (vh - win_h) / 2.0;
                self.position.set(Point::new(cx.max(0.0), cy.max(0.0)));
            }
        }
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(fs) = self.mss.font_size {
            self.mss_title_font_size = Some(fs);
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Group,
            state: crate::a11y::NodeState {
                hidden: !self.is_visible_window(),
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: Some(self.title.clone()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for FloatingWindowElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
