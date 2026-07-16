use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::render::DisplayList;

use crate::widgets::charts::math::{catmull_rom_to_bezier, segment_dashed};
use crate::widgets::charts::types::{AreaFill, DataPoint, LineStyle, PointShape, SeriesStyle, VisualMapPiece};

pub fn render_line_gpu(
    list: &mut DisplayList,
    screen_points: &[(f32, f32)],
    style: &SeriesStyle,
    color: Color,
    appear_progress: f32,
    plot_origin: Point,
    smooth: bool,
) {
    if screen_points.len() < 2 {
        return;
    }

    let clipped = clip_points_by_progress(screen_points, appear_progress);
    if clipped.len() < 2 {
        return;
    }

    let to_abs = |pts: &[(f32, f32)]| -> Vec<[f32; 2]> {
        pts.iter()
            .map(|&(x, y)| [plot_origin.x + x, plot_origin.y + y])
            .collect()
    };

    if smooth && clipped.len() >= 3 && clipped.len() < 80 {
        let bezier_segments = catmull_rom_to_bezier(&clipped, 0.0);
        let flat = flatten_bezier_segments(&bezier_segments);
        let abs_pts = to_abs(&flat);
        list.push_line_strip(abs_pts, color, style.line_width);
    } else {
        match &style.line_style {
            LineStyle::Solid => {
                let abs_pts = to_abs(&clipped);
                list.push_line_strip(abs_pts, color, style.line_width);
            }
            LineStyle::Dashed { dash, gap } => {
                let segments = segment_dashed(&clipped, *dash, *gap);
                for seg in &segments {
                    let abs_pts = to_abs(seg);
                    list.push_line_strip(abs_pts, color, style.line_width);
                }
            }
            LineStyle::Dotted => {
                let segments = segment_dashed(&clipped, 2.0, 4.0);
                for seg in &segments {
                    let abs_pts = to_abs(seg);
                    list.push_line_strip(abs_pts, color, style.line_width);
                }
            }
        }
    }
}

pub fn render_line(
    ctx: &mut CanvasContext,
    screen_points: &[(f32, f32)],
    data_points: &[DataPoint],
    style: &SeriesStyle,
    color: Color,
    appear_progress: f32,
) {
    if screen_points.len() < 2 {
        return;
    }

    let points = clip_points_by_progress(screen_points, appear_progress);
    if points.len() < 2 {
        return;
    }

    if let Some(ref visual_map) = style.visual_map {
        render_line_visual_mapped(ctx, &points, data_points, style, visual_map, color);
        return;
    }

    ctx.save();
    ctx.set_color(color);
    ctx.set_stroke_width(style.line_width);

    if style.smooth && points.len() >= 3 {
        render_smooth_line(ctx, &points, &style.line_style);
    } else {
        render_straight_line(ctx, &points, &style.line_style);
    }

    ctx.restore();
}

fn render_line_visual_mapped(
    ctx: &mut CanvasContext,
    screen_points: &[(f32, f32)],
    data_points: &[DataPoint],
    style: &SeriesStyle,
    visual_map: &[VisualMapPiece],
    fallback_color: Color,
) {
    if screen_points.len() < 2 || data_points.len() < 2 {
        return;
    }

    let mut thresholds: Vec<f64> = Vec::new();
    for piece in visual_map {
        if piece.gt.is_finite() {
            thresholds.push(piece.gt);
        }
        if piece.lte.is_finite() {
            thresholds.push(piece.lte);
        }
    }
    thresholds.sort_by(|a, b| a.partial_cmp(b).unwrap());
    thresholds.dedup();

    let color_for_y = |y: f64| -> Color {
        for piece in visual_map {
            if piece.contains(y) {
                return piece.color;
            }
        }
        fallback_color
    };

    let mut points_with_y: Vec<((f32, f32), f64)> = Vec::new();
    let n = screen_points.len().min(data_points.len());

    for i in 0..n {
        if i > 0 {
            let (sx0, sy0) = screen_points[i - 1];
            let (sx1, sy1) = screen_points[i];
            let dy0 = data_points[i - 1].y;
            let dy1 = data_points[i].y;

            let y_lo = dy0.min(dy1);
            let y_hi = dy0.max(dy1);

            let mut crossings: Vec<f64> = Vec::new();
            for &th in &thresholds {
                if th > y_lo && th < y_hi {
                    crossings.push(th);
                }
            }
            crossings.sort_by(|a, b| {
                let ta = (a - dy0) / (dy1 - dy0);
                let tb = (b - dy0) / (dy1 - dy0);
                ta.partial_cmp(&tb).unwrap()
            });

            for th in crossings {
                let t = (th - dy0) / (dy1 - dy0);
                let cx = sx0 + (sx1 - sx0) * t as f32;
                let cy = sy0 + (sy1 - sy0) * t as f32;
                points_with_y.push(((cx, cy), th));
            }
        }
        points_with_y.push((screen_points[i], data_points[i].y));
    }

    ctx.save();
    ctx.set_stroke_width(style.line_width);

    if points_with_y.len() < 2 {
        ctx.restore();
        return;
    }

    let first_mid = (points_with_y[0].1 + points_with_y[1].1) * 0.5;
    let mut current_color = color_for_y(first_mid);
    let mut seg: Vec<(f32, f32)> = vec![points_with_y[0].0];

    for i in 1..points_with_y.len() {
        let (pt, y_val) = points_with_y[i];
        let mid_y = (points_with_y[i - 1].1 + y_val) * 0.5;
        let seg_color = color_for_y(mid_y);

        if seg_color != current_color {
            if seg.len() >= 2 {
                ctx.set_color(current_color);
                render_straight_line(ctx, &seg, &style.line_style);
            }
            let last_pt = *seg.last().unwrap();
            seg.clear();
            seg.push(last_pt);
            seg.push(pt);
            current_color = seg_color;
        } else {
            seg.push(pt);
        }
    }

    if seg.len() >= 2 {
        ctx.set_color(current_color);
        render_straight_line(ctx, &seg, &style.line_style);
    }

    ctx.restore();
}

