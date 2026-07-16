//! `FramesView` — покадровый видеоплеер из памяти: `Vec<Arc<VideoFrame>>` +
//! fps, без файла и декодера. Одна GPU-текстура на инстанс (update_rgba по
//! смене кадра — без роста ImageStore). Клик — play/pause; внешний seek —
//! через `position_signal` (секунды) на паузе.
//!
//! Источник кадров — генеративные пайплайны (LTX video, будущие галереи) и
//! любые in-memory секвенции.

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::gpu::image_store::{ImageHandle, ImageStore};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{DisplayList, TextureId};
use crate::signal::RwSignal;
use crate::video::VideoFrame;
use crate::widget::context::EventContextExt;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use crate::widgets::ImageFit;

pub struct FramesView {
    frames: Arc<Vec<Arc<VideoFrame>>>,
    fps: f32,
    fit: ImageFit,
    classes: Vec<String>,
    playing_signal: Option<RwSignal<bool>>,
    position_signal: Option<RwSignal<f32>>,
    autoplay: bool,
    loop_playback: bool,
}

impl FramesView {
    pub fn new(frames: Arc<Vec<Arc<VideoFrame>>>, fps: f32) -> Self {
        Self {
            frames,
            fps: fps.max(1.0),
            fit: ImageFit::Contain,
            classes: Vec::new(),
            playing_signal: None,
            position_signal: None,
            autoplay: false,
            loop_playback: true,
        }
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.classes.push(c.into());
        self
    }

    /// Двунаправленный play/pause: виджет читает сигнал, клик пишет в него.
    pub fn playing_signal(mut self, sig: RwSignal<bool>) -> Self {
        self.playing_signal = Some(sig);
        self
    }

    /// Позиция в секундах: виджет пишет при воспроизведении; внешняя запись
    /// на паузе трактуется как seek.
    pub fn position_signal(mut self, sig: RwSignal<f32>) -> Self {
        self.position_signal = Some(sig);
        self
    }

    pub fn autoplay(mut self, on: bool) -> Self {
        self.autoplay = on;
        self
    }

    pub fn loop_playback(mut self, on: bool) -> Self {
        self.loop_playback = on;
        self
    }

    fn natural_size(&self) -> (u32, u32) {
        self.frames
            .first()
            .map(|f| (f.width.max(1), f.height.max(1)))
            .unwrap_or((1, 1))
    }
}

