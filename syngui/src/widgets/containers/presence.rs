//! Появление и уход с анимацией.
//!
//! `ShowIf` и условная сборка убирают поддерево мгновенно: элемент, который
//! перестал быть видимым, выпадает из отрисовки и тика в тот же кадр, так что
//! анимацию ухода на нём не сыграть. `Presence` держит ребёнка смонтированным,
//! пока идёт анимация ухода, и лишь потом прячет его (нулевой размер, без
//! отрисовки и событий). Появление тоже анимируется — с первого кадра или
//! при смене `visible` с `false` на `true`.
//!
//! ```rust,ignore
//! Presence::new(open.get(), card)
//!     .enter(Motion::fade().scale(0.92).slide(0.0, -12.0))
//!     .exit(Motion::fade().scale(0.96))
//!     .duration_ms(240).easing(Easing::EMPHASIZED_DECELERATE)
//!     .origin(TransformOrigin::Custom(0.5, 0.0))
//!     .on_exit_complete(|| close_surface())
//! ```
//!
//! `collapse(AnimationAxis::Height)` — на время анимации размер по оси
//! умножается на прогресс: соседи в колонке сдвигаются вместе с уходящей
//! карточкой (уведомления, строки списка).

use crate::animation::{Animation, Easing};
use crate::core::{Point, Rect, Size, Transform};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    ChildHit, DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement,
    UpdateContext, Widget,
};
use crate::signal::RwSignal;
use crate::widgets::containers::keyed::Keyed;
use crate::widgets::containers::{AnimationAxis, TransformOrigin};
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

/// Откуда приходит (или куда уходит) элемент: начальные значения
/// прозрачности, масштаба и сдвига; к концу появления всё стремится к
/// «показан как есть».
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub opacity: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub dx: f32,
    pub dy: f32,
}

impl Default for Motion {
    fn default() -> Self {
        Self::fade()
    }
}

impl Motion {
    /// Без движения: только вкл/выкл (с `collapse` — чистое схлопывание).
    pub const fn none() -> Self {
        Self { opacity: 1.0, scale_x: 1.0, scale_y: 1.0, dx: 0.0, dy: 0.0 }
    }

    /// Растворение.
    pub const fn fade() -> Self {
        Self { opacity: 0.0, scale_x: 1.0, scale_y: 1.0, dx: 0.0, dy: 0.0 }
    }

    pub const fn opacity(mut self, o: f32) -> Self {
        self.opacity = o;
        self
    }

    /// Общий масштаб (0.9 — чуть меньше, 1.05 — чуть больше).
    pub const fn scale(mut self, s: f32) -> Self {
        self.scale_x = s;
        self.scale_y = s;
        self
    }

    pub const fn scale_x(mut self, s: f32) -> Self {
        self.scale_x = s;
        self
    }

    pub const fn scale_y(mut self, s: f32) -> Self {
        self.scale_y = s;
        self
    }

    /// Сдвиг в px: откуда въезжает (для `enter`) или куда уезжает (для `exit`).
    pub const fn slide(mut self, dx: f32, dy: f32) -> Self {
        self.dx = dx;
        self.dy = dy;
        self
    }

    fn is_identity(&self) -> bool {
        self.opacity >= 1.0 && self.scale_x == 1.0 && self.scale_y == 1.0 && self.dx == 0.0 && self.dy == 0.0
    }
}

type ExitCb = Arc<dyn Fn() + Send + Sync>;
type Builder = Arc<dyn Fn() -> Box<dyn Widget> + Send + Sync>;

pub struct Presence {
    child: Option<Box<dyn Widget>>,
    /// Сигнальный режим: содержимое строится один раз (`Keyed`), а
    /// видимость приходит из сигнала — без пересборки содержимого.
    signal: Option<(RwSignal<bool>, Builder)>,
    visible: bool,
    enter: Motion,
    exit: Motion,
    duration_ms: u32,
    exit_duration_ms: Option<u32>,
    easing: Easing,
    exit_easing: Option<Easing>,
    origin: TransformOrigin,
    collapse: Option<AnimationAxis>,
    initial: bool,
    on_exit_complete: Option<ExitCb>,
    classes: Vec<String>,
}

impl Presence {
    /// `visible` — показывать ли ребёнка; при смене значения играет
    /// анимация появления или ухода.
    pub fn new(visible: bool, child: impl Widget + 'static) -> Self {
        Self::build(Some(Box::new(child)), None, visible)
    }

