use std::sync::Arc;

use crate::audio::{compute_rms_bins, AudioBuffer};
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::core::sync::Mutex;
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension, MssFields};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;

const DEFAULT_BINS: usize = 256;
const DEFAULT_HEIGHT: f32 = 80.0;
const MIN_BAR_HEIGHT: f32 = 2.0;
const FALLBACK_PLAYED: &str = "#3B82F6";
const FALLBACK_PENDING: &str = "#94A3B8";
const FALLBACK_CARET: &str = "#3B82F6";

type SeekCallback = Arc<dyn Fn(f32) + Send + Sync>;

pub struct StaticWaveform {
    pcm: Option<Arc<AudioBuffer>>,
    progress: Option<f32>,
    on_seek: Option<SeekCallback>,
    bins: usize,
    width: Option<Dimension>,
    height: Option<Dimension>,
    classes: Vec<String>,
}

impl Default for StaticWaveform {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticWaveform {
    pub fn new() -> Self {
        Self {
            pcm: None,
            progress: None,
            on_seek: None,
            bins: DEFAULT_BINS,
            width: None,
            height: None,
            classes: Vec::new(),
        }
    }

    pub fn pcm(mut self, buf: impl Into<Option<Arc<AudioBuffer>>>) -> Self {
        self.pcm = buf.into();
        self
    }

    pub fn progress(mut self, p: f32) -> Self {
        self.progress = Some(p.clamp(0.0, 1.0));
        self
    }

    pub fn no_progress(mut self) -> Self {
        self.progress = None;
        self
    }

    pub fn on_seek(mut self, f: impl Fn(f32) + Send + Sync + 'static) -> Self {
        self.on_seek = Some(Arc::new(f));
        self
    }

