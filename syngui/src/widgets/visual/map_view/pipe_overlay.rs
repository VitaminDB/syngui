use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::mss::{TextAlign, TextDecoration};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, TextMeasure};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use super::{tile_math, MapViewport};

/// Форма значка узла сети.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeGlyph {
    Circle,
    Square,
    Diamond,
    Ring,
    Hexagon,
}

/// Трубопровод (участок сети, трек, кабель) поверх карты: ломаная в
/// географических координатах, толщина на опорном масштабе, цвет по
/// вершинам (градиент вдоль линии), подложка, свечение, анимация потока.
#[derive(Clone, Debug, PartialEq)]
pub struct PipeShape {
    pub id: u64,
    pub points: Vec<(f64, f64)>,
    /// Толщина в пикселях на опорном масштабе оверлея.
    pub width: f32,
    /// Один цвет на всю линию или по цвету на каждую вершину.
    pub colors: Vec<Color>,
    pub outline: Option<Color>,
    pub glow: Option<Color>,
    /// Скорость потока относительно базовой (0 — без анимации; знак —
    /// направление: `+` от первой точки к последней).
    pub flow: f32,
    pub dashed: bool,
    /// Поперечное смещение в пикселях экрана: две нитки в одном канале.
    pub offset: f32,
    pub min_zoom: f32,
    pub opacity: f32,
}

impl PipeShape {
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        Self {
            id: 0,
            points,
            width: 4.0,
            colors: vec![Color::new(0.9, 0.3, 0.2, 1.0)],
            outline: None,
            glow: None,
            flow: 0.0,
            dashed: false,
            offset: 0.0,
            min_zoom: 0.0,
            opacity: 1.0,
        }
    }

    pub fn id(mut self, id: u64) -> Self {
        self.id = id;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width.max(0.1);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.colors = vec![color];
        self
    }

    /// Цвет на каждую вершину; при несовпадении длины берётся первый цвет.
    pub fn colors(mut self, colors: Vec<Color>) -> Self {
        if !colors.is_empty() {
            self.colors = colors;
        }
        self
    }

    pub fn outline(mut self, color: Color) -> Self {
        self.outline = Some(color);
        self
    }

    pub fn glow(mut self, color: Color) -> Self {
        self.glow = Some(color);
        self
    }

    pub fn flow(mut self, flow: f32) -> Self {
        self.flow = flow;
        self
    }

    pub fn dashed(mut self, dashed: bool) -> Self {
        self.dashed = dashed;
        self
    }

    pub fn offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }

    pub fn min_zoom(mut self, zoom: f32) -> Self {
        self.min_zoom = zoom;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
}

/// Узел сети на карте: источник, камера, потребитель.
#[derive(Clone, Debug, PartialEq)]
pub struct PipeNodeShape {
    pub id: u64,
    pub lat: f64,
    pub lng: f64,
    /// Радиус в пикселях на опорном масштабе.
    pub radius: f32,
    pub fill: Color,
    pub stroke: Color,
    pub stroke_width: f32,
    pub glyph: NodeGlyph,
    pub label: Option<String>,
    pub label_min_zoom: f32,
    pub min_zoom: f32,
    /// Пульсирующий ореол (источник тепла, выбранный узел).
    pub pulse: bool,
    /// Радиус не масштабируется с картой (ручки, выбранные узлы).
    pub fixed_size: bool,
}

impl PipeNodeShape {
    pub fn new(lat: f64, lng: f64) -> Self {
        Self {
            id: 0,
            lat,
            lng,
            radius: 5.0,
            fill: Color::new(1.0, 1.0, 1.0, 1.0),
            stroke: Color::new(0.1, 0.1, 0.12, 1.0),
            stroke_width: 1.5,
            glyph: NodeGlyph::Circle,
            label: None,
            label_min_zoom: 0.0,
            min_zoom: 0.0,
            pulse: false,
            fixed_size: false,
        }
    }

    pub fn id(mut self, id: u64) -> Self {
        self.id = id;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius.max(0.5);
        self
    }