    /// Видимость из сигнала. Содержимое строится `builder` один раз и не
    /// пересобирается при смене видимости — так можно прятать и показывать
    /// поддерево с состоянием (поле ввода, прокрутка) и не платить за
    /// пересборку.
    pub fn signal(visible: RwSignal<bool>, builder: impl Fn() -> Box<dyn Widget> + Send + Sync + 'static) -> Self {
        let now = visible.get_untracked();
        Self::build(None, Some((visible, Arc::new(builder))), now)
    }

    fn build(child: Option<Box<dyn Widget>>, signal: Option<(RwSignal<bool>, Builder)>, visible: bool) -> Self {
        Self {
            child,
            signal,
            visible,
            enter: Motion::fade(),
            exit: Motion::fade(),
            duration_ms: 220,
            exit_duration_ms: None,
            easing: Easing::EMPHASIZED_DECELERATE,
            exit_easing: None,
            origin: TransformOrigin::Center,
            collapse: None,
            initial: true,
            on_exit_complete: None,
            classes: Vec::new(),
        }
    }

    pub fn enter(mut self, m: Motion) -> Self {
        self.enter = m;
        self
    }

    pub fn exit(mut self, m: Motion) -> Self {
        self.exit = m;
        self
    }

    /// Длительность появления (и ухода, пока не задана отдельно).
    pub fn duration_ms(mut self, ms: u32) -> Self {
        self.duration_ms = ms;
        self
    }

    pub fn exit_duration_ms(mut self, ms: u32) -> Self {
        self.exit_duration_ms = Some(ms);
        self
    }

    pub fn easing(mut self, e: Easing) -> Self {
        self.easing = e;
        self
    }

    /// Кривая ухода; по умолчанию — `EMPHASIZED_ACCELERATE`, если кривая
    /// появления из семейства emphasized, иначе та же, что и у появления.
    pub fn exit_easing(mut self, e: Easing) -> Self {
        self.exit_easing = Some(e);
        self
    }

    /// Точка, относительно которой масштабируется ребёнок.
    pub fn origin(mut self, o: TransformOrigin) -> Self {
        self.origin = o;
        self
    }

    /// Схлопывать размер по оси в такт анимации (соседи сдвигаются).
    pub fn collapse(mut self, axis: AnimationAxis) -> Self {
        self.collapse = Some(axis);
        self
    }

    /// Играть ли появление при первом монтировании (по умолчанию да).
    pub fn initial(mut self, animate_first: bool) -> Self {
        self.initial = animate_first;
        self
    }

    /// Вызывается, когда анимация ухода доиграла и ребёнок спрятан.
    pub fn on_exit_complete(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_exit_complete = Some(Arc::new(f));
        self
    }

    pub fn class(mut self, class: &str) -> Self {
        crate::widget::push_classes(&mut self.classes, class.to_string());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Hidden,
    Entering,
    Shown,
    Exiting,
}

impl Presence {
    fn element(&self) -> PresenceElement {
        let (phase, anim) = match (self.visible, self.initial) {
            (true, true) => (
                Phase::Entering,
                Some(Animation::tween(self.easing).from(0.0).to(1.0).duration_ms(self.duration_ms).build()),
            ),
            (true, false) => (Phase::Shown, None),
            (false, _) => (Phase::Hidden, None),
        };
        PresenceElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            child_size: Size::zero(),
            phase,
            anim,
            enter: self.enter,
            exit: self.exit,
            duration_ms: self.duration_ms,
            exit_duration_ms: self.exit_duration_ms,
            easing: self.easing,
            exit_easing: self.exit_easing,
            origin: self.origin,
            collapse: self.collapse,
            on_exit_complete: self.on_exit_complete.clone(),
            signal: self.signal.clone(),
            mounted: false,
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER | DirtyFlags::ANIMATION,
            mss: MssFields::new(),
        }
    }
}

