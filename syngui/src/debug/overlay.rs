use std::collections::VecDeque;
use crate::core::{Color, Point, Rect, Size};
use crate::render::DisplayList;

pub struct FrameStats {
    pub draw_calls: usize,
    pub vertex_count: usize,
    pub element_count: usize,
    pub font_atlas_glyphs: usize,
    pub font_atlas_mem_kb: usize,
    pub display_list_commands: usize,
}

pub struct DebugOverlay {
    frame_times: VecDeque<f32>,
    max_samples: usize,
    last_stats: FrameStats,
}

impl DebugOverlay {
    pub fn new() -> Self {
        Self {
            frame_times: VecDeque::with_capacity(120),
            max_samples: 120,
            last_stats: FrameStats {
                draw_calls: 0,
                vertex_count: 0,
                element_count: 0,
                font_atlas_glyphs: 0,
                font_atlas_mem_kb: 0,
                display_list_commands: 0,
            },
        }
    }

    pub fn record_frame(&mut self, dt_secs: f32) {
        if self.frame_times.len() >= self.max_samples {
            self.frame_times.pop_front();
        }
        self.frame_times.push_back(dt_secs);
    }

    pub fn update_stats(&mut self, stats: FrameStats) {
        self.last_stats = stats;
    }

    fn fps(&self) -> f32 {
        if self.frame_times.is_empty() { return 0.0; }
        let window = self.frame_times.len().min(30);
        let sum: f32 = self.frame_times.iter().rev().take(window).sum();
        let avg_dt = sum / window as f32;
        if avg_dt > 0.0 { 1.0 / avg_dt } else { 0.0 }
    }

    fn avg_fps(&self) -> f32 {
        if self.frame_times.is_empty() { return 0.0; }
        let avg_dt: f32 = self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32;
        if avg_dt > 0.0 { 1.0 / avg_dt } else { 0.0 }
    }

    fn avg_frame_time_ms(&self) -> f32 {
        if self.frame_times.is_empty() { return 0.0; }
        (self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32) * 1000.0
    }

    fn min_frame_time_ms(&self) -> f32 {
        self.frame_times.iter().copied().fold(f32::MAX, f32::min) * 1000.0
    }

    fn max_frame_time_ms(&self) -> f32 {
        self.frame_times.iter().copied().fold(0.0f32, f32::max) * 1000.0
    }

    pub fn build_display_list(&self, display_list: &mut DisplayList) {
        let surface = display_list.surface_size();
        if surface.width <= 0.0 || surface.height <= 0.0 { return; }

        display_list.begin_overlay_absolute();

        let padding = 8.0;
        let line_height = 16.0;
        let font_size = 12.0;
        let num_lines = 9;
        let overlay_width = 240.0;
        let overlay_height = padding * 2.0 + line_height * num_lines as f32;
        let margin = 10.0;

        let overlay_rect = Rect::new(
            Point::new(surface.width - overlay_width - margin, margin),
            Size::new(overlay_width, overlay_height),
        );

        let bg_color = Color::new(0.0, 0.0, 0.0, 0.75);
        display_list.push_rect(overlay_rect, bg_color, [6.0; 4]);

        let green = Color::new(0.0, 1.0, 0.0, 1.0);
        let yellow = Color::new(1.0, 1.0, 0.0, 1.0);
        let white = Color::new(0.85, 0.85, 0.85, 1.0);

        let x = overlay_rect.origin.x + padding;
        let w = overlay_width - padding * 2.0;
        let mut y = overlay_rect.origin.y + padding;

        let current_fps = self.fps();
        let current_frame_ms = self.frame_times.back().copied().unwrap_or(0.0) * 1000.0;

        let fps_color = if current_fps >= 50.0 { green } else if current_fps >= 30.0 { yellow } else { Color::new(1.0, 0.3, 0.3, 1.0) };

        let lines: [(String, Color); 9] = [
            (format!("FPS: {:.0}", current_fps), fps_color),
            (format!("Avg FPS: {:.0}", self.avg_fps()), white),
            (format!("Frame: {:.2} ms", current_frame_ms), white),
            (format!("Avg: {:.2} ms", self.avg_frame_time_ms()), white),
            (format!("Min/Max: {:.2}/{:.2} ms", self.min_frame_time_ms(), self.max_frame_time_ms()), white),
            (format!("Draw calls: {}", self.last_stats.draw_calls), white),
            (format!("Verts: {}  Elems: {}", self.last_stats.vertex_count, self.last_stats.element_count), white),
            (format!("Glyphs: {}  Atlas: {} KB", self.last_stats.font_atlas_glyphs, self.last_stats.font_atlas_mem_kb), white),
            (format!("DL Commands: {}", self.last_stats.display_list_commands), white),
        ];

        for (text, color) in &lines {
            let rect = Rect::new(Point::new(x, y), Size::new(w, line_height));
            display_list.push_text(text, rect, *color, font_size);
            y += line_height;
        }

        display_list.end_overlay();
    }
}
