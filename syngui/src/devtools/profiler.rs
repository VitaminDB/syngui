use std::collections::VecDeque;
use crate::core::{Point, Rect, Size};
use crate::render::DisplayList;
use super::panel;

#[derive(Clone, Debug, Default)]
pub struct FrameTiming {
    pub layout_us: u64,
    pub display_list_us: u64,
    pub batch_render_us: u64,
    pub total_us: u64,
    pub element_count: usize,
    pub draw_calls: usize,
    pub vertex_count: usize,
    pub font_atlas_glyphs: usize,
    pub font_atlas_mem_kb: usize,
    pub display_list_commands: usize,
}

pub fn render_profiler(
    list: &mut DisplayList,
    content_rect: Rect,
    timings: &VecDeque<FrameTiming>,
) {
    list.push_clip(content_rect);

    let x = content_rect.origin.x;
    let w = content_rect.size.width;
    let mut y = content_rect.origin.y;

    y = render_section_header(list, x, y, w, "Current Frame");

    let current = timings.back().cloned().unwrap_or_default();
    let total_ms = current.total_us as f32 / 1000.0;

    let smooth_window = timings.len().min(30);
    let smooth_fps = if smooth_window > 0 {
        let sum_ms: f32 = timings.iter().rev().take(smooth_window)
            .map(|t| t.total_us as f32 / 1000.0).sum();
        let avg_ms = sum_ms / smooth_window as f32;
        if avg_ms > 0.0 { 1000.0 / avg_ms } else { 0.0 }
    } else { 0.0 };

    let avg_fps = if !timings.is_empty() {
        let sum_ms: f32 = timings.iter().map(|t| t.total_us as f32 / 1000.0).sum();
        let avg_ms = sum_ms / timings.len() as f32;
        if avg_ms > 0.0 { 1000.0 / avg_ms } else { 0.0 }
    } else { 0.0 };

    let stats = [
        ("FPS", format!("{:.0}", smooth_fps), fps_color(smooth_fps)),
        ("Avg FPS", format!("{:.0}", avg_fps), fps_color(avg_fps)),
        ("Layout", format!("{:.2} ms", current.layout_us as f32 / 1000.0), panel::PROF_LAYOUT),
        ("DisplayList", format!("{:.2} ms", current.display_list_us as f32 / 1000.0), panel::PROF_DISPLAY_LIST),
        ("Render", format!("{:.2} ms", current.batch_render_us as f32 / 1000.0), panel::PROF_RENDER),
        ("Total", format!("{:.2} ms", total_ms), panel::TEXT_PRIMARY),
        ("Elements", format!("{}", current.element_count), panel::TEXT_PRIMARY),
        ("Draw calls", format!("{}", current.draw_calls), panel::TEXT_PRIMARY),
        ("Vertices", format!("{}", current.vertex_count), panel::TEXT_PRIMARY),
    ];

    for (label, value, color) in &stats {
        let label_rect = Rect::new(
            Point::new(x + 4.0, y + 2.0),
            Size::new(90.0, panel::FONT_SIZE + 2.0),
        );
        list.push_text(label, label_rect, panel::TEXT_SECONDARY, panel::FONT_SIZE);

        let val_rect = Rect::new(
            Point::new(x + 94.0, y + 2.0),
            Size::new(w - 98.0, panel::FONT_SIZE + 2.0),
        );
        list.push_text(value, val_rect, *color, panel::FONT_SIZE);

        y += panel::LINE_HEIGHT;
    }

    y += 8.0;

    y = render_section_header(list, x, y, w, "Frame Timeline (last 240)");
    y += 4.0;

    let chart_height = 120.0;
    let chart_rect = Rect::new(
        Point::new(x + 4.0, y),
        Size::new(w - 8.0, chart_height),
    );

    list.push_rect(chart_rect, crate::core::Color::new(0.08, 0.08, 0.08, 1.0), [2.0; 4]);

    let max_frames = 240;
    let start = if timings.len() > max_frames { timings.len() - max_frames } else { 0 };
    let visible: Vec<&FrameTiming> = timings.iter().skip(start).collect();

    if !visible.is_empty() {
        let bar_width = chart_rect.size.width / max_frames as f32;
        let max_ms: f32 = 20.0;

        let fps60_y = chart_rect.origin.y + chart_rect.size.height * (1.0 - 16.67 / max_ms);
        if fps60_y > chart_rect.origin.y {
            let line_rect = Rect::new(
                Point::new(chart_rect.origin.x, fps60_y),
                Size::new(chart_rect.size.width, 1.0),
            );
            list.push_rect(line_rect, panel::PROF_LINE_60FPS, [0.0; 4]);
            let label_rect = Rect::new(
                Point::new(chart_rect.origin.x + 2.0, fps60_y - 12.0),
                Size::new(60.0, 10.0),
            );
            list.push_text("60fps", label_rect, panel::PROF_LINE_60FPS, panel::SMALL_FONT_SIZE);
        }

        for (i, timing) in visible.iter().enumerate() {
            let bx = chart_rect.origin.x + (max_frames - visible.len() + i) as f32 * bar_width;
            let by = chart_rect.origin.y + chart_rect.size.height;

            let layout_h = (timing.layout_us as f32 / 1000.0 / max_ms * chart_rect.size.height).min(chart_rect.size.height);
            let dl_h = (timing.display_list_us as f32 / 1000.0 / max_ms * chart_rect.size.height).min(chart_rect.size.height - layout_h);
            let render_h = (timing.batch_render_us as f32 / 1000.0 / max_ms * chart_rect.size.height).min(chart_rect.size.height - layout_h - dl_h);

            let mut current_y = by;

            if layout_h > 0.5 {
                current_y -= layout_h;
                list.push_rect(
                    Rect::new(Point::new(bx, current_y), Size::new(bar_width - 1.0, layout_h)),
                    panel::PROF_LAYOUT, [0.0; 4],
                );
            }

            if dl_h > 0.5 {
                current_y -= dl_h;
                list.push_rect(
                    Rect::new(Point::new(bx, current_y), Size::new(bar_width - 1.0, dl_h)),
                    panel::PROF_DISPLAY_LIST, [0.0; 4],
                );
            }

            if render_h > 0.5 {
                current_y -= render_h;
                list.push_rect(
                    Rect::new(Point::new(bx, current_y), Size::new(bar_width - 1.0, render_h)),
                    panel::PROF_RENDER, [0.0; 4],
                );
            }
        }
    }

    y += chart_height + 8.0;

    let legend_items = [
        (panel::PROF_LAYOUT, "Layout"),
        (panel::PROF_DISPLAY_LIST, "DisplayList"),
        (panel::PROF_RENDER, "Render"),
    ];
    for (color, label) in &legend_items {
        let swatch = Rect::new(
            Point::new(x + 8.0, y + 4.0),
            Size::new(10.0, 10.0),
        );
        list.push_rect(swatch, *color, [2.0; 4]);

        let label_rect = Rect::new(
            Point::new(x + 24.0, y + 2.0),
            Size::new(w - 28.0, panel::FONT_SIZE + 2.0),
        );
        list.push_text(label, label_rect, panel::TEXT_PRIMARY, panel::FONT_SIZE);

        y += panel::LINE_HEIGHT;
    }

    if timings.len() > 1 {
        y += 8.0;
        y = render_section_header(list, x, y, w, "Averages");

        let count = timings.len() as f32;
        let avg_layout = timings.iter().map(|t| t.layout_us).sum::<u64>() as f32 / count / 1000.0;
        let avg_dl = timings.iter().map(|t| t.display_list_us).sum::<u64>() as f32 / count / 1000.0;
        let avg_render = timings.iter().map(|t| t.batch_render_us).sum::<u64>() as f32 / count / 1000.0;
        let avg_total = timings.iter().map(|t| t.total_us).sum::<u64>() as f32 / count / 1000.0;

        let avg_stats = [
            ("Avg Layout", format!("{:.2} ms", avg_layout)),
            ("Avg DL", format!("{:.2} ms", avg_dl)),
            ("Avg Render", format!("{:.2} ms", avg_render)),
            ("Avg Total", format!("{:.2} ms", avg_total)),
        ];

        for (label, value) in &avg_stats {
            let label_rect = Rect::new(
                Point::new(x + 4.0, y + 2.0),
                Size::new(90.0, panel::FONT_SIZE + 2.0),
            );
            list.push_text(label, label_rect, panel::TEXT_SECONDARY, panel::FONT_SIZE);

            let val_rect = Rect::new(
                Point::new(x + 94.0, y + 2.0),
                Size::new(w - 98.0, panel::FONT_SIZE + 2.0),
            );
            list.push_text(value, val_rect, panel::TEXT_PRIMARY, panel::FONT_SIZE);

            y += panel::LINE_HEIGHT;
        }
    }

    y += 8.0;
    y = render_section_header(list, x, y, w, "Memory");

    let mem_stats = [
        ("Glyphs", format!("{}", current.font_atlas_glyphs)),
        ("Atlas Mem", format!("{} KB", current.font_atlas_mem_kb)),
        ("DL Commands", format!("{}", current.display_list_commands)),
    ];

    for (label, value) in &mem_stats {
        let label_rect = Rect::new(
            Point::new(x + 4.0, y + 2.0),
            Size::new(90.0, panel::FONT_SIZE + 2.0),
        );
        list.push_text(label, label_rect, panel::TEXT_SECONDARY, panel::FONT_SIZE);

        let val_rect = Rect::new(
            Point::new(x + 94.0, y + 2.0),
            Size::new(w - 98.0, panel::FONT_SIZE + 2.0),
        );
        list.push_text(value, val_rect, panel::TEXT_PRIMARY, panel::FONT_SIZE);

        y += panel::LINE_HEIGHT;
    }

    list.pop_clip();
}

fn render_section_header(
    list: &mut DisplayList,
    x: f32, y: f32, w: f32,
    title: &str,
) -> f32 {
    let header_rect = Rect::new(
        Point::new(x, y),
        Size::new(w, panel::LINE_HEIGHT),
    );
    list.push_rect(header_rect, panel::TAB_BG, [0.0; 4]);

    let text_rect = Rect::new(
        Point::new(x + 4.0, y + 2.0),
        Size::new(w - 8.0, panel::FONT_SIZE + 2.0),
    );
    list.push_text(title, text_rect, panel::TEXT_PRIMARY, panel::FONT_SIZE);

    y + panel::LINE_HEIGHT
}

fn fps_color(fps: f32) -> crate::core::Color {
    if fps >= 50.0 {
        crate::core::Color::new(0.298, 0.686, 0.314, 1.0)
    } else if fps >= 30.0 {
        crate::core::Color::new(1.0, 0.757, 0.027, 1.0)
    } else {
        crate::core::Color::new(0.898, 0.224, 0.208, 1.0)
    }
}