    pub fn fill(mut self, color: Color) -> Self {
        self.fill = color;
        self
    }

    pub fn stroke(mut self, color: Color, width: f32) -> Self {
        self.stroke = color;
        self.stroke_width = width;
        self
    }

    pub fn glyph(mut self, glyph: NodeGlyph) -> Self {
        self.glyph = glyph;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn label_min_zoom(mut self, zoom: f32) -> Self {
        self.label_min_zoom = zoom;
        self
    }

    pub fn min_zoom(mut self, zoom: f32) -> Self {
        self.min_zoom = zoom;
        self
    }

    pub fn pulse(mut self, pulse: bool) -> Self {
        self.pulse = pulse;
        self
    }

    pub fn fixed_size(mut self, fixed: bool) -> Self {
        self.fixed_size = fixed;
        self
    }
}

/// Слой инженерных сетей поверх [`MapView`](super::MapView): трубы с
/// градиентом цвета вдоль линии, подложкой и бегущим потоком, узлы с
/// подписями без наложений. Прозрачен для ввода: клики разбирает карта
/// (`on_tap`) с помощью [`nearest_pipe`] и [`nearest_node`].
pub struct PipeOverlay {
    pipes: Arc<Vec<PipeShape>>,
    nodes: Arc<Vec<PipeNodeShape>>,
    viewport: MapViewport,
    ref_zoom: f32,
    zoom_exponent: f32,
    width_range: (f32, f32),
    flow_speed: f32,
    flow_spacing: f32,
    flow_dash: f32,
    flow_color: Color,
    animate: bool,
    opacity: f32,
    label_size: f32,
    label_color: Color,
    label_background: Color,
}

impl PipeOverlay {
    pub fn new() -> Self {
        Self {
            pipes: Arc::new(Vec::new()),
            nodes: Arc::new(Vec::new()),
            viewport: MapViewport::default(),
            ref_zoom: 16.0,
            zoom_exponent: 0.55,
            width_range: (1.0, 26.0),
            flow_speed: 36.0,
            flow_spacing: 30.0,
            flow_dash: 9.0,
            flow_color: Color::new(1.0, 1.0, 1.0, 0.7),
            animate: true,
            opacity: 1.0,
            label_size: 11.0,
            label_color: Color::new(0.1, 0.1, 0.12, 1.0),
            label_background: Color::new(1.0, 1.0, 1.0, 0.86),
        }
    }

    pub fn pipes(mut self, pipes: Vec<PipeShape>) -> Self {
        self.pipes = Arc::new(pipes);
        self
    }

    pub fn pipes_arc(mut self, pipes: Arc<Vec<PipeShape>>) -> Self {
        self.pipes = pipes;
        self
    }

    pub fn nodes(mut self, nodes: Vec<PipeNodeShape>) -> Self {
        self.nodes = Arc::new(nodes);
        self
    }

    pub fn nodes_arc(mut self, nodes: Arc<Vec<PipeNodeShape>>) -> Self {
        self.nodes = nodes;
        self
    }

    pub fn viewport(mut self, viewport: MapViewport) -> Self {
        self.viewport = viewport;
        self
    }

    /// Масштаб, на котором `width` и `radius` фигур равны своим пикселям.
    pub fn ref_zoom(mut self, zoom: f32) -> Self {
        self.ref_zoom = zoom;
        self
    }

    /// Насколько толщина следует за масштабом: 1 — как расстояния на карте,
    /// 0 — всегда одинаковая.
    pub fn zoom_exponent(mut self, exponent: f32) -> Self {
        self.zoom_exponent = exponent.clamp(0.0, 1.0);
        self
    }

    pub fn width_range(mut self, min: f32, max: f32) -> Self {
        self.width_range = (min.max(0.1), max.max(min));
        self
    }

    /// Базовая скорость потока, px/с, шаг между штрихами и длина штриха.
    pub fn flow(mut self, speed: f32, spacing: f32, dash: f32) -> Self {
        self.flow_speed = speed;
        self.flow_spacing = spacing.max(4.0);
        self.flow_dash = dash.clamp(1.0, self.flow_spacing);
        self
    }

