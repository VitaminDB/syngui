//! `Lottie` — виджет векторной анимации Lottie (bodymovin JSON). Кадры
//! растеризуются собственным рендером (`crate::lottie`) в RGBA под
//! физическое разрешение области и заливаются в одну GPU-текстуру на
//! инстанс (`update_rgba`, как у `FramesView`). Воспроизведение: автозапуск,
//! повтор, скорость, отрезок кадров или маркер, сигналы play/pause и
//! прогресса, колбэк завершения.

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::gpu::image_store::{ImageHandle, ImageStore};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::lottie::{Composition, Renderer};
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{DisplayList, TextureId};
use crate::signal::RwSignal;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use crate::widgets::ImageFit;

/// Предел стороны растра кадра в пикселях (защита от гигантских областей).
const MAX_SIDE: u32 = 4096;

/// Ключ растра: (кадр, ширина, высота).
type FrameKey = (f32, u32, u32);

/// Готовый кадр из фонового потока.
type FrameSlot = Arc<std::sync::Mutex<Option<(FrameKey, Vec<u8>)>>>;

/// Кэш готовых кадров маленьких анимаций (иконки и т. п.): кадр
/// растеризуется один раз и дальше берётся из памяти — общий для всех
/// экземпляров одной композиции. Кэшируются только анимации, все кадры
/// которых при данном размере укладываются в [`frame_cache::PER_ANIM`];
/// общий объём ограничен [`frame_cache::BUDGET`], вытесняются давно не
/// показанные кадры.
mod frame_cache {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock, Weak};

    use crate::lottie::Composition;

    pub const PER_ANIM: usize = 24 << 20;
    pub const BUDGET: usize = 96 << 20;

    type Key = (usize, u32, u32, u32);

    struct Entry {
        comp: Weak<Composition>,
        data: Arc<Vec<u8>>,
        used: u64,
    }

    #[derive(Default)]
    struct Cache {
        map: HashMap<Key, Entry>,
        bytes: usize,
        tick: u64,
    }

    fn cache() -> &'static Mutex<Cache> {
        static C: OnceLock<Mutex<Cache>> = OnceLock::new();
        C.get_or_init(|| Mutex::new(Cache::default()))
    }

    fn key(comp: &Arc<Composition>, k: super::FrameKey) -> Key {
        (Arc::as_ptr(comp) as usize, k.0.to_bits(), k.1, k.2)
    }

    /// Помещаются ли все кадры анимации размера `w×h` в лимит.
    pub fn eligible(comp: &Composition, w: u32, h: u32) -> bool {
        let frames = comp.frame_count().ceil().max(1.0) as usize;
        (w as usize) * (h as usize) * 4 * frames <= PER_ANIM
    }

    pub fn get(comp: &Arc<Composition>, k: super::FrameKey) -> Option<Arc<Vec<u8>>> {
        let mut c = cache().lock().ok()?;
        c.tick += 1;
        let tick = c.tick;
        let e = c.map.get_mut(&key(comp, k))?;
        // Тот же адрес мог достаться новой композиции.
        if !Weak::ptr_eq(&e.comp, &Arc::downgrade(comp)) || e.comp.strong_count() == 0 {
            return None;
        }
        e.used = tick;
        Some(e.data.clone())
    }

    pub fn put(comp: &Arc<Composition>, k: super::FrameKey, data: Arc<Vec<u8>>) {
        let Ok(mut c) = cache().lock() else {
            return;
        };
        c.tick += 1;
        let used = c.tick;
        let n = data.len();
        if let Some(old) = c.map.insert(
            key(comp, k),
            Entry {
                comp: Arc::downgrade(comp),
                data,
                used,
            },
        ) {
            c.bytes -= old.data.len();
        }
        c.bytes += n;
        if c.bytes > BUDGET {
            // Сначала кадры удалённых композиций, затем давно не показанные.
            let mut dead = 0;
            c.map.retain(|_, e| {
                let alive = e.comp.strong_count() > 0;
                if !alive {
                    dead += e.data.len();
                }
                alive
            });
            c.bytes -= dead;
            if c.bytes > BUDGET {
                let mut v: Vec<(u64, Key)> = c.map.iter().map(|(k, e)| (e.used, *k)).collect();
                v.sort_unstable();
                for (_, k) in v {
                    if c.bytes <= BUDGET * 3 / 4 {
                        break;
                    }
                    if let Some(e) = c.map.remove(&k) {
                        c.bytes -= e.data.len();
                    }
                }
            }
        }
    }

    #[cfg(test)]
    pub fn len() -> usize {
        cache().lock().map(|c| c.map.len()).unwrap_or(0)
    }
}

