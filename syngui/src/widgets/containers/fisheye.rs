//! Масштаб с учётом раскладки ([`ScaleBox`]) и «рыбий глаз» ([`Fisheye`]) —
//! ряд элементов, которые плавно увеличиваются под указателем, как значки
//! дока macOS.
//!
//! `ScaleBox` раскладывает ребёнка в его естественном размере, а занимает
//! место `размер × масштаб`: соседи расступаются, рисунок увеличивается
//! трансформацией (картинки и SVG остаются чёткими), события мыши переводятся
//! обратно в координаты ребёнка.
//!
//! `Fisheye` — ряд или колонка таких коробок. Увеличение считается по
//! расстоянию от указателя до центров элементов в *неувеличенной* раскладке
//! (без обратной связи — значки не «дрожат»), каждый элемент догоняет свою
//! цель пружиной. Параметры — построителем или из MSS:
//!
//! ```css
//! .dock-items {
//!     magnification: 1.8;           /* во сколько раз растёт значок под курсором (1 — выкл.) */
//!     magnification-range: 3;        /* радиус влияния — в размерах элемента */
//!     magnification-falloff: cosine; /* cosine | gaussian | linear */
//!     magnification-speed: 16;       /* скорость догоняния, 1/с */
//!     gap: 6px;
//! }
//! ```

use super::IntoWidget;
use crate::core::sync::Mutex;
use crate::core::{Point, Rect, Size, Transform};
use crate::input::{Event, EventResult};
use crate::layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

// ─── ScaleBox ───────────────────────────────────────────────────────────────

/// Общее состояние `Fisheye` и его элементов.
#[derive(Default)]
struct FisheyeShared {
    /// Естественные размеры элементов (сообщают сами элементы при раскладке).
    natural: Vec<Size>,
    /// Целевые масштабы (считает `Fisheye` по указателю).
    targets: Vec<f32>,
    /// Скорость догоняния, 1/с.
    speed: f32,
}

impl FisheyeShared {
    fn ensure(&mut self, n: usize) {
        if self.natural.len() < n {
            self.natural.resize(n, Size::zero());
        }
        if self.targets.len() < n {
            self.targets.resize(n, 1.0);
        }
    }
}

/// Откуда коробка берёт масштаб.
#[derive(Clone)]
enum ScaleSource {
    Fixed(f32),
    Fisheye(Arc<Mutex<FisheyeShared>>, usize),
}

/// Как увеличение влияет на раскладку.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grow {
    /// Место растёт по обеим осям (рисунок — от левого верхнего угла).
    Both,
    /// Место растёт только вдоль ряда; поперёк рисунок выходит за границы
    /// (полоса дока остаётся той же высоты, значки «выскакивают» над ней).
    /// `vertical` — ряд вертикальный; `anchor` — неподвижная точка поперёк:
    /// 0 — начало (растёт к концу), 1 — конец (растёт к началу), 0.5 — центр.
    Overflow { vertical: bool, anchor: f32 },
}

/// Ребёнок в естественном размере, увеличенный (уменьшенный) вместе с
/// местом, которое он занимает в раскладке.
pub struct ScaleBox {
    child: Box<dyn Widget>,
    source: ScaleSource,
    grow: Grow,
    /// Плавный переход к новому масштабу (для `Fixed`), 1/с; 0 — сразу.
    speed: f32,
    classes: Vec<String>,
}

impl ScaleBox {
    pub fn new<M>(child: impl IntoWidget<M>) -> Self {
        Self {
            child: child.into_widget(),
            source: ScaleSource::Fixed(1.0),
            grow: Grow::Both,
            speed: 0.0,
            classes: Vec::new(),
        }
    }

    pub fn scale(mut self, s: f32) -> Self {
        self.source = ScaleSource::Fixed(s.max(0.01));
        self
    }

    /// Менять масштаб плавно (экспоненциально, `speed` — 1/с).
    pub fn smooth(mut self, speed: f32) -> Self {
        self.speed = speed.max(0.0);
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, c.into());
        self
    }
}

