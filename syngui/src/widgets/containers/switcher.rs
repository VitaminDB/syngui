//! Перетекание содержимого: смена вкладки, страницы, раздела.
//!
//! `AnimatedSwitcher` держит ребёнка по ключу. Когда ключ меняется, старое
//! содержимое остаётся на месте и уезжает (растворяется, сдвигается), новое
//! въезжает, а размер контейнера плавно перетекает от старого к новому — как
//! панель, которая подстраивается под вкладку. Ввод во время перехода идёт
//! только новому содержимому.
//!
//! ```rust,ignore
//! AnimatedSwitcher::new(tab.get() as u64, move || match tab.get() {
//!     0 => Box::new(dashboard()),
//!     _ => Box::new(media()),
//! })
//! .slide(24.0, 0.0)           // новое въезжает справа, старое уезжает влево
//! .directional(true)          // при движении к меньшему ключу — наоборот
//! .duration_ms(260)
//! ```
//!
//! Сборщик вызывается один раз на ключ (как у [`Keyed`]): содержимое
//! читает сигналы само и перестраивается по ним, не трогая переход.

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
use crate::widgets::containers::keyed::Keyed;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

type Builder = Arc<dyn Fn() -> Box<dyn Widget> + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Params {
    duration_ms: u32,
    exit_duration_ms: u32,
    easing: Easing,
    exit_easing: Easing,
    fade: bool,
    /// Растворять ли уходящее (иначе оно просто накрывается новым — для
    /// кроссфейда картинок без просвета).
    exit_fade: bool,
    /// Сдвиг входящего (уходящий идёт в противоположную сторону).
    slide: (f32, f32),
    /// Начальный масштаб входящего.
    scale: f32,
}

pub struct AnimatedSwitcher {
    key: u64,
    version: u64,
    builder: Builder,
    params: Params,
    directional: bool,
    size: bool,
    spring: Option<(f32, f32)>,
    classes: Vec<String>,
}

impl AnimatedSwitcher {
    pub fn new(key: u64, builder: impl Fn() -> Box<dyn Widget> + Send + Sync + 'static) -> Self {
        Self {
            key,
            version: 0,
            builder: Arc::new(builder),
            params: Params {
                duration_ms: 260,
                exit_duration_ms: 160,
                easing: Easing::EMPHASIZED_DECELERATE,
                exit_easing: Easing::EMPHASIZED_ACCELERATE,
                fade: true,
                exit_fade: true,
                slide: (0.0, 0.0),
                scale: 1.0,
            },
            directional: true,
            size: true,
            spring: Some((420.0, 40.0)),
            classes: Vec::new(),
        }
    }

    /// Длительность появления нового содержимого.
    pub fn duration_ms(mut self, ms: u32) -> Self {
        self.params.duration_ms = ms;
        self
    }

    /// Длительность ухода старого (по умолчанию короче появления).
    pub fn exit_duration_ms(mut self, ms: u32) -> Self {
        self.params.exit_duration_ms = ms;
        self
    }

    pub fn easing(mut self, e: Easing) -> Self {
        self.params.easing = e;
        self
    }

    pub fn exit_easing(mut self, e: Easing) -> Self {
        self.params.exit_easing = e;
        self
    }

    /// Версия содержимого при том же ключе: когда она меняется, содержимое
    /// пересобирается на месте (без перехода); уходящее хранит свою версию.
    pub fn version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }

    /// Растворять ли (по умолчанию да).
    pub fn fade(mut self, fade: bool) -> Self {
        self.params.fade = fade;
        self
    }

    /// Растворять ли уходящее содержимое (по умолчанию да). `false` —
    /// оно остаётся непрозрачным, пока новое проявляется сверху.
    pub fn exit_fade(mut self, fade: bool) -> Self {
        self.params.exit_fade = fade;
        self
    }

    /// Откуда въезжает новое содержимое (px); старое уезжает в другую сторону.
    pub fn slide(mut self, dx: f32, dy: f32) -> Self {
        self.params.slide = (dx, dy);
        self
    }

    /// Начальный масштаб нового содержимого (0.92 — «fade through» M3).
    pub fn scale(mut self, s: f32) -> Self {
        self.params.scale = s;
        self
    }

    /// Разворачивать сдвиг, когда ключ уменьшается (вкладки: назад — в
    /// обратную сторону). По умолчанию включено.
    pub fn directional(mut self, on: bool) -> Self {
        self.directional = on;
        self
    }

    /// Перетекать ли размер контейнера (по умолчанию да).
    pub fn animate_size(mut self, on: bool) -> Self {
        self.size = on;
        self
    }

    /// Пружина размера (по умолчанию 420/40); `None` — tween с `easing`.
    pub fn size_spring(mut self, spring: Option<(f32, f32)>) -> Self {
        self.spring = spring;
        self
    }

    pub fn class(mut self, class: &str) -> Self {
        crate::widget::push_classes(&mut self.classes, class.to_string());
        self
    }
}

