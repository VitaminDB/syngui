pub mod controls;

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::gpu::image_store::{ImageHandle, ImageStore};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{DisplayList, TextureId};
use crate::signal::RwSignal;
use crate::video::VideoPlayer;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use crate::widgets::ImageFit;

pub use controls::video_player_view;

pub struct VideoView {
    player: Arc<Mutex<VideoPlayer>>,
    fit: ImageFit,
    classes: Vec<String>,
    position_signal: Option<RwSignal<f32>>,
}

impl VideoView {
    pub fn new(player: Arc<Mutex<VideoPlayer>>) -> Self {
        Self {
            player,
            fit: ImageFit::Contain,
            classes: Vec::new(),
            position_signal: None,
        }
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, c.into());
        self
    }

    pub fn position_signal(mut self, sig: RwSignal<f32>) -> Self {
        self.position_signal = Some(sig);
        self
    }
}

impl Widget for VideoView {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(VideoViewElement {
            id: ElementId::new(),
            player: self.player.clone(),
            fit: self.fit,
            classes: self.classes.clone(),
            bounds: Rect::zero(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            image_handle: None,
            image_store: None,
            natural_size: (0, 0),
            mss: MssFields::new(),
            position_signal: self.position_signal,
            surface_mode: false,
            surface_rect_sent: std::cell::Cell::new(None),
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

pub struct VideoViewElement {
    id: ElementId,
    player: Arc<Mutex<VideoPlayer>>,
    fit: ImageFit,
    classes: Vec<String>,
    bounds: Rect,
    dirty_flags: DirtyFlags,
    image_handle: Option<ImageHandle>,
    image_store: Option<Arc<Mutex<ImageStore>>>,
    natural_size: (u32, u32),
    mss: MssFields,
    position_signal: Option<RwSignal<f32>>,
    /// Кадры показывает сам кодек на системном Surface под окном
    /// (`HwAccel::MediaCodecSurface`): элемент ничего не рисует — оставляет
    /// прозрачную «дырку» — и сообщает платформе, где расположить видео.
    surface_mode: bool,
    /// Последний переданный платформе прямоугольник (доли окна).
    surface_rect_sent: std::cell::Cell<Option<[u32; 4]>>,
}

impl Drop for VideoViewElement {
    fn drop(&mut self) {
        if self.surface_mode {
            #[cfg(all(target_os = "android", feature = "ffmpeg"))]
            crate::video::android::set_surface_video_active(false);
        }
    }
}

impl VideoViewElement {
    fn key(&self) -> String {
        format!("video:{:p}", Arc::as_ptr(&self.player))
    }

    /// Текстура под кадры текущего плеера (натуральный размер из метаданных).
    fn bind_player(&mut self) {
        let (w, h) = if let Ok(p) = self.player.lock() {
            let m = p.meta();
            (m.width.max(1), m.height.max(1))
        } else {
            (1, 1)
        };
        self.natural_size = (w, h);
        let starter = vec![0u8; (w as usize) * (h as usize) * 4];
        if let Some(store) = self.image_store.as_ref() {
            if let Ok(mut s) = store.lock() {
                let key = self.key();
                let (handle, _) = s.request_rgba(&key, w, h, starter);
                self.image_handle = Some(handle);
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

impl Element for VideoViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(v) = widget.as_any().downcast_ref::<VideoView>() {
            self.fit = v.fit;
            self.classes = v.classes.clone();
            self.position_signal = v.position_signal;
            // Виджет пересобрали с другим плеером (переоткрытие после
            // сворачивания, смена серии): старый Arc отпускаем, иначе его
            // декодер и аудио живут дальше и грузят CPU.
            if !Arc::ptr_eq(&self.player, &v.player) {
                self.player = v.player.clone();
                if self.surface_mode {
                    self.surface_mode = false;
                    #[cfg(all(target_os = "android", feature = "ffmpeg"))]
                    crate::video::android::set_surface_video_active(false);
                }
                self.surface_rect_sent.set(None);
                self.bind_player();
                self.mark_dirty(DirtyFlags::LAYOUT);
            }
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
        if self.surface_mode {
            // Дырка под системный Surface: ничего не рисуем, только
            // сообщаем платформе положение видео в долях окна.
            let fit = self.compute_fit_rect();
            let vp = crate::viewport::viewport_size().get_untracked();
            if vp.width > 0.0 && vp.height > 0.0 {
                let origin = crate::viewport::viewport_origin();
                let x = (fit.x() + origin.x) / vp.width;
                let y = (fit.y() + origin.y) / vp.height;
                let w = fit.size.width / vp.width;
                let h = fit.size.height / vp.height;
                // Квантуем до 1/4096, чтобы не дёргать JNI на каждом кадре.
                let q = |v: f32| (v.clamp(0.0, 1.0) * 4096.0).round() as u32;
                let key = [q(x), q(y), q(w), q(h)];
                if self.surface_rect_sent.get() != Some(key) {
                    self.surface_rect_sent.set(Some(key));
                    #[cfg(all(target_os = "android", feature = "ffmpeg"))]
                    crate::video::android::set_video_rect(x, y, w, h);
                }
            }
            return;
        }
        if let Some(handle) = self.image_handle {
            let bg = self
                .mss
                .background_color
                .unwrap_or_else(|| Color::from_hex("#000000"));
            list.push_rect(self.bounds, bg, [0.0; 4]);
            let fit_rect = self.compute_fit_rect();
            let uv_rect = Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0));
            list.push_image(fit_rect, TextureId(handle.0), uv_rect, Color::WHITE);
        } else {
            let bg = Color::from_hex("#000000");
            list.push_rect(self.bounds, bg, [0.0; 4]);
        }
    }

    fn handle_event(
        &mut self,
        _event: &Event,
        _ctx: &mut crate::widget::context::EventContext,
    ) -> EventResult {
        EventResult::Ignored
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        // `ticking` — на паузе тоже, пока ждём кадр после перемотки
        // (`VideoPlayer::wants_seek_preview`): иначе он придёт, а кадр UI,
        // который его покажет, — нет.
        let (frame_opt, pos_sec, ticking) = if let Ok(mut p) = self.player.lock() {
            let frame = p.poll_frame();
            let ticking = !p.is_paused() || p.wants_seek_preview();
            (frame, p.position_sec() as f32, ticking)
        } else {
            (None, 0.0, true)
        };
        let mut changed = false;
        if let Some(frame) = frame_opt {
            let new_size = (frame.width, frame.height);
            if frame.surface.is_some() {
                if !self.surface_mode {
                    self.surface_mode = true;
                    changed = true;
                    #[cfg(all(target_os = "android", feature = "ffmpeg"))]
                    crate::video::android::set_surface_video_active(true);
                    self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
                }
            } else if let (Some(handle), Some(store)) =
                (self.image_handle, self.image_store.as_ref())
            {
                changed = true;
                if let Ok(mut s) = store.lock() {
                    s.update_rgba(handle, frame.width, frame.height, frame.rgba);
                }
            }
            if new_size != self.natural_size {
                self.natural_size = new_size;
                changed = true;
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            } else if changed {
                self.mark_dirty(DirtyFlags::RENDER);
            }
        }
        if let Some(sig) = self.position_signal {
            if (sig.get_untracked() - pos_sec).abs() > 0.05 {
                sig.set(pos_sec);
            }
        }
        if self.surface_mode {
            // Кадры рисует кодек: UI не перерисовываем, но продолжаем тикать,
            // чтобы планировать показ следующих кадров.
            if ticking {
                crate::app::request_tick(Duration::from_millis(40));
            }
            return changed;
        }
        ticking
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
        self.bind_player();
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "VideoView"
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
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
        let label = if let Ok(p) = self.player.lock() {
            crate::i18n::builtin_args(
                "video.a11y",
                "Video {w}×{h}",
                &[("w", &p.meta().width), ("h", &p.meta().height)],
            )
        } else {
            crate::i18n::builtin("video.a11y_short", "Video")
        };
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Image,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(label),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for VideoViewElement {
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
