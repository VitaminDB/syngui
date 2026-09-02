//! Векторные примитивы документа: рамочные (прямоугольник, овал,
//! треугольник, ромб) и линейные (линия, стрелка, двойная стрелка).
//!
//! Оформление живёт в атрибутах блока (`{fill=#… stroke=#… sw=2 dash=6
//! radius=8 opacity=70}`) — как и прочие свойства блока, поэтому переживает
//! дублирование, перенос и round-trip markdown. Рамочная фигура занимает
//! прямоугольник блока (ширина — `w` свободной раскладки, высота — `h`);
//! линейная задаётся концами `x1 y1 x2 y2` в координатах **внутри** блока,
//! а кривая Безье — ещё и двумя направляющими `cx1 cy1 cx2 cy2`. Рамка
//! линейной фигуры — bbox её точек с полем [`LINE_PAD`] под наконечники и
//! хваталки; кривая целиком лежит в выпуклой оболочке своих четырёх точек,
//! поэтому такой bbox её всегда накрывает.
//!
//! Рисуется всё одним `CanvasContext` (заливка тесселяцией, обводка
//! линейными полосами) — как рёбра канваса и ручка ⋮⋮ редактора.

use std::any::Any;
use std::sync::Arc;

use crate::core::canvas::tessellator::{arc_points, flatten_cubic_bezier};
use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, UpdateContext};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, Widget};

use super::model::{Attrs, BlockId, ShapeKind};
use super::props;
use super::style::DocStyle;

/// Цвет заливки (`#rrggbb`); отсутствует — фигура без заливки.
pub const FILL: &str = "fill";
/// Цвет обводки.
pub const STROKE: &str = "stroke";
/// Толщина обводки в пикселях.
pub const STROKE_W: &str = "sw";
/// Длина штриха пунктира (0 — сплошная линия).
pub const DASH: &str = "dash";
/// Скругление углов прямоугольника.
pub const RADIUS: &str = "radius";
/// Непрозрачность в процентах (100 — как есть).
pub const OPACITY: &str = "opacity";
/// Концы линейной фигуры относительно левого-верхнего угла блока.
pub const X1: &str = "x1";
pub const Y1: &str = "y1";
pub const X2: &str = "x2";
pub const Y2: &str = "y2";
/// Направляющие точки кривой Безье (в тех же координатах).
pub const CX1: &str = "cx1";
pub const CY1: &str = "cy1";
pub const CX2: &str = "cx2";
pub const CY2: &str = "cy2";

/// Ключи, которыми управляет панель свойств фигуры.
pub const SHAPE_KEYS: [&str; 6] = [FILL, STROKE, STROKE_W, DASH, RADIUS, OPACITY];

/// Поле вокруг отрезка: под наконечник стрелки и хваталки концов.
pub const LINE_PAD: f32 = 12.0;

/// Высота рамочной фигуры по умолчанию.
pub const DEFAULT_H: f32 = 140.0;
/// Ширина рамочной фигуры по умолчанию (и длина новой линии).
pub const DEFAULT_W: f32 = 220.0;

/// Оформление примитива, собранное из атрибутов поверх темы.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeStyle {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_w: f32,
    pub dash: f32,
    pub radius: f32,
    pub opacity: f32,
}

impl ShapeStyle {
    pub fn from_attrs(attrs: &Attrs, kind: ShapeKind, style: &DocStyle) -> Self {
        let opacity = attrs
            .get(OPACITY)
            .and_then(|v| v.parse::<f32>().ok())
            .map(|v| (v / 100.0).clamp(0.0, 1.0))
            .unwrap_or(1.0);
        // Заливка есть только у рамочных фигур и только если задана явно:
        // пустой контур поверх текста читается лучше глухого прямоугольника.
        let fill = (!kind.is_line())
            .then(|| props::color_of(attrs, FILL))
            .flatten()
            .map(|c| c.with_alpha(c.a * opacity));
        // Обводка снимается только явным `stroke=none` — иначе фигура без
        // заливки стала бы невидимой.
        let stroke = match attrs.get(STROKE) {
            Some("none") => None,
            _ => Some(
                props::color_of(attrs, STROKE)
                    .unwrap_or(style.shape_stroke_color)
                    .let_alpha(opacity),
            ),
        };
        Self {
            fill,
            stroke,
            stroke_w: attrs
                .get(STROKE_W)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| (0.0..=40.0).contains(v))
                .unwrap_or(2.0),
            dash: attrs
                .get(DASH)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| (0.0..=60.0).contains(v))
                .unwrap_or(0.0),
            radius: attrs
                .get(RADIUS)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| (0.0..=200.0).contains(v))
                .unwrap_or(if matches!(kind, ShapeKind::Rect) { 8.0 } else { 0.0 }),
            opacity,
        }
    }
}

