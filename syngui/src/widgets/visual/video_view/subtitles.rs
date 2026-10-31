//! Слой субтитров поверх кадра: берёт у плеера видимые события
//! ([`SubtitleStore::active`]), рисует текст в картинки в своём потоке
//! заранее (пока событие только приближается), раскладывает их по кадру и
//! анимирует появление и исчезновение. Пока видна панель управления,
//! нижние субтитры плавно поднимаются над ней ([`TopProbe`]).

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size, Transform};
use crate::gpu::image_store::{ImageHandle, ImageStore};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::render::{DisplayList, TextureId};
use crate::signal::RwSignal;
use crate::video::subtitle::render::{with_renderer, Rendered};
use crate::video::subtitle::{SubContent, SubText, SubtitleAnimation, SubtitleEvent, SubtitleStore, SubtitleStyle};
use crate::video::VideoPlayer;
use crate::widget::context::EventContext;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, UpdateContext, Widget};
use crate::widgets::ImageFit;

/// Насколько заранее рисовать картинку события, с.
const PREFETCH_SEC: f64 = 1.5;
/// Картинка не нужна столько кадров подряд — выгрузить.
const EVICT_AFTER_FRAMES: u64 = 240;
/// Зазор между субтитрами и панелью управления, px.
const CONTROLS_GAP: f32 = 10.0;

/// «Верх панели управления» — y верхнего края в координатах окна; NaN —
/// панели нет. Пишет [`TopProbe`], читает слой субтитров.
pub type ControlsTop = Arc<AtomicU32>;

pub fn controls_top_slot() -> ControlsTop {
    Arc::new(AtomicU32::new(f32::NAN.to_bits()))
}

/// Субтитры поверх кадра плеера `player`. `fit` — как кадр ложится в
/// площадь (как у `VideoView`), `style` — оформление.
pub struct SubtitleLayer {
    player: Arc<Mutex<VideoPlayer>>,
    fit: ImageFit,
    style: RwSignal<SubtitleStyle>,
    controls_top: Option<ControlsTop>,
}