impl Widget for ScaleBox {
    fn create_element(&self) -> Box<dyn Element> {
        let start = match &self.source {
            ScaleSource::Fixed(s) => *s,
            ScaleSource::Fisheye(..) => 1.0,
        };
        Box::new(ScaleBoxElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            child_size: Size::zero(),
            source: self.source.clone(),
            grow: self.grow,
            speed: self.speed,
            current: start,
            classes: self.classes.clone(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
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

    fn mount(&self, tree: &mut ElementTree, parent_id: ElementId) {
        let el = self.child.create_element();
        let id = tree.insert_with_type_id(el, Some(parent_id), self.child.as_any().type_id());
        self.child.mount(tree, id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![self.child.as_ref()]
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

struct ScaleBoxElement {
    id: ElementId,
    bounds: Rect,
    child_size: Size,
    source: ScaleSource,
    grow: Grow,
    speed: f32,
    /// Действующий масштаб.
    current: f32,
    classes: Vec<String>,
    dirty: DirtyFlags,
    mss: MssFields,
}

impl ScaleBoxElement {
    fn target(&self) -> f32 {
        match &self.source {
            ScaleSource::Fixed(s) => *s,
            ScaleSource::Fisheye(shared, i) => {
                shared.lock().unwrap_or_else(|e| e.into_inner()).targets.get(*i).copied().unwrap_or(1.0)
            }
        }
    }

    fn speed(&self) -> f32 {
        match &self.source {
            ScaleSource::Fixed(_) => self.speed,
            ScaleSource::Fisheye(shared, _) => shared.lock().unwrap_or_else(|e| e.into_inner()).speed,
        }
    }

    /// Неподвижная точка масштабирования (ребёнок лежит в `bounds.origin`).
    fn anchor(&self) -> Point {
        let o = self.bounds.origin;
        match self.grow {
            Grow::Both => o,
            Grow::Overflow { vertical: false, anchor } => Point::new(o.x, o.y + self.child_size.height * anchor),
            Grow::Overflow { vertical: true, anchor } => Point::new(o.x + self.child_size.width * anchor, o.y),
        }
    }

    fn transform(&self) -> Transform {
        let a = self.anchor();
        let s = self.current;
        Transform::translation(-a.x, -a.y).then_scale(s, s).then_translate(euclid::Vector2D::new(a.x, a.y))
    }
}

impl Element for ScaleBoxElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<ScaleBox>() {
            self.source = w.source.clone();
            self.grow = w.grow;
            self.speed = w.speed;
            if self.speed <= 0.0 {
                if let ScaleSource::Fixed(s) = self.source {
                    self.current = s;
                }
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    /// Вызывается после измерения ребёнка (`set_content_size`): место —
    /// естественный размер × масштаб.
    fn layout(&mut self, _constraints: Constraints) -> Size {
        if let ScaleSource::Fisheye(shared, i) = &self.source {
            let mut sh = shared.lock().unwrap_or_else(|e| e.into_inner());
            sh.ensure(i + 1);
            sh.natural[*i] = self.child_size;
        }
        let s = self.current;
        let (w, h) = (self.child_size.width, self.child_size.height);
        let size = match self.grow {
            Grow::Both => Size::new(w * s, h * s),
            Grow::Overflow { vertical: false, .. } => Size::new(w * s, h),
            Grow::Overflow { vertical: true, .. } => Size::new(w, h * s),
        };
        self.bounds.size = size;
        size
    }

    fn set_content_size(&mut self, size: Size) {
        self.child_size = size;
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::AnimatedSize
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        list.push_transform(self.transform());
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        list.pop_transform();
    }

    fn scroll_offset(&self) -> Point {
        // Событие для ребёнка: p' = A + (p − A) / s = (p + A·(s − 1)) / s,
        // A — неподвижная точка.
        let a = self.anchor();
        Point::new(a.x * (self.current - 1.0), a.y * (self.current - 1.0))
    }

    fn event_scale(&self) -> f32 {
        self.current.max(f32::EPSILON)
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn wants_animate_tick(&self) -> bool {
        (self.target() - self.current).abs() > 1e-3
    }

    fn animate(&mut self, dt: Duration) -> bool {
        // анимации выключены глобально — переход за один кадр
        let dt = crate::animation::effective_dt(dt);
        let target = self.target();
        if (target - self.current).abs() <= 1e-3 {
            if self.current != target {
                self.current = target;
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
                return true;
            }
            return false;
        }
        let speed = self.speed();
        self.current = if speed <= 0.0 {
            target
        } else {
            let k = 1.0 - (-speed * dt.as_secs_f32()).exp();
            self.current + (target - self.current) * k
        };
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        true
    }

    fn needs_repaint(&self) -> bool {
        self.wants_animate_tick()
    }

    fn passthrough_hit_test(&self) -> bool {
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
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }
    fn get_classes(&self) -> &[String] {
        &self.classes
    }
    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
    }
    fn element_type_name(&self) -> &str {
        "ScaleBox"
    }
}

// ─── Fisheye ────────────────────────────────────────────────────────────────

/// Как убывает увеличение с расстоянием от указателя.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Falloff {
    /// Половина косинуса — мягкий «колокол», как в доке macOS.
    #[default]
    Cosine,
    /// Гауссиана — острее у центра, длиннее хвосты.
    Gaussian,
    /// Линейно до нуля на границе радиуса.
    Linear,
}

impl Falloff {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "cosine" | "cos" => Some(Self::Cosine),
            "gaussian" | "gauss" => Some(Self::Gaussian),
            "linear" => Some(Self::Linear),
            _ => None,
        }
    }

    /// Вес 0..1 на нормированном расстоянии `d` (1 — граница радиуса).
    pub fn weight(self, d: f32) -> f32 {
        let d = d.abs();
        match self {
            Self::Cosine => {
                if d >= 1.0 {
                    0.0
                } else {
                    (1.0 + (std::f32::consts::PI * d).cos()) * 0.5
                }
            }
            Self::Gaussian => (-4.0 * d * d).exp(),
            Self::Linear => (1.0 - d).max(0.0),
        }
    }
}

/// Где остаётся неувеличенный ряд, когда он растёт.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FisheyeAnchor {
    Start,
    /// Ряд растёт в обе стороны от центра (док по центру края).
    #[default]
    Center,
    End,
}

/// Ряд (колонка) с увеличением элементов под указателем.
pub struct Fisheye {
    children: Vec<Box<dyn Widget>>,
    shared: Arc<Mutex<FisheyeShared>>,
    vertical: bool,
    gap: f32,
    zoom: f32,
    range: f32,
    falloff: Falloff,
    speed: f32,
    anchor: Option<FisheyeAnchor>,
    cross_align: CrossAxisAlignment,
    main_align: MainAxisAlignment,
    overflow: bool,
    on_hover: Option<HoverCb>,
    classes: Vec<String>,
}

/// Наведение: индекс элемента под указателем и его прямоугольник после
/// увеличения (координаты ряда), `None` — указатель ушёл.
type HoverCb = Arc<Mutex<dyn FnMut(Option<(usize, Rect)>) + Send>>;

impl Default for Fisheye {
    fn default() -> Self {
        Self::new()
    }
}

impl Fisheye {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            shared: Arc::new(Mutex::new(FisheyeShared { speed: 16.0, ..Default::default() })),
            vertical: false,
            gap: 0.0,
            zoom: 1.8,
            range: 3.0,
            falloff: Falloff::Cosine,
            speed: 16.0,
            anchor: None,
            cross_align: CrossAxisAlignment::End,
            main_align: MainAxisAlignment::Start,
            overflow: false,
            on_hover: None,
            classes: Vec::new(),
        }
    }

    /// Поперёк ряда увеличенные элементы выходят за его границы, а не
    /// раздвигают его: высота полосы дока постоянна, значки вырастают над
    /// ней (в сторону, противоположную `cross_axis_alignment`).
    pub fn overflow(mut self, on: bool) -> Self {
        self.overflow = on;
        self.finish_children();
        self
    }

    /// Элемент под указателем сменился (или сдвинулся) — с прямоугольником,
    /// который он займёт после увеличения: для подписи над значком и
    /// привязки всплывающих окон.
    pub fn on_hover(mut self, f: impl FnMut(Option<(usize, Rect)>) + Send + 'static) -> Self {
        self.on_hover = Some(Arc::new(Mutex::new(f)));
        self
    }

    fn grow(&self) -> Grow {
        if !self.overflow {
            return Grow::Both;
        }
        let anchor = match self.cross_align {
            CrossAxisAlignment::Start => 0.0,
            CrossAxisAlignment::Center => 0.5,
            _ => 1.0,
        };
        Grow::Overflow { vertical: self.vertical, anchor }
    }

    /// Колонка вместо ряда (док у левого/правого края).
    pub fn vertical(mut self, v: bool) -> Self {
        self.vertical = v;
        self.finish_children();
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Максимальное увеличение (1 — выключено).
    pub fn zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom.max(1.0);
        self
    }

    /// Радиус влияния указателя — в размерах элемента.
    pub fn range(mut self, range: f32) -> Self {
        self.range = range.max(0.5);
        self
    }

    pub fn falloff(mut self, f: Falloff) -> Self {
        self.falloff = f;
        self
    }

    /// Скорость, с которой масштаб догоняет цель, 1/с (больше — резче).
    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = speed.max(0.0);
        self
    }

