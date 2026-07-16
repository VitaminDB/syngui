use crate::core::Color;
use std::time::Duration;
use web_time::Instant;

#[derive(Clone, Debug)]
pub struct MapMarker {
    pub lat: f64,
    pub lng: f64,
    pub label: Option<String>,
    pub color: Color,
    pub size: f32,
    pub appear_at: Option<Instant>,
    pub fade_out_at: Option<Instant>,
    pub animation_duration: Duration,
    pub pulse: bool,
}

impl MapMarker {
    pub fn new(lat: f64, lng: f64) -> Self {
        Self {
            lat,
            lng,
            label: None,
            color: Color::new(0.9, 0.2, 0.2, 1.0),
            size: 12.0,
            appear_at: None,
            fade_out_at: None,
            animation_duration: Duration::from_millis(400),
            pulse: false,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn fade_in_at(mut self, at: Instant) -> Self {
        self.appear_at = Some(at); self
    }

    pub fn fade_out_at(mut self, at: Instant) -> Self {
        self.fade_out_at = Some(at); self
    }

    pub fn animation_duration(mut self, d: Duration) -> Self {
        self.animation_duration = d; self
    }

    pub fn pulse(mut self) -> Self {
        self.pulse = true; self
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        if self.pulse { return true; }
        if let Some(at) = self.appear_at {
            if now.saturating_duration_since(at) < self.animation_duration {
                return true;
            }
        }
        if let Some(fo) = self.fade_out_at {
            if now >= fo && now.saturating_duration_since(fo) < self.animation_duration {
                return true;
            }
        }
        false
    }

    pub fn is_expired(&self, now: Instant) -> bool {
        if let Some(fo) = self.fade_out_at {
            now.saturating_duration_since(fo) >= self.animation_duration
        } else {
            false
        }
    }

    pub fn current_opacity(&self, now: Instant) -> f32 {
        let mut opacity = 1.0_f32;
        if let Some(at) = self.appear_at {
            let elapsed = now.saturating_duration_since(at).as_secs_f32();
            let dur = self.animation_duration.as_secs_f32().max(0.001);
            let t = (elapsed / dur).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - t).powi(3);
            opacity = opacity.min(eased);
        }
        if let Some(fo) = self.fade_out_at {
            if now >= fo {
                let elapsed = now.saturating_duration_since(fo).as_secs_f32();
                let dur = self.animation_duration.as_secs_f32().max(0.001);
                let t = (elapsed / dur).clamp(0.0, 1.0);
                let eased = (1.0 - t).powi(2);
                opacity = opacity.min(eased);
            }
        }
        opacity
    }

    pub fn current_scale(&self, now: Instant) -> f32 {
        let mut scale = 1.0_f32;
        if let Some(at) = self.appear_at {
            let elapsed = now.saturating_duration_since(at).as_secs_f32();
            let dur = self.animation_duration.as_secs_f32().max(0.001);
            let t = (elapsed / dur).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - t).powi(3);
            scale = 0.5 + 0.5 * eased;
        }
        if self.pulse {
            let base = self.appear_at.or(self.fade_out_at).unwrap_or(now);
            let t = now.saturating_duration_since(base).as_secs_f32();
            scale *= 1.0 + 0.1 * (t * std::f32::consts::TAU / 1.5).sin();
        }
        scale
    }
}
