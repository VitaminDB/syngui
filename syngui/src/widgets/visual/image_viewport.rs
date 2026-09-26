//! Область просмотра одной картинки: вписывание, масштаб к курсору,
//! перетаскивание, полосы прокрутки, поворот четвертями и отражение.
//!
//! В отличие от `PanZoomViewport` с `Image` внутри, виджет знает размер
//! картинки и потому умеет то, чего пара не могла: «100 %» — это пиксель в
//! пиксель, а не «как вписалось»; картинку нельзя утащить за край; когда она
//! больше области, появляются полосы прокрутки; при изменении размера
//! области вписанная картинка остаётся вписанной.
//!
//! Состояние вида живёт в элементе. Наружу оно публикуется сигналом
//! [`ImageViewport::info`], а команды снаружи (кнопки «+», «вписать»)
//! приходят свойством [`ImageViewport::command`] с растущим номером: виджет
//! пересобирают через `Reactive`, элемент видит новый номер в `update` и
//! исполняет команду. Сигналом команды не передать: элемент вне реестра
//! анимаций о чужом сигнале не узнает (см. docs/10-custom-widgets.md).

use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, Size, Transform};
use crate::gpu::image_store::{ImageHandle, ImageLoadState, ImageSource, ImageStore};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{DisplayList, TextureId};
use crate::signal::RwSignal;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_MAX_SCALE: f32 = 16.0;
/// Один щелчок колеса — 40 единиц дельты: шаг масштаба ≈ 17 %.
const WHEEL_SPEED: f32 = 0.004;
const BUTTON_STEP: f32 = 1.25;
/// Скорость догона цели (1/с) у масштаба и поворота.
const EASE_RATE: f32 = 18.0;

const BAR_THICKNESS: f32 = 8.0;
const BAR_MARGIN: f32 = 4.0;
const BAR_MIN_THUMB: f32 = 28.0;

/// Команда виду снаружи.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageViewCommand {
    ZoomIn,
    ZoomOut,
    /// Вписать целиком (маленькую картинку не растягивает).
    Fit,
    /// Заполнить область, обрезав лишнее.
    Fill,
    /// Пиксель в пиксель.
    Actual,
    /// Вписать ↔ пиксель в пиксель (то же делает двойной щелчок).
    ToggleFit,
    /// Сдвиг вида на долю области: стрелки клавиатуры.
    PanBy(f32, f32),
}

/// Что сейчас показывает область — для подписи масштаба и кнопок.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ImageViewInfo {
    /// Экранных пикселей на пиксель картинки (цель анимации, не кадр).
    pub scale: f32,
    pub fit_scale: f32,
    /// Картинка вписана и следует за размером области.
    pub fit: bool,
    /// Размер картинки без учёта поворота; нули — ещё не известен.
    pub natural: (u32, u32),
    pub ready: bool,
    pub failed: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Арифметика вида — без дерева и GPU, чтобы её можно было проверить тестами
// ─────────────────────────────────────────────────────────────────────────────

/// Масштаб вписывания. Картинку меньше области не растягивает.
pub fn fit_scale(view: Size, img: Size) -> f32 {
    if img.width <= 0.0 || img.height <= 0.0 || view.width <= 0.0 || view.height <= 0.0 {
        return 1.0;
    }
    (view.width / img.width)
        .min(view.height / img.height)
        .min(1.0)
}

/// Масштаб заполнения: область закрыта целиком.
pub fn fill_scale(view: Size, img: Size) -> f32 {
    if img.width <= 0.0 || img.height <= 0.0 || view.width <= 0.0 || view.height <= 0.0 {
        return 1.0;
    }
    (view.width / img.width).max(view.height / img.height)
}

/// Смещение центра картинки от центра области, при котором край картинки
/// не отходит от края области: по оси, где картинка меньше области, она
/// стоит по центру.
pub fn clamp_offset(offset: Point, scale: f32, view: Size, img: Size) -> Point {
    let lim = |content: f32, view: f32, o: f32| {
        let half = ((content - view) * 0.5).max(0.0);
        o.clamp(-half, half)
    };
    Point::new(
        lim(img.width * scale, view.width, offset.x),
        lim(img.height * scale, view.height, offset.y),
    )
}

/// Смещение после смены масштаба так, чтобы точка `anchor` (от центра
/// области) осталась над тем же местом картинки.
pub fn zoom_offset(offset: Point, old_scale: f32, new_scale: f32, anchor: Point) -> Point {
    let f = new_scale / old_scale.max(1e-6);
    Point::new(
        anchor.x - (anchor.x - offset.x) * f,
        anchor.y - (anchor.y - offset.y) * f,
    )
}