    pub fn flow_color(mut self, color: Color) -> Self {
        self.flow_color = color;
        self
    }

    pub fn animate(mut self, animate: bool) -> Self {
        self.animate = animate;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn labels(mut self, size: f32, color: Color, background: Color) -> Self {
        self.label_size = size;
        self.label_color = color;
        self.label_background = background;
        self
    }
}

impl Default for PipeOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for PipeOverlay {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PipeOverlayElement {
            id: ElementId::new(),
            config: self.snapshot(),
            phase: 0.0,
            bounds: Rect::zero(),
            text_measure: None,
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

impl PipeOverlay {
    fn snapshot(&self) -> OverlayConfig {
        OverlayConfig {
            pipes: self.pipes.clone(),
            nodes: self.nodes.clone(),
            viewport: self.viewport,
            ref_zoom: self.ref_zoom,
            zoom_exponent: self.zoom_exponent,
            width_range: self.width_range,
            flow_speed: self.flow_speed,
            flow_spacing: self.flow_spacing,
            flow_dash: self.flow_dash,
            flow_color: self.flow_color,
            animate: self.animate,
            opacity: self.opacity,
            label_size: self.label_size,
            label_color: self.label_color,
            label_background: self.label_background,
        }
    }
}

struct OverlayConfig {
    pipes: Arc<Vec<PipeShape>>,
    nodes: Arc<Vec<PipeNodeShape>>,
    viewport: MapViewport,
    ref_zoom: f32,
    zoom_exponent: f32,
    width_range: (f32, f32),
    flow_speed: f32,
    flow_spacing: f32,
    flow_dash: f32,
    flow_color: Color,
    animate: bool,
    opacity: f32,
    label_size: f32,
    label_color: Color,
    label_background: Color,
}

impl OverlayConfig {
    fn has_flow(&self) -> bool {
        self.animate
            && (self.pipes.iter().any(|p| p.flow != 0.0) || self.nodes.iter().any(|n| n.pulse))
    }

