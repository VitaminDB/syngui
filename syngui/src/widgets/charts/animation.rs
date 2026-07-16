use std::time::Duration;

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[derive(Debug, Clone)]
pub(crate) struct ChartAnimationState {
    pub appear_progress: f32,
    pub appear_started: bool,
    pub appear_eased: f32,
    pub series_visibility: Vec<f32>,
    pub series_visible: Vec<bool>,
    pub hover_point: Option<(usize, usize)>,
    pub hover_t: f32,
    pub tooltip_opacity: f32,
    pub zoom_level: f32,
    pub pan_offset: (f64, f64),
    appear_duration: f32,
    transition_speed: f32,
}

impl Default for ChartAnimationState {
    fn default() -> Self {
        Self {
            appear_progress: 0.0,
            appear_started: false,
            appear_eased: 0.0,
            series_visibility: Vec::new(),
            series_visible: Vec::new(),
            hover_point: None,
            hover_t: 0.0,
            tooltip_opacity: 0.0,
            zoom_level: 1.0,
            pan_offset: (0.0, 0.0),
            appear_duration: 0.8,
            transition_speed: 6.0,
        }
    }
}

impl ChartAnimationState {
    pub fn ensure_series_count(&mut self, count: usize) {
        while self.series_visibility.len() < count {
            self.series_visibility.push(1.0);
        }
        while self.series_visible.len() < count {
            self.series_visible.push(true);
        }
        self.series_visibility.truncate(count);
        self.series_visible.truncate(count);
    }

    pub fn start_appear(&mut self) {
        if !self.appear_started {
            self.appear_started = true;
            self.appear_progress = 0.0;
            self.appear_eased = 0.0;
        }
    }

    pub fn toggle_series(&mut self, idx: usize) {
        if idx < self.series_visible.len() {
            self.series_visible[idx] = !self.series_visible[idx];
        }
    }

    pub fn is_series_visible(&self, idx: usize) -> bool {
        idx < self.series_visible.len() && self.series_visible[idx]
    }

    pub fn series_opacity(&self, idx: usize) -> f32 {
        if idx < self.series_visibility.len() {
            self.series_visibility[idx]
        } else {
            1.0
        }
    }

    pub fn set_appear_duration_ms(&mut self, ms: f32) {
        self.appear_duration = (ms / 1000.0).max(0.01);
    }

    pub fn zoom_at(&mut self, focal_x: f64, focal_y: f64, factor: f32, view_center: (f64, f64)) {
        let old_zoom = self.zoom_level;
        self.zoom_level = (self.zoom_level * factor).clamp(0.1, 50.0);
        let zoom_ratio = (old_zoom / self.zoom_level) as f64;

        let new_cx = focal_x + (view_center.0 - focal_x) * zoom_ratio;
        let new_cy = focal_y + (view_center.1 - focal_y) * zoom_ratio;

        let data_cx = view_center.0 - self.pan_offset.0;
        let data_cy = view_center.1 - self.pan_offset.1;

        self.pan_offset.0 = new_cx - data_cx;
        self.pan_offset.1 = new_cy - data_cy;
    }

    pub fn tick(&mut self, dt: Duration) -> bool {
        let dt_s = dt.as_secs_f32();
        let mut animating = false;

        if self.appear_started && self.appear_progress < 1.0 {
            let speed = 1.0 / self.appear_duration;
            self.appear_progress = (self.appear_progress + dt_s * speed).min(1.0);
            self.appear_eased = ease_out_cubic(self.appear_progress);
            animating = true;
        }

        for i in 0..self.series_visibility.len() {
            let target = if self.series_visible[i] { 1.0 } else { 0.0 };
            let current = self.series_visibility[i];
            if (current - target).abs() > 0.001 {
                let delta = dt_s * self.transition_speed;
                self.series_visibility[i] = if current < target {
                    (current + delta).min(target)
                } else {
                    (current - delta).max(target)
                };
                animating = true;
            } else {
                self.series_visibility[i] = target;
            }
        }

        let hover_target = if self.hover_point.is_some() { 1.0 } else { 0.0 };
        if (self.hover_t - hover_target).abs() > 0.001 {
            let delta = dt_s * self.transition_speed * 1.5;
            self.hover_t = if self.hover_t < hover_target {
                (self.hover_t + delta).min(hover_target)
            } else {
                (self.hover_t - delta).max(hover_target)
            };
            animating = true;
        } else {
            self.hover_t = hover_target;
        }

        let tooltip_target = if self.hover_point.is_some() { 1.0 } else { 0.0 };
        if (self.tooltip_opacity - tooltip_target).abs() > 0.001 {
            let delta = dt_s * self.transition_speed * 2.0;
            self.tooltip_opacity = if self.tooltip_opacity < tooltip_target {
                (self.tooltip_opacity + delta).min(tooltip_target)
            } else {
                (self.tooltip_opacity - delta).max(tooltip_target)
            };
            animating = true;
        } else {
            self.tooltip_opacity = tooltip_target;
        }

        animating
    }
}