impl SubtitleLayer {
    pub fn new(player: Arc<Mutex<VideoPlayer>>, style: RwSignal<SubtitleStyle>) -> Self {
        Self { player, fit: ImageFit::Contain, style, controls_top: None }
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    /// Поднимать нижние субтитры над панелью, пока она видна.
    pub fn controls_top(mut self, slot: ControlsTop) -> Self {
        self.controls_top = Some(slot);
        self
    }
}

impl Widget for SubtitleLayer {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(SubtitleLayerElement {
            id: ElementId::new(),
            player: self.player.clone(),
            store: self.player.lock().map(|p| p.subtitles()).ok(),
            fit: self.fit,
            style: self.style,
            controls_top: self.controls_top.clone(),
            bounds: Rect::zero(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            image_store: None,
            worker: Worker::spawn(),
            uploaded: HashMap::new(),
            items: Vec::new(),
            scale: std::cell::Cell::new(initial_scale()),
            frame: 0,
            lift: 0.0,
            signature: 0,
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

fn initial_scale() -> f32 {
    #[cfg(feature = "winit")]
    {
        crate::signal::primary_window().map(|w| w.scale_factor() as f32).unwrap_or(1.0).clamp(0.5, 8.0)
    }
    #[cfg(not(feature = "winit"))]
    1.0
}

// ─── поток отрисовки ────────────────────────────────────────────────────────

struct Job {
    key: u64,
    text: SubText,
    style: SubtitleStyle,
    font_px: f32,
    max_w: f32,
}

type Done = Arc<std::sync::Mutex<HashMap<u64, Option<Rendered>>>>;

struct Worker {
    tx: mpsc::Sender<Job>,
    done: Done,
    pending: HashSet<u64>,
}

impl Worker {
    fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let done: Done = Arc::default();
        let out = done.clone();
        let _ = std::thread::Builder::new().name("syngui-subtitles".into()).spawn(move || {
            // Канал закрыт (элемент удалён) — поток выходит.
            for job in rx {
                let r = with_renderer(|r| r.render(&job.text, &job.style, job.font_px, job.max_w));
                if let Ok(mut d) = out.lock() {
                    d.insert(job.key, r);
                }
            }
        });
        Self { tx, done, pending: HashSet::new() }
    }

    fn request(&mut self, job: Job) {
        if self.pending.insert(job.key) {
            let _ = self.tx.send(job);
        }
    }

    fn take(&mut self, key: u64) -> Option<Option<Rendered>> {
        let r = self.done.lock().ok()?.remove(&key)?;
        self.pending.remove(&key);
        Some(r)
    }
}

// ─── элемент ────────────────────────────────────────────────────────────────

struct Uploaded {
    handle: Option<ImageHandle>,
    /// Размер картинки, физ. px.
    w: f32,
    h: f32,
    /// Прямоугольник текста в картинке, физ. px.
    text: [f32; 4],
    used: u64,
}

/// Что нарисовать в кадре.
#[derive(Clone)]
struct DrawItem {
    handle: ImageHandle,
    rect: Rect,
    opacity: f32,
    scale: f32,
}

struct SubtitleLayerElement {
    id: ElementId,
    player: Arc<Mutex<VideoPlayer>>,
    store: Option<Arc<SubtitleStore>>,
    fit: ImageFit,
    style: RwSignal<SubtitleStyle>,
    controls_top: Option<ControlsTop>,
    bounds: Rect,
    dirty: DirtyFlags,
    image_store: Option<Arc<Mutex<ImageStore>>>,
    worker: Worker,
    uploaded: HashMap<u64, Uploaded>,
    items: Vec<DrawItem>,
    scale: std::cell::Cell<f32>,
    frame: u64,
    /// Текущий подъём над панелью, px (плавно догоняет нужный).
    lift: f32,
    signature: u64,
}

impl Drop for SubtitleLayerElement {
    fn drop(&mut self) {
        if let Some(store) = self.image_store.as_ref() {
            if let Ok(mut s) = store.lock() {
                for u in self.uploaded.values() {
                    if let Some(h) = u.handle {
                        s.release(h);
                    }
                }
            }
        }
    }
}

fn ease_out(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

/// Группа выравнивания: 0 — снизу, 1 — посередине, 2 — сверху.
fn row_of(align: u8) -> u8 {
    match align {
        7..=9 => 2,
        4..=6 => 1,
        _ => 0,
    }
}

fn h_frac(align: u8) -> f32 {
    match align {
        1 | 4 | 7 => 0.0,
        3 | 6 | 9 => 1.0,
        _ => 0.5,
    }
}

impl SubtitleLayerElement {
    /// Кадр на экране (как у `VideoView`) и видимая его часть.
    fn video_rect(&self) -> Rect {
        let (w, h, rot) = self.player.lock().map(|p| (p.meta().width, p.meta().height, p.meta().rotation)).unwrap_or((0, 0, 0));
        let (nw, nh) = if rot % 180 == 90 { (h as f32, w as f32) } else { (w as f32, h as f32) };
        let b = self.bounds;
        if nw <= 0.0 || nh <= 0.0 {
            return b;
        }
        let (bw, bh) = (b.size.width, b.size.height);
        let s = match self.fit {
            ImageFit::Fill => return b,
            ImageFit::None => 1.0,
            ImageFit::Contain => (bw / nw).min(bh / nh),
            ImageFit::Cover => (bw / nw).max(bh / nh),
        };
        let (sw, sh) = (nw * s, nh * s);
        Rect::new(Point::new(b.x() + (bw - sw) / 2.0, b.y() + (bh - sh) / 2.0), Size::new(sw, sh))
    }

    fn key_of(ev: &SubtitleEvent, style: &SubtitleStyle, font_px: f32, max_w: f32) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ev.id.hash(&mut h);
        style.raster_key().hash(&mut h);
        (font_px * 4.0).round().to_bits().hash(&mut h);
        max_w.round().to_bits().hash(&mut h);
        h.finish()
    }

    /// Готова ли картинка `key`: загрузить в текстуру, если пришла из потока.
    fn ensure_uploaded(&mut self, key: u64) -> bool {
        if let Some(u) = self.uploaded.get_mut(&key) {
            u.used = self.frame;
            return true;
        }
        let Some(r) = self.worker.take(key) else { return false };
        let handle = match (&r, self.image_store.as_ref()) {
            (Some(img), Some(store)) => store.lock().ok().map(|mut s| s.request_rgba(&format!("subs:{}:{key:x}", self.id.0), img.w, img.h, img.rgba.to_vec()).0),
            _ => None,
        };
        let (w, h, text) = r.as_ref().map(|i| (i.w as f32, i.h as f32, i.text)).unwrap_or((0.0, 0.0, [0.0; 4]));
        self.uploaded.insert(key, Uploaded { handle, w, h, text, used: self.frame });
        true
    }

    /// Картинки PGS/VobSub: прямо в текстуры (рисовать нечего).
    fn ensure_bitmap(&mut self, key: u64, b: &crate::video::subtitle::SubBitmap) {
        if let Some(u) = self.uploaded.get_mut(&key) {
            u.used = self.frame;
            return;
        }
        let handle = self
            .image_store
            .as_ref()
            .and_then(|s| s.lock().ok().map(|mut s| s.request_rgba(&format!("subs:{}:{key:x}", self.id.0), b.w, b.h, b.rgba.to_vec()).0));
        self.uploaded.insert(key, Uploaded { handle, w: b.w as f32, h: b.h as f32, text: [0.0, 0.0, b.w as f32, b.h as f32], used: self.frame });
    }

    fn evict(&mut self) {
        let frame = self.frame;
        let old: Vec<u64> = self.uploaded.iter().filter(|(_, u)| frame.saturating_sub(u.used) > EVICT_AFTER_FRAMES).map(|(k, _)| *k).collect();
        if old.is_empty() {
            return;
        }
        let store = self.image_store.clone();
        let mut s = store.as_ref().and_then(|s| s.lock().ok());
        for k in old {
            if let Some(u) = self.uploaded.remove(&k) {
                if let (Some(h), Some(s)) = (u.handle, s.as_mut()) {
                    s.release(h);
                }
            }
        }
    }

    /// Пересчитать, что и где рисовать. `true` — картинка изменилась или
    /// ещё меняется (анимация, ждём отрисовку).
    fn update_items(&mut self, dt: Duration) -> bool {
        self.frame += 1;
        let Some(store) = self.store.clone() else { return false };
        let style = self.style.get_untracked();
        let clock = match self.player.lock() {
            Ok(p) => p.position_sec(),
            Err(_) => return false,
        };
        let out_sec = if style.anim_out == SubtitleAnimation::None { 0.0 } else { style.anim_out_ms as f64 / 1000.0 };
        let in_sec = if style.anim_in == SubtitleAnimation::None { 0.0 } else { style.anim_in_ms as f64 / 1000.0 };
        let act = store.active(clock, PREFETCH_SEC, out_sec + 0.05);
        let t = act.time;
        let scale = self.scale.get().max(0.5);

        let video = self.video_rect();
        let visible_video = video.intersection(&self.bounds).unwrap_or(self.bounds);
        let area = if style.in_frame { visible_video } else { self.bounds };
        let font_px = (style.size * area.size.height).max(style.min_size_px);
        let max_w = (style.max_width * area.size.width).max(font_px * 4.0);

        // Картинки событий: готовые — в текстуры, остальные — в поток.
        let mut pending = false;
        for ev in act.events.iter().chain(act.upcoming.iter()) {
            match &*ev.content {
                SubContent::Text(text) => {
                    let key = Self::key_of(ev, &style, font_px * scale, max_w * scale);
                    if !self.ensure_uploaded(key) {
                        self.worker.request(Job { key, text: text.clone(), style: style.clone(), font_px: font_px * scale, max_w: max_w * scale });
                        pending |= ev.start <= t;
                    }
                }
                SubContent::Bitmap { rects, .. } => {
                    for (i, b) in rects.iter().enumerate() {
                        self.ensure_bitmap(ev.id.wrapping_mul(64).wrapping_add(i as u64) ^ 0xB17_0000_0000, b);
                    }
                }
            }
        }

        // Подъём над панелью управления.
        let natural_bottom = area.y() + area.size.height - style.margin * area.size.height;
        let limit = self
            .controls_top
            .as_ref()
            .map(|s| f32::from_bits(s.load(Ordering::Relaxed)))
            .filter(|v| v.is_finite())
            .map(|top| top - CONTROLS_GAP);
        let target_lift = limit.map(|l| (natural_bottom - l).max(0.0)).unwrap_or(0.0);
        let k = 1.0 - (-dt.as_secs_f32().min(0.1) * 14.0).exp();
        let prev_lift = self.lift;
        self.lift += (target_lift - self.lift) * if dt.is_zero() { 1.0 } else { k };
        if (self.lift - target_lift).abs() < 0.3 {
            self.lift = target_lift;
        }
        let mut animating = (self.lift - prev_lift).abs() > 0.01;

        // Видимые события по группам.
        let mut items: Vec<DrawItem> = Vec::new();
        let mut events: Vec<&SubtitleEvent> = act.events.iter().collect();
        events.sort_by(|a, b| {
            let la = if let SubContent::Text(t) = &*a.content { t.layer } else { 0 };
            let lb = if let SubContent::Text(t) = &*b.content { t.layer } else { 0 };
            la.cmp(&lb).then(a.start.total_cmp(&b.start)).then(a.id.cmp(&b.id))
        });
        // Следующее событие той же группы начинается сразу — уходящее не
        // анимировать (иначе две строки наползают друг на друга).
        let group_of = |e: &SubtitleEvent| match &*e.content {
            SubContent::Text(t) if t.pos.is_none() || !style.file_positions => Some(row_of(t.align)),
            _ => None,
        };
        let all: Vec<&SubtitleEvent> = act.events.iter().chain(act.upcoming.iter()).collect();
        let followed = |e: &SubtitleEvent| {
            let g = group_of(e);
            all.iter().any(|o| o.id != e.id && group_of(o) == g && o.start >= e.end - 0.05 && o.start <= e.end + out_sec)
        };
        // Сколько уже занято снизу/сверху/в середине, px.
        let mut stack = [0.0f32; 3];
        for ev in events {
            let in_p = if in_sec > 0.0 { ((t - ev.start) / in_sec) as f32 } else { 1.0 };
            let out_p = if t <= ev.end {
                0.0
            } else if out_sec > 0.0 && !followed(ev) {
                ((t - ev.end) / out_sec) as f32
            } else {
                1.0
            };
            if out_p >= 1.0 {
                continue;
            }
            if in_p < 1.0 || out_p > 0.0 {
                animating = true;
            }
            let (ei, eo) = (ease_out(in_p), ease_out(out_p));
            let a_in = if style.anim_in == SubtitleAnimation::None { 1.0 } else { ei };
            let a_out = if style.anim_out == SubtitleAnimation::None { 0.0 } else { eo };
            let opacity = a_in * (1.0 - a_out);
            let mut dy = 0.0f32;
            let mut sc = 1.0f32;
            match style.anim_in {
                SubtitleAnimation::Rise => dy += (1.0 - ei) * 0.35 * font_px,
                SubtitleAnimation::Zoom => sc *= 0.86 + 0.14 * ei,
                _ => {}
            }
            match style.anim_out {
                SubtitleAnimation::Rise => dy -= eo * 0.22 * font_px,
                SubtitleAnimation::Zoom => sc *= 1.0 - 0.06 * eo,
                _ => {}
            }

            match &*ev.content {
                SubContent::Text(text) => {
                    let key = Self::key_of(ev, &style, font_px * scale, max_w * scale);
                    let Some(u) = self.uploaded.get(&key) else { continue };
                    let Some(handle) = u.handle else { continue };
                    let (iw, ih) = (u.w / scale, u.h / scale);
                    let [tx, ty, tw, th] = u.text.map(|v| v / scale);
                    // Где встаёт прямоугольник текста.
                    let (text_x, text_y) = match text.pos.filter(|_| style.file_positions) {
                        Some((px, py)) => {
                            let ax = video.x() + px * video.size.width;
                            let ay = video.y() + py * video.size.height;
                            let vy = match row_of(text.align) {
                                2 => 0.0,
                                1 => 0.5,
                                _ => 1.0,
                            };
                            (ax - tw * h_frac(text.align), ay - th * vy)
                        }
                        None => {
                            let margin_x = area.size.width * (1.0 - style.max_width).max(0.0) / 2.0;
                            let x = match h_frac(text.align) {
                                f if f < 0.25 => area.x() + margin_x,
                                f if f > 0.75 => area.x() + area.size.width - margin_x - tw,
                                _ => area.x() + (area.size.width - tw) / 2.0,
                            };
                            let row = row_of(text.align) as usize;
                            let margin_y = style.margin * area.size.height;
                            let y = match row {
                                2 => area.y() + margin_y + stack[2],
                                1 => area.y() + (area.size.height - th) / 2.0 + stack[1],
                                _ => natural_bottom - self.lift - stack[0] - th,
                            };
                            // следующее событие группы — выше (снизу) или ниже
                            stack[row] += th + font_px * 0.25;
                            (x, y)
                        }
                    };
                    // В окно целиком.
                    let b = self.bounds;
                    let text_x = text_x.clamp(b.x() - tx, (b.x() + b.size.width - tw - tx).max(b.x() - tx));
                    let text_y = text_y.clamp(b.y(), (b.y() + b.size.height - th).max(b.y()));
                    let rect = Rect::new(Point::new(text_x - tx, text_y - ty + dy), Size::new(iw, ih));
                    items.push(DrawItem { handle, rect, opacity, scale: sc });
                }
                SubContent::Bitmap { rects, canvas } => {
                    let (cw, ch) = (canvas.0.max(1) as f32, canvas.1.max(1) as f32);
                    let (sx, sy) = (video.size.width / cw, video.size.height / ch);
                    for (i, b) in rects.iter().enumerate() {
                        let key = ev.id.wrapping_mul(64).wrapping_add(i as u64) ^ 0xB17_0000_0000;
                        let Some(handle) = self.uploaded.get(&key).and_then(|u| u.handle) else { continue };
                        let rect = Rect::new(
                            Point::new(video.x() + b.x as f32 * sx, video.y() + b.y as f32 * sy - self.lift + dy),
                            Size::new(b.w as f32 * sx, b.h as f32 * sy),
                        );
                        items.push(DrawItem { handle, rect, opacity: opacity * style.bitmap_opacity, scale: sc });
                    }
                }
            }
        }
        self.evict();

        // Изменилось ли что-то на экране.
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for it in &items {
            it.handle.0.hash(&mut h);
            for v in [it.rect.x(), it.rect.y(), it.rect.size.width, it.rect.size.height, it.opacity * 255.0, it.scale * 1000.0] {
                (v.round() as i32).hash(&mut h);
            }
        }
        let sig = h.finish();
        let changed = sig != self.signature;
        self.signature = sig;
        self.items = items;
        if changed {
            self.mark_dirty(DirtyFlags::RENDER);
        }
        changed || animating || pending
    }
}

impl Element for SubtitleLayerElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<SubtitleLayer>() {
            self.fit = w.fit;
            self.style = w.style;
            self.controls_top = w.controls_top.clone();
            if !Arc::ptr_eq(&self.player, &w.player) {
                self.player = w.player.clone();
                self.store = self.player.lock().map(|p| p.subtitles()).ok();
            }
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let s = Size::new(constraints.max_width, constraints.max_height);
        self.bounds.size = s;
        s
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let sf = list.scale_factor();
        if sf > 0.0 {
            self.scale.set(sf);
        }
        let uv = Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0));
        for it in &self.items {
            if it.opacity <= 0.003 {
                continue;
            }
            list.push_opacity(it.opacity.min(1.0));
            let scaled = (it.scale - 1.0).abs() > 0.001;
            if scaled {
                let c = it.rect.center();
                list.push_transform(
                    Transform::translation(-c.x, -c.y).then_scale(it.scale, it.scale).then_translate(euclid::Vector2D::new(c.x, c.y)),
                );
            }
            list.push_image(it.rect, TextureId(it.handle.0), uv, Color::WHITE);
            if scaled {
                list.pop_transform();
            }
            list.pop_opacity();
        }
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn hit_test(&self, _point: Point) -> bool {
        false
    }

    fn animate(&mut self, dt: Duration) -> bool {
        self.update_items(dt)
    }

    fn wants_animate_tick(&self) -> bool {
        true
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
        self.dirty |= flags;
    }

    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.remove(flags);
    }

    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty.contains(flags)
    }

