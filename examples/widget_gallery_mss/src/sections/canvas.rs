use syngui::prelude::*;

use super::{label, section_card, section_title};

pub fn build_canvas_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(24.0)
            .child(section_title("Canvas"))
            // Animated line chart
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Animated Line Chart"))
                    .child(
                        DecoratedBox::new()
                            .child(
                                Canvas::new(draw_line_chart)
                                    .size(700.0, 320.0)
                                    .animated(true)
                                    .background(Color::from_hex("#0F172A")),
                            )
                            .class("canvas-card"),
                    ),
            )
            // Shapes demo
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Drawing Primitives"))
                    .child(
                        DecoratedBox::new()
                            .child(
                                Canvas::new(draw_shapes_demo)
                                    .size(700.0, 280.0)
                                    .background(Color::from_hex("#F8FAFC")),
                            )
                            .class("canvas-card"),
                    ),
            )
            // Bezier curves demo
            .child(
                Column::new().gap(8.0).child(label("Bezier Curves")).child(
                    DecoratedBox::new()
                        .child(
                            Canvas::new(draw_bezier_demo)
                                .size(700.0, 200.0)
                                .animated(true)
                                .background(Color::from_hex("#FFFBEB")),
                        )
                        .class("canvas-card"),
                ),
            ),
    )
}

fn draw_line_chart(ctx: &mut CanvasContext, t: f32) {
    let w = ctx.width();
    let h = ctx.height();
    let pad = 50.0;
    let chart_w = w - pad * 2.0;
    let chart_h = h - pad * 2.0;

    // Grid lines
    ctx.set_color(Color::from_hex("#1E293B"));
    ctx.set_stroke_width(1.0);
    for i in 0..=4 {
        let y = pad + chart_h * i as f32 / 4.0;
        ctx.draw_line(pad, y, pad + chart_w, y);
    }
    for i in 0..=12 {
        let x = pad + chart_w * i as f32 / 12.0;
        ctx.draw_line(x, pad, x, pad + chart_h);
    }

    // Axes (brighter)
    ctx.set_color(Color::from_hex("#334155"));
    ctx.set_stroke_width(1.5);
    ctx.draw_line(pad, pad, pad, pad + chart_h);
    ctx.draw_line(pad, pad + chart_h, pad + chart_w, pad + chart_h);

    // Animate: grow factor 0→1 over first 1.5 seconds
    let grow = (t / 1.5).min(1.0);

    // Data series
    let series: &[(Color, &[f32])] = &[
        (
            Color::from_hex("#38BDF8"),
            &[
                0.2, 0.35, 0.3, 0.5, 0.45, 0.6, 0.55, 0.7, 0.65, 0.8, 0.75, 0.9,
            ],
        ),
        (
            Color::from_hex("#34D399"),
            &[
                0.1, 0.15, 0.25, 0.2, 0.35, 0.3, 0.45, 0.5, 0.55, 0.6, 0.7, 0.65,
            ],
        ),
        (
            Color::from_hex("#F472B6"),
            &[
                0.5, 0.45, 0.4, 0.35, 0.3, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55,
            ],
        ),
    ];

    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    for &(color, data) in series {
        // Build points with oscillation
        let points: Vec<(f32, f32)> = data
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let x = pad + chart_w * i as f32 / 11.0;
                let osc = (t * 2.0 + i as f32 * 0.5).sin() * 0.02;
                let val = (v + osc) * grow;
                let y = pad + chart_h * (1.0 - val);
                (x, y)
            })
            .collect();

        // Fill area under the line
        ctx.set_color(color.with_alpha(0.1));
        let mut fill_pts = points.clone();
        fill_pts.push((pad + chart_w, pad + chart_h));
        fill_pts.push((pad, pad + chart_h));
        ctx.fill_polygon(&fill_pts);

        // Draw the line
        ctx.set_color(color);
        ctx.set_stroke_width(2.5);
        ctx.draw_polyline(&points);

        // Data points
        for &(x, y) in &points {
            ctx.fill_circle(x, y, 3.5);
        }
    }

    // Month labels
    ctx.set_color(Color::from_hex("#64748B"));
    for (i, month) in months.iter().enumerate() {
        let x = pad + chart_w * i as f32 / 11.0;
        let _ = month;
        ctx.fill_circle(x, pad + chart_h + 8.0, 2.0);
    }

    // Y-axis scale markers
    for i in 0..=4 {
        let y = pad + chart_h * i as f32 / 4.0;
        ctx.fill_circle(pad - 8.0, y, 2.0);
    }
}

