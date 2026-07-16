use crate::core::{Point, Color};
use crate::core::geometry::Bezier;
use crate::render::Vertex;
use super::paint::Paint;

#[derive(Debug, Default, Clone)]
pub struct TessOutput {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl TessOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    pub fn merge(&mut self, other: &TessOutput) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

const ZERO_DATA: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
const ZERO_UV: [f32; 2] = [0.0, 0.0];

pub fn tessellate_line_segment(
    p0: Point,
    p1: Point,
    paint: &Paint,
    output: &mut TessOutput,
) {
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return;
    }

    let nx = -dy / len;
    let ny = dx / len;

    let hw = paint.stroke_width * 0.5;
    let f = paint.feather;

    let color = paint.color.to_array();
    let color_transparent = [color[0], color[1], color[2], 0.0];

    let base = output.vertices.len() as u32;

    let offsets = [
        (hw + f, color_transparent),
        (hw,     color),
        (-hw,    color),
        (-(hw + f), color_transparent),
    ];

    for &(offset, col) in &offsets {
        let ox = nx * offset;
        let oy = ny * offset;
        output.vertices.extend_from_slice(&[
            Vertex::new([p0.x + ox, p0.y + oy], ZERO_UV, col, ZERO_DATA),
            Vertex::new([p1.x + ox, p1.y + oy], ZERO_UV, col, ZERO_DATA),
        ]);
    }

    for i in 0..3u32 {
        let row = i * 2;
        output.indices.extend_from_slice(&[
            base + row,     base + row + 1, base + row + 3,
            base + row,     base + row + 3, base + row + 2,
        ]);
    }
}

pub fn tessellate_polyline(
    points: &[Point],
    paint: &Paint,
    closed: bool,
    output: &mut TessOutput,
) {
    if points.len() < 2 {
        return;
    }

    let n = points.len();
    let hw = paint.stroke_width * 0.5;
    let f = paint.feather;
    let color = paint.color.to_array();
    let color_t = [color[0], color[1], color[2], 0.0];

    let base = output.vertices.len() as u32;
    let seg_count = if closed { n } else { n - 1 };
    output.vertices.reserve(n * 4);
    output.indices.reserve(seg_count * 18);

    let seg_normal = |a: Point, b: Point| -> (f32, f32) {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 { (0.0, 1.0) } else { (-dy / len, dx / len) }
    };

    for i in 0..n {
        let (nx, ny, scale);

        if closed {
            let prev = points[(i + n - 1) % n];
            let curr = points[i];
            let next = points[(i + 1) % n];
            let (n1x, n1y) = seg_normal(prev, curr);
            let (n2x, n2y) = seg_normal(curr, next);
            let r = miter_normal(n1x, n1y, n2x, n2y);
            nx = r.0; ny = r.1; scale = r.2;
        } else if i == 0 {
            let (snx, sny) = seg_normal(points[0], points[1]);
            nx = snx; ny = sny; scale = 1.0;
        } else if i == n - 1 {
            let (snx, sny) = seg_normal(points[n - 2], points[n - 1]);
            nx = snx; ny = sny; scale = 1.0;
        } else {
            let (n1x, n1y) = seg_normal(points[i - 1], points[i]);
            let (n2x, n2y) = seg_normal(points[i], points[i + 1]);
            let r = miter_normal(n1x, n1y, n2x, n2y);
            nx = r.0; ny = r.1; scale = r.2;
        }

        let p = points[i];
        let offsets: [(f32, [f32; 4]); 4] = [
            ((hw + f) * scale, color_t),
            (hw * scale,       color),
            (-hw * scale,      color),
            (-(hw + f) * scale, color_t),
        ];
        output.vertices.extend_from_slice(&[
            Vertex::new([p.x + nx * offsets[0].0, p.y + ny * offsets[0].0], ZERO_UV, offsets[0].1, ZERO_DATA),
            Vertex::new([p.x + nx * offsets[1].0, p.y + ny * offsets[1].0], ZERO_UV, offsets[1].1, ZERO_DATA),
            Vertex::new([p.x + nx * offsets[2].0, p.y + ny * offsets[2].0], ZERO_UV, offsets[2].1, ZERO_DATA),
            Vertex::new([p.x + nx * offsets[3].0, p.y + ny * offsets[3].0], ZERO_UV, offsets[3].1, ZERO_DATA),
        ]);
    }

    for i in 0..seg_count {
        let c = base + (i as u32) * 4;
        let nx = base + (((i + 1) % n) as u32) * 4;
        for q in 0..3u32 {
            output.indices.extend_from_slice(&[
                c + q,     nx + q,     nx + q + 1,
                c + q,     nx + q + 1, c + q + 1,
            ]);
        }
    }
}

