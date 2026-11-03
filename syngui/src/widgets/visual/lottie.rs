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
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widgets::ImageFit;

/// Предел стороны растра кадра в пикселях (защита от гигантских областей).
const MAX_SIDE: u32 = 4096;

type CompleteFn = Arc<dyn Fn() + Send + Sync>;

pub struct Lottie {
    comp: Option<Arc<Composition>>,
    error: Option<String>,
    fit: ImageFit,
    classes: Vec<String>,
    autoplay: bool,
    repeat: bool,
    speed: f32,
    subframe: bool,
    playing_signal: Option<RwSignal<bool>>,
    progress_signal: Option<RwSignal<f32>>,
    segment: Option<(f32, f32)>,
    marker: Option<String>,
    on_complete: Option<CompleteFn>,
}

impl Lottie {
    /// Из уже разобранной композиции (одна композиция может использоваться
    /// несколькими виджетами).
    pub fn new(comp: Arc<Composition>) -> Self {
        Self {
            comp: Some(comp),
            error: None,
            fit: ImageFit::Contain,
            classes: Vec::new(),
            autoplay: true,
            repeat: true,
            speed: 1.0,
            subframe: false,
            playing_signal: None,
            progress_signal: None,
            segment: None,
            marker: None,
            on_complete: None,
        }
    }

    /// Из текста JSON. Ошибка разбора пишется в лог, виджет остаётся пустым.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(json: &str) -> Self {
        match Composition::from_str(json) {
            Ok(c) => Self::new(Arc::new(c)),
            Err(e) => Self::failed(e.to_string()),
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        match Composition::from_bytes(bytes) {
            Ok(c) => Self::new(Arc::new(c)),
            Err(e) => Self::failed(e.to_string()),
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

    /// Ошибка разбора, если файл не удалось загрузить.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn resolve_segment(&self) -> (f32, f32) {
        let Some(comp) = &self.comp else {
            return (0.0, 0.0);
        };
        if let Some(seg) = self.segment {
            return (seg.0.max(comp.in_point), seg.1.min(comp.out_point).max(seg.0));
        }
        if let Some(name) = &self.marker {
            if let Some(m) = comp.markers.iter().find(|m| &m.name == name) {
                let end = if m.duration > 0.0 { m.frame + m.duration } else { comp.out_point };
                return (m.frame, end.min(comp.out_point));
            }
            log::warn!("Lottie: маркер `{name}` не найден, играет вся анимация");
        }
        (comp.in_point, comp.out_point)
    }
}

fn empty_composition() -> Composition {
    Composition::from_str(r#"{"fr":30,"ip":0,"op":1,"w":1,"h":1,"layers":[]}"#).expect("пустая композиция")
}

/// Декодировать растровые ассеты композиции (PNG/JPEG через `image`).
#[cfg(all(feature = "image", not(target_arch = "wasm32")))]
fn load_images(comp: &mut Composition, base: Option<&std::path::Path>) {
    comp.load_images(|asset| {
        let bytes = match &asset.embedded {
            Some(b) => b.clone(),
            None => std::fs::read(base.unwrap_or(std::path::Path::new(".")).join(&asset.path)).ok()?,
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

impl Widget for Lottie {
    fn create_element(&self) -> Box<dyn Element> {
        let comp = self.comp.clone().unwrap_or_else(|| Arc::new(empty_composition()));
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
            speed: self.speed,
            subframe: self.subframe,
            seg_start,
            seg_end,
            frame: if self.speed < 0.0 { seg_end } else { seg_start },
            renderer: Renderer::new(),
            rendered: None,
            on_complete: self.on_complete.clone(),
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
    speed: f32,
    subframe: bool,
    seg_start: f32,
    seg_end: f32,
    frame: f32,
    renderer: Renderer,
    /// Последний залитый кадр: (кадр, ширина, высота).
    rendered: Option<(f32, u32, u32)>,
    on_complete: Option<CompleteFn>,
}

impl LottieElement {
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
                Point::new(self.bounds.x() + (bw - nw) / 2.0, self.bounds.y() + (bh - nh) / 2.0),
                Size::new(nw, nh),
            ),
            ImageFit::Contain | ImageFit::Cover => {
                let scale = if self.fit == ImageFit::Contain { (bw / nw).min(bh / nh) } else { (bw / nw).max(bh / nh) };
                let sw = nw * scale;
                let sh = nh * scale;
                Rect::new(
                    Point::new(self.bounds.x() + (bw - sw) / 2.0, self.bounds.y() + (bh - sh) / 2.0),
                    Size::new(sw, sh),
                )
            }
        }
    }

    /// Кадр, который реально рисуется: целый или дробный.
    fn display_frame(&self) -> f32 {
        let f = self.frame.clamp(self.seg_start, self.seg_end.max(self.seg_start));
        // Последний кадр отрезка (`op`) не существует — показываем предыдущий.
        let last = (self.seg_end - 1.0).max(self.seg_start);
        let f = f.min(last);
        if self.subframe { f } else { f.floor() }
    }

    /// Растеризовать текущий кадр под физический размер области, если он изменился.
    fn render_if_needed(&mut self) {
        let rect = self.fit_rect();
        let ratio = crate::scale::pixel_ratio().max(0.1);
        let w = ((rect.size.width * ratio).round() as u32).clamp(1, MAX_SIDE);
        let h = ((rect.size.height * ratio).round() as u32).clamp(1, MAX_SIDE);
        let frame = self.display_frame();
        if self.rendered == Some((frame, w, h)) {
            return;
        }
        let rgba = self.renderer.render_rgba8(&self.comp, frame, w, h);
        if let (Some(handle), Some(store)) = (self.image_handle, self.image_store.as_ref()) {
            if let Ok(mut s) = store.lock() {
                s.update_rgba(handle, w, h, rgba);
            }
        }
        self.rendered = Some((frame, w, h));
        self.mark_dirty(DirtyFlags::RENDER);
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
            self.speed = v.speed;
            self.subframe = v.subframe;
            self.classes = v.classes.clone();
            self.on_complete = v.on_complete.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let (nw, nh) = self.natural;
        let max_w = constraints.max_width;
        let max_h = constraints.max_height;
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
                Rect::new(Point::new(x0, y0), Size::new((x1 - x0).max(0.0), (y1 - y0).max(0.0)))
            };
            let radii = self
                .mss
                .border_radius
                .map(|dims| dims.map(|d| d.resolve(visible.size.width.min(visible.size.height))))
                .filter(|r| r.iter().any(|v| *v > 0.0));
            let clipped = radii.is_some() || self.fit == ImageFit::Cover;
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

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
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
                self.frame += dt.as_secs_f32() * self.comp.frame_rate * self.speed;
                let past_end = self.speed >= 0.0 && self.frame >= self.seg_end;
                let past_start = self.speed < 0.0 && self.frame <= self.seg_start;
                if past_end || past_start {
                    if self.repeat {
                        let rel = (self.frame - self.seg_start).rem_euclid(len);
                        self.frame = self.seg_start + rel;
                    } else {
                        self.frame = if past_end { self.seg_end } else { self.seg_start };
                        self.set_playing(false);
                        if let Some(cb) = self.on_complete.clone() {
                            cb();
                        }
                    }
                }
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
        self.mss.apply_transitions(base, hover, active, focus, selected);
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
