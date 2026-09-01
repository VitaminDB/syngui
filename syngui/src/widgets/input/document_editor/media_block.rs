//! Медиа-блоки документа с живыми плеерами (feature `ffmpeg`).
//!
//! Видео: постер (кадр от резолвера) с кнопкой ▶; клик открывает
//! `VideoPlayer`, кадры уходят в ImageStore (паттерн VideoView), снизу —
//! полоса управления (пауза, прогресс с перемоткой, время). Аудио:
//! карточка с волной из PCM-бинов резолвера, кнопкой ▶ и перемоткой по
//! клику в волну; воспроизведение — тем же `VideoPlayer` (libavformat
//! играет и чисто аудио-файлы). Без feature `ffmpeg` build.rs даёт
//! карточку-плейсхолдер.

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use crate::core::canvas::CanvasContext;
use crate::core::sync::Mutex as SyncMutex;
use crate::core::{Color, Point, Rect, Size};
use crate::gpu::image_store::{ImageHandle, ImageSource, ImageStore};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{TextAlign, TextDecoration};
use crate::render::{DisplayList, TextureId};
use crate::video::{HwAccel, VideoPlayer};
use crate::widget::context::{EventContext, UpdateContext};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, Widget};

use super::links::DocMediaResolver;
use super::model::{BlockId, MediaKind};
use super::style::DocStyle;

const CONTROLS_H: f32 = 34.0;
const AUDIO_H: f32 = 64.0;

pub struct MediaBlock {
    pub block_id: BlockId,
    pub kind: MediaKind,
    pub url: String,
    pub alt: String,
    pub style: Arc<DocStyle>,
    pub media: Arc<dyn DocMediaResolver>,
}

impl Widget for MediaBlock {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MediaBlockElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            block_id: self.block_id,
            kind: self.kind,
            url: self.url.clone(),
            alt: self.alt.clone(),
            style: self.style.clone(),
            media: self.media.clone(),
            image_store: None,
            poster_handle: None,
            frame_handle: None,
            natural_size: (0, 0),
            player: None,
            position: 0.0,
            duration: 0.0,
            pcm: None,
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
}

pub struct MediaBlockElement {
    id: ElementId,
    bounds: Rect,
    dirty: DirtyFlags,
    pub block_id: BlockId,
    kind: MediaKind,
    url: String,
    alt: String,
    style: Arc<DocStyle>,
    media: Arc<dyn DocMediaResolver>,
    image_store: Option<Arc<SyncMutex<ImageStore>>>,
    poster_handle: Option<ImageHandle>,
    frame_handle: Option<ImageHandle>,
    natural_size: (u32, u32),
    /// Плеер создаётся первым кликом ▶ и живёт, пока жив элемент.
    player: Option<Arc<SyncMutex<VideoPlayer>>>,
    position: f32,
    duration: f32,
    pcm: Option<Vec<f32>>,
}

impl MediaBlockElement {
    fn activated(&self) -> bool {
        self.player.is_some()
    }

    fn is_playing(&self) -> bool {
        self.player
            .as_ref()
            .and_then(|p| p.lock().ok().map(|p| !p.is_paused()))
            .unwrap_or(false)
    }

    /// Ленивая инициализация плеера (первый клик ▶).
    fn activate(&mut self) {
        if self.player.is_some() {
            return;
        }
        let Some(resolved) = self.media.resolve(&self.url) else {
            log::warn!("media-block: не разрешился url {}", self.url);
            return;
        };
        let path = resolved.path.display().to_string();
        match VideoPlayer::open_with_hwaccel(&path, HwAccel::None) {
            Ok(mut p) => {
                self.duration = p.duration_sec() as f32;
                p.play();
                self.player = Some(Arc::new(SyncMutex::new(p)));
                self.mark_dirty(DirtyFlags::RENDER);
            }
            Err(e) => log::warn!("media-block: не открылся {path}: {e:?}"),
        }
    }