    fn id(&self) -> ElementId {
        self.id
    }

    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }

    fn mount(&mut self, tree: &mut ElementTree) {
        self.image_store = tree.image_store.clone();
    }

    fn element_type_name(&self) -> &str {
        "SubtitleLayer"
    }
}

// ─── верх панели ────────────────────────────────────────────────────────────

/// Обёртка, которая сообщает, где верхний край её содержимого (y в окне):
/// по нему слой субтитров поднимается над панелью управления. Убрали
/// обёртку (панель спряталась) — в слот пишется NaN.
pub struct TopProbe {
    slot: ControlsTop,
    child: Box<dyn Widget>,
}

impl TopProbe {
    pub fn new(slot: ControlsTop, child: impl Widget + 'static) -> Self {
        Self { slot, child: Box::new(child) }
    }
}

impl Widget for TopProbe {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(TopProbeElement { id: ElementId::new(), slot: self.slot.clone(), bounds: Rect::zero(), dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER })
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

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        let element = self.child.create_element();
        let child_id = tree.insert_with_type_id(element, Some(parent_id), self.child.as_any().type_id());
        self.child.mount(tree, child_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![self.child.as_ref() as &dyn Widget]
    }
}

struct TopProbeElement {
    id: ElementId,
    slot: ControlsTop,
    bounds: Rect,
    dirty: DirtyFlags,
}

impl Drop for TopProbeElement {
    fn drop(&mut self) {
        self.slot.store(f32::NAN.to_bits(), Ordering::Relaxed);
    }
}

impl Element for TopProbeElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<TopProbe>() {
            if !Arc::ptr_eq(&self.slot, &w.slot) {
                self.slot.store(f32::NAN.to_bits(), Ordering::Relaxed);
                self.slot = w.slot.clone();
            }
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        Size::new(constraints.max_width, constraints.max_height)
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn passthrough_hit_test(&self) -> bool {
        true
    }

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, origin: Point) {
        self.bounds.origin = origin;
        self.slot.store(origin.y.to_bits(), Ordering::Relaxed);
    }

    fn id(&self) -> ElementId {
        self.id
    }

    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }

    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty |= flags;
    }

    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.remove(flags);
    }

    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty.intersects(flags)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Loose
    }

    fn element_type_name(&self) -> &str {
        "TopProbe"
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }
}