/// Домножение альфы (у `Color` только `with_alpha`, а нужно именно домножить).
trait AlphaMul {
    fn let_alpha(self, k: f32) -> Color;
}

impl AlphaMul for Color {
    fn let_alpha(self, k: f32) -> Color {
        self.with_alpha(self.a * k)
    }
}

/// Высота рамочной фигуры из атрибутов.
pub fn height_of(attrs: &Attrs) -> f32 {
    super::free::height_of(attrs).unwrap_or(DEFAULT_H)
}

/// Концы линейной фигуры (локальные координаты блока, без поля).
/// По умолчанию — горизонтальный отрезок длиной `DEFAULT_W`.
pub fn endpoints_of(attrs: &Attrs) -> ((f32, f32), (f32, f32)) {
    let num = |k: &str, d: f32| {
        attrs.get(k).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(d)
    };
    let w = super::free::width_of(attrs).map(|w| (w - LINE_PAD * 2.0).max(20.0)).unwrap_or(DEFAULT_W);
    ((num(X1, 0.0), num(Y1, 0.0)), (num(X2, w), num(Y2, 0.0)))
}

pub fn set_endpoints(attrs: &mut Attrs, p1: (f32, f32), p2: (f32, f32)) {
    for (key, v) in [(X1, p1.0), (Y1, p1.1), (X2, p2.0), (Y2, p2.1)] {
        attrs.set(key, fmt(v));
    }
}

/// Направляющие кривой. Пока их не трогали мышью, они выводятся из концов —
/// плавная S-образная дуга по длинной оси, как провода нодового редактора.
pub fn controls_of(attrs: &Attrs) -> ((f32, f32), (f32, f32)) {
    let (p1, p2) = endpoints_of(attrs);
    let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
    let (d1, d2) = if dx.abs() >= dy.abs() {
        let off = (dx.abs() * 0.45).max(40.0) * dx.signum();
        ((off, 0.0), (-off, 0.0))
    } else {
        let off = (dy.abs() * 0.45).max(40.0) * dy.signum();
        ((0.0, off), (0.0, -off))
    };
    let num = |k: &str, d: f32| {
        attrs.get(k).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(d)
    };
    (
        (num(CX1, p1.0 + d1.0), num(CY1, p1.1 + d1.1)),
        (num(CX2, p2.0 + d2.0), num(CY2, p2.1 + d2.1)),
    )
}

pub fn set_controls(attrs: &mut Attrs, c1: (f32, f32), c2: (f32, f32)) {
    for (key, v) in [(CX1, c1.0), (CY1, c1.1), (CX2, c2.0), (CY2, c2.1)] {
        attrs.set(key, fmt(v));
    }
}

/// Все точки линейной фигуры: концы, а у кривой — ещё направляющие.
/// Порядок — тот же, что у хваталок: конец, конец, направляющая,
/// направляющая.
pub fn line_handles(attrs: &Attrs, kind: ShapeKind) -> Vec<(f32, f32)> {
    let (p1, p2) = endpoints_of(attrs);
    if !kind.is_curve() {
        return vec![p1, p2];
    }
    let (c1, c2) = controls_of(attrs);
    vec![p1, p2, c1, c2]
}

fn fmt(v: f32) -> String {
    let r = (v * 10.0).round() / 10.0;
    if (r - r.round()).abs() < f32::EPSILON {
        format!("{}", r.round() as i64)
    } else {
        format!("{r}")
    }
}

/// Габариты линейной фигуры: bbox её точек плюс поле под наконечники.
pub fn line_box(attrs: &Attrs, kind: ShapeKind) -> Size {
    let pts = line_handles(attrs, kind);
    let (min, max) = bounds(&pts);
    Size::new(max.0 - min.0 + LINE_PAD * 2.0, max.1 - min.1 + LINE_PAD * 2.0)
}