pub fn render_visual_map_legend(
    list: &mut DisplayList,
    visual_map: &[VisualMapPiece],
    plot_rect: &Rect,
    font_size: f32,
    text_color: Color,
) {
    if visual_map.is_empty() {
        return;
    }

    let swatch_size = font_size;
    let line_height = swatch_size + 6.0;
    let total_h = visual_map.len() as f32 * line_height;

    let legend_x = plot_rect.origin.x + plot_rect.size.width + 12.0;
    let legend_y = plot_rect.origin.y + (plot_rect.size.height - total_h).max(0.0) * 0.5;

    let mut y = legend_y;

    for piece in visual_map.iter().rev() {
        let label = if piece.gt.is_finite() && piece.lte.is_finite() {
            format!("{:.0} - {:.0}", piece.gt, piece.lte)
        } else if piece.lte.is_finite() {
            format!("0 - {:.0}", piece.lte)
        } else if piece.gt.is_finite() {
            format!("> {:.0}", piece.gt)
        } else {
            String::new()
        };

        let label_rect = Rect::new(
            Point::new(legend_x, y),
            Size::new(64.0, swatch_size + 2.0),
        );
        list.push_text_aligned(
            &label,
            label_rect,
            text_color,
            font_size,
            crate::mss::TextAlign::RIGHT,
            crate::mss::TextDecoration::None,
            400,
        );

        let swatch_rect = Rect::new(
            Point::new(legend_x + 68.0, y + 1.0),
            Size::new(swatch_size, swatch_size),
        );
        list.push_rect(swatch_rect, piece.color, [2.0; 4]);

        y += line_height;
    }
}

pub fn render_area(
    ctx: &mut CanvasContext,
    screen_points: &[(f32, f32)],
    baseline_y: f32,
    color: Color,
    fill: &AreaFill,
    appear_progress: f32,
    _smooth: bool,
) {
    if screen_points.len() < 2 {
        return;
    }

    let points = clip_points_by_progress(screen_points, appear_progress);
    if points.len() < 2 {
        return;
    }

    ctx.save();
    ctx.set_color(color.with_alpha(fill.opacity));
    ctx.fill_area_strip(&points, baseline_y);
    ctx.restore();
}

pub fn render_points(
    list: &mut DisplayList,
    screen_points: &[(f32, f32)],
    style: &SeriesStyle,
    color: Color,
    hover_point_idx: Option<usize>,
    hover_t: f32,
    appear_progress: f32,
    origin: Point,
) {
    if !style.show_points || screen_points.is_empty() {
        return;
    }

    let base_size = style.point_size;
    let hover_size = base_size * 1.8;

    for (i, &(px, py)) in screen_points.iter().enumerate() {
        if !screen_points.is_empty() {
            let first_x = screen_points[0].0;
            let last_x = screen_points.last().unwrap().0;
            let range = last_x - first_x;
            if range > 0.0 {
                let point_t = (px - first_x) / range;
                if point_t > appear_progress {
                    continue;
                }
            }
        }

        let is_hovered = hover_point_idx == Some(i);
        let size = if is_hovered {
            base_size + (hover_size - base_size) * hover_t
        } else {
            base_size
        };

        let abs_x = origin.x + px;
        let abs_y = origin.y + py;

        match style.point_shape {
            PointShape::Circle => {
                let rect = Rect::new(
                    Point::new(abs_x - size, abs_y - size),
                    Size::new(size * 2.0, size * 2.0),
                );
                list.push_rect(rect, color, [size; 4]);
            }
            PointShape::Square => {
                let rect = Rect::new(
                    Point::new(abs_x - size, abs_y - size),
                    Size::new(size * 2.0, size * 2.0),
                );
                list.push_rect(rect, color, [0.0; 4]);
            }
            PointShape::Diamond => {
                let rect = Rect::new(
                    Point::new(abs_x - size, abs_y - size),
                    Size::new(size * 2.0, size * 2.0),
                );
                list.push_rect(rect, color, [size * 0.3; 4]);
            }
            PointShape::Triangle => {
                let rect = Rect::new(
                    Point::new(abs_x - size, abs_y - size * 0.6),
                    Size::new(size * 2.0, size * 1.6),
                );
                list.push_rect(rect, color, [size * 0.2; 4]);
            }
        }
    }
}

