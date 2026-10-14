//! Смахивание вбок, чтобы убрать (уведомление, строка списка).
//!
//! Палец ведёт элемент по горизонтали, элемент бледнеет. Отпустили дальше
//! доли ширины ([`Dismissible::threshold`]) или бросили в ту же сторону —
//! элемент улетает за край, и по прилёту вызывается
//! [`Dismissible::on_dismiss`] (убрать его из данных — дело владельца).
//! Иначе — пружиной на место. В запрещённую сторону ([`DismissDirection`])
//! элемент едет туго и всегда возвращается.
//!
//! Вертикальный жест не трогается: он уходит родителю (прокрутке списка,
//! шторке), как у `GestureDetector::pan_axis(PanAxis::Horizontal)`. Тап
//! по-прежнему достаётся содержимому.
//!
//! ```rust,ignore
//! Dismissible::new(card(n))
//!     .direction(DismissDirection::Both)
//!     .on_dismiss(move |_dir| close(id))
//! ```

use crate::animation::Spring;
use crate::core::{Point, Rect, Size, Transform};
use crate::input::{Event, EventResult, VelocityTracker};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::EventContextExt;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::SwipeDirection;

/// В какую сторону можно смахнуть.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DismissDirection {
    /// В обе стороны.
    #[default]
    Both,
    /// Только влево.
    Left,
    /// Только вправо.
    Right,
    /// Смахивание выключено: жест целиком уходит родителю.
    Off,
}

impl DismissDirection {
    fn allows(self, dx: f32) -> bool {
        match self {
            Self::Both => dx != 0.0,
            Self::Left => dx < 0.0,
            Self::Right => dx > 0.0,
            Self::Off => false,
        }
    }
}

type DismissCb = Arc<Mutex<dyn FnMut(SwipeDirection) + Send>>;

/// Скорость броска (лог. px/с), которой хватает на смахивание без пути.
const FLING_VELOCITY: f32 = 700.0;
/// Минимальный путь для броска — чтобы дрожь пальца не смахивала.
const FLING_MIN_DISTANCE: f32 = 24.0;
/// Сопротивление в запрещённую сторону.
const RESIST: f32 = 0.18;

pub struct Dismissible {
    child: Box<dyn Widget>,
    direction: DismissDirection,
    threshold: f32,
    on_dismiss: Option<DismissCb>,
    classes: Vec<String>,
}

impl Dismissible {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self { child: Box::new(child), direction: DismissDirection::Both, threshold: 0.4, on_dismiss: None, classes: Vec::new() }
    }

    pub fn direction(mut self, d: DismissDirection) -> Self {
        self.direction = d;
        self
    }

    /// Доля ширины, после которой отпускание смахивает (по умолчанию 0,4).
    pub fn threshold(mut self, t: f32) -> Self {
        self.threshold = t.clamp(0.05, 0.95);
        self
    }

    /// Элемент улетел за край; аргумент — куда смахнули.
    pub fn on_dismiss(mut self, cb: impl FnMut(SwipeDirection) + Send + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn class(mut self, class: &str) -> Self {
        crate::widget::push_classes(&mut self.classes, class.to_string());
        self
    }

    fn element(&self) -> DismissibleElement {
        DismissibleElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            direction: self.direction,
            threshold: self.threshold,
            on_dismiss: self.on_dismiss.clone(),
            touch: None,
            offset: 0.0,
            velocity: 0.0,
            phase: Phase::Idle,
            spring: Spring::new().with_stiffness(520.0).with_damping(42.0),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        }
    }
}

impl Widget for Dismissible {
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
        let child_element = self.child.create_element();
        let child_id = tree.insert_with_type_id(child_element, Some(parent_id), self.child.as_any().type_id());
        self.child.mount(tree, child_id);
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        vec![self.child.as_ref() as &dyn Widget]
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Idle,
    /// Палец ведёт элемент.
    Drag,
    /// Возврат на место.
    Settle,
    /// Улёт за край со скоростью, px/с.
    Fly { speed: f32 },
    /// Улетел, `on_dismiss` вызван; ждёт, пока владелец уберёт.
    Gone,
}

#[derive(Debug)]
struct Touch {
    id: u64,
    start: Point,
    /// `None` — ось ещё не ясна; `Some(false)` — жест вертикальный, отдан.
    ours: Option<bool>,
    tracker: VelocityTracker,
}