fn bounds(points: &[(f32, f32)]) -> ((f32, f32), (f32, f32)) {
    let mut min = (f32::MAX, f32::MAX);
    let mut max = (f32::MIN, f32::MIN);
    for p in points {
        min = (min.0.min(p.0), min.1.min(p.1));
        max = (max.0.max(p.0), max.1.max(p.1));
    }
    (min, max)
}

/// Точки в координатах виджета (с полем и с нулём в минимуме bbox).
pub fn local_handles(attrs: &Attrs, kind: ShapeKind) -> Vec<(f32, f32)> {
    let pts = line_handles(attrs, kind);
    let (min, _) = bounds(&pts);
    pts.iter().map(|p| (p.0 - min.0 + LINE_PAD, p.1 - min.1 + LINE_PAD)).collect()
}

/// Концы в координатах виджета (частый случай — прямая линия).
pub fn local_endpoints(attrs: &Attrs, kind: ShapeKind) -> ((f32, f32), (f32, f32)) {
    let h = local_handles(attrs, kind);
    (h[0], h[1])
}

/// Ломаная, по которой рисуется и щупается линейная фигура: у прямой это
/// сам отрезок, у кривой — сглаженная кубическая Безье.
pub fn line_polyline(attrs: &Attrs, kind: ShapeKind) -> Vec<(f32, f32)> {
    let h = local_handles(attrs, kind);
    if !kind.is_curve() {
        return vec![h[0], h[1]];
    }
    let pt = |p: (f32, f32)| Point::new(p.0, p.1);
    flatten_cubic_bezier(pt(h[0]), pt(h[2]), pt(h[3]), pt(h[1]), 0.25)
        .into_iter()
        .map(|p| (p.x, p.y))
        .collect()
}

// ─── Отрисовка ──────────────────────────────────────────────────────────────

/// Нарисовать примитив в локальных координатах холста размера `size`.
pub fn draw(c: &mut CanvasContext, kind: ShapeKind, size: Size, st: &ShapeStyle, attrs: &Attrs) {
    c.set_anti_alias(1.0);
    if kind.is_line() {
        draw_line_shape(c, kind, st, attrs);
        return;
    }
    let sw = st.stroke.map(|_| st.stroke_w).unwrap_or(0.0);
    let inset = sw / 2.0;
    let (w, h) = (size.width - inset * 2.0, size.height - inset * 2.0);
    if w <= 1.0 || h <= 1.0 {
        return;
    }
    let contour = match kind {
        ShapeKind::Rect => rect_contour(inset, inset, w, h, st.radius.min(w / 2.0).min(h / 2.0)),
        ShapeKind::Ellipse => ellipse_contour(inset + w / 2.0, inset + h / 2.0, w / 2.0, h / 2.0),
        ShapeKind::Triangle => vec![
            (inset + w / 2.0, inset),
            (inset + w, inset + h),
            (inset, inset + h),
        ],
        ShapeKind::Diamond => vec![
            (inset + w / 2.0, inset),
            (inset + w, inset + h / 2.0),
            (inset + w / 2.0, inset + h),
            (inset, inset + h / 2.0),
        ],
        _ => return,
    };
    if let Some(fill) = st.fill {
        c.set_color(fill);
        c.fill_polygon_concave(&contour);
    }
    if let Some(stroke) = st.stroke {
        c.set_color(stroke);
        c.set_stroke_width(st.stroke_w);
        let mut closed = contour.clone();
        if let Some(&first) = contour.first() {
            closed.push(first);
        }
        stroke_path(c, &closed, st.dash);
    }
}