impl Widget for FramesView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(FramesViewElement {
            id: ElementId::new(),
            frames: self.frames.clone(),
            fps: self.fps,
            fit: self.fit,
            classes: self.classes.clone(),
            bounds: Rect::zero(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            image_handle: None,
            image_store: None,
            natural_size: self.natural_size(),
            mss: MssFields::new(),
            playing_signal: self.playing_signal,
            position_signal: self.position_signal,
            playing: self.autoplay,
            loop_playback: self.loop_playback,
            t_sec: 0.0,
            last_idx: usize::MAX,
        })
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

pub struct FramesViewElement {
    id: ElementId,
    frames: Arc<Vec<Arc<VideoFrame>>>,
    fps: f32,
    fit: ImageFit,
    classes: Vec<String>,
    bounds: Rect,
    dirty_flags: DirtyFlags,
    image_handle: Option<ImageHandle>,
    image_store: Option<Arc<Mutex<ImageStore>>>,
    natural_size: (u32, u32),
    mss: MssFields,
    playing_signal: Option<RwSignal<bool>>,
    position_signal: Option<RwSignal<f32>>,
    playing: bool,
    loop_playback: bool,
    t_sec: f64,
    last_idx: usize,
}

impl FramesViewElement {
    fn duration_sec(&self) -> f64 {
        self.frames.len() as f64 / self.fps as f64
    }

    fn idx_at(&self, t: f64) -> usize {
        ((t * self.fps as f64).floor() as usize).min(self.frames.len().saturating_sub(1))
    }

    fn upload_frame(&mut self, idx: usize) {
        if idx == self.last_idx {
            return;
        }
        let Some(frame) = self.frames.get(idx) else {
            return;
        };
        if let (Some(handle), Some(store)) = (self.image_handle, self.image_store.as_ref()) {
            if let Ok(mut s) = store.lock() {
                s.update_rgba(handle, frame.width, frame.height, frame.rgba.clone());
            }
        }
        let new_size = (frame.width, frame.height);
        if new_size != self.natural_size {
            self.natural_size = new_size;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        } else {
            self.mark_dirty(DirtyFlags::RENDER);
        }
        self.last_idx = idx;
    }

    fn set_playing(&mut self, on: bool) {
        self.playing = on;
        if let Some(sig) = self.playing_signal {
            if sig.get_untracked() != on {
                sig.set(on);
            }
        }
    }

    fn compute_fit_rect(&self) -> Rect {
        let (nw, nh) = self.natural_size;
        if nw == 0 || nh == 0 {
            return self.bounds;
        }
        let nw = nw as f32;
        let nh = nh as f32;
        let bw = self.bounds.size.width;
        let bh = self.bounds.size.height;
        match self.fit {
            ImageFit::Fill => self.bounds,
            ImageFit::None => {
                let x = self.bounds.x() + (bw - nw) / 2.0;
                let y = self.bounds.y() + (bh - nh) / 2.0;
                Rect::new(Point::new(x, y), Size::new(nw, nh))
            }
            ImageFit::Contain => {
                let scale = (bw / nw).min(bh / nh);
                let sw = nw * scale;
                let sh = nh * scale;
                let x = self.bounds.x() + (bw - sw) / 2.0;
                let y = self.bounds.y() + (bh - sh) / 2.0;
                Rect::new(Point::new(x, y), Size::new(sw, sh))
            }
            ImageFit::Cover => {
                let scale = (bw / nw).max(bh / nh);
                let sw = nw * scale;
                let sh = nh * scale;
                let x = self.bounds.x() + (bw - sw) / 2.0;
                let y = self.bounds.y() + (bh - sh) / 2.0;
                Rect::new(Point::new(x, y), Size::new(sw, sh))
            }
        }
    }
}

impl Element for FramesViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(v) = widget.as_any().downcast_ref::<FramesView>() {
            if !Arc::ptr_eq(&self.frames, &v.frames) {
                self.frames = v.frames.clone();
                self.t_sec = 0.0;
                self.last_idx = usize::MAX;
                self.upload_frame(0);
            }
            self.fps = v.fps;
            self.fit = v.fit;
            self.loop_playback = v.loop_playback;
            self.classes = v.classes.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let (nw, nh) = self.natural_size;
        let max_w = constraints.max_width;
        let max_h = constraints.max_height;
        let width = if let Some(d) = self.mss.width {
            d.resolve(max_w).min(max_w)
        } else if nw > 0 {
            (nw as f32).min(max_w)
        } else {
            max_w.min(320.0)
        };
        let height = if let Some(d) = self.mss.height {
            d.resolve(max_h).min(max_h)
        } else if nw > 0 && nh > 0 {
            let aspect = nh as f32 / nw as f32;
            (width * aspect).min(max_h)
        } else {
            max_h.min(180.0)
        };
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self
            .mss
            .background_color
            .unwrap_or_else(|| Color::from_hex("#000000"));
        list.push_rect(self.bounds, bg, [0.0; 4]);
        if let Some(handle) = self.image_handle {
            let fit_rect = self.compute_fit_rect();
            let uv_rect = Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0));
            list.push_image(fit_rect, TextureId(handle.0), uv_rect, Color::WHITE);
        }
    }

    fn handle_event(
        &mut self,
        event: &Event,
        ctx: &mut crate::widget::context::EventContext,
    ) -> EventResult {
        if let Event::MouseDown { button, position } = event {
            if *button == MouseButton::Left && self.bounds.contains(*position) {
                let on = !self.playing;
                self.set_playing(on);
                ctx.request_paint();
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if self.frames.is_empty() {
            return false;
        }
        if let Some(sig) = self.playing_signal {
            let external = sig.get_untracked();
            if external != self.playing {
                self.playing = external;
            }
        }
        if self.playing {
            // Внешний seek во время воспроизведения: если position_signal
            // ушёл от внутреннего времени больше чем на ~3 кадра, это правка
            // слайдером (а не наш собственный writeback) — принимаем как seek.
            if let Some(sig) = self.position_signal {
                let external = sig.get_untracked() as f64;
                if (external - self.t_sec).abs() > 3.0 / self.fps as f64 {
                    self.t_sec = external.clamp(0.0, self.duration_sec());
                }
            }
            self.t_sec += dt.as_secs_f64();
            let dur = self.duration_sec();
            if self.t_sec >= dur {
                if self.loop_playback {
                    self.t_sec %= dur.max(f64::EPSILON);
                } else {
                    self.t_sec = dur;
                    self.set_playing(false);
                }
            }
            if let Some(sig) = self.position_signal {
                let t = self.t_sec as f32;
                if (sig.get_untracked() - t).abs() > 0.05 {
                    sig.set(t);
                }
            }
        } else if let Some(sig) = self.position_signal {
            let external = sig.get_untracked() as f64;
            if (external - self.t_sec).abs() > 0.5 / self.fps as f64 {
                self.t_sec = external.clamp(0.0, self.duration_sec());
            }
        }
        let idx = self.idx_at(self.t_sec);
        self.upload_frame(idx);
        self.playing
    }

    fn wants_animate_tick(&self) -> bool {
        true
    }

    fn clip_content(&self) -> bool {
        self.fit == ImageFit::Cover
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

    fn mount(&mut self, tree: &mut ElementTree) {
        self.image_store = tree.image_store.clone();
        let (w, h) = self.natural_size;
        let starter = self
            .frames
            .first()
            .map(|f| f.rgba.to_vec())
            .unwrap_or_else(|| vec![0u8; (w as usize) * (h as usize) * 4]);
        if let Some(store) = self.image_store.as_ref() {
            if let Ok(mut s) = store.lock() {
                let key = format!("frames:{:p}:{}", Arc::as_ptr(&self.frames), self.id.0);
                let (handle, _) = s.request_rgba(&key, w, h, starter);
                self.image_handle = Some(handle);
            }
        }
        if !self.frames.is_empty() {
            self.last_idx = 0;
        }
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "FramesView"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
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
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        let (w, h) = self.natural_size;
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Image,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(format!("Видео {}×{} ({} кадров)", w, h, self.frames.len())),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for FramesViewElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