    fn toggle_play(&mut self) {
        if let Some(player) = &self.player {
            if let Ok(mut p) = player.lock() {
                if p.is_paused() {
                    p.play();
                } else {
                    p.pause();
                }
            }
        } else {
            self.activate();
        }
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn seek_fraction(&mut self, f: f32) {
        self.activate();
        if let Some(player) = &self.player {
            if let Ok(mut p) = player.lock() {
                let dur = p.duration_sec();
                if dur > 0.0 {
                    let _ = p.seek((f.clamp(0.0, 1.0) as f64) * dur);
                }
            }
        }
    }

    /// Зоны управления видео (полоса снизу).
    fn video_controls_rect(&self) -> Rect {
        Rect::new(
            Point::new(self.bounds.origin.x, self.bounds.origin.y + self.bounds.size.height - CONTROLS_H),
            Size::new(self.bounds.size.width, CONTROLS_H),
        )
    }

    fn progress_rect(&self) -> Rect {
        let c = self.video_controls_rect();
        Rect::new(
            Point::new(c.origin.x + 44.0, c.origin.y + c.size.height / 2.0 - 3.0),
            Size::new((c.size.width - 44.0 - 78.0).max(20.0), 6.0),
        )
    }

    // ─── Отрисовка ─────────────────────────────────────────────────────────

    fn draw_video(&self, list: &mut DisplayList) {
        let s = &self.style;
        let r = s.media_radius;
        list.push_clip_rounded(self.bounds, [r, r, r, r]);
        list.push_rect(self.bounds, Color::from_hex("#000000"), [0.0; 4]);

        // Кадр или постер, вписанный по contain.
        let handle = if self.activated() { self.frame_handle } else { self.poster_handle };
        if let Some(h) = handle {
            let rect = self.fit_rect();
            list.push_image(
                rect,
                TextureId(h.0),
                Rect::new(Point::zero(), Size::new(1.0, 1.0)),
                Color::WHITE,
            );
        }

        let cx = self.bounds.origin.x + self.bounds.size.width / 2.0;
        let cy = self.bounds.origin.y + self.bounds.size.height / 2.0;
        if !self.activated() {
            // Кружок с ▶ по центру.
            let mut c = CanvasContext::new(self.bounds.origin, self.bounds.size);
            let (lx, ly) = (cx - self.bounds.origin.x, cy - self.bounds.origin.y);
            c.set_color(Color::rgba(0.0, 0.0, 0.0, 0.55));
            c.fill_circle(lx, ly, 26.0);
            c.set_color(Color::rgba(1.0, 1.0, 1.0, 0.95));
            c.fill_polygon(&[(lx - 8.0, ly - 12.0), (lx - 8.0, ly + 12.0), (lx + 13.0, ly)]);
            c.flush(list);
        } else {
            // Полоса управления.
            let bar = self.video_controls_rect();
            list.push_rect(bar, Color::rgba(0.0, 0.0, 0.0, 0.55), [0.0; 4]);
            let mut c = CanvasContext::new(bar.origin, bar.size);
            c.set_color(Color::rgba(1.0, 1.0, 1.0, 0.92));
            let bcy = bar.size.height / 2.0;
            if self.is_playing() {
                c.fill_rounded_rect(16.0, bcy - 8.0, 4.5, 16.0, 1.5);
                c.fill_rounded_rect(24.0, bcy - 8.0, 4.5, 16.0, 1.5);
            } else {
                c.fill_polygon(&[(16.0, bcy - 9.0), (16.0, bcy + 9.0), (30.0, bcy)]);
            }
            c.flush(list);

            let progress = self.progress_rect();
            list.push_rect(progress, Color::rgba(1.0, 1.0, 1.0, 0.25), [3.0; 4]);
            if self.duration > 0.0 {
                let f = (self.position / self.duration).clamp(0.0, 1.0);
                let mut played = progress;
                played.size.width *= f;
                list.push_rect(played, s.caret_color, [3.0; 4]);
            }
            let time = format!("{} / {}", fmt_time(self.position), fmt_time(self.duration));
            list.push_text_styled_singleline(
                &time,
                Rect::new(
                    Point::new(progress.origin.x + progress.size.width + 8.0, bar.origin.y + 9.0),
                    Size::new(70.0, 16.0),
                ),
                Color::rgba(1.0, 1.0, 1.0, 0.9),
                11.0,
                TextAlign::DEFAULT,
                TextDecoration::None,
                400,
                None,
            );
        }
        list.pop_clip();
    }

    fn draw_audio(&self, list: &mut DisplayList) {
        let s = &self.style;
        let r = s.media_radius;
        list.push_rect(self.bounds, s.media_bg, [r, r, r, r]);
        let o = self.bounds.origin;
        let h = self.bounds.size.height;

        // Кнопка ▶/⏸ слева.
        let mut c = CanvasContext::new(o, self.bounds.size);
        let (bx, by) = (h / 2.0, h / 2.0);
        c.set_color(s.checkbox_check_color);
        c.fill_circle(bx, by, 16.0);
        c.set_color(Color::rgba(1.0, 1.0, 1.0, 0.95));
        if self.is_playing() {
            c.fill_rounded_rect(bx - 6.0, by - 7.0, 4.0, 14.0, 1.5);
            c.fill_rounded_rect(bx + 2.0, by - 7.0, 4.0, 14.0, 1.5);
        } else {
            c.fill_polygon(&[(bx - 5.0, by - 8.0), (bx - 5.0, by + 8.0), (bx + 8.0, by)]);
        }

        // Волна.
        let wave = self.wave_rect();
        let bins = 72usize;
        let bw = wave.size.width / bins as f32;
        let played_f = if self.duration > 0.0 {
            (self.position / self.duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        for i in 0..bins {
            let amp = self
                .pcm
                .as_ref()
                .and_then(|p| {
                    let idx = i * p.len() / bins.max(1);
                    p.get(idx).copied()
                })
                .unwrap_or(0.35);
            let bh = (amp.clamp(0.05, 1.0)) * (wave.size.height - 8.0);
            let x = wave.origin.x - o.x + i as f32 * bw;
            let y = wave.origin.y - o.y + (wave.size.height - bh) / 2.0;
            let played = (i as f32 + 0.5) / bins as f32 <= played_f;
            c.set_color(if played { s.checkbox_check_color } else { s.muted_color.with_alpha(0.5) });
            c.fill_rounded_rect(x + 1.0, y, (bw - 2.0).max(1.5), bh, 1.0);
        }
        c.flush(list);

        // Название и время.
        list.push_text_styled_singleline(
            &self.alt,
            Rect::new(
                Point::new(o.x + h + 2.0, o.y + 6.0),
                Size::new((self.bounds.size.width - h - 80.0).max(20.0), 14.0),
            ),
            s.text_color,
            (s.text_size - 2.0).max(10.0),
            TextAlign::DEFAULT,
            TextDecoration::None,
            500,
            None,
        );
        let time = format!("{} / {}", fmt_time(self.position), fmt_time(self.duration));
        list.push_text_styled_singleline(
            &time,
            Rect::new(
                Point::new(o.x + self.bounds.size.width - 84.0, o.y + 6.0),
                Size::new(76.0, 14.0),
            ),
            s.muted_color,
            10.5,
            TextAlign::RIGHT,
            TextDecoration::None,
            400,
            None,
        );
    }

    fn wave_rect(&self) -> Rect {
        let o = self.bounds.origin;
        let h = self.bounds.size.height;
        Rect::new(
            Point::new(o.x + h + 2.0, o.y + 24.0),
            Size::new((self.bounds.size.width - h - 14.0).max(20.0), h - 30.0),
        )
    }

    fn fit_rect(&self) -> Rect {
        let (nw, nh) = self.natural_size;
        if nw == 0 || nh == 0 {
            return self.bounds;
        }
        let (nw, nh) = (nw as f32, nh as f32);
        let bw = self.bounds.size.width;
        let bh = self.bounds.size.height;
        let scale = (bw / nw).min(bh / nh);
        let (sw, sh) = (nw * scale, nh * scale);
        Rect::new(
            Point::new(self.bounds.origin.x + (bw - sw) / 2.0, self.bounds.origin.y + (bh - sh) / 2.0),
            Size::new(sw, sh),
        )
    }
}

impl Element for MediaBlockElement {
    fn update(&mut self, widget: &dyn Widget, ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<MediaBlock>() else { return };
        if self.url != w.url || self.kind != w.kind {
            self.url = w.url.clone();
            self.kind = w.kind;
            self.player = None;
            self.pcm = None;
            self.position = 0.0;
            self.duration = 0.0;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            ctx.mark_layout_dirty();
        }
        self.alt = w.alt.clone();
        self.style = w.style.clone();
        self.media = w.media.clone();
        self.block_id = w.block_id;
    }

    fn mount(&mut self, tree: &mut ElementTree) {
        self.image_store = tree.image_store.clone();
        // Постер видео — сразу; PCM-волна аудио — сразу (дёшево).
        match self.kind {
            MediaKind::Video => {
                if let (Some(store), Some(poster)) =
                    (self.image_store.as_ref(), self.media.poster(&self.url))
                {
                    if let Ok(mut s) = store.lock() {
                        let (h, _) =
                            s.request(&ImageSource::Path(poster.display().to_string()));
                        self.poster_handle = Some(h);
                    }
                }
                if let Some(store) = self.image_store.as_ref() {
                    if let Ok(mut s) = store.lock() {
                        let key = format!("doc-media:{}", self.url);
                        let (h, _) = s.request_rgba(&key, 2, 2, vec![0u8; 16]);
                        self.frame_handle = Some(h);
                    }
                }
            }
            MediaKind::Audio => {
                self.pcm = self.media.pcm_bins(&self.url, 96);
            }
            _ => {}
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let avail = if constraints.max_width.is_finite() { constraints.max_width } else { 600.0 };
        let width = match self.style.max_content_width {
            Some(cap) => avail.min(cap),
            None => avail,
        };
        let height = match self.kind {
            MediaKind::Audio => AUDIO_H,
            _ => {
                let (nw, nh) = self.natural_size;
                if nw > 0 && nh > 0 {
                    (width * nh as f32 / nw as f32).min(520.0)
                } else {
                    width * 9.0 / 16.0
                }
            }
        };
        self.bounds.size = Size::new(width, height);
        self.bounds.size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        match self.kind {
            MediaKind::Audio => self.draw_audio(list),
            _ => self.draw_video(list),
        }
    }

    fn handle_event(&mut self, event: &Event, _ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseDown { button: MouseButton::Left, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                match self.kind {
                    MediaKind::Audio => {
                        let wave = self.wave_rect();
                        if wave.contains(*position) && self.duration > 0.0 {
                            let f = (position.x - wave.origin.x) / wave.size.width;
                            self.seek_fraction(f);
                        } else {
                            self.toggle_play();
                        }
                    }
                    _ => {
                        if self.activated() {
                            let progress = self.progress_rect();
                            let controls = self.video_controls_rect();
                            if progress.contains(*position) {
                                let f =
                                    (position.x - progress.origin.x) / progress.size.width;
                                self.seek_fraction(f);
                            } else if controls.contains(*position) {
                                self.toggle_play();
                            } else {
                                self.toggle_play();
                            }
                        } else {
                            self.activate();
                        }
                    }
                }
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        let Some(player) = self.player.clone() else { return false };
        let (frame, pos, dur, paused) = match player.lock() {
            Ok(mut p) => (
                p.poll_frame(),
                p.position_sec() as f32,
                p.duration_sec() as f32,
                p.is_paused(),
            ),
            Err(_) => return false,
        };
        if let Some(frame) = frame {
            if let (Some(h), Some(store)) = (self.frame_handle, self.image_store.as_ref()) {
                if let Ok(mut s) = store.lock() {
                    s.update_rgba(h, frame.width, frame.height, frame.rgba.to_vec());
                }
            }
            let new_size = (frame.width, frame.height);
            if new_size != self.natural_size {
                self.natural_size = new_size;
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            } else {
                self.mark_dirty(DirtyFlags::RENDER);
            }
        }
        if (pos - self.position).abs() > 0.05 || (dur - self.duration).abs() > 0.05 {
            self.position = pos;
            self.duration = dur;
            self.mark_dirty(DirtyFlags::RENDER);
        }
        !paused
    }

    fn wants_animate_tick(&self) -> bool {
        self.activated()
    }

    fn element_type_name(&self) -> &str {
        "doc-media-player"
    }

    fn id(&self) -> ElementId {
        self.id
    }
    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }
    fn children(&self) -> &[ElementId] {
        &[]
    }
    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty |= flags;
    }
    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.remove(flags);
    }
    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty.contains(flags)
    }
}

fn fmt_time(sec: f32) -> String {
    let total = sec.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}