    /// Где неувеличенный ряд внутри своих границ (для расчёта центров).
    /// По умолчанию — по центру: ряд занимает место по содержимому
    /// (`main_axis_alignment` = `Start`), а родитель держит его по центру.
    /// Ряд, прижатый к краю, — `Start`/`End`.
    pub fn anchor(mut self, a: FisheyeAnchor) -> Self {
        self.anchor = Some(a);
        self
    }

    /// Выравнивание поперёк ряда: `End` — значки растут вверх от нижнего
    /// края (док снизу), `Start` — вниз (док сверху).
    pub fn cross_axis_alignment(mut self, a: CrossAxisAlignment) -> Self {
        self.cross_align = a;
        self.finish_children();
        self
    }

    pub fn main_axis_alignment(mut self, a: MainAxisAlignment) -> Self {
        self.main_align = a;
        self
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        let i = self.children.len();
        self.children.push(Box::new(ScaleBox {
            child: child.into_widget(),
            source: ScaleSource::Fisheye(self.shared.clone(), i),
            grow: self.grow(),
            speed: 0.0,
            classes: vec!["fisheye-item".into()],
        }));
        self
    }

    /// Проставить элементам режим роста (он зависит от `overflow`,
    /// направления и выравнивания, которые могли задать после `child`).
    fn finish_children(&mut self) {
        let grow = self.grow();
        for c in &mut self.children {
            if let Some(sb) = c.as_any_mut().downcast_mut::<ScaleBox>() {
                sb.grow = grow;
            }
        }
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Box<dyn Widget>>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, c.into());
        self
    }
}