/// Фоновые потоки растеризации Lottie. Отдельный небольшой пул, а не общий
/// rayon: долгие кадры не должны занимать воркеры параллельной раскладки.
#[cfg(not(target_arch = "wasm32"))]
mod pool {
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, Mutex, OnceLock};

    type Job = Box<dyn FnOnce() + Send>;

    static SENDER: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();

    fn worker(rx: Arc<Mutex<Receiver<Job>>>) {
        loop {
            let job = match rx.lock() {
                Ok(r) => r.recv(),
                Err(_) => return,
            };
            match job {
                // Паника в рендере не роняет поток пула.
                Ok(job) => {
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err() {
                        log::error!("Lottie: паника при растеризации кадра");
                    }
                }
                Err(_) => return,
            }
        }
    }

    pub fn spawn(job: impl FnOnce() + Send + 'static) {
        let sender = SENDER.get_or_init(|| {
            let (tx, rx) = channel::<Job>();
            let rx = Arc::new(Mutex::new(rx));
            let n = std::thread::available_parallelism()
                .map(|n| (n.get() / 2).clamp(1, 4))
                .unwrap_or(2);
            for i in 0..n {
                let rx = rx.clone();
                let _ = std::thread::Builder::new()
                    .name(format!("lottie-{i}"))
                    .spawn(move || worker(rx));
            }
            Mutex::new(tx)
        });
        if let Ok(tx) = sender.lock() {
            let _ = tx.send(Box::new(job));
        }
    }
}

type CompleteFn = Arc<dyn Fn() + Send + Sync>;
type MarkerFn = Arc<dyn Fn(&str) + Send + Sync>;

pub struct Lottie {
    comp: Option<Arc<Composition>>,
    error: Option<String>,
    fit: ImageFit,
    classes: Vec<String>,
    autoplay: bool,
    repeat: bool,
    bounce: bool,
    speed: f32,
    subframe: bool,
    playing_signal: Option<RwSignal<bool>>,
    progress_signal: Option<RwSignal<f32>>,
    segment: Option<(f32, f32)>,
    marker: Option<String>,
    on_complete: Option<CompleteFn>,
    on_marker: Option<MarkerFn>,
    size: Option<(f32, f32)>,
    zoom: f32,
}

impl Lottie {
    /// Из уже разобранной композиции (одна композиция может использоваться
    /// несколькими виджетами).
    /// Параметры воспроизведения из манифеста dotLottie (скорость, повтор,
    /// направление, bounce, автозапуск) становятся значениями по умолчанию;
    /// методы-построители их переопределяют.
    pub fn new(comp: Arc<Composition>) -> Self {
        let pb = comp.playback.clone().unwrap_or_default();
        let speed = pb
            .speed
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(1.0)
            * if pb.direction.unwrap_or(1.0) < 0.0 {
                -1.0
            } else {
                1.0
            };
        Self {
            comp: Some(comp),
            error: None,
            fit: ImageFit::Contain,
            classes: Vec::new(),
            autoplay: pb.autoplay.unwrap_or(true),
            repeat: pb.looping.unwrap_or(true),
            bounce: pb.bounce,
            speed,
            subframe: false,
            playing_signal: None,
            progress_signal: None,
            segment: None,
            marker: None,
            on_complete: None,
            on_marker: None,
            size: None,
            zoom: 1.0,
        }
    }

