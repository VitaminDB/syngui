use crate::core::canvas::CanvasContext;
use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, Modifiers, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;

use super::pipe_overlay::{cumulative_lengths, sub_polyline};
use super::{tile_math, MapViewport};

/// Форма ручки редактирования.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleShape {
    Circle,
    Square,
    Diamond,
}

/// Перетаскиваемая ручка на карте: узел трассы, вершина, середина отрезка.
#[derive(Clone, Debug, PartialEq)]
pub struct EditHandle {
    pub id: u64,
    pub lat: f64,
    pub lng: f64,
    /// Радиус в пикселях экрана (не зависит от масштаба).
    pub radius: f32,
    pub fill: Color,
    pub stroke: Color,
    pub stroke_width: f32,
    pub shape: HandleShape,
    pub selected: bool,
    pub hovered: bool,
    pub min_zoom: f32,
}

impl EditHandle {
    pub fn new(id: u64, lat: f64, lng: f64) -> Self {
        Self {
            id,
            lat,
            lng,
            radius: 6.0,
            fill: Color::new(1.0, 1.0, 1.0, 1.0),
            stroke: Color::new(0.15, 0.45, 0.95, 1.0),
            stroke_width: 2.0,
            shape: HandleShape::Circle,
            selected: false,
            hovered: false,
            min_zoom: 0.0,
        }
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius.max(1.0);
        self
    }

    pub fn colors(mut self, fill: Color, stroke: Color) -> Self {
        self.fill = fill;
        self.stroke = stroke;
        self
    }

    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width;
        self
    }

    pub fn shape(mut self, shape: HandleShape) -> Self {
        self.shape = shape;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    pub fn min_zoom(mut self, zoom: f32) -> Self {
        self.min_zoom = zoom;
        self
    }
}

/// Вспомогательная линия редактора: подсветка выбранного участка, «резинка»
/// от последней вершины к курсору, предпросмотр. Без хит-теста.
#[derive(Clone, Debug, PartialEq)]
pub struct EditPath {
    pub id: u64,
    pub points: Vec<(f64, f64)>,
    pub color: Color,
    pub width: f32,
    pub dashed: bool,
    pub glow: Option<Color>,
}

impl EditPath {
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        Self {
            id: 0,
            points,
            color: Color::new(0.15, 0.45, 0.95, 0.9),
            width: 3.0,
            dashed: false,
            glow: None,
        }
    }

    pub fn id(mut self, id: u64) -> Self {
        self.id = id;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width.max(0.5);
        self
    }

    pub fn dashed(mut self, dashed: bool) -> Self {
        self.dashed = dashed;
        self
    }

    pub fn glow(mut self, color: Color) -> Self {
        self.glow = Some(color);
        self
    }
}

type HandleGeoCb = Arc<Mutex<dyn FnMut(u64, f64, f64) + Send>>;
type PressCb = Arc<Mutex<dyn FnMut(u64, f64, f64, Modifiers) + Send>>;
type ReleaseCb = Arc<Mutex<dyn FnMut(u64, f64, f64, bool) + Send>>;
type SecondaryCb = Arc<Mutex<dyn FnMut(u64, f64, f64, Point) + Send>>;

/// Слой ручек редактирования поверх [`MapView`](super::MapView). Берёт
/// ввод только над ручкой: нажатие захватывает её и ведёт перетаскивание,
/// остальная карта панорамируется как обычно. Правая кнопка над ручкой —
/// `on_secondary` (контекстное меню). Пути рисуются без хит-теста.
pub struct MapEditOverlay {
    handles: Arc<Vec<EditHandle>>,
    paths: Arc<Vec<EditPath>>,
    viewport: MapViewport,
    hit_slop: f32,
    drag_threshold: f32,
    on_press: Option<PressCb>,
    on_drag: Option<HandleGeoCb>,
    on_release: Option<ReleaseCb>,
    on_secondary: Option<SecondaryCb>,
}

impl MapEditOverlay {
    pub fn new() -> Self {
        Self {
            handles: Arc::new(Vec::new()),
            paths: Arc::new(Vec::new()),
            viewport: MapViewport::default(),
            hit_slop: 4.0,
            drag_threshold: 3.0,
            on_press: None,
            on_drag: None,
            on_release: None,
            on_secondary: None,
        }
    }

    pub fn handles(mut self, handles: Vec<EditHandle>) -> Self {
        self.handles = Arc::new(handles);
        self
    }