    fn scale(&self) -> f32 {
        zoom_scale(
            self.viewport.zoom_level as f32,
            self.ref_zoom,
            self.zoom_exponent,
        )
    }

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
}

pub struct PipeOverlayElement {
    id: ElementId,
    config: OverlayConfig,
    /// Фаза потока, 0..1 от шага между штрихами.
    phase: f32,
    bounds: Rect,
    text_measure: Option<Arc<dyn TextMeasure>>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

/// Толщина на экране: `width` на опорном масштабе, дальше степенная
/// зависимость от разницы масштабов.
pub fn zoom_scale(zoom: f32, ref_zoom: f32, exponent: f32) -> f32 {
    2f32.powf((zoom - ref_zoom) * exponent)
}

/// Ломаная, сдвинутая на `d` пикселей вбок (положительное — вправо по
/// ходу линии); на изломах — усреднённая нормаль с ограничением уса.
pub fn offset_polyline(points: &[(f32, f32)], d: f32) -> Vec<(f32, f32)> {
    let n = points.len();
    if n < 2 || d == 0.0 {
        return points.to_vec();
    }
    let normal = |a: (f32, f32), b: (f32, f32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            (0.0, 0.0)
        } else {
            (-dy / len, dx / len)
        }
    };
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (nx, ny, scale) = if i == 0 {
            let (x, y) = normal(points[0], points[1]);
            (x, y, 1.0)
        } else if i == n - 1 {
            let (x, y) = normal(points[n - 2], points[n - 1]);
            (x, y, 1.0)
        } else {
            let (ax, ay) = normal(points[i - 1], points[i]);
            let (bx, by) = normal(points[i], points[i + 1]);
            let (mx, my) = ((ax + bx) * 0.5, (ay + by) * 0.5);
            let mlen = (mx * mx + my * my).sqrt();
            if mlen < 1e-6 {
                (ax, ay, 1.0)
            } else {
                let (ux, uy) = (mx / mlen, my / mlen);
                let dot = ux * ax + uy * ay;
                (ux, uy, if dot > 0.3 { (1.0 / dot).min(2.5) } else { 2.5 })
            }
        };
        out.push((points[i].0 + nx * d * scale, points[i].1 + ny * d * scale));
    }
    out
}

/// Длины вдоль ломаной от начала до каждой вершины.
pub fn cumulative_lengths(points: &[(f32, f32)]) -> Vec<f32> {
    let mut out = Vec::with_capacity(points.len());
    let mut acc = 0.0;
    out.push(0.0);
    for w in points.windows(2) {
        acc += ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        out.push(acc);
    }
    out
}

/// Точка ломаной на расстоянии `s` от начала.
fn point_at(points: &[(f32, f32)], cum: &[f32], s: f32) -> (f32, f32) {
    let last = points.len() - 1;
    if s <= 0.0 {
        return points[0];
    }
    if s >= cum[last] {
        return points[last];
    }
    let i = match cum.binary_search_by(|v| v.partial_cmp(&s).unwrap_or(std::cmp::Ordering::Less)) {
        Ok(i) => i.min(last - 1),
        Err(i) => i.saturating_sub(1).min(last - 1),
    };
    let seg = cum[i + 1] - cum[i];
    let t = if seg <= 1e-6 { 0.0 } else { (s - cum[i]) / seg };
    (
        points[i].0 + (points[i + 1].0 - points[i].0) * t,
        points[i].1 + (points[i + 1].1 - points[i].1) * t,
    )
}

/// Кусок ломаной между расстояниями `from` и `to` вдоль неё.
pub fn sub_polyline(points: &[(f32, f32)], cum: &[f32], from: f32, to: f32) -> Vec<(f32, f32)> {
    let total = cum[cum.len() - 1];
    let from = from.clamp(0.0, total);
    let to = to.clamp(0.0, total);
    if to - from < 1e-3 {
        return Vec::new();
    }
    let mut out = vec![point_at(points, cum, from)];
    for (i, &c) in cum.iter().enumerate() {
        if c > from && c < to {
            out.push(points[i]);
        }
    }
    out.push(point_at(points, cum, to));
    out
}

/// Расстояние от точки до отрезка и положение проекции `t` ∈ [0, 1].
pub fn point_segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 1e-9 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (qx, qy) = (a.0 + dx * t, a.1 + dy * t);
    (((p.0 - qx).powi(2) + (p.1 - qy).powi(2)).sqrt(), t)
}

/// Ближайшая к точке позиция на ломаной: расстояние, индекс отрезка и доля
/// вдоль него.
pub fn polyline_distance(points: &[(f32, f32)], p: (f32, f32)) -> Option<(f32, usize, f32)> {
    let mut best: Option<(f32, usize, f32)> = None;
    for (i, w) in points.windows(2).enumerate() {
        let (d, t) = point_segment_distance(p, w[0], w[1]);
        if best.is_none_or(|b| d < b.0) {
            best = Some((d, i, t));
        }
    }
    best
}

/// Попадание в трубу: id, расстояние в пикселях, индекс отрезка и доля вдоль
/// него. `tolerance` — допуск сверх половины толщины.
pub struct PipeHit {
    pub id: u64,
    pub distance: f32,
    pub segment: usize,
    pub t: f32,
}

pub fn nearest_pipe(
    pipes: &[PipeShape],
    viewport: MapViewport,
    ref_zoom: f32,
    zoom_exponent: f32,
    width_range: (f32, f32),
    px: f32,
    py: f32,
    tolerance: f32,
) -> Option<PipeHit> {
    let zoom = viewport.zoom_level as f32;
    let scale = zoom_scale(zoom, ref_zoom, zoom_exponent);
    let mut best: Option<PipeHit> = None;
    for pipe in pipes {
        if zoom < pipe.min_zoom || pipe.points.len() < 2 {
            continue;
        }
        let w = (pipe.width * scale).clamp(width_range.0, width_range.1);
        let pts = project_pipe(pipe, viewport);
        let Some((d, seg, t)) = polyline_distance(&pts, (px, py)) else {
            continue;
        };
        let reach = w * 0.5 + tolerance;
        if d <= reach && best.as_ref().is_none_or(|b| d < b.distance) {
            best = Some(PipeHit {
                id: pipe.id,
                distance: d,
                segment: seg,
                t,
            });
        }
    }
    best
}

/// Ближайший узел в пределах радиуса плюс допуск.
pub fn nearest_node(
    nodes: &[PipeNodeShape],
    viewport: MapViewport,
    ref_zoom: f32,
    zoom_exponent: f32,
    px: f32,
    py: f32,
    tolerance: f32,
) -> Option<(u64, f32)> {
    let zoom = viewport.zoom_level as f32;
    let scale = zoom_scale(zoom, ref_zoom, zoom_exponent);
    let mut best: Option<(u64, f32)> = None;
    for node in nodes {
        if zoom < node.min_zoom {
            continue;
        }
        let (x, y) = tile_math::geo_to_pixel_f(
            node.lat,
            node.lng,
            viewport.center_lat,
            viewport.center_lng,
            viewport.zoom_level,
            viewport.viewport_w,
            viewport.viewport_h,
        );
        let r = if node.fixed_size {
            node.radius
        } else {
            node.radius * scale
        };
        let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
        if d <= r + tolerance && best.is_none_or(|b| d < b.1) {
            best = Some((node.id, d));
        }
    }
    best
}

fn project_pipe(pipe: &PipeShape, vp: MapViewport) -> Vec<(f32, f32)> {
    let pts: Vec<(f32, f32)> = pipe
        .points
        .iter()
        .map(|&(lat, lng)| {
            tile_math::geo_to_pixel_f(
                lat,
                lng,
                vp.center_lat,
                vp.center_lng,
                vp.zoom_level,
                vp.viewport_w,
                vp.viewport_h,
            )
        })
        .collect();
    if pipe.offset != 0.0 {
        offset_polyline(&pts, pipe.offset)
    } else {
        pts
    }
}

fn bbox(points: &[(f32, f32)]) -> (f32, f32, f32, f32) {
    points.iter().fold(
        (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        |(x0, y0, x1, y1), &(x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
    )
}

fn visible(b: (f32, f32, f32, f32), w: f32, h: f32, margin: f32) -> bool {
    b.2 >= -margin && b.0 <= w + margin && b.3 >= -margin && b.1 <= h + margin
}

fn with_opacity(c: Color, k: f32) -> Color {
    c.with_alpha(c.a * k)
}

struct ProjectedPipe<'a> {
    shape: &'a PipeShape,
    points: Vec<(f32, f32)>,
    width: f32,
}

impl PipeOverlayElement {
    fn paint_pipes(&self, ctx: &mut CanvasContext) {
        let cfg = &self.config;
        let vp = cfg.viewport;
        let zoom = vp.zoom_level as f32;
        let scale = cfg.scale();
        let (min_w, max_w) = cfg.width_range;

        let mut projected: Vec<ProjectedPipe> = Vec::with_capacity(cfg.pipes.len());
        for pipe in cfg.pipes.iter() {
            if pipe.points.len() < 2 || zoom < pipe.min_zoom || pipe.opacity <= 0.001 {
                continue;
            }
            let width = (pipe.width * scale).clamp(min_w, max_w);
            let points = project_pipe(pipe, vp);
            let margin = width * 3.0 + 4.0;
            if !visible(bbox(&points), vp.viewport_w, vp.viewport_h, margin) {
                continue;
            }
            projected.push(ProjectedPipe {
                shape: pipe,
                points,
                width,
            });
        }
        if projected.is_empty() {
            return;
        }

        ctx.set_anti_alias(1.0);
        for p in projected.iter().filter(|p| p.shape.glow.is_some()) {
            let glow = p.shape.glow.unwrap_or_default();
            ctx.set_color(with_opacity(glow, cfg.opacity * p.shape.opacity));
            ctx.set_stroke_width(p.width * 2.6 + 4.0);
            ctx.set_anti_alias((p.width * 1.2 + 3.0).max(2.0));
            ctx.stroke_path(&p.points, false);
        }

        ctx.set_anti_alias(0.9);
        for p in projected.iter().filter(|p| p.shape.outline.is_some()) {
            let outline = p.shape.outline.unwrap_or_default();
            let ow = (p.width * 0.28).clamp(0.8, 3.0);
            ctx.set_color(with_opacity(outline, cfg.opacity * p.shape.opacity));
            ctx.set_stroke_width(p.width + ow * 2.0);
            ctx.stroke_path(&p.points, false);
        }

        for p in &projected {
            let k = cfg.opacity * p.shape.opacity;
            ctx.set_stroke_width(p.width);
            if p.shape.dashed {
                let base = with_opacity(p.shape.colors[0], k);
                ctx.set_color(base);
                let cum = cumulative_lengths(&p.points);
                let total = cum[cum.len() - 1];
                let dash = (p.width * 2.2).max(6.0);
                let gap = (p.width * 1.4).max(4.0);
                let mut s = 0.0;
                while s < total {
                    let piece = sub_polyline(&p.points, &cum, s, s + dash);
                    if piece.len() >= 2 {
                        ctx.stroke_path(&piece, false);
                    }
                    s += dash + gap;
                }
            } else if p.shape.colors.len() == p.points.len() && p.shape.colors.len() > 1 {
                let colors: Vec<Color> = p
                    .shape
                    .colors
                    .iter()
                    .map(|c| with_opacity(*c, k))
                    .collect();
                ctx.stroke_path_gradient(&p.points, &colors);
            } else {
                ctx.set_color(with_opacity(p.shape.colors[0], k));
                ctx.stroke_path(&p.points, false);
            }
        }

        if cfg.animate {
            let spacing = cfg.flow_spacing;
            let dash = cfg.flow_dash;
            for p in projected.iter().filter(|p| p.shape.flow != 0.0 && p.width >= 1.8) {
                let cum = cumulative_lengths(&p.points);
                let total = cum[cum.len() - 1];
                if total < dash {
                    continue;
                }
                let k = cfg.opacity * p.shape.opacity;
                let w = (p.width * 0.42).max(1.0);
                ctx.set_color(with_opacity(cfg.flow_color, k));
                ctx.set_stroke_width(w);
                ctx.set_anti_alias(0.8);
                let shift = (self.phase * p.shape.flow.abs()).fract() * spacing;
                let mut s = if p.shape.flow > 0.0 {
                    shift - spacing
                } else {
                    total - shift
                };
                let step = if p.shape.flow > 0.0 { spacing } else { -spacing };
                let mut guard = 0;
                while (p.shape.flow > 0.0 && s < total) || (p.shape.flow < 0.0 && s > -spacing) {
                    guard += 1;
                    if guard > 4000 {
                        break;
                    }
                    let (a, b) = if p.shape.flow > 0.0 {
                        (s, s + dash)
                    } else {
                        (s - dash, s)
                    };
                    let piece = sub_polyline(&p.points, &cum, a, b);
                    if piece.len() >= 2 {
                        ctx.stroke_path(&piece, false);
                    }
                    s += step;
                }
            }
        }
    }

    fn paint_nodes(&self, ctx: &mut CanvasContext, list_labels: &mut Vec<(Rect, String)>) {
        let cfg = &self.config;
        let vp = cfg.viewport;
        let zoom = vp.zoom_level as f32;
        let scale = cfg.scale();
        let pulse = 0.5 + 0.5 * (self.phase * std::f32::consts::TAU * 0.5).sin();

        for node in cfg.nodes.iter() {
            if zoom < node.min_zoom {
                continue;
            }
            let (x, y) = cfg.project(node.lat, node.lng);
            let r = if node.fixed_size {
                node.radius
            } else {
                (node.radius * scale).clamp(1.0, 40.0)
            };
            if x < -r * 4.0 || y < -r * 4.0 || x > vp.viewport_w + r * 4.0 || y > vp.viewport_h + r * 4.0 {
                continue;
            }
            let k = cfg.opacity;
            if node.pulse && cfg.animate {
                let halo = r * (1.6 + pulse * 1.2);
                ctx.set_anti_alias(halo * 0.5);
                ctx.set_color(with_opacity(node.fill, k * (0.35 - pulse * 0.25)));
                ctx.fill_circle(x, y, halo);
            }
            ctx.set_anti_alias(0.9);
            ctx.set_color(with_opacity(node.stroke, k));
            paint_glyph(ctx, node.glyph, x, y, r + node.stroke_width);
            ctx.set_color(with_opacity(node.fill, k));
            paint_glyph(ctx, node.glyph, x, y, r);
            if node.glyph == NodeGlyph::Ring {
                ctx.set_color(with_opacity(node.stroke, k));
                paint_glyph(ctx, NodeGlyph::Circle, x, y, r * 0.45);
            }

            if let Some(label) = &node.label {
                if zoom >= node.label_min_zoom && !label.is_empty() {
                    let font = cfg.label_size;
                    let text_w = self
                        .text_measure
                        .as_deref()
                        .map(|tm| tm.measure_text_width(label, font, label.chars().count()))
                        .unwrap_or(label.chars().count() as f32 * font * 0.58);
                    let (pad_x, h) = (5.0, font + 6.0);
                    let w = text_w + pad_x * 2.0;
                    let rect = Rect::new(
                        Point::new(x + r + 4.0, y - h * 0.5),
                        Size::new(w, h),
                    );
                    list_labels.push((rect, label.clone()));
                }
            }
        }
    }
}

fn paint_glyph(ctx: &mut CanvasContext, glyph: NodeGlyph, x: f32, y: f32, r: f32) {
    match glyph {
        NodeGlyph::Circle => ctx.fill_circle(x, y, r),
        NodeGlyph::Ring => ctx.fill_circle(x, y, r),
        NodeGlyph::Square => ctx.fill_rect(x - r, y - r, r * 2.0, r * 2.0),
        NodeGlyph::Diamond => {
            ctx.fill_polygon(&[(x, y - r), (x + r, y), (x, y + r), (x - r, y)]);
        }
        NodeGlyph::Hexagon => {
            let pts: Vec<(f32, f32)> = (0..6)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / 6.0 + std::f32::consts::FRAC_PI_6;
                    (x + r * a.cos(), y + r * a.sin())
                })
                .collect();
            ctx.fill_polygon(&pts);
        }
    }
}

impl Element for PipeOverlayElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<PipeOverlay>() {
            self.config = w.snapshot();
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
        let vp = self.config.viewport;
        if vp.viewport_w <= 1.0 || vp.viewport_h <= 1.0 {
            return;
        }
        if self.config.pipes.is_empty() && self.config.nodes.is_empty() {
            return;
        }
        list.push_clip(self.bounds);
        let mut ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);
        self.paint_pipes(&mut ctx);
        let mut labels = Vec::new();
        self.paint_nodes(&mut ctx, &mut labels);
        ctx.flush(list);

