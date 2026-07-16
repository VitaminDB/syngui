use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::render::DisplayList;
use crate::widget::context::TextMeasure;

use std::sync::Arc;

use crate::widgets::charts::math::{compute_ticks, format_tick_value, LinearScale};
use crate::widgets::charts::types::{AxisConfig, ChartLayout};

use super::estimate_text_width;

pub(crate) struct AxisColors {
    pub grid_color: Color,
    pub axis_color: Color,
    pub label_color: Color,
    pub title_color: Color,
    pub axis_font_size: f32,
    pub title_font_size: f32,
}

impl Default for AxisColors {
    fn default() -> Self {
        Self {
            grid_color: Color::from_hex("#e2e8f0"),
            axis_color: Color::from_hex("#94a3b8"),
            label_color: Color::from_hex("#64748b"),
            title_color: Color::from_hex("#1e293b"),
            axis_font_size: 11.0,
            title_font_size: 14.0,
        }
    }
}

pub fn render_x_axis(
    list: &mut DisplayList,
    ctx: &mut CanvasContext,
    layout: &ChartLayout,
    config: &AxisConfig,
    scale: &LinearScale,
    colors: &AxisColors,
) {
    let ticks = compute_ticks(scale.domain.0, scale.domain.1, config.tick_count);
    let plot = &layout.plot_rect;

    if config.show_grid {
        ctx.save();
        ctx.set_color(colors.grid_color);
        ctx.set_stroke_width(1.0);
        for &tick_val in &ticks {
            let x = scale.map(tick_val);
            if x >= 0.0 && x <= plot.size.width {
                ctx.draw_line(x, 0.0, x, plot.size.height);
            }
        }
        ctx.restore();
    }

    if config.show_axis_line {
        let y = plot.origin.y + plot.size.height;
        let line_rect = Rect::new(
            Point::new(plot.origin.x, y),
            Size::new(plot.size.width, 1.0),
        );
        list.push_rect(line_rect, colors.axis_color, [0.0; 4]);
    }

    let label_y = plot.origin.y + plot.size.height + 4.0;
    for &tick_val in &ticks {
        let x = scale.map(tick_val);
        let abs_x = plot.origin.x + x;

        if x < -10.0 || x > plot.size.width + 10.0 {
            continue;
        }

        let label = if let Some(ref fmt) = config.format_fn {
            fmt(tick_val)
        } else {
            format_tick_value(tick_val)
        };

        let label_rect = Rect::new(
            Point::new(abs_x - 30.0, label_y),
            Size::new(60.0, colors.axis_font_size + 4.0),
        );
        list.push_text_centered(&label, label_rect, colors.label_color, colors.axis_font_size);
    }

    if let Some(ref title) = config.title {
        let title_y = label_y + colors.axis_font_size + 8.0;
        let title_rect = Rect::new(
            Point::new(plot.origin.x, title_y),
            Size::new(plot.size.width, colors.title_font_size + 4.0),
        );
        list.push_text_centered(title, title_rect, colors.title_color, colors.title_font_size);
    }
}

pub fn render_y_axis(
    list: &mut DisplayList,
    ctx: &mut CanvasContext,
    layout: &ChartLayout,
    config: &AxisConfig,
    scale: &LinearScale,
    colors: &AxisColors,
) {
    let ticks = compute_ticks(scale.domain.0, scale.domain.1, config.tick_count);
    let plot = &layout.plot_rect;

    if config.show_grid {
        ctx.save();
        ctx.set_color(colors.grid_color);
        ctx.set_stroke_width(1.0);
        for &tick_val in &ticks {
            let y = scale.map(tick_val);
            if y >= 0.0 && y <= plot.size.height {
                ctx.draw_line(0.0, y, plot.size.width, y);
            }
        }
        ctx.restore();
    }

    if config.show_axis_line {
        let line_rect = Rect::new(
            Point::new(plot.origin.x - 1.0, plot.origin.y),
            Size::new(1.0, plot.size.height),
        );
        list.push_rect(line_rect, colors.axis_color, [0.0; 4]);
    }

    let label_x = plot.origin.x - 4.0;
    for &tick_val in &ticks {
        let y = scale.map(tick_val);
        let abs_y = plot.origin.y + y;

        if y < -10.0 || y > plot.size.height + 10.0 {
            continue;
        }

        let label = if let Some(ref fmt) = config.format_fn {
            fmt(tick_val)
        } else {
            format_tick_value(tick_val)
        };

        let label_width = 50.0;
        let label_rect = Rect::new(
            Point::new(label_x - label_width, abs_y - colors.axis_font_size * 0.5),
            Size::new(label_width, colors.axis_font_size + 2.0),
        );
        list.push_text_aligned(
            &label,
            label_rect,
            colors.label_color,
            colors.axis_font_size,
            crate::mss::TextAlign::RIGHT,
            crate::mss::TextDecoration::None,
            400,
        );
    }

    if let Some(ref title) = config.title {
        let title_rect = Rect::new(
            Point::new(plot.origin.x - 50.0, plot.origin.y - colors.title_font_size - 4.0),
            Size::new(50.0, colors.title_font_size + 4.0),
        );
        list.push_text_centered(title, title_rect, colors.title_color, colors.title_font_size);
    }
}

pub fn estimate_y_axis_width(
    config: &AxisConfig,
    y_min: f64,
    y_max: f64,
    font_size: f32,
    text_measure: Option<&Arc<dyn TextMeasure>>,
) -> f32 {
    let ticks = compute_ticks(y_min, y_max, config.tick_count);
    let mut max_w: f32 = 0.0;
    for tick in &ticks {
        let label = if let Some(ref fmt) = config.format_fn {
            fmt(*tick)
        } else {
            format_tick_value(*tick)
        };
        max_w = max_w.max(estimate_text_width(&label, font_size, text_measure));
    }
    (max_w + 8.0).max(30.0)
}