impl Widget for Presence {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(self.element())
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
        let Some(child) = &self.child else { return };
        let child_element = child.create_element();
        let child_id = tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
        child.mount(tree, child_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.iter().map(|c| c.as_ref() as &dyn Widget).collect()
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct PresenceElement {
    id: ElementId,
    bounds: Rect,
    child_size: Size,
    phase: Phase,
    /// Прогресс показа 0..1 (1 — показан полностью).
    anim: Option<Animation>,
    enter: Motion,
    exit: Motion,
    duration_ms: u32,
    exit_duration_ms: Option<u32>,
    easing: Easing,
    exit_easing: Option<Easing>,
    origin: TransformOrigin,
    collapse: Option<AnimationAxis>,
    on_exit_complete: Option<ExitCb>,
    signal: Option<(RwSignal<bool>, Builder)>,
    mounted: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl PresenceElement {
    fn progress(&self) -> f32 {
        match self.phase {
            Phase::Hidden => 0.0,
            Phase::Shown => 1.0,
            _ => self.anim.as_ref().map_or(1.0, |a| a.current_value()),
        }
    }

    fn exit_easing(&self) -> Easing {
        self.exit_easing.unwrap_or(match self.easing {
            Easing::EMPHASIZED | Easing::EMPHASIZED_DECELERATE => Easing::EMPHASIZED_ACCELERATE,
            Easing::STANDARD_DECELERATE => Easing::STANDARD_ACCELERATE,
            e => e,
        })
    }

    fn set_visible(&mut self, visible: bool) {
        let p = self.progress();
        match (visible, self.phase) {
            (true, Phase::Hidden) | (true, Phase::Exiting) => {
                self.phase = Phase::Entering;
                self.anim = Some(
                    Animation::tween(self.easing)
                        .from(p)
                        .to(1.0)
                        .duration_ms(((1.0 - p) * self.duration_ms as f32).round() as u32)
                        .build(),
                );
            }
            (false, Phase::Shown) | (false, Phase::Entering) => {
                self.phase = Phase::Exiting;
                let ms = self.exit_duration_ms.unwrap_or(self.duration_ms.saturating_mul(3) / 4);
                self.anim = Some(
                    Animation::tween(self.exit_easing())
                        .from(p)
                        .to(0.0)
                        .duration_ms((p * ms as f32).round() as u32)
                        .build(),
                );
            }
            _ => return,
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER | DirtyFlags::ANIMATION);
    }

    fn motion(&self) -> Option<(Motion, f32)> {
        match self.phase {
            Phase::Entering => Some((self.enter, self.progress())),
            Phase::Exiting => Some((self.exit, self.progress())),
            _ => None,
        }
    }

    fn transform_origin(&self) -> (f32, f32) {
        let b = self.bounds.origin;
        match self.origin {
            TransformOrigin::TopLeft => (b.x, b.y),
            TransformOrigin::Center => (b.x + self.child_size.width / 2.0, b.y + self.child_size.height / 2.0),
            TransformOrigin::Custom(x, y) => (b.x + x * self.child_size.width, b.y + y * self.child_size.height),
        }
    }
}

impl Element for PresenceElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<Presence>() {
            self.enter = w.enter;
            self.exit = w.exit;
            self.duration_ms = w.duration_ms;
            self.exit_duration_ms = w.exit_duration_ms;
            self.easing = w.easing;
            self.exit_easing = w.exit_easing;
            self.origin = w.origin;
            self.collapse = w.collapse;
            self.on_exit_complete = w.on_exit_complete.clone();
            match (&mut self.signal, &w.signal) {
                (Some(mine), Some(new)) => {
                    mine.1 = new.1.clone();
                    if mine.0 != new.0 {
                        mine.0 = new.0;
                        self.set_visible(new.0.get_untracked());
                    }
                }
                (None, None) => self.set_visible(w.visible),
                _ => {}
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let mut size = self.child_size;
        if self.phase == Phase::Hidden {
            size = Size::zero();
        } else if let (Some(axis), Some((_, p))) = (self.collapse, self.motion()) {
            let p = p.clamp(0.0, 1.0);
            if matches!(axis, AnimationAxis::Width | AnimationAxis::Both) {
                size.width *= p;
            }
            if matches!(axis, AnimationAxis::Height | AnimationAxis::Both) {
                size.height *= p;
            }
        }
        let w = size.width.clamp(constraints.min_width.min(constraints.max_width), constraints.max_width);
        let h = size.height.clamp(constraints.min_height.min(constraints.max_height), constraints.max_height);
        self.bounds.size = Size::new(w, h);
        self.bounds.size
    }

    fn set_content_size(&mut self, size: Size) {
        self.child_size = size;
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let Some(anim) = self.anim.as_mut() else { return false };
        let running = anim.tick(dt);
        if !running {
            self.anim = None;
            let mut done_exit = false;
            self.phase = match self.phase {
                Phase::Entering => Phase::Shown,
                Phase::Exiting => {
                    done_exit = true;
                    Phase::Hidden
                }
                p => p,
            };
            if done_exit {
                if let Some(cb) = self.on_exit_complete.clone() {
                    cb();
                }
            }
        }
        let flags = if self.collapse.is_some() || !running {
            DirtyFlags::LAYOUT | DirtyFlags::RENDER
        } else {
            DirtyFlags::RENDER
        };
        self.mark_dirty(flags);
        running
    }

    fn needs_repaint(&self) -> bool {
        self.anim.is_some()
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let Some((m, p)) = self.motion() else { return };
        if m.is_identity() {
            return;
        }
        let q = 1.0 - p.clamp(0.0, 1.0);
        let sx = 1.0 + (m.scale_x - 1.0) * q;
        let sy = 1.0 + (m.scale_y - 1.0) * q;
        let tx = m.dx * q;
        let ty = m.dy * q;
        let (ox, oy) = self.transform_origin();
        let mut t = Transform::identity();
        if sx != 1.0 || sy != 1.0 {
            t = t
                .then(&Transform::translation(-ox, -oy))
                .then(&Transform::new(sx, 0.0, 0.0, sy, 0.0, 0.0))
                .then_translate(euclid::Vector2D::new(ox, oy));
        }
        if tx != 0.0 || ty != 0.0 {
            t = t.then_translate(euclid::Vector2D::new(tx, ty));
        }
        list.push_transform(t);
        let opacity = 1.0 + (m.opacity - 1.0) * q;
        list.push_opacity(opacity.clamp(0.0, 1.0));
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let Some((m, _)) = self.motion() else { return };
        if m.is_identity() {
            return;
        }
        list.pop_opacity();
        list.pop_transform();
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn child_at_position(&self, _pos: Point) -> ChildHit {
        // Уходящий элемент уже не принимает ввод: клик по растворяющейся
        // карточке не должен срабатывать.
        if matches!(self.phase, Phase::Exiting | Phase::Hidden) {
            ChildHit::None
        } else {
            ChildHit::Unknown
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

    fn mount(&mut self, _tree: &mut ElementTree) {
        if let Some((sig, _)) = &self.signal {
            sig.subscribe_element(self.id);
        }
    }

    fn manages_own_children(&self) -> bool {
        self.signal.is_some()
    }

    fn needs_rebuild(&self) -> bool {
        self.signal.is_some() && (!self.mounted || crate::signal::is_element_dirty(self.id))
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        match &self.signal {
            // Ключ и версия постоянны: содержимое строится один раз.
            Some((_, builder)) => vec![Box::new(Keyed::from_arc(1, 0, builder.clone()))],
            None => Vec::new(),
        }
    }

    fn clear_rebuild(&mut self) {
        self.mounted = true;
        crate::signal::clear_element_dirty(self.id);
        if let Some((sig, _)) = &self.signal {
            let v = sig.get_untracked();
            self.set_visible(v);
        }
    }

    fn is_visible(&self) -> bool {
        self.phase != Phase::Hidden
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::AnimatedSize
    }

    fn clip_content(&self) -> bool {
        self.collapse.is_some() && self.phase != Phase::Shown
    }

    fn element_type_name(&self) -> &str {
        "Presence"
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
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
}

impl StyledElement for PresenceElement {
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
    use crate::widget::Text;

    fn element(visible: bool, initial: bool) -> PresenceElement {
        Presence::new(visible, Text::new("x")).initial(initial).duration_ms(100).exit_duration_ms(100).element()
    }

    #[test]
    fn hidden_takes_no_space() {
        let mut el = element(false, true);
        el.set_content_size(Size::new(100.0, 40.0));
        let s = el.layout(Constraints::loose(Size::new(500.0, 500.0)));
        assert_eq!(s, Size::zero());
        assert!(!el.is_visible());
    }

    #[test]
    fn exit_plays_then_hides() {
        let mut el = element(true, false);
        assert_eq!(el.phase, Phase::Shown);
        el.set_visible(false);
        assert_eq!(el.phase, Phase::Exiting);
        assert!(el.is_visible());
        assert!(el.animate(Duration::from_millis(50)));
        assert!(el.progress() < 1.0 && el.progress() > 0.0);
        assert!(!el.animate(Duration::from_millis(100)));
        assert_eq!(el.phase, Phase::Hidden);
        assert!(!el.is_visible());
    }

    #[test]
    fn reopen_while_exiting_continues_from_current() {
        let mut el = element(true, false);
        el.set_visible(false);
        el.animate(Duration::from_millis(50));
        let p = el.progress();
        el.set_visible(true);
        assert_eq!(el.phase, Phase::Entering);
        assert!((el.progress() - p).abs() < 1e-3);
        el.animate(Duration::from_millis(200));
        assert_eq!(el.phase, Phase::Shown);
    }

    #[test]
    fn collapse_scales_height() {
        let mut el = element(true, false);
        let mut w = Presence::new(true, Text::new("x")).collapse(AnimationAxis::Height);
        w.visible = false;
        let mut ctx = UpdateContext::new(el.id);
        el.update(&w, &mut ctx);
        el.set_content_size(Size::new(100.0, 40.0));
        el.animate(Duration::from_millis(1));
        let s = el.layout(Constraints::loose(Size::new(500.0, 500.0)));
        assert_eq!(s.width, 100.0);
        assert!(s.height < 40.0 && s.height > 0.0);
        assert!(el.clip_content());
    }
}