fn miter_normal(n1x: f32, n1y: f32, n2x: f32, n2y: f32) -> (f32, f32, f32) {
    let mx = (n1x + n2x) * 0.5;
    let my = (n1y + n2y) * 0.5;
    let mlen = (mx * mx + my * my).sqrt();
    if mlen < 1e-6 {
        (n1x, n1y, 1.0)
    } else {
        let nx = mx / mlen;
        let ny = my / mlen;
        let dot = nx * n1x + ny * n1y;
        let scale = if dot > 0.15 { (1.0 / dot).min(3.0) } else { 3.0 };
        (nx, ny, scale)
    }
}

pub fn tessellate_fill_polygon(
    points: &[Point],
    color: Color,
    output: &mut TessOutput,
) {
    if points.len() < 3 {
        return;
    }

    let base = output.vertices.len() as u32;
    let col = color.to_array();

    output.vertices.reserve(points.len());
    output.indices.reserve((points.len() - 2) * 3);

    output.vertices.extend(points.iter().map(|p| {
        Vertex::new([p.x, p.y], ZERO_UV, col, ZERO_DATA)
    }));

    for i in 1..points.len() as u32 - 1 {
        output.indices.extend_from_slice(&[base, base + i, base + i + 1]);
    }
}

pub fn tessellate_area_strip(
    curve_points: &[(f32, f32)],
    baseline_y: f32,
    color: Color,
    output: &mut TessOutput,
) {
    if curve_points.len() < 2 {
        return;
    }

    let base = output.vertices.len() as u32;
    let col = color.to_array();

    output.vertices.reserve(curve_points.len() * 2);
    output.indices.reserve((curve_points.len() - 1) * 6);

    for &(x, y) in curve_points {
        output.vertices.extend_from_slice(&[
            Vertex::new([x, y], ZERO_UV, col, ZERO_DATA),
            Vertex::new([x, baseline_y], ZERO_UV, col, ZERO_DATA),
        ]);
    }

    for i in 0..curve_points.len() as u32 - 1 {
        let t0 = base + i * 2;
        let b0 = base + i * 2 + 1;
        let t1 = base + (i + 1) * 2;
        let b1 = base + (i + 1) * 2 + 1;
        output.indices.extend_from_slice(&[t0, b0, t1, t1, b0, b1]);
    }
}

fn polygon_signed_area(points: &[Point]) -> f32 {
    let n = points.len();
    let mut s = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        s += points[i].x * points[j].y - points[j].x * points[i].y;
    }
    s * 0.5
}