/// Ползунок полосы прокрутки вдоль одной оси: `(начало, длина)` на дорожке
/// длиной `track`. `None` — содержимое помещается, полоса не нужна.
pub fn bar_thumb(track: f32, content: f32, view: f32, offset: f32) -> Option<(f32, f32)> {
    let hidden = content - view;
    if hidden <= 0.5 || track <= BAR_MIN_THUMB {
        return None;
    }
    let len = (track * view / content).clamp(BAR_MIN_THUMB, track);
    // Смещение вправо открывает левую часть: начало видимого окна меньше.
    let start = (hidden * 0.5 - offset).clamp(0.0, hidden);
    Some(((track - len) * start / hidden, len))
}

/// Обратное к [`bar_thumb`]: смещение вида по положению ползунка.
pub fn offset_from_thumb(track: f32, content: f32, view: f32, thumb_pos: f32) -> f32 {
    let hidden = content - view;
    let len = (track * view / content).clamp(BAR_MIN_THUMB, track);
    let free = (track - len).max(1e-3);
    let start = (thumb_pos / free).clamp(0.0, 1.0) * hidden;
    hidden * 0.5 - start
}

// ─────────────────────────────────────────────────────────────────────────────
// Виджет
// ─────────────────────────────────────────────────────────────────────────────

pub struct ImageViewport {
    source: ImageSource,
    command: Option<(u64, ImageViewCommand)>,
    quarter_turns: i32,
    flip_h: bool,
    flip_v: bool,
    natural_hint: Option<(u32, u32)>,
    max_scale: f32,
    insets: [f32; 4],
    info: Option<RwSignal<ImageViewInfo>>,
    classes: Vec<String>,
}