    /// Из текста JSON. Ошибка разбора пишется в лог, виджет остаётся пустым.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(json: &str) -> Self {
        match Composition::from_str(json) {
            Ok(mut c) => {
                load_system_glyphs(&mut c);
                Self::new(Arc::new(c))
            }
            Err(e) => Self::failed(e.to_string()),
        }
    }

    /// Из байтов: JSON или dotLottie (.lottie).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match Composition::from_bytes(bytes) {
            Ok(mut c) => {
                load_system_glyphs(&mut c);
                Self::new(Arc::new(c))
            }
            Err(e) => Self::failed(e.to_string()),
        }
    }

    /// Из байтов dotLottie: анимация с заданным `id` из манифеста
    /// (список — [`Composition::dotlottie_manifest`]).
    pub fn from_bytes_animation(bytes: &[u8], id: &str) -> Self {
        match Composition::from_dotlottie_animation(bytes, Some(id)) {
            Ok(mut c) => {
                load_images(&mut c, None);
                load_system_glyphs(&mut c);
                Self::new(Arc::new(c))
            }
            Err(e) => Self::failed(format!("{id}: {e}")),
        }
    }

    /// Из файла dotLottie: анимация с заданным `id` из манифеста.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_file_animation(path: impl AsRef<std::path::Path>, id: &str) -> Self {
        let path = path.as_ref();
        match std::fs::read(path) {
            Ok(bytes) => match Composition::from_dotlottie_animation(&bytes, Some(id)) {
                Ok(mut c) => {
                    load_images(&mut c, path.parent());
                    load_system_glyphs(&mut c);
                    Self::new(Arc::new(c))
                }
                Err(e) => Self::failed(format!("{} ({id}): {e}", path.display())),
            },
            Err(e) => Self::failed(format!("{}: {e}", path.display())),
        }
    }

    /// Из файла на диске; внешние растровые ассеты ищутся рядом с файлом
    /// (декодируются при включённой feature `image`).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Self {
        let path = path.as_ref();
        match std::fs::read(path) {
            Ok(bytes) => match Composition::from_bytes(&bytes) {
                Ok(mut c) => {
                    load_images(&mut c, path.parent());
                    load_system_glyphs(&mut c);
                    Self::new(Arc::new(c))
                }
                Err(e) => Self::failed(format!("{}: {e}", path.display())),
            },
            Err(e) => Self::failed(format!("{}: {e}", path.display())),
        }
    }

    fn failed(msg: String) -> Self {
        log::error!("Lottie: {msg}");
        let mut s = Self::new(Arc::new(empty_composition()));
        s.comp = None;
        s.error = Some(msg);
        s
    }

    /// Приближение к центру кадра (1 — как есть): у анимаций с широкими
    /// пустыми полями вокруг героя. Края за рамкой обрезаются.
    pub fn zoom(mut self, z: f32) -> Self {
        self.zoom = z.max(0.1);
        self
    }

    /// Размер виджета в пикселях (без него — из стиля или по композиции).
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.size = Some((w, h));
        self
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, c.into());
        self
    }

    /// Запускать сразу (по умолчанию `true`).
    pub fn autoplay(mut self, on: bool) -> Self {
        self.autoplay = on;
        self
    }

    /// Повторять по кругу (по умолчанию `true`); иначе — остановка на последнем кадре.
    pub fn repeat(mut self, on: bool) -> Self {
        self.repeat = on;
        self
    }

    /// Туда-обратно (режим bounce): дойдя до конца, анимация идёт назад.
    /// Без повтора — один проход вперёд и назад, затем `on_complete`.
    pub fn bounce(mut self, on: bool) -> Self {
        self.bounce = on;
        self
    }

    /// Множитель скорости (отрицательный — в обратную сторону).
    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = if speed.is_finite() { speed } else { 1.0 };
        self
    }

    /// Рисовать дробные кадры (плавнее на высокой частоте экрана, но
    /// растеризация на каждый тик); по умолчанию — только целые кадры анимации.
    pub fn subframe(mut self, on: bool) -> Self {
        self.subframe = on;
        self
    }

    /// Двунаправленный play/pause.
    pub fn playing_signal(mut self, sig: RwSignal<bool>) -> Self {
        self.playing_signal = Some(sig);
        self
    }

    /// Прогресс 0..1 по отрезку: виджет пишет при воспроизведении; запись
    /// извне на паузе — переход к кадру.
    pub fn progress_signal(mut self, sig: RwSignal<f32>) -> Self {
        self.progress_signal = Some(sig);
        self
    }

    /// Воспроизводить только кадры `start..end` (в кадрах композиции).
    pub fn segment(mut self, start: f32, end: f32) -> Self {
        self.segment = Some((start.min(end), end.max(start)));
        self.marker = None;
        self
    }

    /// Отрезок по имени маркера композиции (`markers[].cm`).
    pub fn marker(mut self, name: impl Into<String>) -> Self {
        self.marker = Some(name.into());
        self.segment = None;
        self
    }

    /// Вызывается при достижении конца без повтора.
    pub fn on_complete(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_complete = Some(Arc::new(f));
        self
    }

    /// Вызывается с именем маркера (`markers[].cm`), когда воспроизведение
    /// проходит его кадр (в любую сторону, с учётом повтора и bounce).
    pub fn on_marker(mut self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_marker = Some(Arc::new(f));
        self
    }

    /// Ошибка разбора, если файл не удалось загрузить.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn resolve_segment(&self) -> (f32, f32) {
        let Some(comp) = &self.comp else {
            return (0.0, 0.0);
        };
        if let Some(seg) = self.segment {
            return (
                seg.0.max(comp.in_point),
                seg.1.min(comp.out_point).max(seg.0),
            );
        }
        if let Some(name) = &self.marker {
            if let Some(m) = comp.markers.iter().find(|m| &m.name == name) {
                let end = if m.duration > 0.0 {
                    m.frame + m.duration
                } else {
                    comp.out_point
                };
                return (m.frame, end.min(comp.out_point));
            }
            log::warn!("Lottie: маркер `{name}` не найден, играет вся анимация");
        }
        (comp.in_point, comp.out_point)
    }
}