pub struct DismissibleElement {
    id: ElementId,
    bounds: Rect,
    direction: DismissDirection,
    threshold: f32,
    on_dismiss: Option<DismissCb>,
    touch: Option<Touch>,
    /// Сдвиг по горизонтали от места в раскладке.
    offset: f32,
    /// Скорость возврата пружиной.
    velocity: f32,
    phase: Phase,
    spring: Spring,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl DismissibleElement {
    fn width(&self) -> f32 {
        self.bounds.size.width.max(1.0)
    }

    /// Сдвиг за пальцем: в запрещённую сторону — туго.
    fn follow(&mut self, dx: f32) {
        self.offset = if self.direction.allows(dx) { dx } else { dx * RESIST };
    }

    /// Палец отпущен со скоростью `vx`: улететь или вернуться.
    fn release(&mut self, vx: f32) {
        let dx = self.offset;
        let far = dx.abs() > self.width() * self.threshold;
        let fling = vx.abs() > FLING_VELOCITY && vx.signum() == dx.signum() && dx.abs() > FLING_MIN_DISTANCE;
        if self.direction.allows(dx) && (far || fling) {
            // Не медленнее броска и так, чтобы улететь примерно за 0,2 с.
            self.phase = Phase::Fly { speed: vx.abs().max(self.width() * 5.0) };
        } else {
            self.velocity = vx;
            self.phase = Phase::Settle;
        }
    }

    fn dismissed(&mut self) {
        self.phase = Phase::Gone;
        let dir = if self.offset < 0.0 { SwipeDirection::Left } else { SwipeDirection::Right };
        if let Some(cb) = &self.on_dismiss {
            if let Ok(mut f) = cb.lock() {
                f(dir);
            }
        }
    }

    /// Прозрачность по сдвигу: у края — почти невидим.
    fn opacity(&self) -> f32 {
        if self.phase == Phase::Gone {
            return 0.0;
        }
        (1.0 - self.offset.abs() / self.width() * 0.9).clamp(0.0, 1.0)
    }

    fn shifted(&self) -> bool {
        self.offset != 0.0 || self.phase == Phase::Gone
    }
}

impl Element for DismissibleElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<Dismissible>() {
            self.direction = w.direction;
            self.threshold = w.threshold;
            self.on_dismiss = w.on_dismiss.clone();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, _constraints: Constraints) -> Size {
        Size::zero()
    }

