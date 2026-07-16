use std::sync::{Arc, Mutex};

use crate::audio::VisHandle;
use crate::core::Color;
use crate::widget::styled::StyledWidget;
use crate::widget::WidgetExt;
use crate::widgets::visual::canvas::Canvas;

const DEFAULT_BARS: usize = 48;
const DEFAULT_HEIGHT: f32 = 36.0;
const SMOOTHING: f32 = 0.45;
const MIN_BAR_HEIGHT: f32 = 2.0;
const FALLBACK_COLOR: &str = "#3B82F6";

pub struct AudioWaveform {
    handle: VisHandle,
    bars: usize,
    height: f32,
    color: Option<Color>,
}

impl AudioWaveform {
    pub fn new(handle: VisHandle) -> Self {
        Self {
            handle,
            bars: DEFAULT_BARS,
            height: DEFAULT_HEIGHT,
            color: None,
        }
    }

    pub fn bars(mut self, n: usize) -> Self {
        self.bars = n.max(1);
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h.max(MIN_BAR_HEIGHT);
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    pub fn into_canvas(self) -> Canvas {
        let handle = self.handle;
        let bars = self.bars;
        let height = self.height;
        let explicit_color = self.color;
        let prev: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(vec![0.0; bars]));

        Canvas::new(move |ctx, _t| {
            let target = handle.snapshot_bars(bars);
            let mut prev_g = prev.lock().expect("waveform smoothing lock poisoned");
            if prev_g.len() != target.len() {
                prev_g.clear();
                prev_g.resize(target.len(), 0.0);
            }
            for i in 0..target.len() {
                let p = prev_g[i];
                prev_g[i] = p + (target[i] - p) * SMOOTHING;
            }

            let fill = explicit_color
                .or_else(|| ctx.mss_accent())
                .or_else(|| ctx.mss_color())
                .unwrap_or_else(|| Color::from_hex(FALLBACK_COLOR));
            ctx.set_color(fill);

            let total_w = ctx.width();
            let total_h = ctx.height();
            let n = prev_g.len().max(1) as f32;
            let bar_w = (total_w / n).max(1.0);
            let gap = (bar_w * 0.35).min(bar_w - 1.0).max(0.0);
            let inner_w = (bar_w - gap).max(1.0);
            let center_y = total_h * 0.5;

            for (i, v) in prev_g.iter().enumerate() {
                let amp = v.clamp(0.0, 1.0);
                let h = (amp * total_h).max(MIN_BAR_HEIGHT);
                let x = i as f32 * bar_w + gap * 0.5;
                let y = center_y - h * 0.5;
                let radius = (inner_w * 0.5).min(h * 0.5);
                ctx.fill_rounded_rect(x, y, inner_w, h, radius);
            }
        })
        .height(height)
        .animated(true)
    }
}

impl AudioWaveform {
    pub fn class(self, class: impl Into<String>) -> StyledWidget<Canvas> {
        self.into_canvas().class(class)
    }
}