fn render_straight_line(ctx: &mut CanvasContext, points: &[(f32, f32)], line_style: &LineStyle) {
    match line_style {
        LineStyle::Solid => {
            ctx.draw_polyline(points);
        }
        LineStyle::Dashed { dash, gap } => {
            let segments = segment_dashed(points, *dash, *gap);
            for seg in &segments {
                ctx.draw_polyline(seg);
            }
        }
        LineStyle::Dotted => {
            let segments = segment_dashed(points, 2.0, 4.0);
            for seg in &segments {
                ctx.draw_polyline(seg);
            }
        }
    }
}

fn render_smooth_line(ctx: &mut CanvasContext, points: &[(f32, f32)], line_style: &LineStyle) {
    let bezier_segments = catmull_rom_to_bezier(points, 0.0);

    match line_style {
        LineStyle::Solid => {
            for (p0, cp1, cp2, p1) in &bezier_segments {
                ctx.draw_cubic_bezier(p0.0, p0.1, cp1.0, cp1.1, cp2.0, cp2.1, p1.0, p1.1);
            }
        }
        LineStyle::Dashed { dash, gap } => {
            let flat = flatten_bezier_segments(&bezier_segments);
            let segments = segment_dashed(&flat, *dash, *gap);
            for seg in &segments {
                ctx.draw_polyline(seg);
            }
        }
        LineStyle::Dotted => {
            let flat = flatten_bezier_segments(&bezier_segments);
            let segments = segment_dashed(&flat, 2.0, 4.0);
            for seg in &segments {
                ctx.draw_polyline(seg);
            }
        }
    }
}

fn flatten_bezier_segments(
    segments: &[((f32, f32), (f32, f32), (f32, f32), (f32, f32))],
) -> Vec<(f32, f32)> {
    let mut result: Vec<(f32, f32)> = Vec::new();

    for (p0, cp1, cp2, p1) in segments {
        let dx = (p1.0 - p0.0).abs();
        let steps = ((dx / 4.0) as usize).clamp(2, 16);
        if result.is_empty() {
            result.push(*p0);
        }
        for j in 1..=steps {
            let t = j as f32 / steps as f32;
            let it = 1.0 - t;
            let x = it * it * it * p0.0
                + 3.0 * it * it * t * cp1.0
                + 3.0 * it * t * t * cp2.0
                + t * t * t * p1.0;
            let y = it * it * it * p0.1
                + 3.0 * it * it * t * cp1.1
                + 3.0 * it * t * t * cp2.1
                + t * t * t * p1.1;
            result.push((x, y));
        }
    }

    result
}

fn clip_points_by_progress(points: &[(f32, f32)], progress: f32) -> Vec<(f32, f32)> {
    if progress >= 1.0 || points.len() < 2 {
        return points.to_vec();
    }
    if progress <= 0.0 {
        return Vec::new();
    }

    let x_min = points[0].0;
    let x_max = points.last().unwrap().0;
    let x_range = x_max - x_min;
    if x_range <= 0.0 {
        return points.to_vec();
    }

    let x_cutoff = x_min + x_range * progress;
    let mut result: Vec<(f32, f32)> = Vec::new();

    for i in 0..points.len() {
        if points[i].0 <= x_cutoff {
            result.push(points[i]);
        } else {
            if i > 0 {
                let (x0, y0) = points[i - 1];
                let (x1, y1) = points[i];
                let dx = x1 - x0;
                if dx > 0.0 {
                    let t = (x_cutoff - x0) / dx;
                    let y = y0 + t * (y1 - y0);
                    result.push((x_cutoff, y));
                }
            }
            break;
        }
    }

    result
}