        let origin = self.bounds.origin;
        let mut placed: Vec<Rect> = Vec::with_capacity(labels.len());
        let font = self.config.label_size;
        let k = self.config.opacity;
        for (rect, text) in labels {
            if placed.iter().any(|r| r.intersects(&rect)) {
                continue;
            }
            placed.push(rect);
            let screen = Rect::new(
                Point::new(origin.x + rect.origin.x, origin.y + rect.origin.y),
                rect.size,
            );
            list.push_rect(
                screen,
                with_opacity(self.config.label_background, k),
                [4.0; 4],
            );
            list.push_text_styled_singleline(
                &text,
                Rect::new(
                    Point::new(screen.origin.x + 5.0, screen.origin.y + 2.0),
                    Size::new(screen.size.width - 10.0, screen.size.height - 4.0),
                ),
                with_opacity(self.config.label_color, k),
                font,
                TextAlign::LEFT,
                TextDecoration::None,
                600,
                None,
            );
        }
        list.pop_clip();
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if !self.config.has_flow() {
            return false;
        }
        let dt = crate::animation::effective_dt(dt).as_secs_f32();
        let spacing = self.config.flow_spacing.max(1.0);
        self.phase = (self.phase + dt * self.config.flow_speed / spacing).rem_euclid(1024.0);
        self.mark_dirty(DirtyFlags::RENDER);
        true
    }

    fn needs_repaint(&self) -> bool {
        self.config.has_flow()
    }

    fn wants_animate_tick(&self) -> bool {
        self.config.has_flow()
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

    fn passthrough_hit_test(&self) -> bool {
        true
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

    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

    fn element_type_name(&self) -> &str {
        "PipeOverlay"
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

impl StyledElement for PipeOverlayElement {
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

    #[test]
    fn offset_shifts_straight_line_sideways() {
        let line = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)];
        let shifted = offset_polyline(&line, 2.0);
        for p in &shifted {
            assert!((p.1 - 2.0).abs() < 1e-5, "{shifted:?}");
        }
        assert_eq!(offset_polyline(&line, 0.0), line.to_vec());
    }

    #[test]
    fn offset_keeps_right_angle_corner_at_miter() {
        let line = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let shifted = offset_polyline(&line, 1.0);
        let corner = shifted[1];
        assert!((corner.0 - 9.0).abs() < 1e-4 && (corner.1 - 1.0).abs() < 1e-4, "{corner:?}");
    }

    #[test]
    fn sub_polyline_cuts_between_lengths() {
        let line = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let cum = cumulative_lengths(&line);
        assert_eq!(cum, vec![0.0, 10.0, 20.0]);
        let piece = sub_polyline(&line, &cum, 5.0, 15.0);
        assert_eq!(piece, vec![(5.0, 0.0), (10.0, 0.0), (10.0, 5.0)]);
        assert!(sub_polyline(&line, &cum, 25.0, 30.0).is_empty());
    }

    #[test]
    fn polyline_distance_finds_nearest_segment() {
        let line = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let (d, seg, t) = polyline_distance(&line, (10.0, 7.0)).unwrap();
        assert!(d.abs() < 1e-5);
        assert_eq!(seg, 1);
        assert!((t - 0.7).abs() < 1e-5);
        let (d, seg, _) = polyline_distance(&line, (3.0, 4.0)).unwrap();
        assert!((d - 4.0).abs() < 1e-5);
        assert_eq!(seg, 0);
    }

    #[test]
    fn zoom_scale_is_one_at_reference() {
        assert!((zoom_scale(16.0, 16.0, 0.55) - 1.0).abs() < 1e-6);
        assert!(zoom_scale(18.0, 16.0, 1.0) > 3.99);
        assert!((zoom_scale(10.0, 16.0, 0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn nearest_pipe_respects_width_and_tolerance() {
        let vp = MapViewport {
            center_lat: 53.2,
            center_lng: 63.6,
            zoom: 16,
            zoom_level: 16.0,
            viewport_w: 400.0,
            viewport_h: 300.0,
        };
        let a = tile_math::pixel_to_geo_f(100.0, 150.0, 53.2, 63.6, 16.0, 400.0, 300.0);
        let b = tile_math::pixel_to_geo_f(300.0, 150.0, 53.2, 63.6, 16.0, 400.0, 300.0);
        let pipes = vec![PipeShape::new(vec![a, b]).id(7).width(6.0)];
        let hit = nearest_pipe(&pipes, vp, 16.0, 0.55, (1.0, 20.0), 200.0, 154.0, 2.0).unwrap();
        assert_eq!(hit.id, 7);
        assert!((hit.t - 0.5).abs() < 0.01);
        assert!(nearest_pipe(&pipes, vp, 16.0, 0.55, (1.0, 20.0), 200.0, 170.0, 2.0).is_none());
    }
}