    pub fn handles_arc(mut self, handles: Arc<Vec<EditHandle>>) -> Self {
        self.handles = handles;
        self
    }

    pub fn paths(mut self, paths: Vec<EditPath>) -> Self {
        self.paths = Arc::new(paths);
        self
    }

    pub fn paths_arc(mut self, paths: Arc<Vec<EditPath>>) -> Self {
        self.paths = paths;
        self
    }

    pub fn viewport(mut self, viewport: MapViewport) -> Self {
        self.viewport = viewport;
        self
    }

    /// Допуск попадания сверх радиуса ручки, px.
    pub fn hit_slop(mut self, slop: f32) -> Self {
        self.hit_slop = slop.max(0.0);
        self
    }

    /// Нажатие на ручку: id, её координаты, модификаторы клавиатуры.
    pub fn on_press(mut self, cb: impl FnMut(u64, f64, f64, Modifiers) + Send + 'static) -> Self {
        self.on_press = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Перетаскивание: id и новое положение под курсором.
    pub fn on_drag(mut self, cb: impl FnMut(u64, f64, f64) + Send + 'static) -> Self {
        self.on_drag = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Отпускание: id, положение и сдвигалась ли ручка (иначе это щелчок).
    pub fn on_release(mut self, cb: impl FnMut(u64, f64, f64, bool) + Send + 'static) -> Self {
        self.on_release = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Правая кнопка над ручкой: id, координаты ручки и точка экрана.
    pub fn on_secondary(mut self, cb: impl FnMut(u64, f64, f64, Point) + Send + 'static) -> Self {
        self.on_secondary = Some(Arc::new(Mutex::new(cb)));
        self
    }
}

impl Default for MapEditOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for MapEditOverlay {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MapEditOverlayElement {
            id: ElementId::new(),
            handles: self.handles.clone(),
            paths: self.paths.clone(),
            viewport: self.viewport,
            hit_slop: self.hit_slop,
            drag_threshold: self.drag_threshold,
            on_press: self.on_press.clone(),
            on_drag: self.on_drag.clone(),
            on_release: self.on_release.clone(),
            on_secondary: self.on_secondary.clone(),
            drag: None,
            bounds: Rect::zero(),
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

struct Drag {
    handle: u64,
    start: Point,
    moved: bool,
}

pub struct MapEditOverlayElement {
    id: ElementId,
    handles: Arc<Vec<EditHandle>>,
    paths: Arc<Vec<EditPath>>,
    viewport: MapViewport,
    hit_slop: f32,
    drag_threshold: f32,
    on_press: Option<PressCb>,
    on_drag: Option<HandleGeoCb>,
    on_release: Option<ReleaseCb>,
    on_secondary: Option<SecondaryCb>,
    drag: Option<Drag>,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl MapEditOverlayElement {
    fn project(&self, lat: f64, lng: f64) -> (f32, f32) {
        let vp = self.viewport;
        tile_math::geo_to_pixel_f(
            lat,
            lng,
            vp.center_lat,
            vp.center_lng,
            vp.zoom_level,
            vp.viewport_w,
            vp.viewport_h,
        )
    }

    fn geo_at(&self, p: Point) -> (f64, f64) {
        let vp = self.viewport;
        tile_math::pixel_to_geo_f(
            p.x - self.bounds.origin.x,
            p.y - self.bounds.origin.y,
            vp.center_lat,
            vp.center_lng,
            vp.zoom_level,
            vp.viewport_w,
            vp.viewport_h,
        )
    }

    /// Верхняя (последняя в списке) ручка под точкой экрана.
    fn handle_at(&self, p: Point) -> Option<&EditHandle> {
        let zoom = self.viewport.zoom_level as f32;
        let lx = p.x - self.bounds.origin.x;
        let ly = p.y - self.bounds.origin.y;
        let mut best: Option<(&EditHandle, f32)> = None;
        for h in self.handles.iter().rev() {
            if zoom < h.min_zoom {
                continue;
            }
            let (x, y) = self.project(h.lat, h.lng);
            let d = ((x - lx).powi(2) + (y - ly).powi(2)).sqrt();
            let reach = h.radius + self.hit_slop;
            if d <= reach && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((h, d));
            }
        }
        best.map(|(h, _)| h)
    }

    fn paint(&self, ctx: &mut CanvasContext) {
        let vp = self.viewport;
        let zoom = vp.zoom_level as f32;

        for path in self.paths.iter() {
            if path.points.len() < 2 {
                continue;
            }
            let pts: Vec<(f32, f32)> = path
                .points
                .iter()
                .map(|&(lat, lng)| self.project(lat, lng))
                .collect();
            if let Some(glow) = path.glow {
                ctx.set_color(glow);
                ctx.set_stroke_width(path.width * 2.4 + 4.0);
                ctx.set_anti_alias(path.width + 3.0);
                ctx.stroke_path(&pts, false);
            }
            ctx.set_anti_alias(0.9);
            ctx.set_color(path.color);
            ctx.set_stroke_width(path.width);
            if path.dashed {
                let cum = cumulative_lengths(&pts);
                let total = cum[cum.len() - 1];
                let dash = (path.width * 2.5).max(6.0);
                let gap = (path.width * 1.5).max(4.0);
                let mut s = 0.0;
                while s < total {
                    let piece = sub_polyline(&pts, &cum, s, s + dash);
                    if piece.len() >= 2 {
                        ctx.stroke_path(&piece, false);
                    }
                    s += dash + gap;
                }
            } else {
                ctx.stroke_path(&pts, false);
            }
        }

        for h in self.handles.iter() {
            if zoom < h.min_zoom {
                continue;
            }
            let (x, y) = self.project(h.lat, h.lng);
            if x < -40.0 || y < -40.0 || x > vp.viewport_w + 40.0 || y > vp.viewport_h + 40.0 {
                continue;
            }
            let dragging = self.drag.as_ref().is_some_and(|d| d.handle == h.id);
            let r = if dragging || h.hovered {
                h.radius * 1.25
            } else {
                h.radius
            };
            if h.selected || dragging {
                ctx.set_anti_alias(r * 0.8);
                ctx.set_color(h.stroke.with_alpha(0.28));
                ctx.fill_circle(x, y, r * 2.2);
            }
            ctx.set_anti_alias(0.9);
            ctx.set_color(h.stroke);
            paint_handle(ctx, h.shape, x, y, r + h.stroke_width);
            ctx.set_color(h.fill);
            paint_handle(ctx, h.shape, x, y, r);
            if h.selected {
                ctx.set_color(h.stroke);
                paint_handle(ctx, HandleShape::Circle, x, y, (r * 0.4).max(1.5));
            }
        }
    }
}

fn paint_handle(ctx: &mut CanvasContext, shape: HandleShape, x: f32, y: f32, r: f32) {
    match shape {
        HandleShape::Circle => ctx.fill_circle(x, y, r),
        HandleShape::Square => ctx.fill_rect(x - r, y - r, r * 2.0, r * 2.0),
        HandleShape::Diamond => {
            ctx.fill_polygon(&[(x, y - r), (x + r, y), (x, y + r), (x - r, y)]);
        }
    }
}

impl Element for MapEditOverlayElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<MapEditOverlay>() {
            self.handles = w.handles.clone();
            self.paths = w.paths.clone();
            self.viewport = w.viewport;
            self.hit_slop = w.hit_slop;
            self.drag_threshold = w.drag_threshold;
            self.on_press = w.on_press.clone();
            self.on_drag = w.on_drag.clone();
            self.on_release = w.on_release.clone();
            self.on_secondary = w.on_secondary.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            400.0
        };
        let h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            400.0
        };
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.viewport.viewport_w <= 1.0 || self.viewport.viewport_h <= 1.0 {
            return;
        }
        if self.handles.is_empty() && self.paths.is_empty() {
            return;
        }
        list.push_clip(self.bounds);
        let mut ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);
        self.paint(&mut ctx);
        ctx.flush(list);
        list.pop_clip();
    }

    fn hit_test(&self, point: Point) -> bool {
        self.drag.is_some() || (self.bounds.contains(point) && self.handle_at(point).is_some())
    }

    fn drags_with_touch(&self, pos: Point) -> bool {
        self.handle_at(Point::new(pos.x + self.bounds.origin.x, pos.y + self.bounds.origin.y))
            .is_some()
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseDown {
                button: MouseButton::Left,
                position,
            } => {
                let Some(h) = self.handle_at(*position) else {
                    return EventResult::Ignored;
                };
                let (id, lat, lng) = (h.id, h.lat, h.lng);
                self.drag = Some(Drag {
                    handle: id,
                    start: *position,
                    moved: false,
                });
                ctx.capture();
                ctx.set_cursor(CursorIcon::Grabbing);
                if let Some(cb) = self.on_press.clone() {
                    if let Ok(mut f) = cb.lock() {
                        f(id, lat, lng, ctx.modifiers);
                    }
                }
                self.mark_dirty(DirtyFlags::RENDER);
                EventResult::Handled
            }
            Event::MouseDown {
                button: MouseButton::Right,
                position,
            } => {
                let Some(h) = self.handle_at(*position) else {
                    return EventResult::Ignored;
                };
                let (id, lat, lng) = (h.id, h.lat, h.lng);
                if let Some(cb) = self.on_secondary.clone() {
                    if let Ok(mut f) = cb.lock() {
                        f(id, lat, lng, *position);
                    }
                }
                EventResult::Handled
            }
            Event::MouseMove(position) => {
                if let Some(drag) = self.drag.as_mut() {
                    let dx = position.x - drag.start.x;
                    let dy = position.y - drag.start.y;
                    if !drag.moved && dx * dx + dy * dy < self.drag_threshold * self.drag_threshold {
                        return EventResult::Handled;
                    }
                    drag.moved = true;
                    let id = drag.handle;
                    let (lat, lng) = self.geo_at(*position);
                    ctx.set_cursor(CursorIcon::Grabbing);
                    if let Some(cb) = self.on_drag.clone() {
                        if let Ok(mut f) = cb.lock() {
                            f(id, lat, lng);
                        }
                    }
                    self.mark_dirty(DirtyFlags::RENDER);
                    return EventResult::Handled;
                }
                if self.handle_at(*position).is_some() {
                    ctx.set_cursor(CursorIcon::Move);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp {
                button: MouseButton::Left,
                position,
            } => {
                let Some(drag) = self.drag.take() else {
                    return EventResult::Ignored;
                };
                let (lat, lng) = if drag.moved {
                    self.geo_at(*position)
                } else {
                    self.handles
                        .iter()
                        .find(|h| h.id == drag.handle)
                        .map(|h| (h.lat, h.lng))
                        .unwrap_or_else(|| self.geo_at(*position))
                };
                ctx.set_cursor(CursorIcon::Default);
                if let Some(cb) = self.on_release.clone() {
                    if let Ok(mut f) = cb.lock() {
                        f(drag.handle, lat, lng, drag.moved);
                    }
                }
                self.mark_dirty(DirtyFlags::RENDER);
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
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
        "MapEditOverlay"
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

    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

impl StyledElement for MapEditOverlayElement {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn element(handles: Vec<EditHandle>) -> MapEditOverlayElement {
        let vp = MapViewport {
            center_lat: 53.2,
            center_lng: 63.6,
            zoom: 16,
            zoom_level: 16.0,
            viewport_w: 400.0,
            viewport_h: 300.0,
        };
        let w = MapEditOverlay::new().handles(handles).viewport(vp);
        let mut e = MapEditOverlayElement {
            id: ElementId::new(),
            handles: w.handles.clone(),
            paths: w.paths.clone(),
            viewport: w.viewport,
            hit_slop: w.hit_slop,
            drag_threshold: w.drag_threshold,
            on_press: None,
            on_drag: None,
            on_release: None,
            on_secondary: None,
            drag: None,
            bounds: Rect::new(Point::new(10.0, 20.0), Size::new(400.0, 300.0)),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::empty(),
            mss: MssFields::new(),
        };
        e.layout(Constraints::tight(Size::new(400.0, 300.0)));
        e
    }

    #[test]
    fn hit_test_only_over_handles() {
        let e = element(vec![EditHandle::new(1, 53.2, 63.6).radius(6.0)]);
        assert!(e.hit_test(Point::new(210.0, 170.0)));
        assert!(e.hit_test(Point::new(218.0, 170.0)));
        assert!(!e.hit_test(Point::new(240.0, 170.0)));
        assert!(!e.hit_test(Point::new(100.0, 100.0)));
    }

    #[test]
    fn topmost_handle_wins_when_overlapping() {
        let e = element(vec![
            EditHandle::new(1, 53.2, 63.6).radius(8.0),
            EditHandle::new(2, 53.2, 63.6).radius(8.0),
        ]);
        assert_eq!(e.handle_at(Point::new(210.0, 170.0)).map(|h| h.id), Some(2));
    }
}