fn draw_line_shape(c: &mut CanvasContext, kind: ShapeKind, st: &ShapeStyle, attrs: &Attrs) {
    let Some(stroke) = st.stroke else { return };
    let mut path = line_polyline(attrs, kind);
    if path.len() < 2 {
        return;
    }
    // Наконечник рисуется отдельным треугольником, поэтому линию под ним
    // укорачиваем — иначе она торчит из острия.
    let head = 4.0 + st.stroke_w * 2.2;
    let start_dir = direction(path[0], path[1]);
    let end_dir = direction(path[path.len() - 2], path[path.len() - 1]);
    let (tip_start, tip_end) = (path[0], path[path.len() - 1]);
    if kind.arrow_start() {
        trim_front(&mut path, head * 0.8);
    }
    if kind.arrow_end() {
        path.reverse();
        trim_front(&mut path, head * 0.8);
        path.reverse();
    }
    if path.len() >= 2 {
        c.set_color(stroke);
        c.set_stroke_width(st.stroke_w);
        stroke_path(c, &path, st.dash);
    }
    if kind.arrow_end() {
        arrow_head(c, tip_end, end_dir, head, stroke);
    }
    if kind.arrow_start() {
        arrow_head(c, tip_start, (-start_dir.0, -start_dir.1), head, stroke);
    }
}

/// Единичный вектор от `a` к `b` (нулевой отрезок — вправо).
fn direction(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        (1.0, 0.0)
    } else {
        (dx / len, dy / len)
    }
}

/// Убрать `cut` пикселей длины с начала ломаной (под наконечник).
fn trim_front(path: &mut Vec<(f32, f32)>, cut: f32) {
    let mut left = cut;
    while path.len() >= 2 {
        let (a, b) = (path[0], path[1]);
        let seg = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if seg > left {
            let t = left / seg.max(0.001);
            path[0] = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            return;
        }
        left -= seg;
        path.remove(0);
    }
}

/// Наконечник: закрашенный треугольник в точке `tip` по направлению `dir`.
fn arrow_head(c: &mut CanvasContext, tip: (f32, f32), dir: (f32, f32), head: f32, color: Color) {
    let (ux, uy) = dir;
    let (nx, ny) = (-uy, ux);
    let base = (tip.0 - ux * head, tip.1 - uy * head);
    let half = head * 0.5;
    c.set_color(color);
    c.fill_polygon(&[
        tip,
        (base.0 + nx * half, base.1 + ny * half),
        (base.0 - nx * half, base.1 - ny * half),
    ]);
}

/// Обводка пути: сплошная полилиния либо набор штрихов.
fn stroke_path(c: &mut CanvasContext, points: &[(f32, f32)], dash: f32) {
    if dash < 1.0 {
        c.draw_polyline(points);
        return;
    }
    let gap = dash * 0.75;
    let mut phase = 0.0f32; // Пройдено внутри текущего штриха/пробела.
    let mut on = true;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.01 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        let mut done = 0.0f32;
        while done < len {
            let limit = if on { dash } else { gap } - phase;
            let step = limit.min(len - done);
            if on {
                let s = (a.0 + ux * done, a.1 + uy * done);
                let e = (a.0 + ux * (done + step), a.1 + uy * (done + step));
                c.draw_line(s.0, s.1, e.0, e.1);
            }
            done += step;
            phase += step;
            if phase >= if on { dash } else { gap } - 0.001 {
                on = !on;
                phase = 0.0;
            }
        }
    }
}