impl Widget for Fisheye {
    fn create_element(&self) -> Box<dyn Element> {
        let el = FisheyeElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            shared: self.shared.clone(),
            count: self.children.len(),
            vertical: self.vertical,
            gap: self.gap,
            zoom: self.zoom,
            range: self.range,
            falloff: self.falloff,
            speed: self.speed,
            anchor: self.anchor,
            cross_align: self.cross_align,
            main_align: self.main_align,
            overflow: self.overflow,
            on_hover: self.on_hover.clone(),
            last_hover: None,
            mss_zoom: None,
            mss_range: None,
            mss_falloff: None,
            mss_speed: None,
            pointer: None,
            classes: self.classes.clone(),
            dirty: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        };
        el.sync_shared();
        Box::new(el)
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
        for child in &self.children {
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.children.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

struct FisheyeElement {
    id: ElementId,
    bounds: Rect,
    shared: Arc<Mutex<FisheyeShared>>,
    count: usize,
    vertical: bool,
    gap: f32,
    zoom: f32,
    range: f32,
    falloff: Falloff,
    speed: f32,
    anchor: Option<FisheyeAnchor>,
    cross_align: CrossAxisAlignment,
    main_align: MainAxisAlignment,
    overflow: bool,
    on_hover: Option<HoverCb>,
    /// Что последним отдали в `on_hover` (не звать с тем же).
    last_hover: Option<(usize, [i32; 4])>,
    mss_zoom: Option<f32>,
    mss_range: Option<f32>,
    mss_falloff: Option<Falloff>,
    mss_speed: Option<f32>,
    /// Последнее положение указателя (координаты элемента).
    pointer: Option<Point>,
    classes: Vec<String>,
    dirty: DirtyFlags,
    mss: MssFields,
}

impl FisheyeElement {
    fn zoom(&self) -> f32 {
        self.mss_zoom.unwrap_or(self.zoom).max(1.0)
    }

    fn sync_shared(&self) {
        let mut sh = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        sh.speed = self.mss_speed.unwrap_or(self.speed);
        sh.ensure(self.count);
    }

    /// Доля поперечного размера, в сторону начала которой растут элементы
    /// (см. `Grow::Overflow::anchor`).
    fn cross_anchor(&self) -> f32 {
        match self.cross_align {
            CrossAxisAlignment::Start => 0.0,
            CrossAxisAlignment::Center => 0.5,
            _ => 1.0,
        }
    }

    /// Область, где указатель «над рядом»: границы плюс место, куда
    /// вырастают элементы при `overflow`.
    fn hover_area(&self) -> Rect {
        if !self.overflow {
            return self.bounds;
        }
        let sh = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let cross = sh.natural[..self.count.min(sh.natural.len())]
            .iter()
            .map(|s| if self.vertical { s.width } else { s.height })
            .fold(0.0f32, f32::max);
        let extra = cross * (self.zoom() - 1.0);
        let a = self.cross_anchor();
        let (before, after) = (extra * a, extra * (1.0 - a));
        let b = self.bounds;
        if self.vertical {
            Rect::new(Point::new(b.origin.x - before, b.origin.y), Size::new(b.size.width + before + after, b.size.height))
        } else {
            Rect::new(Point::new(b.origin.x, b.origin.y - before), Size::new(b.size.width, b.size.height + before + after))
        }
    }

    /// Пересчитать цели по указателю. Возвращает, изменилась ли какая-то
    /// цель, и элемент под указателем с прямоугольником после увеличения.
    fn retarget(&mut self) -> (bool, Option<(usize, Rect)>) {
        let zoom = self.zoom();
        let range = self.mss_range.unwrap_or(self.range).max(0.5);
        let falloff = self.mss_falloff.unwrap_or(self.falloff);
        let gap = self.mss.gap.unwrap_or(self.gap);
        let pad = self.mss.padding_ltrb([0.0; 4]);
        let area = self.hover_area();
        let inside = self.pointer.filter(|p| area.contains(*p));
        let mut sh = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        sh.ensure(self.count);
        let n = self.count;
        let vertical = self.vertical;
        let main = |s: Size| if vertical { s.height } else { s.width };
        let cross = |s: Size| if vertical { s.width } else { s.height };
        // Центры элементов в неувеличенной раскладке.
        let total: f32 = sh.natural[..n].iter().map(|s| main(*s)).sum::<f32>() + gap * n.saturating_sub(1) as f32;
        let (b0, blen, pad0, pad1) = if vertical {
            (self.bounds.origin.y, self.bounds.size.height, pad[1], pad[3])
        } else {
            (self.bounds.origin.x, self.bounds.size.width, pad[0], pad[2])
        };
        // По умолчанию — центр: ряд по размеру содержимого, который родитель
        // держит по центру (док), растёт в обе стороны и центр не сдвигается.
        let anchor = self.anchor.unwrap_or(FisheyeAnchor::Center);
        let start = match anchor {
            FisheyeAnchor::Start => b0 + pad0,
            FisheyeAnchor::Center => b0 + pad0 + (blen - pad0 - pad1 - total) / 2.0,
            FisheyeAnchor::End => b0 + blen - pad1 - total,
        };
        let u = inside.map(|p| if vertical { p.y } else { p.x });
        let mut changed = false;
        let mut pos = start;
        let mut hovered: Option<(usize, f32)> = None;
        for i in 0..n {
            let len = main(sh.natural[i]);
            let center = pos + len / 2.0;
            pos += len + gap;
            let target = match u {
                Some(u) if zoom > 1.0 => {
                    let radius = len.max(1.0) * range;
                    1.0 + (zoom - 1.0) * falloff.weight((u - center) / radius)
                }
                _ => 1.0,
            };
            if let Some(u) = u {
                let d = (u - center).abs();
                if hovered.map_or(true, |(_, best)| d < best) {
                    hovered = Some((i, d));
                }
            }
            if (sh.targets[i] - target).abs() > 1e-4 {
                sh.targets[i] = target;
                changed = true;
            }
        }
        // Где окажется элемент под указателем, когда масштабы дойдут до целей.
        let hovered = hovered.map(|(i, _)| {
            let t = &sh.targets;
            let final_total: f32 =
                (0..n).map(|j| main(sh.natural[j]) * t[j]).sum::<f32>() + gap * n.saturating_sub(1) as f32;
            let fstart = match anchor {
                FisheyeAnchor::Start => b0 + pad0,
                FisheyeAnchor::Center => b0 + blen / 2.0 - (final_total + pad0 + pad1) / 2.0 + pad0,
                FisheyeAnchor::End => b0 + blen - pad1 - final_total,
            };
            let m0 = fstart + (0..i).map(|j| main(sh.natural[j]) * t[j] + gap).sum::<f32>();
            let mlen = main(sh.natural[i]) * t[i];
            let nc = cross(sh.natural[i]);
            let (c0, clen) = if vertical {
                (self.bounds.origin.x + pad[0], self.bounds.size.width - pad[0] - pad[2])
            } else {
                (self.bounds.origin.y + pad[1], self.bounds.size.height - pad[1] - pad[3])
            };
            let a = self.cross_anchor();
            // Поперёк: элемент лежит у начала/центра/конца, растёт от якоря.
            let item_c0 = c0 + (clen - nc) * a;
            let (cc0, cclen) = if self.overflow {
                (item_c0 + nc * a * (1.0 - t[i]), nc * t[i])
            } else {
                (c0 + (clen - nc * t[i]) * a, nc * t[i])
            };
            let r = if vertical {
                Rect::new(Point::new(cc0, m0), Size::new(cclen, mlen))
            } else {
                Rect::new(Point::new(m0, cc0), Size::new(mlen, cclen))
            };
            (i, r)
        });
        (changed, hovered)
    }

    fn report_hover(&mut self, hovered: Option<(usize, Rect)>) {
        let Some(cb) = self.on_hover.clone() else { return };
        let key = hovered.map(|(i, r)| {
            (i, [r.origin.x.round() as i32, r.origin.y.round() as i32, r.size.width.round() as i32, r.size.height.round() as i32])
        });
        if key == self.last_hover {
            return;
        }
        self.last_hover = key;
        let mut f = cb.lock().unwrap_or_else(|e| e.into_inner());
        f(hovered);
    }
}

impl Element for FisheyeElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<Fisheye>() {
            // Новый виджет — новое общее состояние (его получили и элементы).
            // Цели переносим, чтобы увеличение не сбрасывалось при пересборке.
            let old = self.shared.lock().unwrap_or_else(|e| e.into_inner()).targets.clone();
            self.shared = w.shared.clone();
            {
                let mut sh = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                sh.ensure(w.children.len());
                for (i, t) in old.into_iter().take(w.children.len()).enumerate() {
                    sh.targets[i] = t;
                }
            }
            self.count = w.children.len();
            self.vertical = w.vertical;
            self.gap = w.gap;
            self.zoom = w.zoom;
            self.range = w.range;
            self.falloff = w.falloff;
            self.speed = w.speed;
            self.anchor = w.anchor;
            self.cross_align = w.cross_align;
            self.main_align = w.main_align;
            self.overflow = w.overflow;
            self.on_hover = w.on_hover.clone();
            self.sync_shared();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { constraints.min_width.max(0.0) };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { constraints.min_height.max(0.0) };
        self.bounds.size = Size::new(w, h);
        self.bounds.size
    }

    fn layout_hint(&self) -> LayoutHint {
        let pad = self.mss.padding_ltrb([0.0; 4]);
        let gap = self.mss.gap.unwrap_or(self.gap);
        if self.vertical {
            LayoutHint::Column {
                gap,
                cross_align: self.cross_align,
                main_align: self.main_align,
                padding_left: pad[0],
                padding_top: pad[1],
                padding_right: pad[2],
                padding_bottom: pad[3],
                expand: false,
            }
        } else {
            LayoutHint::Row {
                gap,
                offset_x: 0.0,
                cross_align: self.cross_align,
                main_align: self.main_align,
                padding_left: pad[0],
                padding_top: pad[1],
                padding_right: pad[2],
                padding_bottom: pad[3],
            }
        }
    }

    fn explicit_dimensions(&self, pw: f32, ph: f32) -> (Option<f32>, Option<f32>) {
        (self.mss.width.and_then(|d| d.resolve_opt(pw)), self.mss.height.and_then(|d| d.resolve_opt(ph)))
    }

    /// Полоса-подложка: тени, фон, рамка (наклон `background-rotate-x` —
    /// 3D-«полка» под ровными значками).
    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        self.mss.paint_box(list, self.bounds);
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if let Event::MouseMove(p) = event {
            self.pointer = Some(*p);
            let (changed, hovered) = self.retarget();
            self.report_hover(hovered);
            if changed {
                // Раскладка заново — элементы с новой целью встанут в тик.
                ctx.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
        }
        EventResult::Ignored
    }

    /// Не «прозрачен» для попадания: указатель над промежутком между
    /// элементами — всё ещё над рядом, иначе увеличение гасло бы на каждом
    /// зазоре.
    fn passthrough_hit_test(&self) -> bool {
        false
    }

    /// Вместе с местом, куда вырастают элементы (`overflow`): увеличенный
    /// значок над полосой дока ловит указатель и клики.
    fn hit_test(&self, point: Point) -> bool {
        self.hover_area().contains(point)
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
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }
    fn get_classes(&self) -> &[String] {
        &self.classes
    }
    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        let num = |k: &str| style.get(k).and_then(|v| v.as_px().or_else(|| v.as_string().and_then(|s| s.trim().parse().ok())));
        self.mss_zoom = num("magnification");
        self.mss_range = num("magnification-range");
        self.mss_speed = num("magnification-speed");
        self.mss_falloff = style.get("magnification-falloff").and_then(|v| v.as_string()).and_then(Falloff::parse);
        self.sync_shared();
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
    fn element_type_name(&self) -> &str {
        "Fisheye"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHarness;
    use crate::widget::WidgetExt;
    use crate::widgets::DecoratedBox;

    fn item() -> impl Widget {
        DecoratedBox::new().class("it")
    }

    fn sizes(h: &TestHarness) -> Vec<f32> {
        h.find_by_type_name("ScaleBox").into_iter().map(|id| h.element_bounds(id).size.width).collect()
    }

    fn settle(h: &mut TestHarness) {
        for _ in 0..200 {
            h.tree.animate(h.root_id, Duration::from_millis(16));
            h.layout(400.0, 100.0);
        }
    }

    #[test]
    fn falloff_weights() {
        assert!((Falloff::Cosine.weight(0.0) - 1.0).abs() < 1e-6);
        assert_eq!(Falloff::Cosine.weight(1.0), 0.0);
        assert!((Falloff::Linear.weight(0.5) - 0.5).abs() < 1e-6);
        assert!(Falloff::Gaussian.weight(0.5) < Falloff::Gaussian.weight(0.1));
    }

    #[test]
    fn items_under_pointer_grow_and_relax() {
        let w = Fisheye::new()
            .zoom(2.0)
            .range(1.5)
            .gap(0.0)
            .main_axis_alignment(MainAxisAlignment::Center)
            .child(item())
            .child(item())
            .child(item())
            .class("fe");
        let mut h = TestHarness::new(Box::new(w));
        h.apply_mss(".it { width: 40; height: 40; }");
        h.layout(400.0, 100.0);
        assert_eq!(sizes(&h), vec![40.0, 40.0, 40.0]);

        // Центр ряда 3×40 по центру 400 → средний элемент в 180..220.
        h.tree.handle_event(h.root_id, &Event::MouseMove(Point::new(200.0, 90.0)));
        h.layout(400.0, 100.0);
        settle(&mut h);
        let s = sizes(&h);
        assert!((s[1] - 80.0).abs() < 0.5, "{s:?}");
        assert!(s[0] > 40.0 && s[0] < s[1] && (s[0] - s[2]).abs() < 0.5, "{s:?}");

        h.tree.handle_event(h.root_id, &Event::MouseMove(Point::new(-1.0, -1.0)));
        h.layout(400.0, 100.0);
        settle(&mut h);
        assert_eq!(sizes(&h).iter().map(|v| v.round()).collect::<Vec<_>>(), vec![40.0, 40.0, 40.0]);
    }

    #[test]
    fn mss_disables_magnification() {
        let w = Fisheye::new().zoom(2.0).child(item()).child(item()).class("fe");
        let mut h = TestHarness::new(Box::new(w));
        h.apply_mss(".it { width: 40; height: 40; } .fe { magnification: 1; }");
        h.layout(400.0, 100.0);
        h.tree.handle_event(h.root_id, &Event::MouseMove(Point::new(200.0, 90.0)));
        h.layout(400.0, 100.0);
        settle(&mut h);
        assert_eq!(sizes(&h), vec![40.0, 40.0]);
    }

    #[test]
    fn fixed_scale_box_takes_scaled_space() {
        let w = crate::widgets::Row::new().child(ScaleBox::new(item()).scale(1.5)).child(item());
        let mut h = TestHarness::new(Box::new(w));
        h.apply_mss(".it { width: 40; height: 20; }");
        h.layout(400.0, 100.0);
        let sb = h.find_by_type_name("ScaleBox")[0];
        assert_eq!(h.element_bounds(sb).size, Size::new(60.0, 30.0));
    }

    /// Клик по увеличенному элементу попадает в него, а место, куда он
    /// «вырос», у соседа не отбирает клики соседа.
    #[test]
    fn clicks_map_into_scaled_child() {
        use crate::input::MouseButton;
        use std::sync::atomic::{AtomicUsize, Ordering};
        static A: AtomicUsize = AtomicUsize::new(0);
        static B: AtomicUsize = AtomicUsize::new(0);
        let w = crate::widgets::Row::new()
            .child(ScaleBox::new(
                crate::widgets::GestureDetector::new().child(item()).on_click(|| {
                    A.fetch_add(1, Ordering::SeqCst);
                }),
            ).scale(2.0))
            .child(crate::widgets::GestureDetector::new().child(item()).on_click(|| {
                B.fetch_add(1, Ordering::SeqCst);
            }));
        let mut h = TestHarness::new(Box::new(w));
        h.apply_mss(".it { width: 40; height: 40; }");
        h.layout(400.0, 100.0);
        let click = |h: &mut TestHarness, x: f32, y: f32| {
            let p = Point::new(x, y);
            h.tree.handle_event(h.root_id, &Event::MouseDown { button: MouseButton::Left, position: p });
            h.tree.handle_event(h.root_id, &Event::MouseUp { button: MouseButton::Left, position: p });
        };
        // Увеличенный первый элемент занимает 0..80; точка 70 — его.
        click(&mut h, 70.0, 30.0);
        assert_eq!(A.load(Ordering::SeqCst), 1);
        assert_eq!(B.load(Ordering::SeqCst), 0);
        // Второй — 80..120.
        click(&mut h, 100.0, 30.0);
        assert_eq!(B.load(Ordering::SeqCst), 1);
    }

    /// `overflow`: полоса не растёт в высоту, значок под указателем
    /// вырастает над ней; `on_hover` сообщает его итоговый прямоугольник.
    #[test]
    fn overflow_keeps_bar_height_and_reports_hover_rect() {
        use std::sync::{Arc, Mutex as StdMutex};
        let seen: Arc<StdMutex<Option<(usize, Rect)>>> = Arc::new(StdMutex::new(None));
        let s2 = seen.clone();
        let w = Fisheye::new()
            .zoom(2.0)
            .range(1.5)
            .overflow(true)
            .on_hover(move |h| *s2.lock().unwrap() = h)
            .child(item())
            .child(item())
            .child(item());
        let mut h = TestHarness::new(Box::new(crate::widgets::Column::new()
            .main_axis_alignment(MainAxisAlignment::End)
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .child(w)));
        h.apply_mss(".it { width: 40; height: 40; }");
        h.layout(400.0, 200.0);
        let fe = h.find_by_type_name("Fisheye")[0];
        assert_eq!(h.element_bounds(fe).size, Size::new(120.0, 40.0));
        // Указатель над средним значком (у верхнего края полосы).
        h.tree.handle_event(h.root_id, &Event::MouseMove(Point::new(200.0, 165.0)));
        h.layout(400.0, 200.0);
        settle_at(&mut h, 400.0, 200.0);
        let b = h.element_bounds(fe);
        assert_eq!(b.size.height, 40.0, "полоса не растёт в высоту");
        assert!(b.size.width > 150.0, "{b:?}");
        let (i, r) = seen.lock().unwrap().expect("наведение");
        assert_eq!(i, 1);
        // Итоговый средний значок: 80×80, низ на нижнем крае полосы.
        assert!((r.size.width - 80.0).abs() < 0.5 && (r.size.height - 80.0).abs() < 0.5, "{r:?}");
        assert!((r.origin.y + r.size.height - 200.0).abs() < 0.5, "{r:?}");
        assert!((r.origin.x + r.size.width / 2.0 - 200.0).abs() < 0.5, "{r:?}");
        // Точка над полосой (в увеличенном значке) — всё ещё «над рядом».
        h.tree.handle_event(h.root_id, &Event::MouseMove(Point::new(200.0, 130.0)));
        assert!(seen.lock().unwrap().is_some());
    }

    fn settle_at(h: &mut TestHarness, w: f32, hgt: f32) {
        for _ in 0..200 {
            h.tree.animate(h.root_id, Duration::from_millis(16));
            h.layout(w, hgt);
        }
    }
}