impl AnimatedSwitcher {
    fn element(&self) -> SwitcherElement {
        SwitcherElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            current: (self.key, self.version, self.builder.clone()),
            outgoing: None,
            exit_left: 0.0,
            direction: 1.0,
            params: self.params,
            directional: self.directional,
            size: self.size,
            spring: self.spring,
            target_size: Size::zero(),
            current_size: Size::zero(),
            width_anim: None,
            height_anim: None,
            initialized: false,
            mounted: false,
            needs_child_rebuild: false,
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        }
    }
}

impl Widget for AnimatedSwitcher {
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

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct SwitcherElement {
    id: ElementId,
    bounds: Rect,
    current: (u64, u64, Builder),
    outgoing: Option<(u64, u64, Builder)>,
    /// Сколько секунд уходящему осталось жить.
    exit_left: f32,
    direction: f32,
    params: Params,
    directional: bool,
    size: bool,
    spring: Option<(f32, f32)>,
    target_size: Size,
    current_size: Size,
    width_anim: Option<Animation>,
    height_anim: Option<Animation>,
    initialized: bool,
    mounted: bool,
    needs_child_rebuild: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl SwitcherElement {
    fn retarget(&self, anim: &mut Option<Animation>, current: f32, target: f32) {
        if let Some((k, c)) = self.spring {
            if let Some(a @ Animation::Spring { .. }) = anim.as_mut() {
                a.set_target(target);
                return;
            }
            *anim = Some(Animation::spring().from(current).to(target).stiffness(k).damping(c).build());
        } else {
            *anim = Some(
                Animation::tween(self.params.easing)
                    .from(current)
                    .to(target)
                    .duration_ms(self.params.duration_ms)
                    .build(),
            );
        }
    }

    fn size_animating(&self) -> bool {
        self.width_anim.is_some() || self.height_anim.is_some()
    }
}

impl Element for SwitcherElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<AnimatedSwitcher>() {
            self.params = w.params;
            self.directional = w.directional;
            self.size = w.size;
            self.spring = w.spring;
            if w.key != self.current.0 {
                self.direction = if self.directional && w.key < self.current.0 { -1.0 } else { 1.0 };
                // Если ключ вернулся к уходящему, его слот снова станет
                // входящим, а бывший текущий — уходящим: сверка по ключам
                // подхватит оба без пересоздания.
                let old = std::mem::replace(&mut self.current, (w.key, w.version, w.builder.clone()));
                self.outgoing = Some(old);
                self.exit_left = self.params.exit_duration_ms as f32 / 1000.0;
                self.needs_child_rebuild = true;
            } else {
                if self.current.1 != w.version {
                    self.needs_child_rebuild = true;
                }
                self.current = (w.key, w.version, w.builder.clone());
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let size = if self.size { self.current_size } else { self.target_size };
        let w = size.width.clamp(constraints.min_width.min(constraints.max_width), constraints.max_width);
        let h = size.height.clamp(constraints.min_height.min(constraints.max_height), constraints.max_height);
        self.bounds.size = Size::new(w, h);
        self.bounds.size
    }

    fn set_content_size(&mut self, size: Size) {
        let old = self.target_size;
        self.target_size = size;
        if !self.initialized {
            self.current_size = size;
            self.initialized = true;
            return;
        }
        if !self.size {
            self.current_size = size;
            return;
        }
        if (size.width - old.width).abs() > 0.5 {
            let mut a = self.width_anim.take();
            self.retarget(&mut a, self.current_size.width, size.width);
            self.width_anim = a;
        }
        if (size.height - old.height).abs() > 0.5 {
            let mut a = self.height_anim.take();
            self.retarget(&mut a, self.current_size.height, size.height);
            self.height_anim = a;
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let mut running = false;
        if let Some(a) = self.width_anim.as_mut() {
            let more = a.tick(dt);
            self.current_size.width = a.current_value();
            if more {
                running = true;
            } else {
                self.width_anim = None;
            }
        }
        if let Some(a) = self.height_anim.as_mut() {
            let more = a.tick(dt);
            self.current_size.height = a.current_value();
            if more {
                running = true;
            } else {
                self.height_anim = None;
            }
        }
        if self.outgoing.is_some() {
            self.exit_left -= dt.as_secs_f32();
            if self.exit_left <= 0.0 {
                self.outgoing = None;
                self.needs_child_rebuild = true;
            } else {
                running = true;
            }
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        running
    }

    fn needs_repaint(&self) -> bool {
        self.size_animating() || self.outgoing.is_some()
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] {
        // Детей ведёт дерево (как у `Reactive`); уходящий слот сам
        // отказывается от ввода.
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

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str {
        "AnimatedSwitcher"
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Switcher
    }

    fn clip_content(&self) -> bool {
        self.size_animating() || self.outgoing.is_some()
    }

    fn manages_own_children(&self) -> bool {
        true
    }

    fn needs_rebuild(&self) -> bool {
        !self.mounted || self.needs_child_rebuild
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        let mut out: Vec<Box<dyn Widget>> = Vec::with_capacity(2);
        if let Some((key, version, builder)) = &self.outgoing {
            out.push(Box::new(Slot {
                key: *key,
                version: *version,
                role: Role::Exit,
                direction: self.direction,
                params: self.params,
                builder: builder.clone(),
            }));
        }
        out.push(Box::new(Slot {
            key: self.current.0,
            version: self.current.1,
            role: Role::Enter,
            direction: self.direction,
            params: self.params,
            builder: self.current.2.clone(),
        }));
        out
    }

    fn clear_rebuild(&mut self) {
        self.mounted = true;
        self.needs_child_rebuild = false;
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

impl StyledElement for SwitcherElement {
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

// ─── Слот: одно содержимое со своей анимацией входа/выхода ──────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Enter,
    Exit,
}

struct Slot {
    key: u64,
    version: u64,
    role: Role,
    direction: f32,
    params: Params,
    builder: Builder,
}

impl Widget for Slot {
    fn create_element(&self) -> Box<dyn Element> {
        let p = self.params;
        Box::new(SlotElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            child_size: Size::zero(),
            role: self.role,
            direction: self.direction,
            params: p,
            anim: Some(Animation::tween(p.easing).from(0.0).to(1.0).duration_ms(p.duration_ms).build()),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER | DirtyFlags::ANIMATION,
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
        let keyed = Keyed::from_arc(self.key, self.version, self.builder.clone());
        let el = keyed.create_element();
        let id = tree.insert_with_type_id(el, Some(parent_id), keyed.as_any().type_id());
        keyed.mount(tree, id);
    }

    fn widget_key(&self) -> Option<u64> {
        Some(self.key)
    }
}

struct SlotElement {
    id: ElementId,
    bounds: Rect,
    child_size: Size,
    role: Role,
    direction: f32,
    params: Params,
    /// Прогресс показа 0..1.
    anim: Option<Animation>,
    dirty_flags: DirtyFlags,
}

impl SlotElement {
    fn progress(&self) -> f32 {
        match (&self.anim, self.role) {
            (Some(a), _) => a.current_value(),
            (None, Role::Enter) => 1.0,
            (None, Role::Exit) => 0.0,
        }
    }
}

impl Element for SlotElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<Slot>() {
            self.params = w.params;
            if w.role != self.role {
                let p = self.progress();
                self.direction = w.direction;
                self.role = w.role;
                self.anim = Some(match w.role {
                    Role::Exit => Animation::tween(self.params.exit_easing)
                        .from(p)
                        .to(0.0)
                        .duration_ms((p * self.params.exit_duration_ms as f32).round() as u32)
                        .build(),
                    Role::Enter => Animation::tween(self.params.easing)
                        .from(p)
                        .to(1.0)
                        .duration_ms(((1.0 - p) * self.params.duration_ms as f32).round() as u32)
                        .build(),
                });
                self.mark_dirty(DirtyFlags::ANIMATION);
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, _constraints: Constraints) -> Size {
        Size::zero()
    }

    fn set_content_size(&mut self, size: Size) {
        self.bounds.size = size;
        if size.width > 0.0 || size.height > 0.0 {
            self.child_size = size;
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let Some(a) = self.anim.as_mut() else { return false };
        let running = a.tick(dt);
        if !running {
            self.anim = None;
        }
        self.mark_dirty(DirtyFlags::RENDER);
        running
    }

    fn needs_repaint(&self) -> bool {
        self.anim.is_some()
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let p = self.progress().clamp(0.0, 1.0);
        if p >= 1.0 && self.role == Role::Enter {
            return;
        }
        let q = 1.0 - p;
        let sign = if self.role == Role::Enter { 1.0 } else { -1.0 } * self.direction;
        let tx = self.params.slide.0 * q * sign;
        let ty = self.params.slide.1 * q * sign;
        let s = if self.role == Role::Enter { 1.0 + (self.params.scale - 1.0) * q } else { 1.0 };
        let mut t = Transform::identity();
        if s != 1.0 {
            let ox = self.bounds.origin.x + self.child_size.width / 2.0;
            let oy = self.bounds.origin.y + self.child_size.height / 2.0;
            t = t
                .then(&Transform::translation(-ox, -oy))
                .then(&Transform::new(s, 0.0, 0.0, s, 0.0, 0.0))
                .then_translate(euclid::Vector2D::new(ox, oy));
        }
        t = t.then_translate(euclid::Vector2D::new(tx, ty));
        list.push_transform(t);
        let fade = if self.role == Role::Enter { self.params.fade } else { self.params.fade && self.params.exit_fade };
        list.push_opacity(if fade { p } else { 1.0 });
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let p = self.progress().clamp(0.0, 1.0);
        if p >= 1.0 && self.role == Role::Enter {
            return;
        }
        list.pop_opacity();
        list.pop_transform();
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn child_at_position(&self, _pos: Point) -> ChildHit {
        if self.role == Role::Exit {
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

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn element_type_name(&self) -> &str {
        "SwitcherSlot"
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Text;

    fn switcher(key: u64) -> AnimatedSwitcher {
        AnimatedSwitcher::new(key, || Box::new(Text::new("x"))).exit_duration_ms(100)
    }

    fn element(key: u64) -> SwitcherElement {
        switcher(key).element()
    }

    #[test]
    fn key_change_keeps_outgoing_until_exit_ends() {
        let mut el = element(1);
        el.clear_rebuild();
        let mut ctx = UpdateContext::new(el.id);
        el.update(&switcher(2), &mut ctx);
        assert!(el.needs_rebuild());
        let kids = el.build_children();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].widget_key(), Some(1));
        assert_eq!(kids[1].widget_key(), Some(2));
        el.clear_rebuild();
        assert!(el.animate(Duration::from_millis(50)));
        assert!(!el.needs_rebuild());
        el.animate(Duration::from_millis(60));
        assert!(el.needs_rebuild());
        assert_eq!(el.build_children().len(), 1);
    }

    #[test]
    fn backwards_key_reverses_direction() {
        let mut el = element(5);
        el.clear_rebuild();
        let mut ctx = UpdateContext::new(el.id);
        el.update(&switcher(2), &mut ctx);
        assert_eq!(el.direction, -1.0);
        el.update(&switcher(7), &mut ctx);
        assert_eq!(el.direction, 1.0);
    }

    #[test]
    fn size_flows_to_incoming() {
        let mut el = element(1);
        el.set_content_size(Size::new(100.0, 100.0));
        el.set_content_size(Size::new(300.0, 50.0));
        let s = el.layout(Constraints::loose(Size::new(1000.0, 1000.0)));
        assert_eq!(s, Size::new(100.0, 100.0));
        for _ in 0..300 {
            if !el.animate(Duration::from_millis(16)) {
                break;
            }
        }
        let s = el.layout(Constraints::loose(Size::new(1000.0, 1000.0)));
        assert!((s.width - 300.0).abs() < 0.5 && (s.height - 50.0).abs() < 0.5);
    }
}