    pub fn bins(mut self, n: usize) -> Self {
        self.bins = n.max(8);
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(Dimension::Px(h.max(MIN_BAR_HEIGHT)));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.classes.push(c.into());
        self
    }
}

impl Widget for StaticWaveform {
    fn create_element(&self) -> Box<dyn Element> {
        let mut elem = StaticWaveformElement {
            id: ElementId::new(),
            pcm: self.pcm.clone(),
            bins_cache_ptr: 0,
            bins_cache: Vec::new(),
            progress: self.progress,
            on_seek: self.on_seek.clone(),
            bins: self.bins,
            width: self.width,
            height: self.height,
            classes: self.classes.clone(),
            bounds: Rect::zero(),
            hover: false,
            seeking: Arc::new(Mutex::new(false)),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        };
        if elem.pcm.is_some() {
            elem.recompute_bins();
        }
        Box::new(elem)
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

struct StaticWaveformElement {
    id: ElementId,
    pcm: Option<Arc<AudioBuffer>>,
    bins_cache_ptr: usize,
    bins_cache: Vec<f32>,
    progress: Option<f32>,
    on_seek: Option<SeekCallback>,
    bins: usize,
    width: Option<Dimension>,
    height: Option<Dimension>,
    classes: Vec<String>,
    bounds: Rect,
    hover: bool,
    seeking: Arc<Mutex<bool>>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl StaticWaveformElement {
    fn x_to_progress(&self, x: f32) -> f32 {
        let w = self.bounds.size.width.max(1.0);
        ((x - self.bounds.x()) / w).clamp(0.0, 1.0)
    }

    fn invoke_seek(&self, x: f32) {
        if let Some(cb) = self.on_seek.as_ref() {
            cb(self.x_to_progress(x));
        }
    }

    fn recompute_bins(&mut self) {
        let want_ptr = self
            .pcm
            .as_ref()
            .map(|b| Arc::as_ptr(b) as usize)
            .unwrap_or(0);
        self.bins_cache = match &self.pcm {
            Some(buf) => compute_rms_bins(&buf.pcm, buf.channels, self.bins),
            None => vec![0.0; self.bins],
        };
        self.bins_cache_ptr = want_ptr;
    }
}

impl Element for StaticWaveformElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<StaticWaveform>() {
            let new_ptr = w
                .pcm
                .as_ref()
                .map(|b| Arc::as_ptr(b) as usize)
                .unwrap_or(0);
            let pcm_changed = new_ptr != self.bins_cache_ptr;
            let bins_changed = w.bins != self.bins;
            let progress_changed = w.progress != self.progress;

            self.pcm = w.pcm.clone();
            self.progress = w.progress;
            self.on_seek = w.on_seek.clone();
            self.bins = w.bins;
            self.width = w.width;
            self.height = w.height;

            if pcm_changed || bins_changed {
                self.recompute_bins();
            }
            if pcm_changed || bins_changed || progress_changed {
                self.mark_dirty(DirtyFlags::RENDER);
            }
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = self
            .width
            .or(self.mss.width)
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(constraints.max_width)
            .min(constraints.max_width);
        let height = self
            .height
            .or(self.mss.height)
            .map(|d| d.resolve(constraints.max_height))
            .unwrap_or(DEFAULT_HEIGHT)
            .min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg) = self.mss.background_color {
            let radius = self
                .mss
                .border_radius
                .map(|r| r.map(|d| d.resolve(self.bounds.size.height)))
                .unwrap_or([0.0; 4]);
            list.push_rect(self.bounds, bg, radius);
        }

        let total_w = self.bounds.size.width.max(1.0);
        let total_h = self.bounds.size.height.max(1.0);
        let center_y = self.bounds.y() + total_h * 0.5;

        if self.pcm.is_none() {
            let line_color = self
                .mss
                .color
                .unwrap_or_else(|| Color::from_hex(FALLBACK_PENDING))
                .with_alpha(0.4);
            let line = Rect::new(
                Point::new(self.bounds.x(), center_y - 0.5),
                Size::new(total_w, 1.0),
            );
            list.push_rect(line, line_color, [0.0; 4]);
            return;
        }

        let owned_fallback: Vec<f32>;
        let source: &[f32] = if self.bins_cache.len() == self.bins {
            &self.bins_cache
        } else if let Some(buf) = self.pcm.as_ref() {
            owned_fallback = compute_rms_bins(&buf.pcm, buf.channels, self.bins);
            &owned_fallback
        } else {
            owned_fallback = vec![0.0; self.bins];
            &owned_fallback
        };
        let max_bars = (total_w as usize).max(8);
        let target_bins = source.len().min(max_bars);
        let resampled: Vec<f32>;
        let bins: &[f32] = if target_bins == source.len() {
            source
        } else {
            let src_n = source.len();
            resampled = (0..target_bins)
                .map(|i| {
                    let start = i * src_n / target_bins;
                    let end = ((i + 1) * src_n / target_bins).max(start + 1).min(src_n);
                    let slice = &source[start..end];
                    slice.iter().sum::<f32>() / slice.len() as f32
                })
                .collect();
            &resampled
        };

        let played_color = self
            .mss
            .accent_color
            .unwrap_or_else(|| Color::from_hex(FALLBACK_PLAYED));
        let pending_color = self
            .mss
            .color
            .unwrap_or_else(|| Color::from_hex(FALLBACK_PENDING))
            .with_alpha(0.55);

        let n = bins.len().max(1) as f32;
        let bar_w = (total_w / n).max(1.0);
        let gap = (bar_w * 0.30).min(bar_w - 1.0).max(0.0);
        let inner_w = (bar_w - gap).max(1.0);

        let progress_x = self
            .progress
            .map(|p| self.bounds.x() + total_w * p.clamp(0.0, 1.0));

        for (i, v) in bins.iter().enumerate() {
            let amp = v.clamp(0.0, 1.0);
            let h = (amp * total_h * 0.92).max(MIN_BAR_HEIGHT);
            let x = self.bounds.x() + i as f32 * bar_w + gap * 0.5;
            let y = center_y - h * 0.5;
            let radius = (inner_w * 0.5).min(h * 0.5);

            let bar_center_x = x + inner_w * 0.5;
            let is_played = match progress_x {
                Some(px) => bar_center_x <= px,
                None => false,
            };
            let color = if is_played { played_color } else { pending_color };
            let bar_rect = Rect::new(Point::new(x, y), Size::new(inner_w, h));
            list.push_rect(bar_rect, color, [radius; 4]);
        }

        if let Some(px) = progress_x {
            let caret_color = self
                .mss
                .accent_color
                .unwrap_or_else(|| Color::from_hex(FALLBACK_CARET));
            let glow_rect = Rect::new(
                Point::new(px - 1.5, self.bounds.y()),
                Size::new(3.0, total_h),
            );
            list.push_rect(glow_rect, caret_color.with_alpha(0.25), [1.5, 1.5, 1.5, 1.5]);
            let core_rect = Rect::new(
                Point::new(px - 0.5, self.bounds.y()),
                Size::new(1.0, total_h),
            );
            list.push_rect(core_rect, caret_color, [0.5, 0.5, 0.5, 0.5]);
        }
    }

    fn animate(&mut self, _dt: std::time::Duration) -> bool {
        false
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.on_seek.is_none() {
            return EventResult::Ignored;
        }
        match event {
            Event::MouseMove(pos) => {
                let was_hover = self.hover;
                self.hover = self.bounds.contains(*pos);
                if self.hover {
                    ctx.set_cursor(CursorIcon::Pointer);
                }
                let dragging = self
                    .seeking
                    .lock()
                    .map(|g| *g)
                    .unwrap_or(false);
                if dragging {
                    self.invoke_seek(pos.x);
                    ctx.set_cursor(CursorIcon::Pointer);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hover != was_hover {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hover {
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    if let Ok(mut g) = self.seeking.lock() {
                        *g = true;
                    }
                    self.invoke_seek(position.x);
                    ctx.set_cursor(CursorIcon::Pointer);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } => {
                if *button == MouseButton::Left {
                    let was_seeking = self
                        .seeking
                        .lock()
                        .map(|mut g| {
                            let prev = *g;
                            *g = false;
                            prev
                        })
                        .unwrap_or(false);
                    if was_seeking {
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    fn clip_content(&self) -> bool {
        false
    }

    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags |= flags;
    }

    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags.remove(flags);
    }

    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty_flags.contains(flags)
    }

    fn id(&self) -> ElementId {
        self.id
    }

    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "StaticWaveform"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(d) = self.mss.width {
            self.width = Some(d);
        }
        if let Some(d) = self.mss.height {
            self.height = Some(d);
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Slider,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some("Audio waveform".to_string()),
                value: self
                    .progress
                    .map(|p| format!("{:.0}%", p * 100.0)),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for StaticWaveformElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buffer(samples: usize) -> Arc<AudioBuffer> {
        let pcm: Arc<[f32]> = Arc::from(
            (0..samples)
                .map(|i| (i as f32 * 0.001).sin())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        Arc::new(AudioBuffer::new(pcm, 48000, 1))
    }

    #[test]
    fn widget_classes_returned() {
        let w = StaticWaveform::new().class("audio-waveform");
        assert_eq!(w.widget_classes(), &["audio-waveform".to_string()]);
    }

    fn make_element(pcm: Option<Arc<AudioBuffer>>) -> StaticWaveformElement {
        StaticWaveformElement {
            id: ElementId::new(),
            pcm,
            bins_cache_ptr: 0,
            bins_cache: Vec::new(),
            progress: None,
            on_seek: None,
            bins: DEFAULT_BINS,
            width: None,
            height: None,
            classes: Vec::new(),
            bounds: Rect::zero(),
            hover: false,
            seeking: Arc::new(Mutex::new(false)),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        }
    }

    #[test]
    fn pcm_change_recomputes_cache() {
        let buf1 = make_buffer(1024);
        let buf2 = make_buffer(2048);
        let mut elem = make_element(Some(buf1.clone()));
        elem.recompute_bins();
        assert_eq!(elem.bins_cache.len(), DEFAULT_BINS);
        let first_ptr = elem.bins_cache_ptr;
        assert_ne!(first_ptr, 0);
        elem.pcm = Some(buf2);
        elem.recompute_bins();
        assert_ne!(elem.bins_cache_ptr, first_ptr);
        assert_eq!(elem.bins_cache.len(), DEFAULT_BINS);
    }

    #[test]
    fn x_to_progress_clamps() {
        let mut elem = make_element(None);
        elem.bounds = Rect::new(Point::new(10.0, 0.0), Size::new(100.0, 80.0));
        assert!((elem.x_to_progress(10.0) - 0.0).abs() < 1e-6);
        assert!((elem.x_to_progress(60.0) - 0.5).abs() < 1e-6);
        assert!((elem.x_to_progress(110.0) - 1.0).abs() < 1e-6);
        assert!((elem.x_to_progress(-50.0) - 0.0).abs() < 1e-6);
        assert!((elem.x_to_progress(500.0) - 1.0).abs() < 1e-6);
    }
}
