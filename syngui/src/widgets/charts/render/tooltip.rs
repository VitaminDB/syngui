use crate::core::{Color, Point, Rect, Size};
use crate::render::DisplayList;
use crate::widget::context::TextMeasure;

use std::sync::Arc;

use crate::widgets::charts::math::LinearScale;
use crate::widgets::charts::types::Series;

use super::estimate_text_width;

pub(crate) struct TooltipColors {
    pub background: Color,
    pub border_color: Color,
    pub text_color: Color,
    pub font_size: f32,
}

impl Default for TooltipColors {
    fn default() -> Self {
        Self {
            background: Color::from_hex("#1e293b"),
            border_color: Color::from_hex("#334155"),
            text_color: Color::from_hex("#f1f5f9"),
            font_size: 12.0,
        }
    }
}

pub fn render_tooltip(
    list: &mut DisplayList,
    mouse_pos: Point,
    chart_bounds: &Rect,
    series: &[Series],
    resolved_colors: &[Color],
    visibility: &[f32],
    hover_point: (usize, usize),
    shared: bool,
    _x_scale: &LinearScale,
    _y_scale: &LinearScale,
    opacity: f32,
    colors: &TooltipColors,
    x_format: &Option<std::sync::Arc<dyn Fn(f64) -> String + Send + Sync>>,
    y_format: &Option<std::sync::Arc<dyn Fn(f64) -> String + Send + Sync>>,
    text_measure: Option<&Arc<dyn TextMeasure>>,
) {
    if opacity < 0.01 {
        return;
    }

    let (_si, pi) = hover_point;

    let mut lines: Vec<(String, Color)> = Vec::new();

    if let Some(dp) = series.get(_si).and_then(|s| s.data.get(pi)) {
        let x_label = if let Some(ref fmt) = x_format {
            fmt(dp.x)
        } else {
            crate::widgets::charts::math::format_tick_value(dp.x)
        };
        lines.push((x_label, colors.text_color));
    }

    if shared {
        for (i, s) in series.iter().enumerate() {
            if i < visibility.len() && visibility[i] < 0.01 {
                continue;
            }
            if let Some(dp) = s.data.get(pi) {
                let y_label = if let Some(ref fmt) = y_format {
                    fmt(dp.y)
                } else {
                    crate::widgets::charts::math::format_tick_value(dp.y)
                };
                let color = resolved_colors.get(i).copied().unwrap_or(Color::WHITE);
                lines.push((format!("{}: {}", s.name, y_label), color));
            }
        }
    } else {
        if let Some(s) = series.get(_si) {
            if let Some(dp) = s.data.get(pi) {
                let y_label = if let Some(ref fmt) = y_format {
                    fmt(dp.y)
                } else {
                    crate::widgets::charts::math::format_tick_value(dp.y)
                };
                let color = resolved_colors.get(_si).copied().unwrap_or(Color::WHITE);
                lines.push((format!("{}: {}", s.name, y_label), color));
            }
        }
    }

    if lines.is_empty() {
        return;
    }

    let line_height = colors.font_size + 4.0;
    let padding = 8.0;
    let max_text_width = lines
        .iter()
        .map(|(text, _)| estimate_text_width(text, colors.font_size, text_measure))
        .fold(0.0_f32, f32::max);
    let tooltip_width = max_text_width + padding * 2.0;
    let tooltip_height = lines.len() as f32 * line_height + padding * 2.0;

    let offset_x = 12.0;
    let offset_y = -12.0;

    let mut x = mouse_pos.x + offset_x;
    let mut y = mouse_pos.y + offset_y - tooltip_height;

    if x + tooltip_width > chart_bounds.origin.x + chart_bounds.size.width {
        x = mouse_pos.x - offset_x - tooltip_width;
    }
    if y < chart_bounds.origin.y {
        y = mouse_pos.y + offset_y + 16.0;
    }

    x = x.max(chart_bounds.origin.x);
    y = y.max(chart_bounds.origin.y);

    let tooltip_rect = Rect::new(
        Point::new(x, y),
        Size::new(tooltip_width, tooltip_height),
    );

    list.push_shadow(
        tooltip_rect,
        Color::new(0.0, 0.0, 0.0, 0.2 * opacity),
        8.0,
        (0.0, 2.0),
        [6.0; 4],
    );
    list.push_rect(
        tooltip_rect,
        colors.background.with_alpha(opacity * 0.95),
        [6.0; 4],
    );

    let mut text_y = y + padding;
    for (i, (text, color)) in lines.iter().enumerate() {
        let text_rect = Rect::new(
            Point::new(x + padding, text_y),
            Size::new(max_text_width, line_height),
        );
        let text_color = if i == 0 {
            colors.text_color.with_alpha(opacity * 0.7)
        } else {
            color.with_alpha(opacity)
        };
        list.push_text(text, text_rect, text_color, colors.font_size);
        text_y += line_height;
    }

    if let Some(s) = series.get(_si) {
        if let Some(dp) = s.data.get(pi) {
            let crosshair_x = chart_bounds.origin.x
                + (chart_bounds.size.width * 0.0)
                ;
            let _ = (dp, crosshair_x);
        }
    }
}

pub fn render_crosshair(
    list: &mut DisplayList,
    plot_rect: &Rect,
    x_pixel: f32,
    opacity: f32,
    color: Color,
) {
    if opacity < 0.01 {
        return;
    }
    let crosshair_rect = Rect::new(
        Point::new(plot_rect.origin.x + x_pixel, plot_rect.origin.y),
        Size::new(1.0, plot_rect.size.height),
    );
    list.push_rect(crosshair_rect, color.with_alpha(opacity * 0.5), [0.0; 4]);
}