impl ImageViewport {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            source: ImageSource::Path(path.into()),
            command: None,
            quarter_turns: 0,
            flip_h: false,
            flip_v: false,
            natural_hint: None,
            max_scale: DEFAULT_MAX_SCALE,
            insets: [0.0; 4],
            info: None,
            classes: Vec::new(),
        }
    }

    /// Поля области (сверху, справа, снизу, слева), занятые чем-то поверх
    /// неё — панелью инструментов, стрелками. Картинка вписывается и
    /// центрируется в том, что осталось, а приближенную можно довести краем
    /// до границы полей, то есть вытащить из-под панели. Рисуется картинка
    /// по-прежнему во всей области.
    pub fn insets(mut self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        self.insets = [top.max(0.0), right.max(0.0), bottom.max(0.0), left.max(0.0)];
        self
    }

    /// Команда с номером: исполняется один раз, когда номер сменился.
    pub fn command(mut self, seq: u64, command: ImageViewCommand) -> Self {
        self.command = Some((seq, command));
        self
    }

    /// Поворот по часовой стрелке четвертями оборота (можно отрицательный).
    pub fn quarter_turns(mut self, turns: i32) -> Self {
        self.quarter_turns = turns;
        self
    }

    pub fn flip(mut self, horizontal: bool, vertical: bool) -> Self {
        self.flip_h = horizontal;
        self.flip_v = vertical;
        self
    }

    /// Размер картинки, известный заранее (из метаданных): вид считается
    /// верно ещё до конца декодирования, без скачка после загрузки.
    pub fn natural_size(mut self, width: u32, height: u32) -> Self {
        if width > 0 && height > 0 {
            self.natural_hint = Some((width, height));
        }
        self
    }

    pub fn max_scale(mut self, max: f32) -> Self {
        self.max_scale = max.max(1.0);
        self
    }

    /// Куда публиковать состояние вида.
    pub fn info(mut self, signal: RwSignal<ImageViewInfo>) -> Self {
        self.info = Some(signal);
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Widget for ImageViewport {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ImageViewportElement {
            id: ElementId::new(),
            source: self.source.clone(),
            // Команда, с которой виджет создан, уже исполнена прежним
            // элементом (или устарела) — новый её не повторяет.
            seen_command: self.command.map(|(seq, _)| seq),
            quarter_turns: self.quarter_turns,
            flip_h: self.flip_h,
            flip_v: self.flip_v,
            natural_hint: self.natural_hint,
            max_scale: self.max_scale,
            insets: self.insets,
            info: self.info,
            classes: self.classes.clone(),
            bounds: Rect::zero(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            image_handle: None,
            image_state: ImageLoadState::Loading,
            image_store: None,
            natural: None,
            scale: 1.0,
            target_scale: 1.0,
            offset: Point::zero(),
            anchor: Point::zero(),
            fit_mode: true,
            settled_once: false,
            angle: self.quarter_turns as f32 * 90.0,
            pan_drag: None,
            bar_drag: None,
            hover_bar: None,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

pub struct ImageViewportElement {
    id: ElementId,
    source: ImageSource,
    seen_command: Option<u64>,
    quarter_turns: i32,
    flip_h: bool,
    flip_v: bool,
    natural_hint: Option<(u32, u32)>,
    max_scale: f32,
    insets: [f32; 4],
    info: Option<RwSignal<ImageViewInfo>>,
    classes: Vec<String>,
    bounds: Rect,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    image_handle: Option<ImageHandle>,
    image_state: ImageLoadState,
    image_store: Option<Arc<Mutex<ImageStore>>>,
    natural: Option<(u32, u32)>,
    /// Масштаб в кадре и тот, к которому он идёт.
    scale: f32,
    target_scale: f32,
    /// Центр картинки относительно центра области.
    offset: Point,
    /// Точка (от центра области), вокруг которой идёт смена масштаба.
    anchor: Point,
    fit_mode: bool,
    /// Первая раскладка с известным размером уже была: дальше вписывание
    /// анимируется, а не ставится сразу.
    settled_once: bool,
    /// Текущий угол в градусах; цель — `quarter_turns * 90`.
    angle: f32,
    pan_drag: Option<(Point, Point)>,
    /// Ось, координата мыши вдоль оси и положение ползунка в момент нажатия.
    bar_drag: Option<(Axis, f32, f32)>,
    hover_bar: Option<Axis>,
}

impl ImageViewportElement {
    fn request_load(&mut self) {
        if let Some(ref store) = self.image_store {
            let mut store = store.lock().unwrap_or_else(|e| e.into_inner());
            let (handle, state) = store.request(&self.source);
            if let Some(old) = self.image_handle.replace(handle) {
                store.release(old);
            }
            self.image_state = state;
            if state == ImageLoadState::Ready {
                self.natural = store.dimensions(handle);
            }
        }
    }

    fn natural_size(&self) -> Option<(u32, u32)> {
        self.natural.or(self.natural_hint)
    }

    /// Размер картинки на экране при масштабе 1 — с учётом поворота.
    fn img_size(&self) -> Option<Size> {
        let (w, h) = self.natural_size()?;
        Some(if self.quarter_turns.rem_euclid(2) == 1 {
            Size::new(h as f32, w as f32)
        } else {
            Size::new(w as f32, h as f32)
        })
    }

    /// Область за вычетом полей: в ней картинка вписывается и держится.
    /// Поля, съедающие больше половины области, не учитываются — иначе в
    /// маленьком окне картинке не осталось бы места.
    fn inner(&self) -> Rect {
        let [top, right, bottom, left] = self.insets;
        let b = self.bounds;
        let (left, right) = if left + right > b.size.width * 0.5 {
            (0.0, 0.0)
        } else {
            (left, right)
        };
        let (top, bottom) = if top + bottom > b.size.height * 0.5 {
            (0.0, 0.0)
        } else {
            (top, bottom)
        };
        Rect::new(
            Point::new(b.origin.x + left, b.origin.y + top),
            Size::new(b.size.width - left - right, b.size.height - top - bottom),
        )
    }

    fn view(&self) -> Size {
        self.inner().size
    }

    fn center(&self) -> Point {
        let r = self.inner();
        Point::new(
            r.origin.x + r.size.width * 0.5,
            r.origin.y + r.size.height * 0.5,
        )
    }

    fn target_angle(&self) -> f32 {
        self.quarter_turns as f32 * 90.0
    }

    fn scale_limits(&self, img: Size) -> (f32, f32) {
        let fit = fit_scale(self.view(), img);
        ((fit * 0.25).max(0.01), self.max_scale.max(fit))
    }

    fn is_animating(&self) -> bool {
        (self.scale - self.target_scale).abs() > self.target_scale * 0.002
            || (self.angle - self.target_angle()).abs() > 0.2
            || (self.fit_mode && (self.offset.x.abs() > 0.5 || self.offset.y.abs() > 0.5))
    }

    fn set_target(&mut self, scale: f32, anchor: Point, fit: bool) {
        let Some(img) = self.img_size() else {
            return;
        };
        let (lo, hi) = self.scale_limits(img);
        self.target_scale = scale.clamp(lo, hi);
        self.anchor = anchor;
        self.fit_mode = fit;
        self.mark_dirty(DirtyFlags::RENDER);
        self.publish();
    }

    fn run(&mut self, command: ImageViewCommand, anchor: Point) {
        let Some(img) = self.img_size() else {
            return;
        };
        let view = self.view();
        let fit = fit_scale(view, img);
        match command {
            ImageViewCommand::ZoomIn => self.set_target(self.target_scale * BUTTON_STEP, anchor, false),
            ImageViewCommand::ZoomOut => {
                self.set_target(self.target_scale / BUTTON_STEP, anchor, false)
            }
            ImageViewCommand::Fit => self.set_target(fit, Point::zero(), true),
            ImageViewCommand::Fill => self.set_target(fill_scale(view, img), Point::zero(), false),
            ImageViewCommand::Actual => self.set_target(1.0, anchor, false),
            ImageViewCommand::ToggleFit => {
                let at_fit = self.fit_mode || (self.target_scale - fit).abs() < fit * 0.01;
                if !at_fit {
                    self.set_target(fit, Point::zero(), true);
                } else if (fit - 1.0).abs() < 0.01 {
                    // Вписанная и так пиксель в пиксель — приближаем вдвое.
                    self.set_target(2.0, anchor, false);
                } else {
                    self.set_target(1.0, anchor, false);
                }
            }
            ImageViewCommand::PanBy(fx, fy) => {
                let moved = Point::new(
                    self.offset.x - fx * view.width,
                    self.offset.y - fy * view.height,
                );
                self.offset = clamp_offset(moved, self.scale, view, img);
                self.mark_dirty(DirtyFlags::RENDER);
            }
        }
    }

    /// Вид под новую геометрию (размер области, поворот, загрузка).
    fn resettle(&mut self, animate: bool) {
        let Some(img) = self.img_size() else {
            return;
        };
        let view = self.view();
        if view.width <= 0.0 || view.height <= 0.0 {
            return;
        }
        if self.fit_mode {
            self.target_scale = fit_scale(view, img);
            if !animate {
                self.scale = self.target_scale;
                self.offset = Point::zero();
            }
            self.anchor = Point::zero();
        } else {
            let (lo, hi) = self.scale_limits(img);
            self.target_scale = self.target_scale.clamp(lo, hi);
            if !animate {
                self.scale = self.scale.clamp(lo, hi);
            }
        }
        self.offset = clamp_offset(self.offset, self.scale, view, img);
        self.publish();
    }

    fn publish(&self) {
        let Some(sig) = self.info else {
            return;
        };
        let fit = self
            .img_size()
            .map(|img| fit_scale(self.view(), img))
            .unwrap_or(1.0);
        let next = ImageViewInfo {
            scale: self.target_scale,
            fit_scale: fit,
            fit: self.fit_mode,
            natural: self.natural_size().unwrap_or((0, 0)),
            ready: self.image_state == ImageLoadState::Ready,
            failed: self.image_state == ImageLoadState::Failed,
        };
        if sig.get_untracked() != next {
            sig.set(next);
        }
    }

    // ── Полосы прокрутки ────────────────────────────────────────────────────

    fn track(&self, axis: Axis) -> Rect {
        let b = self.bounds;
        // Угол оставлен свободным, чтобы полосы не пересекались.
        let corner = BAR_THICKNESS + BAR_MARGIN * 2.0;
        match axis {
            Axis::X => Rect::new(
                Point::new(
                    b.origin.x + BAR_MARGIN,
                    b.origin.y + b.size.height - BAR_MARGIN - BAR_THICKNESS,
                ),
                Size::new((b.size.width - BAR_MARGIN - corner).max(0.0), BAR_THICKNESS),
            ),
            Axis::Y => Rect::new(
                Point::new(
                    b.origin.x + b.size.width - BAR_MARGIN - BAR_THICKNESS,
                    b.origin.y + BAR_MARGIN,
                ),
                Size::new(BAR_THICKNESS, (b.size.height - BAR_MARGIN - corner).max(0.0)),
            ),
        }
    }

    /// `(дорожка, содержимое, область, смещение)` вдоль оси.
    fn axis_metrics(&self, axis: Axis) -> Option<(f32, f32, f32, f32)> {
        let img = self.img_size()?;
        let track = self.track(axis);
        Some(match axis {
            Axis::X => (
                track.size.width,
                img.width * self.scale,
                self.view().width,
                self.offset.x,
            ),
            Axis::Y => (
                track.size.height,
                img.height * self.scale,
                self.view().height,
                self.offset.y,
            ),
        })
    }

    fn thumb_rect(&self, axis: Axis) -> Option<Rect> {
        let (track_len, content, view, offset) = self.axis_metrics(axis)?;
        let (pos, len) = bar_thumb(track_len, content, view, offset)?;
        let track = self.track(axis);
        Some(match axis {
            Axis::X => Rect::new(
                Point::new(track.origin.x + pos, track.origin.y),
                Size::new(len, BAR_THICKNESS),
            ),
            Axis::Y => Rect::new(
                Point::new(track.origin.x, track.origin.y + pos),
                Size::new(BAR_THICKNESS, len),
            ),
        })
    }

    /// Полоса под курсором. Зона попадания шире самой полосы: в 8 пикселей
    /// у края целиться неудобно.
    fn bar_at(&self, pos: Point) -> Option<Axis> {
        [Axis::X, Axis::Y].into_iter().find(|&axis| {
            self.thumb_rect(axis).is_some() && self.track(axis).inflate(4.0, 4.0).contains(pos)
        })
    }

    fn along(axis: Axis, p: Point) -> f32 {
        match axis {
            Axis::X => p.x,
            Axis::Y => p.y,
        }
    }

    fn set_thumb_pos(&mut self, axis: Axis, thumb_pos: f32) {
        let (Some((track_len, content, view, _)), Some(img)) =
            (self.axis_metrics(axis), self.img_size())
        else {
            return;
        };
        let o = offset_from_thumb(track_len, content, view, thumb_pos);
        let next = match axis {
            Axis::X => Point::new(o, self.offset.y),
            Axis::Y => Point::new(self.offset.x, o),
        };
        self.offset = clamp_offset(next, self.scale, self.view(), img);
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn pannable(&self) -> bool {
        self.thumb_rect(Axis::X).is_some() || self.thumb_rect(Axis::Y).is_some()
    }

    fn draw_bars(&self, list: &mut DisplayList) {
        let base = self.mss.color.unwrap_or(Color::WHITE);
        for axis in [Axis::X, Axis::Y] {
            let Some(thumb) = self.thumb_rect(axis) else {
                continue;
            };
            let active =
                self.hover_bar == Some(axis) || self.bar_drag.is_some_and(|(a, ..)| a == axis);
            let r = [BAR_THICKNESS * 0.5; 4];
            list.push_rect(
                self.track(axis),
                Color::BLACK.with_alpha(if active { 0.38 } else { 0.22 }),
                r,
            );
            list.push_rect(thumb, base.with_alpha(if active { 0.85 } else { 0.5 }), r);
        }
    }
}

impl Drop for ImageViewportElement {
    fn drop(&mut self) {
        if let (Some(store), Some(handle)) = (&self.image_store, self.image_handle) {
            if let Ok(mut store) = store.lock() {
                store.release(handle);
            }
        }
    }
}

impl Element for ImageViewportElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        let Some(w) = widget.as_any().downcast_ref::<ImageViewport>() else {
            return;
        };
        self.info = w.info;
        self.max_scale = w.max_scale;
        if self.insets != w.insets {
            self.insets = w.insets;
            self.resettle(false);
        }
        self.natural_hint = w.natural_hint;

        let source_changed = match (&self.source, &w.source) {
            (ImageSource::Path(a), ImageSource::Path(b)) => a != b,
            _ => true,
        };
        if source_changed {
            self.source = w.source.clone();
            self.natural = None;
            self.image_state = ImageLoadState::Loading;
            self.request_load();
            self.quarter_turns = w.quarter_turns;
            self.angle = self.target_angle();
            self.fit_mode = true;
            self.settled_once = false;
            self.offset = Point::zero();
            self.pan_drag = None;
            self.bar_drag = None;
        }
        if self.quarter_turns != w.quarter_turns {
            self.quarter_turns = w.quarter_turns;
            // Вписанная остаётся вписанной, приближенная — на своём
            // масштабе, но в новых границах.
            self.resettle(true);
        }
        self.flip_h = w.flip_h;
        self.flip_v = w.flip_v;

        let seq = w.command.map(|(seq, _)| seq);
        if seq != self.seen_command {
            self.seen_command = seq;
            if let (Some((_, command)), false) = (w.command, source_changed) {
                self.run(command, Point::zero());
            }
        }
        if source_changed {
            self.resettle(false);
        }
        // LAYOUT — чтобы дерево перечитало заявку на кадры анимации.
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            0.0
        };
        let h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            0.0
        };
        let size = Size::new(w, h);
        let resized = self.bounds.size != size;
        self.bounds = Rect::new(self.bounds.origin, size);
        if resized || !self.settled_once {
            // Область поменяла размер (ресайз окна, разворот): вписанную
            // картинку подгоняем сразу — анимация тут выглядела бы отставанием.
            self.resettle(false);
            self.settled_once = self.img_size().is_some() && w > 0.0 && h > 0.0;
        }
        size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if let Some(bg) = self.mss.background_color {
            if bg.a > 0.0 {
                list.push_rect(self.bounds, bg, [0.0; 4]);
            }
        }
        list.push_clip(self.bounds);

        let ready = self.image_state == ImageLoadState::Ready;
        if let (true, Some(handle), Some((nw, nh))) = (ready, self.image_handle, self.natural) {
            let c = self.center();
            let c = Point::new(c.x + self.offset.x, c.y + self.offset.y);
            let (dw, dh) = (nw as f32 * self.scale, nh as f32 * self.scale);
            let rect = Rect::new(Point::new(c.x - dw * 0.5, c.y - dh * 0.5), Size::new(dw, dh));

            let turning = (self.angle - self.target_angle()).abs() > 0.2;
            if !turning {
                // Тень — в экранных координатах, по габариту уже повёрнутой
                // картинки: внутри поворота она легла бы не с той стороны.
                if let Some(img) = self.img_size() {
                    let (sw, sh) = (img.width * self.scale, img.height * self.scale);
                    let shadow = Rect::new(
                        Point::new(c.x - sw * 0.5, c.y - sh * 0.5),
                        Size::new(sw, sh),
                    );
                    list.push_shadow(
                        shadow,
                        Color::BLACK.with_alpha(0.45),
                        36.0,
                        (0.0, 10.0),
                        [0.0; 4],
                    );
                }
            }

            let identity = self.angle.rem_euclid(360.0).abs() < 0.01 && !self.flip_h && !self.flip_v;
            if !identity {
                let t = Transform::translation(-c.x, -c.y)
                    .then_rotate(euclid::Angle::degrees(self.angle))
                    .then_scale(
                        if self.flip_h { -1.0 } else { 1.0 },
                        if self.flip_v { -1.0 } else { 1.0 },
                    )
                    .then_translate(euclid::Vector2D::new(c.x, c.y));
                list.push_transform(t);
            }
            list.push_image(
                rect,
                TextureId(handle.0),
                Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0)),
                Color::WHITE,
            );
            if !identity {
                list.pop_transform();
            }
        } else {
            let text = if self.image_state == ImageLoadState::Failed {
                crate::i18n::builtin("image.failed", "Failed to load image")
            } else {
                crate::i18n::builtin("image.loading", "Loading...")
            };
            let color = self.mss.color.unwrap_or(Color::WHITE).with_alpha(0.55);
            let c = self.center();
            let line = Rect::new(
                Point::new(self.bounds.origin.x, c.y - 10.0),
                Size::new(self.bounds.size.width, 20.0),
            );
            list.push_text_centered(&text, line, color, 13.0);
        }

        self.draw_bars(list);
        list.pop_clip();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseWheel {
                delta, position, ..
            } if self.bounds.contains(*position) => {
                let c = self.center();
                let anchor = Point::new(position.x - c.x, position.y - c.y);
                let next = self.target_scale * (delta * WHEEL_SPEED).exp();
                self.set_target(next, anchor, false);
                ctx.capture();
                EventResult::Handled
            }
            Event::DoubleClick {
                button: MouseButton::Left,
                position,
            } if self.bounds.contains(*position) => {
                if self.bar_at(*position).is_some() {
                    return EventResult::Handled;
                }
                let c = self.center();
                self.pan_drag = None;
                self.run(
                    ImageViewCommand::ToggleFit,
                    Point::new(position.x - c.x, position.y - c.y),
                );
                EventResult::Handled
            }
            Event::MouseDown {
                button: MouseButton::Left | MouseButton::Middle,
                position,
            } if self.bounds.contains(*position) => {
                if let Some(axis) = self.bar_at(*position) {
                    let track = self.track(axis);
                    let along = Self::along(axis, *position);
                    let origin = Self::along(axis, track.origin);
                    let inside = self.thumb_rect(axis).filter(|t| t.inflate(4.0, 4.0).contains(*position));
                    let thumb_pos = match inside {
                        Some(t) => Self::along(axis, t.origin) - origin,
                        None => {
                            // Щелчок мимо ползунка — ползунок встаёт серединой
                            // под курсор и дальше тянется как обычно.
                            let len = self
                                .thumb_rect(axis)
                                .map(|t| Self::along(axis, Point::new(t.size.width, t.size.height)))
                                .unwrap_or(0.0);
                            let pos = along - origin - len * 0.5;
                            self.set_thumb_pos(axis, pos);
                            pos
                        }
                    };
                    self.bar_drag = Some((axis, along, thumb_pos));
                } else if self.pannable() {
                    self.pan_drag = Some((*position, self.offset));
                    ctx.set_cursor(CursorIcon::Grabbing);
                }
                ctx.capture();
                EventResult::Handled
            }
            Event::MouseMove(pos) => {
                if let Some((axis, start, thumb0)) = self.bar_drag {
                    self.set_thumb_pos(axis, thumb0 + Self::along(axis, *pos) - start);
                    return EventResult::Handled;
                }
                if let Some((start, offset0)) = self.pan_drag {
                    if let Some(img) = self.img_size() {
                        let moved =
                            Point::new(offset0.x + pos.x - start.x, offset0.y + pos.y - start.y);
                        self.offset = clamp_offset(moved, self.scale, self.view(), img);
                        // Перетаскивание во время анимации масштаба иначе
                        // боролось бы с якорем.
                        self.anchor = Point::zero();
                        self.fit_mode = false;
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    ctx.set_cursor(CursorIcon::Grabbing);
                    return EventResult::Handled;
                }
                let hover = self.bar_at(*pos);
                if hover != self.hover_bar {
                    self.hover_bar = hover;
                    self.mark_dirty(DirtyFlags::RENDER);
                }
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(if hover.is_none() && self.pannable() {
                        CursorIcon::Grab
                    } else {
                        CursorIcon::Default
                    });
                }
                EventResult::Ignored
            }
            Event::MouseUp { .. } => {
                let had = self.bar_drag.take().is_some() | self.pan_drag.take().is_some();
                if had {
                    ctx.set_cursor(CursorIcon::Default);
                    self.mark_dirty(DirtyFlags::RENDER);
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let mut active = false;

        if self.image_state == ImageLoadState::Loading {
            active = true;
            let mut next = None;
            if let (Some(store), Some(handle)) = (&self.image_store, self.image_handle) {
                let store = store.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(state) = store.state_of(handle) {
                    if state != self.image_state {
                        next = Some((state, store.dimensions(handle)));
                    }
                }
            }
            if let Some((state, dims)) = next {
                self.image_state = state;
                if dims.is_some() {
                    self.natural = dims;
                }
                self.resettle(false);
                self.publish();
                self.mark_dirty(DirtyFlags::RENDER);
            }
        }

        if self.is_animating() {
            active = true;
            let k = 1.0 - (-dt.as_secs_f32() * EASE_RATE).exp();

            let old = self.scale;
            let mut next = old + (self.target_scale - old) * k;
            if (next - self.target_scale).abs() <= self.target_scale * 0.002 {
                next = self.target_scale;
            }
            self.offset = zoom_offset(self.offset, old, next, self.anchor);
            self.scale = next;
            if self.fit_mode {
                self.offset = Point::new(self.offset.x * (1.0 - k), self.offset.y * (1.0 - k));
                if self.offset.x.abs() <= 0.5 && self.offset.y.abs() <= 0.5 {
                    self.offset = Point::zero();
                }
            }

            let ta = self.target_angle();
            self.angle += (ta - self.angle) * k;
            if (self.angle - ta).abs() <= 0.2 {
                self.angle = ta;
            }

            if let Some(img) = self.img_size() {
                self.offset = clamp_offset(self.offset, self.scale, self.view(), img);
            }
            self.mark_dirty(DirtyFlags::RENDER);
        }
        active
    }

    fn wants_animate_tick(&self) -> bool {
        self.image_state == ImageLoadState::Loading || self.is_animating()
    }

    fn clip_content(&self) -> bool {
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
        self.request_load();
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "ImageViewport"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER);
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
        let label = match &self.source {
            ImageSource::Path(p) => p.clone(),
            ImageSource::Bytes { key, .. } | ImageSource::RawRgba { key, .. } => key.clone(),
            ImageSource::Url(url) => url.clone(),
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

impl StyledElement for ImageViewportElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
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

    fn sz(w: f32, h: f32) -> Size {
        Size::new(w, h)
    }

    #[test]
    fn fit_shrinks_large_and_keeps_small() {
        assert!((fit_scale(sz(1000.0, 500.0), sz(2000.0, 500.0)) - 0.5).abs() < 1e-6);
        assert!((fit_scale(sz(1000.0, 500.0), sz(500.0, 1000.0)) - 0.5).abs() < 1e-6);
        // Маленькую не растягиваем.
        assert_eq!(fit_scale(sz(1000.0, 500.0), sz(100.0, 100.0)), 1.0);
        // Вырожденные размеры не дают NaN/inf.
        assert_eq!(fit_scale(sz(0.0, 500.0), sz(100.0, 100.0)), 1.0);
        assert_eq!(fit_scale(sz(100.0, 100.0), sz(0.0, 0.0)), 1.0);
    }

    #[test]
    fn fill_covers_the_view() {
        let s = fill_scale(sz(1000.0, 500.0), sz(500.0, 500.0));
        assert!((s - 2.0).abs() < 1e-6);
    }

    #[test]
    fn small_content_is_centered_large_is_limited() {
        let view = sz(800.0, 600.0);
        let img = sz(400.0, 2000.0);
        let o = clamp_offset(Point::new(300.0, 5000.0), 1.0, view, img);
        assert_eq!(o.x, 0.0, "ось, где картинка меньше области, — по центру");
        assert_eq!(o.y, 700.0, "край картинки не отходит от края области");
        let o = clamp_offset(Point::new(0.0, -5000.0), 1.0, view, img);
        assert_eq!(o.y, -700.0);
    }

    #[test]
    fn zoom_keeps_anchor_over_same_pixel() {
        let offset = Point::new(40.0, -25.0);
        let anchor = Point::new(120.0, 80.0);
        let (s0, s1) = (0.5, 1.75);
        // Точка картинки под якорем (от её центра, в пикселях картинки).
        let before = ((anchor.x - offset.x) / s0, (anchor.y - offset.y) / s0);
        let next = zoom_offset(offset, s0, s1, anchor);
        let after = ((anchor.x - next.x) / s1, (anchor.y - next.y) / s1);
        assert!((before.0 - after.0).abs() < 1e-3);
        assert!((before.1 - after.1).abs() < 1e-3);
    }

    #[test]
    fn bar_appears_only_when_content_overflows() {
        assert!(bar_thumb(500.0, 400.0, 500.0, 0.0).is_none());
        assert!(bar_thumb(500.0, 500.0, 500.0, 0.0).is_none());
        let (pos, len) = bar_thumb(500.0, 1000.0, 500.0, 0.0).unwrap();
        assert!((len - 250.0).abs() < 1e-3);
        assert!((pos - 125.0).abs() < 1e-3, "по центру — ползунок посередине");
        // Картинка сдвинута вправо до упора — видно её левый край.
        let (pos, _) = bar_thumb(500.0, 1000.0, 500.0, 250.0).unwrap();
        assert!(pos.abs() < 1e-3);
        let (pos, len) = bar_thumb(500.0, 1000.0, 500.0, -250.0).unwrap();
        assert!((pos + len - 500.0).abs() < 1e-3);
    }

    #[test]
    fn thumb_position_round_trips() {
        let (track, content, view) = (480.0, 3000.0, 600.0);
        for offset in [-1200.0, -333.0, 0.0, 10.0, 1200.0] {
            let (pos, _) = bar_thumb(track, content, view, offset).unwrap();
            let back = offset_from_thumb(track, content, view, pos);
            assert!((back - offset).abs() < 0.5, "{offset} → {pos} → {back}");
        }
    }

    // ── Элемент в харнессе: картинки нет (стора в харнессе нет), размер —
    //    из natural_size, этого арифметике вида достаточно. ─────────────────

    use crate::testing::TestHarness;

    fn harness(w: ImageViewport) -> (TestHarness, ElementId) {
        let mut h = TestHarness::new(Box::new(w));
        h.layout(800.0, 600.0);
        let id = h.find_by_type_name("ImageViewport")[0];
        (h, id)
    }

    fn settle(h: &mut TestHarness) {
        for _ in 0..200 {
            h.animate(Duration::from_millis(16));
        }
    }

    #[test]
    fn opens_fitted_and_wheel_zooms_with_animation() {
        let info = crate::signal::use_signal(ImageViewInfo::default());
        let (mut h, id) = harness(
            ImageViewport::new("/nonexistent.png")
                .natural_size(1600, 600)
                .info(info),
        );
        let got = info.get_untracked();
        assert!(got.fit);
        assert!((got.scale - 0.5).abs() < 1e-6, "1600 в 800 — половина");

        h.send_event(&Event::MouseWheel {
            delta: 120.0,
            delta_x: 0.0,
            position: Point::new(400.0, 300.0),
        });
        let got = info.get_untracked();
        assert!(!got.fit);
        assert!(got.scale > 0.5);
        assert!(h.is_animating(id), "после колеса элемент обязан получать кадры");
        settle(&mut h);
    }

    #[test]
    fn command_runs_once_per_sequence_number() {
        let info = crate::signal::use_signal(ImageViewInfo::default());
        let make = |seq: u64, cmd| {
            Box::new(
                ImageViewport::new("/nonexistent.png")
                    .natural_size(1600, 600)
                    .info(info)
                    .command(seq, cmd),
            ) as Box<dyn Widget>
        };
        let mut h = TestHarness::new(make(0, ImageViewCommand::Actual));
        h.layout(800.0, 600.0);
        // Команда, с которой элемент создан, не исполняется.
        assert!(info.get_untracked().fit);

        h.update_widget(make(1, ImageViewCommand::Actual));
        h.layout(800.0, 600.0);
        assert_eq!(info.get_untracked().scale, 1.0);
        assert!(!info.get_untracked().fit);

        // Тот же номер — пересборка без команды: масштаб не трогаем.
        h.update_widget(make(1, ImageViewCommand::Fit));
        h.layout(800.0, 600.0);
        assert_eq!(info.get_untracked().scale, 1.0);

        h.update_widget(make(2, ImageViewCommand::Fit));
        h.layout(800.0, 600.0);
        assert!(info.get_untracked().fit);
    }

    #[test]
    fn fitted_image_follows_view_resize_and_rotation() {
        let info = crate::signal::use_signal(ImageViewInfo::default());
        let make = |turns: i32| {
            Box::new(
                ImageViewport::new("/nonexistent.png")
                    .natural_size(1600, 600)
                    .info(info)
                    .quarter_turns(turns),
            ) as Box<dyn Widget>
        };
        let mut h = TestHarness::new(make(0));
        h.layout(800.0, 600.0);
        h.layout(400.0, 600.0);
        assert!((info.get_untracked().scale - 0.25).abs() < 1e-6);

        // Повёрнутая на четверть: 600×1600 в 400×600 — по высоте.
        h.update_widget(make(1));
        h.layout(400.0, 600.0);
        assert!((info.get_untracked().scale - 0.375).abs() < 1e-6);
    }

    #[test]
    fn insets_shrink_the_fit_area() {
        let info = crate::signal::use_signal(ImageViewInfo::default());
        let (_h, _) = harness(
            ImageViewport::new("/nonexistent.png")
                .natural_size(1600, 1200)
                .insets(0.0, 0.0, 120.0, 0.0)
                .info(info),
        );
        // 800×600 без нижних 120 — 800×480: по высоте 480/1200.
        assert!((info.get_untracked().scale - 0.4).abs() < 1e-6);
    }

    #[test]
    fn double_click_toggles_fit_and_actual() {
        let info = crate::signal::use_signal(ImageViewInfo::default());
        let (mut h, _) = harness(
            ImageViewport::new("/nonexistent.png")
                .natural_size(1600, 600)
                .info(info),
        );
        let at = Point::new(200.0, 300.0);
        let dbl = Event::DoubleClick {
            button: MouseButton::Left,
            position: at,
        };
        h.send_event(&dbl);
        assert_eq!(info.get_untracked().scale, 1.0);
        settle(&mut h);
        h.send_event(&dbl);
        assert!(info.get_untracked().fit);
    }
}