/// Контур прямоугольника со скруглением (по часовой стрелке).
fn rect_contour(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<(f32, f32)> {
    if r < 0.5 {
        return vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    }
    let seg = 8;
    let pi = std::f32::consts::PI;
    let mut out = Vec::with_capacity(seg * 4 + 4);
    let corner = |cx: f32, cy: f32, from: f32, to: f32| {
        arc_points(Point::new(cx, cy), r, from, to, seg)
            .into_iter()
            .map(|p| (p.x, p.y))
            .collect::<Vec<_>>()
    };
    out.extend(corner(x + r, y + r, pi, pi * 1.5));
    out.extend(corner(x + w - r, y + r, pi * 1.5, pi * 2.0));
    out.extend(corner(x + w - r, y + h - r, 0.0, pi * 0.5));
    out.extend(corner(x + r, y + h - r, pi * 0.5, pi));
    out
}

fn ellipse_contour(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<(f32, f32)> {
    let seg = ((rx.max(ry) * 1.6) as usize).clamp(28, 160);
    (0..seg)
        .map(|i| {
            let a = i as f32 / seg as f32 * std::f32::consts::TAU;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

// ─── Виджет ─────────────────────────────────────────────────────────────────

pub struct ShapeView {
    pub block_id: BlockId,
    pub shape: ShapeKind,
    pub attrs: Attrs,
    pub style: Arc<DocStyle>,
}

impl Widget for ShapeView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ShapeElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            block_id: self.block_id,
            shape: self.shape,
            attrs: self.attrs.clone(),
            style: self.style.clone(),
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

pub struct ShapeElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    block_id: BlockId,
    shape: ShapeKind,
    attrs: Attrs,
    style: Arc<DocStyle>,
}

impl Element for ShapeElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<ShapeView>() else { return };
        let resized = self.shape != w.shape || self.attrs != w.attrs;
        self.block_id = w.block_id;
        self.shape = w.shape;
        self.attrs = w.attrs.clone();
        self.style = w.style.clone();
        if resized {
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            ctx.mark_layout_dirty();
        } else {
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn layout(&mut self, constraints: Constraints) -> Size {
        let avail =
            if constraints.max_width.is_finite() { constraints.max_width } else { DEFAULT_W };
        self.bounds.size = if self.shape.is_line() {
            let b = line_box(&self.attrs, self.shape);
            Size::new(b.width.min(avail.max(40.0)), b.height)
        } else {
            Size::new(avail, height_of(&self.attrs))
        };
        self.bounds.size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let st = ShapeStyle::from_attrs(&self.attrs, self.shape, &self.style);
        let mut c = CanvasContext::new(self.bounds.origin, self.bounds.size);
        draw(&mut c, self.shape, self.bounds.size, &st, &self.attrs);
        c.flush(list);
    }

    fn element_type_name(&self) -> &str {
        "doc-shape"
    }

    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }
    fn children(&self) -> &[ElementId] {
        &[]
    }
    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty |= flags;
    }
    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.remove(flags);
    }
    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty.contains(flags)
    }
    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_roundtrip() {
        for k in ShapeKind::ALL {
            assert_eq!(ShapeKind::from_name(k.name()), Some(k), "{}", k.name());
        }
        assert_eq!(ShapeKind::from_name("circle"), Some(ShapeKind::Ellipse));
        assert!(ShapeKind::from_name("нечто").is_none());
    }

    #[test]
    fn style_from_attrs() {
        let mut a = Attrs::default();
        a.set(FILL, "#4f8cff");
        a.set(STROKE_W, "3");
        a.set(OPACITY, "50");
        let st = ShapeStyle::from_attrs(&a, ShapeKind::Rect, &DocStyle::default());
        assert_eq!(st.stroke_w, 3.0);
        assert!(st.fill.is_some_and(|c| (c.a - 0.5).abs() < 0.01));
        // У линии заливки нет, даже если атрибут задан.
        let line = ShapeStyle::from_attrs(&a, ShapeKind::Line, &DocStyle::default());
        assert!(line.fill.is_none());
        // Мусор в значениях не должен ломать умолчания.
        let mut bad = Attrs::default();
        bad.set(STROKE_W, "толстая");
        bad.set(RADIUS, "-5");
        let st = ShapeStyle::from_attrs(&bad, ShapeKind::Rect, &DocStyle::default());
        assert_eq!(st.stroke_w, 2.0);
        assert_eq!(st.radius, 8.0);
    }

    #[test]
    fn stroke_none_removes_outline() {
        let mut a = Attrs::default();
        a.set(STROKE, "none");
        let st = ShapeStyle::from_attrs(&a, ShapeKind::Rect, &DocStyle::default());
        assert!(st.stroke.is_none());
    }

    #[test]
    fn line_geometry() {
        let mut a = Attrs::default();
        set_endpoints(&mut a, (10.0, 40.0), (110.0, 0.0));
        let b = line_box(&a, ShapeKind::Line);
        assert_eq!(b.width, 100.0 + LINE_PAD * 2.0);
        assert_eq!(b.height, 40.0 + LINE_PAD * 2.0);
        let (p1, p2) = local_endpoints(&a, ShapeKind::Line);
        assert_eq!(p1, (LINE_PAD, 40.0 + LINE_PAD));
        assert_eq!(p2, (100.0 + LINE_PAD, LINE_PAD));
    }

    #[test]
    fn default_line_is_horizontal() {
        let (p1, p2) = endpoints_of(&Attrs::default());
        assert_eq!(p1, (0.0, 0.0));
        assert_eq!(p2, (DEFAULT_W, 0.0));
    }
}