fn draw_shapes_demo(ctx: &mut CanvasContext, _t: f32) {
    let section_w = 160.0;
    let y_center = 130.0;

    // 1. Lines with different widths
    let x0 = 30.0;
    ctx.set_color(Color::from_hex("#3B82F6"));
    for i in 0..5 {
        let width = 1.0 + i as f32 * 1.5;
        ctx.set_stroke_width(width);
        let y = 40.0 + i as f32 * 30.0;
        ctx.draw_line(x0, y, x0 + 100.0, y);
    }

    ctx.set_color(Color::from_hex("#1E40AF"));
    ctx.fill_circle(x0 + 50.0, 20.0, 4.0);

    // 2. Rectangles
    let x1 = x0 + section_w;
    ctx.set_color(Color::from_hex("#8B5CF6"));
    ctx.set_stroke_width(2.0);
    ctx.draw_rect(x1, 40.0, 80.0, 50.0);

    ctx.set_color(Color::from_hex("#A78BFA"));
    ctx.fill_rect(x1 + 10.0, 110.0, 60.0, 40.0);

    ctx.set_color(Color::from_hex("#7C3AED"));
    ctx.fill_rounded_rect(x1, 170.0, 80.0, 40.0, 10.0);

    ctx.set_color(Color::from_hex("#5B21B6"));
    ctx.fill_circle(x1 + 40.0, 20.0, 4.0);

    // 3. Circles
    let x2 = x1 + section_w;
    ctx.set_color(Color::from_hex("#EC4899"));
    ctx.fill_circle(x2 + 40.0, 70.0, 25.0);

    ctx.set_color(Color::from_hex("#F472B6"));
    ctx.set_stroke_width(2.5);
    ctx.stroke_circle(x2 + 40.0, y_center, 30.0);

    ctx.set_color(Color::from_hex("#DB2777"));
    ctx.fill_circle(x2 + 40.0, 200.0, 15.0);

    ctx.set_color(Color::from_hex("#9D174D"));
    ctx.fill_circle(x2 + 40.0, 20.0, 4.0);

    // 4. Arcs
    let x3 = x2 + section_w;
    let pi = std::f32::consts::PI;

    ctx.set_color(Color::from_hex("#F59E0B"));
    ctx.set_stroke_width(3.0);
    ctx.draw_arc(x3 + 40.0, 80.0, 30.0, 0.0, pi * 1.5);

    ctx.set_color(Color::from_hex("#D97706"));
    ctx.set_stroke_width(2.0);
    ctx.draw_arc(x3 + 40.0, 160.0, 25.0, -pi * 0.25, pi * 1.0);

    ctx.set_color(Color::from_hex("#B45309"));
    ctx.set_stroke_width(4.0);
    ctx.draw_arc(x3 + 40.0, 230.0, 20.0, 0.0, pi * 2.0);

    ctx.set_color(Color::from_hex("#92400E"));
    ctx.fill_circle(x3 + 40.0, 20.0, 4.0);
}

fn draw_bezier_demo(ctx: &mut CanvasContext, t: f32) {
    let w = ctx.width();
    let h = ctx.height();

    // Animated control points
    let osc1 = (t * 1.5).sin() * 40.0;
    let osc2 = (t * 1.2 + 1.0).cos() * 30.0;

    // Cubic bezier 1
    ctx.set_color(Color::from_hex("#7C3AED"));
    ctx.set_stroke_width(3.0);
    ctx.draw_cubic_bezier(
        50.0,
        h * 0.5,
        w * 0.25,
        30.0 + osc1,
        w * 0.5,
        h - 30.0 + osc2,
        w * 0.75,
        h * 0.5,
    );

    // Cubic bezier 2
    ctx.set_color(Color::from_hex("#0891B2"));
    ctx.set_stroke_width(2.5);
    ctx.draw_cubic_bezier(
        w * 0.25,
        h * 0.5,
        w * 0.4,
        20.0 - osc2,
        w * 0.6,
        h - 20.0 - osc1,
        w - 50.0,
        h * 0.5,
    );

    // Quadratic bezier
    ctx.set_color(Color::from_hex("#DC2626"));
    ctx.set_stroke_width(2.0);
    ctx.draw_quad_bezier(
        80.0,
        h - 40.0,
        w * 0.5,
        20.0 + osc1 * 0.5,
        w - 80.0,
        h - 40.0,
    );

    // Control point markers
    ctx.set_color(Color::from_hex("#7C3AED").with_alpha(0.5));
    ctx.fill_circle(w * 0.25, 30.0 + osc1, 4.0);
    ctx.fill_circle(w * 0.5, h - 30.0 + osc2, 4.0);

    ctx.set_color(Color::from_hex("#0891B2").with_alpha(0.5));
    ctx.fill_circle(w * 0.4, 20.0 - osc2, 4.0);
    ctx.fill_circle(w * 0.6, h - 20.0 - osc1, 4.0);
}