fn empty_composition() -> Composition {
    Composition::from_str(r#"{"fr":30,"ip":0,"op":1,"w":1,"h":1,"layers":[]}"#)
        .expect("пустая композиция")
}

/// Декодировать растровые ассеты композиции (PNG/JPEG через `image`).
#[cfg(all(feature = "image", not(target_arch = "wasm32")))]
fn load_images(comp: &mut Composition, base: Option<&std::path::Path>) {
    comp.load_images(|asset| {
        let bytes = match &asset.embedded {
            Some(b) => b.clone(),
            None => {
                std::fs::read(base.unwrap_or(std::path::Path::new(".")).join(&asset.path)).ok()?
            }
        };
        let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        Some(crate::lottie::ImageRgba {
            width: w,
            height: h,
            data: img.into_raw().into(),
        })
    });
}

#[cfg(not(all(feature = "image", not(target_arch = "wasm32"))))]
fn load_images(comp: &mut Composition, _base: Option<&std::path::Path>) {
    if comp.has_images() {
        log::warn!("Lottie: растровые слои пропущены — нужна feature `image`");
    }
}

/// Глифы системных шрифтов для текста без встроенных `chars`.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn load_system_glyphs(comp: &mut Composition) {
    crate::lottie::sysfont::load_missing_glyphs(comp);
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
fn load_system_glyphs(_comp: &mut Composition) {}

impl Widget for Lottie {
    fn create_element(&self) -> Box<dyn Element> {
        let comp = self
            .comp
            .clone()
            .unwrap_or_else(|| Arc::new(empty_composition()));
        let (seg_start, seg_end) = self.resolve_segment();
        let natural = (comp.width.max(1.0), comp.height.max(1.0));
        Box::new(LottieElement {
            id: ElementId::new(),
            comp,
            fit: self.fit,
            classes: self.classes.clone(),
            bounds: Rect::zero(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            image_handle: None,
            image_store: None,
            natural,
            mss: MssFields::new(),
            playing_signal: self.playing_signal,
            progress_signal: self.progress_signal,
            playing: self.autoplay && self.comp.is_some(),
            repeat: self.repeat,
            bounce: self.bounce,
            bounce_dir: 1.0,
            bounced: false,
            speed: self.speed,
            subframe: self.subframe,
            seg_start,
            seg_end,
            frame: if self.speed < 0.0 { seg_end } else { seg_start },
            renderer: Arc::new(std::sync::Mutex::new(Renderer::new())),
            rendered: None,
            inflight: None,
            slot: Arc::new(std::sync::Mutex::new(None)),
            on_complete: self.on_complete.clone(),
            on_marker: self.on_marker.clone(),
            size: self.size,
            zoom: self.zoom,
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

pub struct LottieElement {
    id: ElementId,
    comp: Arc<Composition>,
    fit: ImageFit,
    classes: Vec<String>,
    bounds: Rect,
    dirty_flags: DirtyFlags,
    image_handle: Option<ImageHandle>,
    image_store: Option<Arc<Mutex<ImageStore>>>,
    natural: (f32, f32),
    mss: MssFields,
    playing_signal: Option<RwSignal<bool>>,
    progress_signal: Option<RwSignal<f32>>,
    playing: bool,
    repeat: bool,
    bounce: bool,
    /// Текущее направление в режиме bounce (±1).
    bounce_dir: f32,
    /// В этом проходе уже был разворот (для bounce без повтора).
    bounced: bool,
    speed: f32,
    subframe: bool,
    seg_start: f32,
    seg_end: f32,
    frame: f32,
    /// Рендерер (с буферами) делится с фоновой задачей.
    renderer: Arc<std::sync::Mutex<Renderer>>,
    /// Последний залитый кадр.
    rendered: Option<FrameKey>,
    /// Кадр, который сейчас растеризуется в фоне.
    inflight: Option<FrameKey>,
    /// Куда фоновая задача кладёт готовый кадр.
    slot: FrameSlot,
    on_complete: Option<CompleteFn>,
    on_marker: Option<MarkerFn>,
    size: Option<(f32, f32)>,
    zoom: f32,
}

impl LottieElement {
    /// Сообщить о маркерах, пройденных на отрезках `(from, to]` (в сторону
    /// движения; начальная точка не включается).
    fn fire_markers(&self, spans: &[(f32, f32)]) {
        let Some(cb) = &self.on_marker else {
            return;
        };
        for &(from, to) in spans {
            for m in &self.comp.markers {
                let hit = if to >= from {
                    m.frame > from && m.frame <= to
                } else {
                    m.frame < from && m.frame >= to
                };
                if hit {
                    cb(&m.name);
                }
            }
        }
    }

    fn seg_len(&self) -> f32 {
        (self.seg_end - self.seg_start).max(0.0)
    }

    fn progress(&self) -> f32 {
        let len = self.seg_len();
        if len <= 0.0 {
            0.0
        } else {
            ((self.frame - self.seg_start) / len).clamp(0.0, 1.0)
        }
    }

    fn set_progress(&mut self, p: f32) {
        self.frame = self.seg_start + self.seg_len() * p.clamp(0.0, 1.0);
    }

    fn set_playing(&mut self, on: bool) {
        self.playing = on;
        if let Some(sig) = self.playing_signal {
            if sig.get_untracked() != on {
                sig.set(on);
            }
        }
    }

    /// Прямоугольник отрисовки по режиму вписывания.
    fn fit_rect(&self) -> Rect {
        let (nw, nh) = self.natural;
        let bw = self.bounds.size.width;
        let bh = self.bounds.size.height;
        match self.fit {
            ImageFit::Fill => self.bounds,
            ImageFit::None => Rect::new(
                Point::new(
                    self.bounds.x() + (bw - nw) / 2.0,
                    self.bounds.y() + (bh - nh) / 2.0,
                ),
                Size::new(nw, nh),
            ),
            ImageFit::Contain | ImageFit::Cover => {
                let scale = if self.fit == ImageFit::Contain {
                    (bw / nw).min(bh / nh)
                } else {
                    (bw / nw).max(bh / nh)
                } * self.zoom;
                let sw = nw * scale;
                let sh = nh * scale;
                Rect::new(
                    Point::new(
                        self.bounds.x() + (bw - sw) / 2.0,
                        self.bounds.y() + (bh - sh) / 2.0,
                    ),
                    Size::new(sw, sh),
                )
            }
        }
    }

    /// Кадр, который реально рисуется: целый или дробный.
    fn display_frame(&self) -> f32 {
        let f = self
            .frame
            .clamp(self.seg_start, self.seg_end.max(self.seg_start));
        // Последний кадр отрезка (`op`) не существует — показываем предыдущий.
        let last = (self.seg_end - 1.0).max(self.seg_start);
        let f = f.min(last);
        if self.subframe {
            f
        } else {
            f.floor()
        }
    }

    /// Нужный сейчас растр: кадр и физический размер области.
    fn wanted_key(&self) -> FrameKey {
        let rect = self.fit_rect();
        let ratio = crate::scale::pixel_ratio().max(0.1);
        let w = ((rect.size.width * ratio).round() as u32).clamp(1, MAX_SIDE);
        let h = ((rect.size.height * ratio).round() as u32).clamp(1, MAX_SIDE);
        (self.display_frame(), w, h)
    }

    fn upload(&mut self, key: FrameKey, rgba: Vec<u8>) {
        if let (Some(handle), Some(store)) = (self.image_handle, self.image_store.as_ref()) {
            if let Ok(mut s) = store.lock() {
                s.update_rgba(handle, key.1, key.2, rgba);
            }
        }
        self.rendered = Some(key);
        self.mark_dirty(DirtyFlags::RENDER);
    }

    /// Забрать готовый фоновый кадр и, если нужен другой, заказать его.
    /// Кадр растеризуется в фоновом потоке и появляется на следующем тике;
    /// пока он считается, новый не заказывается (кадры не копятся в очереди).
    fn render_if_needed(&mut self) {
        let done = self.slot.lock().ok().and_then(|mut s| s.take());
        if let Some((key, rgba)) = done {
            self.inflight = None;
            self.upload(key, rgba);
        }
        let key = self.wanted_key();
        if self.rendered == Some(key) || self.inflight == Some(key) {
            return;
        }
        if self.inflight.is_some() {
            return;
        }
        let cacheable = !self.subframe && frame_cache::eligible(&self.comp, key.1, key.2);
        if cacheable {
            if let Some(data) = frame_cache::get(&self.comp, key) {
                self.upload(key, (*data).clone());
                return;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let comp = self.comp.clone();
            let renderer = self.renderer.clone();
            let slot = self.slot.clone();
            self.inflight = Some(key);
            pool::spawn(move || {
                let rgba = match renderer.lock() {
                    Ok(mut r) => r.render_rgba8(&comp, key.0, key.1, key.2),
                    Err(_) => return,
                };
                if cacheable {
                    frame_cache::put(&comp, key, Arc::new(rgba.clone()));
                }
                if let Ok(mut s) = slot.lock() {
                    *s = Some((key, rgba));
                }
            });
        }
        // В браузере потоков нет — рисуем сразу.
        #[cfg(target_arch = "wasm32")]
        {
            let rgba = match self.renderer.lock() {
                Ok(mut r) => r.render_rgba8(&self.comp, key.0, key.1, key.2),
                Err(_) => return,
            };
            if cacheable {
                frame_cache::put(&self.comp, key, Arc::new(rgba.clone()));
            }
            self.upload(key, rgba);
        }
    }
}

impl Drop for LottieElement {
    /// Отпустить текстуру кадра в сторе картинок.
    fn drop(&mut self) {
        if let (Some(store), Some(handle)) = (&self.image_store, self.image_handle) {
            if let Ok(mut store) = store.lock() {
                store.release(handle);
            }
        }
    }
}

impl Element for LottieElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(v) = widget.as_any().downcast_ref::<Lottie>() {
            if let Some(c) = &v.comp {
                if !Arc::ptr_eq(&self.comp, c) {
                    self.comp = c.clone();
                    self.natural = (c.width.max(1.0), c.height.max(1.0));
                    let (s, e) = v.resolve_segment();
                    self.seg_start = s;
                    self.seg_end = e;
                    self.frame = if v.speed < 0.0 { e } else { s };
                    self.rendered = None;
                    // Кадр старой композиции, ещё считающийся в фоне, уйдёт в
                    // прежний слот и будет отброшен.
                    self.slot = Arc::new(std::sync::Mutex::new(None));
                    self.inflight = None;
                    self.playing = v.autoplay;
                    self.mark_dirty(DirtyFlags::LAYOUT);
                }
            }
            let (s, e) = v.resolve_segment();
            if v.comp.is_some() && (s != self.seg_start || e != self.seg_end) {
                self.seg_start = s;
                self.seg_end = e;
                self.frame = self.frame.clamp(s, e);
                self.rendered = None;
            }
            self.fit = v.fit;
            self.repeat = v.repeat;
            if self.bounce != v.bounce {
                self.bounce = v.bounce;
                self.bounce_dir = 1.0;
                self.bounced = false;
            }
            self.speed = v.speed;
            self.subframe = v.subframe;
            self.classes = v.classes.clone();
            self.on_complete = v.on_complete.clone();
            self.on_marker = v.on_marker.clone();
            if self.zoom != v.zoom {
                self.zoom = v.zoom;
                self.rendered = None;
            }
            if self.size != v.size {
                self.size = v.size;
                self.mark_dirty(DirtyFlags::LAYOUT);
            }
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let (nw, nh) = self.natural;
        let max_w = constraints.max_width;
        let max_h = constraints.max_height;
        if let Some((w, h)) = self.size {
            let size = Size::new(w.min(max_w), h.min(max_h));
            self.bounds = Rect::new(Point::zero(), size);
            return size;
        }
        let width = if let Some(d) = self.mss.width {
            d.resolve(max_w).min(max_w)
        } else {
            nw.min(max_w)
        };
        let height = if let Some(d) = self.mss.height {
            d.resolve(max_h).min(max_h)
        } else {
            (width * nh / nw).min(max_h)
        };
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg) = self.mss.background_color {
            list.push_rect(self.bounds, bg, [0.0; 4]);
        }
        if let Some(handle) = self.image_handle {
            let uv = Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0));
            let fit_rect = self.fit_rect();
            // Видимая часть: у Cover — рамка виджета, иначе сам кадр.
            let visible = {
                let (b, f) = (self.bounds, fit_rect);
                let x0 = b.x().max(f.x());
                let y0 = b.y().max(f.y());
                let x1 = (b.x() + b.size.width).min(f.x() + f.size.width);
                let y1 = (b.y() + b.size.height).min(f.y() + f.size.height);
                Rect::new(
                    Point::new(x0, y0),
                    Size::new((x1 - x0).max(0.0), (y1 - y0).max(0.0)),
                )
            };
            let radii = self
                .mss
                .border_radius
                .map(|dims| dims.map(|d| d.resolve(visible.size.width.min(visible.size.height))))
                .filter(|r| r.iter().any(|v| *v > 0.0));
            let clipped = radii.is_some() || self.fit == ImageFit::Cover || self.zoom > 1.0;
            match radii {
                Some(r) => list.push_clip_rounded(visible, r),
                None if clipped => list.push_clip(visible),
                None => {}
            }
            list.push_image(fit_rect, TextureId(handle.0), uv, Color::WHITE);
            if clipped {
                list.pop_clip();
            }
        }
    }

    fn handle_event(
        &mut self,
        _event: &Event,
        _ctx: &mut crate::widget::context::EventContext,
    ) -> EventResult {
        EventResult::Ignored
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if let Some(sig) = self.playing_signal {
            let external = sig.get_untracked();
            if external != self.playing {
                self.playing = external;
            }
        }
        if self.playing {
            // Внешняя правка прогресса (слайдер) во время воспроизведения.
            if let Some(sig) = self.progress_signal {
                let external = sig.get_untracked();
                if (external - self.progress()).abs() > 0.05 {
                    self.set_progress(external);
                }
            }
            let len = self.seg_len();
            if len <= 0.0 {
                self.set_playing(false);
            } else {
                let eff = self.speed * if self.bounce { self.bounce_dir } else { 1.0 };
                let prev = self.frame;
                self.frame += dt.as_secs_f32() * self.comp.frame_rate * eff;
                let mut spans: [(f32, f32); 2] = [(prev, self.frame), (0.0, 0.0)];
                let mut nspans = 1;
                let past_end = eff >= 0.0 && self.frame >= self.seg_end;
                let past_start = eff < 0.0 && self.frame <= self.seg_start;
                if (past_end || past_start) && self.bounce && (self.repeat || !self.bounced) {
                    // Разворот: отражение от края отрезка.
                    self.frame = if past_end {
                        self.seg_end - (self.frame - self.seg_end).min(len)
                    } else {
                        self.seg_start + (self.seg_start - self.frame).min(len)
                    };
                    self.bounce_dir = -self.bounce_dir;
                    self.bounced = true;
                    let edge = if past_end {
                        self.seg_end
                    } else {
                        self.seg_start
                    };
                    spans = [(prev, edge), (edge, self.frame)];
                    nspans = 2;
                } else if past_end || past_start {
                    if self.repeat {
                        let rel = (self.frame - self.seg_start).rem_euclid(len);
                        self.frame = self.seg_start + rel;
                        // Переход через край: начало следующего круга включается.
                        spans = if past_end {
                            [(prev, self.seg_end), (self.seg_start - 1e-3, self.frame)]
                        } else {
                            [(prev, self.seg_start), (self.seg_end + 1e-3, self.frame)]
                        };
                        nspans = 2;
                    } else {
                        self.frame = if past_end {
                            self.seg_end
                        } else {
                            self.seg_start
                        };
                        spans[0] = (prev, self.frame);
                        self.set_playing(false);
                        self.bounce_dir = 1.0;
                        self.bounced = false;
                        self.fire_markers(&spans[..nspans]);
                        nspans = 0;
                        if let Some(cb) = self.on_complete.clone() {
                            cb();
                        }
                    }
                }
                self.fire_markers(&spans[..nspans]);
            }
            if let Some(sig) = self.progress_signal {
                let p = self.progress();
                if (sig.get_untracked() - p).abs() > 1e-3 {
                    sig.set(p);
                }
            }
        } else if let Some(sig) = self.progress_signal {
            let external = sig.get_untracked();
            if (external - self.progress()).abs() > 1e-3 {
                self.set_progress(external);
            }
        }
        self.render_if_needed();
        // Пока кадр считается в фоне, тики нужны, чтобы его забрать.
        self.playing || self.inflight.is_some()
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
        if let Some(store) = self.image_store.as_ref() {
            if let Ok(mut s) = store.lock() {
                let key = format!("lottie:{:p}:{}", Arc::as_ptr(&self.comp), self.id.0);
                let (handle, _) = s.request_rgba(&key, 1, 1, vec![0u8; 4]);
                self.image_handle = Some(handle);
            }
        }
        self.rendered = None;
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "Lottie"
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }
    fn mss_mut(&mut self) -> Option<&mut crate::mss::MssFields> {
        Some(&mut self.mss)
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
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Image,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(if self.comp.name.is_empty() {
                    crate::i18n::builtin("lottie.a11y", "Animation")
                } else {
                    self.comp.name.clone()
                }),
                ..Default::default()
            },
        })
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

impl StyledElement for LottieElement {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    const JSON: &str = r#"{"fr":30,"ip":0,"op":30,"w":10,"h":10,"layers":[]}"#;

    fn element(w: &Lottie) -> Box<dyn Element> {
        w.create_element()
    }

    fn frame_of(e: &mut Box<dyn Element>) -> (f32, bool) {
        let el = e
            .as_any_mut()
            .and_then(|a| a.downcast_mut::<LottieElement>())
            .expect("LottieElement");
        (el.frame, el.playing)
    }

    #[test]
    fn bounce_once_then_complete() {
        let done = Arc::new(AtomicBool::new(false));
        let d = done.clone();
        let w = Lottie::from_str(JSON)
            .bounce(true)
            .repeat(false)
            .on_complete(move || d.store(true, Ordering::SeqCst));
        let mut e = element(&w);
        e.animate(Duration::from_millis(600));
        assert!((frame_of(&mut e).0 - 18.0).abs() < 1e-3);
        // Дошли до конца — разворот, отражение на 6 кадров назад.
        e.animate(Duration::from_millis(600));
        assert!((frame_of(&mut e).0 - 24.0).abs() < 1e-3);
        assert!(!done.load(Ordering::SeqCst));
        e.animate(Duration::from_millis(1000));
        let (f, playing) = frame_of(&mut e);
        assert_eq!(f, 0.0);
        assert!(!playing);
        assert!(done.load(Ordering::SeqCst));
    }

    #[test]
    fn bounce_repeat_keeps_going() {
        let w = Lottie::from_str(JSON).bounce(true);
        let mut e = element(&w);
        for _ in 0..10 {
            e.animate(Duration::from_millis(700));
        }
        let (f, playing) = frame_of(&mut e);
        assert!(playing && (0.0..=30.0).contains(&f));
    }

    #[test]
    fn markers_fire_when_crossed() {
        use std::sync::Mutex as M;
        let json = r#"{"fr":30,"ip":0,"op":30,"w":10,"h":10,"layers":[],
            "markers":[{"cm":"start","tm":0,"dr":0},{"cm":"mid","tm":15,"dr":0},{"cm":"end","tm":30,"dr":0}]}"#;
        let log = Arc::new(M::new(Vec::<String>::new()));
        let l2 = log.clone();
        let w = Lottie::from_str(json).on_marker(move |n| l2.lock().unwrap().push(n.to_string()));
        let mut e = element(&w);
        e.animate(Duration::from_millis(400)); // 0 → 12
        assert!(log.lock().unwrap().is_empty());
        e.animate(Duration::from_millis(400)); // 12 → 24: mid
        e.animate(Duration::from_millis(400)); // 24 → 36 → круг: end, start
        assert_eq!(*log.lock().unwrap(), vec!["mid", "end", "start"]);
    }

    #[test]
    fn frame_cache_shared_and_validated() {
        let comp = Arc::new(Composition::from_str(JSON).unwrap());
        assert!(frame_cache::eligible(&comp, 64, 64));
        assert!(!frame_cache::eligible(&comp, 2000, 2000));
        let k = (3.0, 8, 8);
        assert!(frame_cache::get(&comp, k).is_none());
        frame_cache::put(&comp, k, Arc::new(vec![7u8; 256]));
        assert_eq!(frame_cache::get(&comp, k).unwrap()[0], 7);
        assert!(frame_cache::len() >= 1);
        // Другая композиция — промах, даже с тем же ключом кадра.
        let other = Arc::new(Composition::from_str(JSON).unwrap());
        assert!(frame_cache::get(&other, k).is_none());
    }

    #[test]
    fn dotlottie_hints_become_defaults() {
        let mut c = Composition::from_str(JSON).unwrap();
        c.playback = Some(crate::lottie::DotLottieAnimation {
            id: "a".into(),
            speed: Some(2.0),
            looping: Some(false),
            autoplay: Some(true),
            direction: Some(-1.0),
            bounce: true,
            theme_color: None,
        });
        let w = Lottie::new(Arc::new(c));
        assert_eq!(w.speed, -2.0);
        assert!(!w.repeat && w.bounce);
        // Построитель переопределяет манифест.
        let w = w.speed(1.0).repeat(true);
        assert!(w.repeat && w.speed == 1.0);
    }
}