fn triangle_sign(p1: Point, p2: Point, p3: Point) -> f32 {
    (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
}

fn point_in_triangle(p: Point, a: Point, b: Point, c: Point) -> bool {
    let d1 = triangle_sign(p, a, b);
    let d2 = triangle_sign(p, b, c);
    let d3 = triangle_sign(p, c, a);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

fn cross_turn(a: Point, b: Point, c: Point) -> f32 {
    (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)
}

/// Заливка произвольного простого полигона (в т.ч. невыпуклого) методом
/// ear-clipping. В отличие от `tessellate_fill_polygon` (триангуляция веером,
/// верна только для выпуклых), корректно заливает Г-/П-образные контуры.
/// Ориентация полигона любая — тест выпуклости привязан к знаку площади.
pub fn tessellate_fill_polygon_concave(points: &[Point], color: Color, output: &mut TessOutput) {
    let n = points.len();
    if n < 3 {
        return;
    }

    let base = output.vertices.len() as u32;
    let col = color.to_array();
    output.vertices.extend(points.iter().map(|p| {
        Vertex::new([p.x, p.y], ZERO_UV, col, ZERO_DATA)
    }));

    if n == 3 {
        output.indices.extend_from_slice(&[base, base + 1, base + 2]);
        return;
    }

    let ws = if polygon_signed_area(points) >= 0.0 { 1.0 } else { -1.0 };

    let mut idx: Vec<usize> = (0..n).collect();
    let mut guard = 0;
    let guard_max = n * n + 4;
    while idx.len() > 3 {
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let i_prev = idx[(i + m - 1) % m];
            let i_curr = idx[i];
            let i_next = idx[(i + 1) % m];
            let a = points[i_prev];
            let b = points[i_curr];
            let c = points[i_next];
            if cross_turn(a, b, c) * ws <= 0.0 {
                continue;
            }
            let mut contains = false;
            for &j in &idx {
                if j == i_prev || j == i_curr || j == i_next {
                    continue;
                }
                if point_in_triangle(points[j], a, b, c) {
                    contains = true;
                    break;
                }
            }
            if contains {
                continue;
            }
            output.indices.extend_from_slice(&[
                base + i_prev as u32,
                base + i_curr as u32,
                base + i_next as u32,
            ]);
            idx.remove(i);
            clipped = true;
            break;
        }
        guard += 1;
        if !clipped || guard > guard_max {
            break;
        }
    }
    if idx.len() == 3 {
        output.indices.extend_from_slice(&[
            base + idx[0] as u32,
            base + idx[1] as u32,
            base + idx[2] as u32,
        ]);
    }
}

pub fn tessellate_fill_polygon_aa(
    points: &[Point],
    color: Color,
    feather: f32,
    output: &mut TessOutput,
) {
    if points.len() < 3 {
        return;
    }

    let n = points.len();
    let cx: f32 = points.iter().map(|p| p.x).sum::<f32>() / n as f32;
    let cy: f32 = points.iter().map(|p| p.y).sum::<f32>() / n as f32;

    let base = output.vertices.len() as u32;
    let col = color.to_array();
    let col_transparent = [col[0], col[1], col[2], 0.0];

    output.vertices.reserve(n * 2);
    output.indices.reserve((n - 2) * 3 + n * 6);

    output.vertices.extend(points.iter().map(|p| {
        Vertex::new([p.x, p.y], ZERO_UV, col, ZERO_DATA)
    }));

    {
        use wide::f32x4;
        let chunks = points.chunks_exact(4);
        let remainder = chunks.remainder();
        for chunk in chunks {
            let dxs = f32x4::new([chunk[0].x - cx, chunk[1].x - cx, chunk[2].x - cx, chunk[3].x - cx]);
            let dys = f32x4::new([chunk[0].y - cy, chunk[1].y - cy, chunk[2].y - cy, chunk[3].y - cy]);
            let dist_sq = dxs * dxs + dys * dys;
            let dists = dist_sq.sqrt();
            let d: [f32; 4] = dists.into();
            let dx: [f32; 4] = dxs.into();
            let dy: [f32; 4] = dys.into();
            for j in 0..4 {
                if d[j] < 0.001 {
                    output.vertices.push(Vertex::new([chunk[j].x, chunk[j].y], ZERO_UV, col_transparent, ZERO_DATA));
                } else {
                    let scale = feather / d[j];
                    output.vertices.push(Vertex::new([chunk[j].x + dx[j] * scale, chunk[j].y + dy[j] * scale], ZERO_UV, col_transparent, ZERO_DATA));
                }
            }
        }
        for p in remainder {
            let dx = p.x - cx;
            let dy = p.y - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 0.001 {
                output.vertices.push(Vertex::new([p.x, p.y], ZERO_UV, col_transparent, ZERO_DATA));
            } else {
                let scale = feather / dist;
                output.vertices.push(Vertex::new([p.x + dx * scale, p.y + dy * scale], ZERO_UV, col_transparent, ZERO_DATA));
            }
        }
    }

    let n = n as u32;
    for i in 1..n - 1 {
        output.indices.extend_from_slice(&[base, base + i, base + i + 1]);
    }

    for i in 0..n {
        let next = (i + 1) % n;
        let inner_i = base + i;
        let inner_next = base + next;
        let outer_i = base + n + i;
        let outer_next = base + n + next;
        output.indices.extend_from_slice(&[
            inner_i, inner_next, outer_next,
            inner_i, outer_next, outer_i,
        ]);
    }
}

pub fn circle_points(center: Point, radius: f32, segments: usize) -> Vec<Point> {
    let mut points = Vec::with_capacity(segments);
    for i in 0..segments {
        let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
        points.push(Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    points
}

pub fn arc_points(
    center: Point,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    segments: usize,
) -> Vec<Point> {
    let mut points = Vec::with_capacity(segments + 1);
    let sweep = end_angle - start_angle;
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let angle = start_angle + sweep * t;
        points.push(Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    points
}

pub fn circle_segment_count(radius: f32) -> usize {
    let count = (std::f32::consts::TAU * radius * 0.5) as usize;
    count.clamp(32, 512)
}

pub fn flatten_quad_bezier(
    p0: Point,
    p1: Point,
    p2: Point,
    tolerance: f32,
) -> Vec<Point> {
    let mut points = vec![p0];
    flatten_quad_recursive(p0, p1, p2, tolerance * tolerance, &mut points);
    points
}

fn flatten_quad_recursive(
    p0: Point,
    p1: Point,
    p2: Point,
    tol_sq: f32,
    output: &mut Vec<Point>,
) {
    let mid = Bezier::quad(p0, p1, p2, 0.5);
    let chord_mid = Point::new((p0.x + p2.x) * 0.5, (p0.y + p2.y) * 0.5);
    let dx = mid.x - chord_mid.x;
    let dy = mid.y - chord_mid.y;
    let dist_sq = dx * dx + dy * dy;

    if dist_sq <= tol_sq {
        output.push(p2);
    } else {
        let p01 = Point::new((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
        let p12 = Point::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
        let p012 = Point::new((p01.x + p12.x) * 0.5, (p01.y + p12.y) * 0.5);

        flatten_quad_recursive(p0, p01, p012, tol_sq, output);
        flatten_quad_recursive(p012, p12, p2, tol_sq, output);
    }
}

pub fn flatten_cubic_bezier(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    tolerance: f32,
) -> Vec<Point> {
    let mut points = vec![p0];
    flatten_cubic_recursive(p0, p1, p2, p3, tolerance * tolerance, 0, &mut points);
    points
}

fn flatten_cubic_recursive(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    tol_sq: f32,
    depth: u32,
    output: &mut Vec<Point>,
) {
    if depth > 10 {
        output.push(p3);
        return;
    }

    let dx = p3.x - p0.x;
    let dy = p3.y - p0.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 0.0001 {
        output.push(p3);
        return;
    }

    let inv_len = 1.0 / len_sq.sqrt();
    let d1 = ((p1.x - p0.x) * dy - (p1.y - p0.y) * dx).abs() * inv_len;
    let d2 = ((p2.x - p0.x) * dy - (p2.y - p0.y) * dx).abs() * inv_len;
    let max_d = d1.max(d2);

    if max_d * max_d <= tol_sq {
        output.push(p3);
    } else {
        let p01 = midpoint(p0, p1);
        let p12 = midpoint(p1, p2);
        let p23 = midpoint(p2, p3);
        let p012 = midpoint(p01, p12);
        let p123 = midpoint(p12, p23);
        let p0123 = midpoint(p012, p123);

        flatten_cubic_recursive(p0, p01, p012, p0123, tol_sq, depth + 1, output);
        flatten_cubic_recursive(p0123, p123, p23, p3, tol_sq, depth + 1, output);
    }
}

fn midpoint(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    fn default_paint() -> Paint {
        Paint::default()
    }

    #[test]
    fn tess_output_new_is_empty() {
        let o = TessOutput::new();
        assert!(o.is_empty());
        assert!(o.vertices.is_empty());
        assert!(o.indices.is_empty());
    }

    #[test]
    fn tess_output_clear() {
        let mut o = TessOutput::new();
        o.vertices.push(Vertex::new([0.0, 0.0], [0.0, 0.0], [1.0; 4], [0.0; 4]));
        o.indices.push(0);
        o.clear();
        assert!(o.is_empty());
    }

    #[test]
    fn tess_output_merge() {
        let mut a = TessOutput::new();
        a.vertices.push(Vertex::new([0.0, 0.0], [0.0, 0.0], [1.0; 4], [0.0; 4]));
        a.vertices.push(Vertex::new([1.0, 0.0], [0.0, 0.0], [1.0; 4], [0.0; 4]));
        a.indices.extend_from_slice(&[0, 1]);

        let mut b = TessOutput::new();
        b.vertices.push(Vertex::new([2.0, 0.0], [0.0, 0.0], [1.0; 4], [0.0; 4]));
        b.indices.push(0);

        a.merge(&b);
        assert_eq!(a.vertices.len(), 3);
        assert_eq!(a.indices.len(), 3);
        assert_eq!(a.indices[2], 2);
    }

    #[test]
    fn tess_output_merge_empty() {
        let mut a = TessOutput::new();
        let b = TessOutput::new();
        a.merge(&b);
        assert!(a.is_empty());
    }

    #[test]
    fn concave_fill_excludes_notch() {
        let dart = [
            Point::new(0.0, 0.0),
            Point::new(4.0, 2.0),
            Point::new(0.0, 4.0),
            Point::new(1.0, 2.0),
        ];
        let mut out = TessOutput::new();
        tessellate_fill_polygon_concave(&dart, Color::WHITE, &mut out);

        let covered = |px: f32, py: f32| -> bool {
            out.indices.chunks_exact(3).any(|t| {
                let a = out.vertices[t[0] as usize].position;
                let b = out.vertices[t[1] as usize].position;
                let c = out.vertices[t[2] as usize].position;
                point_in_triangle(
                    Point::new(px, py),
                    Point::new(a[0], a[1]),
                    Point::new(b[0], b[1]),
                    Point::new(c[0], c[1]),
                )
            })
        };

        assert!(covered(2.0, 2.0), "центр дротика должен быть залит");
        assert!(
            !covered(0.5, 2.0),
            "выемка не должна заливаться — это ear-clipping, а не веер"
        );
    }

    #[test]
    fn line_segment_produces_8_verts_18_indices() {
        let mut out = TessOutput::new();
        tessellate_line_segment(p(0.0, 0.0), p(10.0, 0.0), &default_paint(), &mut out);
        assert_eq!(out.vertices.len(), 8);
        assert_eq!(out.indices.len(), 18);
    }

    #[test]
    fn line_segment_degenerate_skipped() {
        let mut out = TessOutput::new();
        tessellate_line_segment(p(5.0, 5.0), p(5.0, 5.0), &default_paint(), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn line_segment_vertical() {
        let mut out = TessOutput::new();
        tessellate_line_segment(p(0.0, 0.0), p(0.0, 100.0), &default_paint(), &mut out);
        assert_eq!(out.vertices.len(), 8);
    }

    #[test]
    fn line_segment_wider_stroke() {
        let mut paint = default_paint();
        paint.stroke_width = 10.0;
        let mut out = TessOutput::new();
        tessellate_line_segment(p(0.0, 0.0), p(100.0, 0.0), &paint, &mut out);
        let min_y = out.vertices.iter().map(|v| v.position[1]).fold(f32::MAX, f32::min);
        let max_y = out.vertices.iter().map(|v| v.position[1]).fold(f32::MIN, f32::max);
        let span = max_y - min_y;
        assert!(span > 11.0 && span < 13.0);
    }

    #[test]
    fn polyline_less_than_2_noop() {
        let mut out = TessOutput::new();
        tessellate_polyline(&[p(0.0, 0.0)], &default_paint(), false, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn polyline_open_2_points() {
        let mut out = TessOutput::new();
        tessellate_polyline(&[p(0.0, 0.0), p(10.0, 0.0)], &default_paint(), false, &mut out);
        assert_eq!(out.vertices.len(), 8);
        assert_eq!(out.indices.len(), 18);
    }

    #[test]
    fn polyline_closed_triangle() {
        let pts = [p(0.0, 0.0), p(10.0, 0.0), p(5.0, 10.0)];
        let mut out = TessOutput::new();
        tessellate_polyline(&pts, &default_paint(), true, &mut out);
        assert_eq!(out.vertices.len(), 12);
        assert_eq!(out.indices.len(), 54);
    }

    #[test]
    fn polyline_open_3_points() {
        let pts = [p(0.0, 0.0), p(50.0, 0.0), p(100.0, 50.0)];
        let mut out = TessOutput::new();
        tessellate_polyline(&pts, &default_paint(), false, &mut out);
        assert_eq!(out.vertices.len(), 12);
        assert_eq!(out.indices.len(), 36);
    }

    #[test]
    fn fill_polygon_less_than_3_noop() {
        let mut out = TessOutput::new();
        tessellate_fill_polygon(&[p(0.0, 0.0), p(1.0, 1.0)], Color::RED, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn fill_polygon_triangle() {
        let pts = [p(0.0, 0.0), p(10.0, 0.0), p(5.0, 10.0)];
        let mut out = TessOutput::new();
        tessellate_fill_polygon(&pts, Color::RED, &mut out);
        assert_eq!(out.vertices.len(), 3);
        assert_eq!(out.indices.len(), 3);
        assert_eq!(out.indices, &[0, 1, 2]);
    }

    #[test]
    fn fill_polygon_quad() {
        let pts = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)];
        let mut out = TessOutput::new();
        tessellate_fill_polygon(&pts, Color::WHITE, &mut out);
        assert_eq!(out.vertices.len(), 4);
        assert_eq!(out.indices.len(), 6);
    }

    #[test]
    fn fill_polygon_color_applied() {
        let pts = [p(0.0, 0.0), p(10.0, 0.0), p(5.0, 10.0)];
        let mut out = TessOutput::new();
        tessellate_fill_polygon(&pts, Color::GREEN, &mut out);
        let expected = Color::GREEN.to_array();
        for v in &out.vertices {
            assert_eq!(v.color, expected);
        }
    }

    #[test]
    fn fill_polygon_aa_less_than_3_noop() {
        let mut out = TessOutput::new();
        tessellate_fill_polygon_aa(&[p(0.0, 0.0)], Color::RED, 1.0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn fill_polygon_aa_triangle() {
        let pts = [p(0.0, 0.0), p(100.0, 0.0), p(50.0, 100.0)];
        let mut out = TessOutput::new();
        tessellate_fill_polygon_aa(&pts, Color::RED, 1.0, &mut out);
        assert_eq!(out.vertices.len(), 6);
        assert_eq!(out.indices.len(), 21);
    }

    #[test]
    fn fill_polygon_aa_outer_has_zero_alpha() {
        let pts = [p(0.0, 0.0), p(100.0, 0.0), p(50.0, 100.0)];
        let mut out = TessOutput::new();
        tessellate_fill_polygon_aa(&pts, Color::RED, 1.0, &mut out);
        for i in 3..6 {
            assert_eq!(out.vertices[i].color[3], 0.0, "outer vertex {} should have alpha=0", i);
        }
    }

    #[test]
    fn circle_points_count() {
        let pts = circle_points(p(0.0, 0.0), 10.0, 16);
        assert_eq!(pts.len(), 16);
    }

    #[test]
    fn circle_points_on_radius() {
        let center = p(50.0, 50.0);
        let r = 25.0;
        let pts = circle_points(center, r, 32);
        for pt in &pts {
            let dx = pt.x - center.x;
            let dy = pt.y - center.y;
            let dist = (dx * dx + dy * dy).sqrt();
            assert!((dist - r).abs() < 1e-4, "point should be on circle, dist={}", dist);
        }
    }

    #[test]
    fn circle_points_first_at_zero_angle() {
        let pts = circle_points(p(0.0, 0.0), 10.0, 8);
        assert!((pts[0].x - 10.0).abs() < 1e-4);
        assert!(pts[0].y.abs() < 1e-4);
    }

    #[test]
    fn arc_points_count() {
        let pts = arc_points(p(0.0, 0.0), 10.0, 0.0, std::f32::consts::PI, 8);
        assert_eq!(pts.len(), 9);
    }

    #[test]
    fn arc_points_on_radius() {
        let center = p(0.0, 0.0);
        let r = 20.0;
        let pts = arc_points(center, r, 0.0, std::f32::consts::FRAC_PI_2, 10);
        for pt in &pts {
            let dist = (pt.x * pt.x + pt.y * pt.y).sqrt();
            assert!((dist - r).abs() < 1e-4);
        }
    }

    #[test]
    fn arc_points_half_circle() {
        let pts = arc_points(p(0.0, 0.0), 10.0, 0.0, std::f32::consts::PI, 10);
        assert!((pts[0].x - 10.0).abs() < 1e-4);
        assert!((pts[10].x + 10.0).abs() < 1e-3);
    }

    #[test]
    fn circle_segment_count_minimum() {
        assert_eq!(circle_segment_count(1.0), 32);
    }

    #[test]
    fn circle_segment_count_maximum() {
        assert_eq!(circle_segment_count(10000.0), 512);
    }

    #[test]
    fn circle_segment_count_scales_with_radius() {
        let small = circle_segment_count(10.0);
        let big = circle_segment_count(100.0);
        assert!(big >= small);
    }

    #[test]
    fn circle_segment_count_midrange_follows_formula() {
        let n = circle_segment_count(50.0);
        assert!(
            (150..=170).contains(&n),
            "expected ~157 from TAU*r*0.5, got {n}"
        );
    }

    #[test]
    fn flatten_quad_starts_and_ends_correctly() {
        let pts = flatten_quad_bezier(p(0.0, 0.0), p(50.0, 100.0), p(100.0, 0.0), 0.5);
        assert!((pts[0].x).abs() < 1e-5);
        assert!((pts[0].y).abs() < 1e-5);
        let last = pts.last().unwrap();
        assert!((last.x - 100.0).abs() < 1e-5);
        assert!((last.y).abs() < 1e-5);
    }

    #[test]
    fn flatten_quad_straight_line_few_points() {
        let pts = flatten_quad_bezier(p(0.0, 0.0), p(50.0, 50.0), p(100.0, 100.0), 0.5);
        assert!(pts.len() <= 3, "straight quad should flatten to few points, got {}", pts.len());
    }

    #[test]
    fn flatten_quad_curved_more_points() {
        let pts = flatten_quad_bezier(p(0.0, 0.0), p(50.0, 200.0), p(100.0, 0.0), 0.5);
        assert!(pts.len() > 3, "curved quad should produce more points");
    }

    #[test]
    fn flatten_cubic_starts_and_ends_correctly() {
        let pts = flatten_cubic_bezier(
            p(0.0, 0.0), p(30.0, 100.0), p(70.0, 100.0), p(100.0, 0.0), 0.5,
        );
        assert!((pts[0].x).abs() < 1e-5);
        let last = pts.last().unwrap();
        assert!((last.x - 100.0).abs() < 1e-5);
        assert!((last.y).abs() < 1e-5);
    }

    #[test]
    fn flatten_cubic_straight_line_few_points() {
        let pts = flatten_cubic_bezier(
            p(0.0, 0.0), p(33.0, 33.0), p(66.0, 66.0), p(100.0, 100.0), 0.5,
        );
        assert!(pts.len() <= 4);
    }

    #[test]
    fn flatten_cubic_s_curve() {
        let pts = flatten_cubic_bezier(
            p(0.0, 0.0), p(0.0, 100.0), p(100.0, -100.0), p(100.0, 0.0), 0.5,
        );
        assert!(pts.len() > 4, "S-curve should produce many points, got {}", pts.len());
    }

    #[test]
    fn flatten_cubic_degenerate_point() {
        let pts = flatten_cubic_bezier(
            p(5.0, 5.0), p(5.0, 5.0), p(5.0, 5.0), p(5.0, 5.0), 0.5,
        );
        assert!(pts.len() >= 2);
    }

    #[test]
    fn midpoint_basic() {
        let m = midpoint(p(0.0, 0.0), p(10.0, 20.0));
        assert_eq!(m.x, 5.0);
        assert_eq!(m.y, 10.0);
    }

    #[test]
    fn midpoint_same_point() {
        let m = midpoint(p(5.0, 5.0), p(5.0, 5.0));
        assert_eq!(m.x, 5.0);
        assert_eq!(m.y, 5.0);
    }
}
