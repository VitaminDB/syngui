use crate::core::{Color, Point, Rect, Size};
use crate::render::DisplayList;
use crate::widget::context::TextMeasure;

use std::sync::Arc;

use crate::widgets::charts::types::{LegendPosition, Series};

use super::estimate_text_width;

pub fn render_legend(
    list: &mut DisplayList,
    layout_rect: &Rect,
    series: &[Series],
    resolved_colors: &[Color],
    visibility: &[f32],
    legend_font_size: f32,
    label_color: Color,
    text_measure: Option<&Arc<dyn TextMeasure>>,
) -> Vec<Rect> {
    if series.is_empty() || layout_rect.size.width < 1.0 {
        return Vec::new();
    }

    let mut hit_rects = Vec::with_capacity(series.len());
    let swatch_size = legend_font_size * 0.8;
    let item_gap = 16.0;
    let swatch_text_gap = 4.0;

    let total_width: f32 = series
        .iter()
        .enumerate()
        .map(|(i, s)| {
            swatch_size + swatch_text_gap + estimate_text_width(&s.name, legend_font_size, text_measure)
                + if i < series.len() - 1 { item_gap } else { 0.0 }
        })
        .sum();

    let start_x = layout_rect.origin.x + (layout_rect.size.width - total_width).max(0.0) * 0.5;
    let center_y = layout_rect.origin.y + (layout_rect.size.height - swatch_size) * 0.5;

    let mut x = start_x;

    for (i, s) in series.iter().enumerate() {
        let opacity = if i < visibility.len() { visibility[i] } else { 1.0 };
        let color = if i < resolved_colors.len() {
            resolved_colors[i]
        } else {
            Color::from_hex("#888888")
        };

        let swatch_rect = Rect::new(
            Point::new(x, center_y),
            Size::new(swatch_size, swatch_size),
        );
        let alpha_color = color.with_alpha(opacity.max(0.3));
        list.push_rect(swatch_rect, alpha_color, [swatch_size * 0.5; 4]);

        x += swatch_size + swatch_text_gap;

        let text_width = estimate_text_width(&s.name, legend_font_size, text_measure);
        let text_rect = Rect::new(
            Point::new(x, center_y - 1.0),
            Size::new(text_width, legend_font_size + 2.0),
        );
        let text_color = if opacity > 0.5 {
            label_color
        } else {
            label_color.with_alpha(0.4)
        };
        list.push_text(&s.name, text_rect, text_color, legend_font_size);

        let entry_width = swatch_size + swatch_text_gap + text_width;
        let hit_rect = Rect::new(
            Point::new(x - swatch_size - swatch_text_gap, center_y - 2.0),
            Size::new(entry_width + 4.0, legend_font_size + 6.0),
        );
        hit_rects.push(hit_rect);

        x += text_width + item_gap;
    }

    hit_rects
}

pub fn render_legend_items(
    list: &mut DisplayList,
    layout_rect: &Rect,
    names: &[&str],
    colors: &[Color],
    visibility: &[f32],
    legend_font_size: f32,
    label_color: Color,
    text_measure: Option<&Arc<dyn TextMeasure>>,
) -> Vec<Rect> {
    if names.is_empty() || layout_rect.size.width < 1.0 {
        return Vec::new();
    }

    let count = names.len();
    let mut hit_rects = Vec::with_capacity(count);
    let swatch_size = legend_font_size * 0.8;
    let item_gap = 16.0;
    let swatch_text_gap = 4.0;

    let total_width: f32 = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            swatch_size + swatch_text_gap + estimate_text_width(name, legend_font_size, text_measure)
                + if i < count - 1 { item_gap } else { 0.0 }
        })
        .sum();

    let start_x = layout_rect.origin.x + (layout_rect.size.width - total_width).max(0.0) * 0.5;
    let center_y = layout_rect.origin.y + (layout_rect.size.height - swatch_size) * 0.5;

    let mut x = start_x;

    for (i, name) in names.iter().enumerate() {
        let opacity = if i < visibility.len() { visibility[i] } else { 1.0 };
        let color = if i < colors.len() { colors[i] } else { Color::from_hex("#888888") };

        let swatch_rect = Rect::new(
            Point::new(x, center_y),
            Size::new(swatch_size, swatch_size),
        );
        let alpha_color = color.with_alpha(opacity.max(0.3));
        list.push_rect(swatch_rect, alpha_color, [swatch_size * 0.5; 4]);

        x += swatch_size + swatch_text_gap;

        let text_width = estimate_text_width(name, legend_font_size, text_measure);
        let text_rect = Rect::new(
            Point::new(x, center_y - 1.0),
            Size::new(text_width, legend_font_size + 2.0),
        );
        let text_color = if opacity > 0.5 { label_color } else { label_color.with_alpha(0.4) };
        list.push_text(name, text_rect, text_color, legend_font_size);

        let entry_width = swatch_size + swatch_text_gap + text_width;
        let hit_rect = Rect::new(
            Point::new(x - swatch_size - swatch_text_gap, center_y - 2.0),
            Size::new(entry_width + 4.0, legend_font_size + 6.0),
        );
        hit_rects.push(hit_rect);

        x += text_width + item_gap;
    }

    hit_rects
}

pub fn legend_height(position: LegendPosition, font_size: f32) -> f32 {
    match position {
        LegendPosition::None => 0.0,
        LegendPosition::Top | LegendPosition::Bottom => font_size + 12.0,
        LegendPosition::Left | LegendPosition::Right => 0.0,
    }
}