    fn set_content_size(&mut self, size: Size) {
        self.bounds.size = size;
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    fn animate(&mut self, dt: Duration) -> bool {
        match self.phase {
            Phase::Settle => {
                if !crate::animation::enabled() {
                    self.offset = 0.0;
                    self.velocity = 0.0;
                    self.phase = Phase::Idle;
                } else {
                    let (x, v) = self.spring.update(self.offset, 0.0, self.velocity, dt.as_secs_f32());
                    self.offset = x;
                    self.velocity = v;
                    if self.spring.is_at_rest(x.abs(), v.abs()) {
                        self.offset = 0.0;
                        self.velocity = 0.0;
                        self.phase = Phase::Idle;
                    }
                }
            }
            Phase::Fly { speed } => {
                let target = (self.width() + 16.0) * if self.offset < 0.0 { -1.0 } else { 1.0 };
                if !crate::animation::enabled() {
                    self.offset = target;
                } else {
                    let step = speed * crate::animation::effective_dt(dt).as_secs_f32();
                    self.offset = if target > 0.0 { (self.offset + step).min(target) } else { (self.offset - step).max(target) };
                }
                if self.offset == target {
                    self.dismissed();
                }
            }
            Phase::Idle | Phase::Drag | Phase::Gone => return false,
        }
        self.mark_dirty(DirtyFlags::RENDER);
        true
    }

    fn needs_repaint(&self) -> bool {
        matches!(self.phase, Phase::Settle | Phase::Fly { .. })
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.shifted() {
            list.push_transform(Transform::translation(self.offset, 0.0));
            list.push_opacity(self.opacity());
        }
    }

    fn post_build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if self.shifted() {
            list.pop_opacity();
            list.pop_transform();
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.direction == DismissDirection::Off {
            return EventResult::Ignored;
        }
        match event {
            Event::TouchStart { id, position } => {
                if !self.bounds.contains(*position) || self.touch.is_some() {
                    return EventResult::Ignored;
                }
                // Улетающий или убранный элемент касаний не берёт.
                if matches!(self.phase, Phase::Fly { .. } | Phase::Gone) {
                    return EventResult::Handled;
                }
                let mut tracker = VelocityTracker::new();
                tracker.add(*position);
                // Поймали возвращающийся элемент — палец ведёт его с текущего места.
                let start = Point::new(position.x - self.offset, position.y);
                self.touch = Some(Touch { id: *id, start, ours: None, tracker });
                EventResult::Handled
            }
            Event::TouchMove { id, position } => {
                let Some(t) = self.touch.as_mut().filter(|t| t.id == *id) else { return EventResult::Ignored };
                t.tracker.add(*position);
                let (dx, dy) = (position.x - t.start.x, position.y - t.start.y);
                match t.ours {
                    Some(false) => return EventResult::Ignored,
                    Some(true) => {}
                    None => {
                        if self.offset == 0.0 && dx.abs().max(dy.abs()) < crate::input::touch_config().tap_slop {
                            return EventResult::Handled;
                        }
                        let ours = self.offset != 0.0 || dx.abs() > dy.abs();
                        t.ours = Some(ours);
                        if !ours {
                            return EventResult::Ignored;
                        }
                        self.phase = Phase::Drag;
                    }
                }
                self.follow(dx);
                self.mark_dirty(DirtyFlags::RENDER);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::TouchEnd { id, .. } => {
                let Some(t) = self.touch.take_if(|t| t.id == *id) else { return EventResult::Ignored };
                match t.ours {
                    Some(false) => EventResult::Ignored,
                    None => EventResult::Handled,
                    Some(true) => {
                        self.release(t.tracker.velocity().x);
                        self.mark_dirty(DirtyFlags::RENDER);
                        ctx.request_paint();
                        EventResult::Handled
                    }
                }
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
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
        "Dismissible"
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
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

impl StyledElement for DismissibleElement {
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
    use std::sync::atomic::{AtomicU32, Ordering};

    fn element(d: DismissDirection, hits: Arc<AtomicU32>) -> DismissibleElement {
        let mut el = Dismissible::new(Text::new("x"))
            .direction(d)
            .on_dismiss(move |dir| {
                hits.fetch_add(if dir == SwipeDirection::Left { 10 } else { 1 }, Ordering::SeqCst);
            })
            .element();
        el.set_content_size(Size::new(300.0, 80.0));
        el.set_position(Point::new(0.0, 0.0));
        el
    }

    fn drag(el: &mut DismissibleElement, path: &[(f32, f32)]) {
        let mut ctx = EventContext::new(el.id);
        let (x0, y0) = path[0];
        el.handle_event(&Event::TouchStart { id: 1, position: Point::new(x0, y0) }, &mut ctx);
        for &(x, y) in &path[1..] {
            el.handle_event(&Event::TouchMove { id: 1, position: Point::new(x, y) }, &mut ctx);
        }
        let (x, y) = *path.last().unwrap();
        el.handle_event(&Event::TouchEnd { id: 1, position: Point::new(x, y) }, &mut ctx);
    }

    fn settle(el: &mut DismissibleElement) {
        for _ in 0..400 {
            if !el.animate(Duration::from_millis(16)) {
                break;
            }
        }
    }

    #[test]
    fn far_drag_dismisses_in_its_direction() {
        let hits = Arc::new(AtomicU32::new(0));
        let mut el = element(DismissDirection::Both, hits.clone());
        drag(&mut el, &[(150.0, 40.0), (170.0, 40.0), (250.0, 40.0), (290.0, 41.0)]);
        settle(&mut el);
        assert_eq!(el.phase, Phase::Gone);
        assert_eq!(hits.load(Ordering::SeqCst), 1, "вправо");
    }

    #[test]
    fn short_drag_returns() {
        let hits = Arc::new(AtomicU32::new(0));
        let mut el = element(DismissDirection::Both, hits.clone());
        drag(&mut el, &[(150.0, 40.0), (160.0, 40.0), (175.0, 40.0)]);
        // палец стоял — броска нет
        std::thread::sleep(Duration::from_millis(120));
        el.release(0.0);
        settle(&mut el);
        assert_eq!(el.phase, Phase::Idle);
        assert_eq!(el.offset, 0.0);
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn forbidden_side_resists_and_returns() {
        let hits = Arc::new(AtomicU32::new(0));
        let mut el = element(DismissDirection::Right, hits.clone());
        drag(&mut el, &[(250.0, 40.0), (230.0, 40.0), (20.0, 40.0)]);
        settle(&mut el);
        assert_eq!(el.phase, Phase::Idle);
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn vertical_gesture_goes_to_parent() {
        let hits = Arc::new(AtomicU32::new(0));
        let mut el = element(DismissDirection::Both, hits);
        let mut ctx = EventContext::new(el.id);
        el.handle_event(&Event::TouchStart { id: 1, position: Point::new(150.0, 40.0) }, &mut ctx);
        let r = el.handle_event(&Event::TouchMove { id: 1, position: Point::new(152.0, 70.0) }, &mut ctx);
        assert!(!r.is_handled());
        let r = el.handle_event(&Event::TouchEnd { id: 1, position: Point::new(152.0, 70.0) }, &mut ctx);
        assert!(!r.is_handled());
        assert_eq!(el.offset, 0.0);
    }

    #[test]
    fn off_ignores_touch() {
        let hits = Arc::new(AtomicU32::new(0));
        let mut el = element(DismissDirection::Off, hits);
        let mut ctx = EventContext::new(el.id);
        let r = el.handle_event(&Event::TouchStart { id: 1, position: Point::new(150.0, 40.0) }, &mut ctx);
        assert!(!r.is_handled());
    }
}
