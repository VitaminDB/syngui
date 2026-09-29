use super::IntoWidget;
use crate::core::sync::Mutex;
use crate::core::{Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;

type ClickCb = Arc<Mutex<dyn FnMut() + Send>>;
type ClickAtCb = Arc<Mutex<dyn FnMut(Point) + Send>>;
type ClickBoundsCb = Arc<Mutex<dyn FnMut(Point, Rect) + Send>>;
type HoverCb = Arc<Mutex<dyn FnMut(bool) + Send>>;
type MouseBtnCb = Arc<Mutex<dyn FnMut(Point) + Send>>;
type BackCb = Arc<Mutex<dyn FnMut() -> bool + Send>>;
type PanCb = Arc<Mutex<dyn FnMut(PanUpdate) + Send>>;
type PanEndCb = Arc<Mutex<dyn FnMut(Point) + Send>>;
type SwipeCb = Arc<Mutex<dyn FnMut(SwipeDirection, f32) + Send>>;
type PinchCb = Arc<Mutex<dyn FnMut(PinchUpdate) + Send>>;
type PinchEndCb = Arc<Mutex<dyn FnMut() + Send>>;

/// По какой оси панорама забирает жест. По чужой оси жест отдаётся
/// родителю (прокрутке, карусели) — вложенные жесты не дерутся.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PanAxis {
    #[default]
    Free,
    Horizontal,
    Vertical,
}

/// Направление смахивания (движение пальца).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Шаг панорамы: точка (в координатах элемента), сдвиг с прошлого шага и
/// с начала жеста.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanUpdate {
    pub position: Point,
    pub delta: Point,
    pub total: Point,
}

/// Шаг щипка двумя пальцами: масштаб от начала жеста, центр между
/// пальцами и сдвиг центра с прошлого шага.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinchUpdate {
    pub scale: f32,
    pub center: Point,
    pub delta: Point,
}

/// Порог скорости броска (лог. px/с) и пути для смахивания.
const SWIPE_VELOCITY: f32 = 450.0;
const SWIPE_DISTANCE: f32 = 64.0;

pub struct GestureDetector {
    child: Option<Box<dyn Widget>>,
    on_click: Option<ClickCb>,
    on_click_at: Option<ClickAtCb>,
    on_click_with_bounds: Option<ClickBoundsCb>,
    on_double_click: Option<ClickCb>,
    on_hover_change: Option<HoverCb>,
    on_mouse_down: Option<MouseBtnCb>,
    on_mouse_up: Option<MouseBtnCb>,
    on_back: Option<BackCb>,
    on_secondary_click: Option<MouseBtnCb>,
    on_middle_click: Option<MouseBtnCb>,
    on_long_press: Option<MouseBtnCb>,
    on_pan_start: Option<MouseBtnCb>,
    on_pan_update: Option<PanCb>,
    on_pan_end: Option<PanEndCb>,
    on_swipe: Option<SwipeCb>,
    on_pinch: Option<PinchCb>,
    on_pinch_end: Option<PinchEndCb>,
    pan_axis: PanAxis,
    pan_mouse: bool,
    cursor: CursorIcon,
    classes: Vec<String>,
}

impl GestureDetector {
    pub fn new() -> Self {
        Self {
            child: None,
            on_click: None,
            on_click_at: None,
            on_click_with_bounds: None,
            on_double_click: None,
            on_hover_change: None,
            on_mouse_down: None,
            on_mouse_up: None,
            on_back: None,
            on_secondary_click: None,
            on_middle_click: None,
            on_long_press: None,
            on_pan_start: None,
            on_pan_update: None,
            on_pan_end: None,
            on_swipe: None,
            on_pinch: None,
            on_pinch_end: None,
            pan_axis: PanAxis::Free,
            pan_mouse: true,
            cursor: CursorIcon::Pointer,
            classes: Vec::new(),
        }
    }

    /// Долгое нажатие пальцем (точка в координатах элемента). Без него
    /// удержание работает как правая кнопка (`on_secondary_click`).
    pub fn on_long_press(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_long_press = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Начало панорамы: палец (или мышь с зажатой кнопкой) сдвинулся дальше
    /// порога тапа.
    pub fn on_pan_start(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_pan_start = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_pan_update(mut self, cb: impl FnMut(PanUpdate) + Send + 'static) -> Self {
        self.on_pan_update = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Конец панорамы со скоростью броска (лог. px/с).
    pub fn on_pan_end(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_pan_end = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Смахивание: быстрый бросок или длинный путь по одной оси; второй
    /// аргумент — скорость (лог. px/с).
    pub fn on_swipe(mut self, cb: impl FnMut(SwipeDirection, f32) + Send + 'static) -> Self {
        self.on_swipe = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Щипок двумя пальцами.
    pub fn on_pinch(mut self, cb: impl FnMut(PinchUpdate) + Send + 'static) -> Self {
        self.on_pinch = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_pinch_end(mut self, cb: impl FnMut() + Send + 'static) -> Self {
        self.on_pinch_end = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Ось панорамы и смахивания: по чужой оси жест уходит родителю.
    pub fn pan_axis(mut self, axis: PanAxis) -> Self {
        self.pan_axis = axis;
        self
    }

    /// Панорама мышью (зажатая левая кнопка); по умолчанию включена.
    pub fn pan_mouse(mut self, on: bool) -> Self {
        self.pan_mouse = on;
        self
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    /// Правая кнопка (на нажатии): точка в координатах окна — для меню.
    pub fn on_secondary_click(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_secondary_click = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Средняя кнопка (на нажатии).
    pub fn on_middle_click(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_middle_click = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_click(mut self, cb: impl FnMut() + Send + 'static) -> Self {
        self.on_click = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_click_at(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_click_at = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_click_with_bounds(mut self, cb: impl FnMut(Point, Rect) + Send + 'static) -> Self {
        self.on_click_with_bounds = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_double_click(mut self, cb: impl FnMut() + Send + 'static) -> Self {
        self.on_double_click = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_hover_change(mut self, cb: impl FnMut(bool) + Send + 'static) -> Self {
        self.on_hover_change = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_mouse_down(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_mouse_down = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_mouse_up(mut self, cb: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_mouse_up = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_back(mut self, cb: impl FnMut() -> bool + Send + 'static) -> Self {
        self.on_back = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn cursor(mut self, cursor: CursorIcon) -> Self {
        self.cursor = cursor;
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        crate::widget::push_classes(&mut self.classes, class.into());
        self
    }
}

impl Default for GestureDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for GestureDetector {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(GestureDetectorElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            hovered: false,
            pressed: false,
            on_click: self.on_click.clone(),
            on_click_at: self.on_click_at.clone(),
            on_click_with_bounds: self.on_click_with_bounds.clone(),
            on_double_click: self.on_double_click.clone(),
            on_hover_change: self.on_hover_change.clone(),
            on_mouse_down: self.on_mouse_down.clone(),
            on_mouse_up: self.on_mouse_up.clone(),
            on_back: self.on_back.clone(),
            on_secondary_click: self.on_secondary_click.clone(),
            on_middle_click: self.on_middle_click.clone(),
            on_long_press: self.on_long_press.clone(),
            on_pan_start: self.on_pan_start.clone(),
            on_pan_update: self.on_pan_update.clone(),
            on_pan_end: self.on_pan_end.clone(),
            on_swipe: self.on_swipe.clone(),
            on_pinch: self.on_pinch.clone(),
            on_pinch_end: self.on_pinch_end.clone(),
            pan_axis: self.pan_axis,
            pan_mouse: self.pan_mouse,
            fingers: Vec::new(),
            pan: None,
            pinch: None,
            mouse_pan: None,
            cursor: self.cursor,
            child_id: None,
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
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
        if let Some(child) = &self.child {
            let el = child.create_element();
            let id = tree.insert_with_type_id(el, Some(parent_id), child.as_any().type_id());
            child.mount(tree, id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child
            .as_ref()
            .map(|c| vec![c.as_ref() as &dyn Widget])
            .unwrap_or_default()
    }

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct GestureDetectorElement {
    id: ElementId,
    bounds: Rect,
    hovered: bool,
    pressed: bool,
    on_click: Option<ClickCb>,
    on_click_at: Option<ClickAtCb>,
    on_click_with_bounds: Option<ClickBoundsCb>,
    on_double_click: Option<ClickCb>,
    on_hover_change: Option<HoverCb>,
    on_mouse_down: Option<MouseBtnCb>,
    on_mouse_up: Option<MouseBtnCb>,
    on_back: Option<BackCb>,
    on_secondary_click: Option<MouseBtnCb>,
    on_middle_click: Option<MouseBtnCb>,
    on_long_press: Option<MouseBtnCb>,
    on_pan_start: Option<MouseBtnCb>,
    on_pan_update: Option<PanCb>,
    on_pan_end: Option<PanEndCb>,
    on_swipe: Option<SwipeCb>,
    on_pinch: Option<PinchCb>,
    on_pinch_end: Option<PinchEndCb>,
    pan_axis: PanAxis,
    pan_mouse: bool,
    /// Пальцы, чей жест забрал этот элемент: (id, точка).
    fingers: Vec<(u64, Point)>,
    pan: Option<PanState>,
    pinch: Option<PinchState>,
    /// Панорама мышью: начало нажатия; `Some` — кнопка зажата.
    mouse_pan: Option<PanState>,
    cursor: CursorIcon,
    child_id: Option<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

#[derive(Debug)]
struct PanState {
    id: u64,
    start: Point,
    last: Point,
    /// Порог пройден, `on_pan_start` вызван.
    active: bool,
    /// Жест по чужой оси — отдан родителю.
    foreign: bool,
    velocity: crate::input::VelocityTracker,
}

impl PanState {
    fn new(id: u64, at: Point) -> Self {
        let mut velocity = crate::input::VelocityTracker::new();
        velocity.add(at);
        Self { id, start: at, last: at, active: false, foreign: false, velocity }
    }
}

#[derive(Debug, Clone, Copy)]
struct PinchState {
    a: u64,
    b: u64,
    start_dist: f32,
    last_center: Point,
}

fn call<T>(cb: &Option<Arc<Mutex<dyn FnMut(T) + Send>>>, v: T) {
    if let Some(cb) = cb {
        if let Ok(mut f) = cb.lock() {
            f(v);
        }
    }
}

impl GestureDetectorElement {
    fn wants_pan(&self) -> bool {
        self.on_pan_start.is_some() || self.on_pan_update.is_some() || self.on_pan_end.is_some() || self.on_swipe.is_some()
    }

    fn wants_touch(&self) -> bool {
        self.wants_pan() || self.on_pinch.is_some()
    }

    fn finger(&self, id: u64) -> Option<Point> {
        self.fingers.iter().find(|(i, _)| *i == id).map(|(_, p)| *p)
    }

    fn pinch_geometry(&self, p: &PinchState) -> Option<(f32, Point)> {
        let (a, b) = (self.finger(p.a)?, self.finger(p.b)?);
        let dist = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
        Some((dist, Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)))
    }

    /// Шаг панорамы; `false` — жест не наш (чужая ось), отдать родителю.
    fn pan_step(pan: &mut PanState, axis: PanAxis, pos: Point, on_start: &Option<MouseBtnCb>, on_update: &Option<PanCb>) -> bool {
        if pan.foreign {
            return false;
        }
        pan.velocity.add(pos);
        if !pan.active {
            let dx = pos.x - pan.start.x;
            let dy = pos.y - pan.start.y;
            let slop = crate::input::touch_config().tap_slop;
            if dx.abs().max(dy.abs()) < slop {
                return true;
            }
            let horizontal = dx.abs() > dy.abs();
            let ours = match axis {
                PanAxis::Free => true,
                PanAxis::Horizontal => horizontal,
                PanAxis::Vertical => !horizontal,
            };
            if !ours {
                pan.foreign = true;
                return false;
            }
            pan.active = true;
            call(on_start, pan.start);
        }
        let delta = Point::new(pos.x - pan.last.x, pos.y - pan.last.y);
        let (delta, total) = match axis {
            PanAxis::Free => (delta, Point::new(pos.x - pan.start.x, pos.y - pan.start.y)),
            PanAxis::Horizontal => (Point::new(delta.x, 0.0), Point::new(pos.x - pan.start.x, 0.0)),
            PanAxis::Vertical => (Point::new(0.0, delta.y), Point::new(0.0, pos.y - pan.start.y)),
        };
        pan.last = pos;
        call(on_update, PanUpdate { position: pos, delta, total });
        true
    }

    /// Конец панорамы: бросок и смахивание. `true` — жест был панорамой.
    fn pan_finish(&mut self, pan: PanState) -> bool {
        if !pan.active {
            return false;
        }
        let v = pan.velocity.velocity();
        let v = match self.pan_axis {
            PanAxis::Free => v,
            PanAxis::Horizontal => Point::new(v.x, 0.0),
            PanAxis::Vertical => Point::new(0.0, v.y),
        };
        call(&self.on_pan_end, v);
        if self.on_swipe.is_some() {
            let total = Point::new(pan.last.x - pan.start.x, pan.last.y - pan.start.y);
            let horizontal = total.x.abs() > total.y.abs();
            let (dist, speed) = if horizontal { (total.x, v.x) } else { (total.y, v.y) };
            // Бросок в ту же сторону, что и путь, или просто длинный путь.
            let fling = speed.abs() > SWIPE_VELOCITY && speed.signum() == dist.signum();
            if fling || dist.abs() > SWIPE_DISTANCE && speed.abs() > SWIPE_VELOCITY * 0.25 {
                let dir = match (horizontal, dist > 0.0) {
                    (true, true) => SwipeDirection::Right,
                    (true, false) => SwipeDirection::Left,
                    (false, true) => SwipeDirection::Down,
                    (false, false) => SwipeDirection::Up,
                };
                if let Some(cb) = &self.on_swipe {
                    if let Ok(mut f) = cb.lock() {
                        f(dir, speed.abs());
                    }
                }
            }
        }
        true
    }

    fn handle_touch(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::TouchStart { id, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                self.fingers.retain(|(i, _)| i != id);
                self.fingers.push((*id, *position));
                if self.fingers.len() == 1 {
                    if self.wants_pan() {
                        self.pan = Some(PanState::new(*id, *position));
                    }
                } else if self.on_pinch.is_some() && self.pinch.is_none() {
                    // Второй палец: панорама превращается в щипок.
                    if let Some(pan) = self.pan.take() {
                        self.pan_finish(pan);
                    }
                    let (a, b) = (self.fingers[0].0, self.fingers[1].0);
                    let mut p = PinchState { a, b, start_dist: 1.0, last_center: Point::zero() };
                    if let Some((dist, center)) = self.pinch_geometry(&p) {
                        p.start_dist = dist.max(1.0);
                        p.last_center = center;
                    }
                    self.pinch = Some(p);
                }
                EventResult::Handled
            }
            Event::TouchMove { id, position } => {
                if self.finger(*id).is_none() {
                    // Жест отдала вложенная прокрутка (не её ось) — подхватываем.
                    if !self.wants_pan() || self.pan.is_some() || !self.bounds.contains(*position) {
                        return EventResult::Ignored;
                    }
                    self.fingers.push((*id, *position));
                    self.pan = Some(PanState::new(*id, *position));
                    return EventResult::Handled;
                }
                for f in &mut self.fingers {
                    if f.0 == *id {
                        f.1 = *position;
                    }
                }
                if let Some(p) = self.pinch {
                    if let Some((dist, center)) = self.pinch_geometry(&p) {
                        let delta = Point::new(center.x - p.last_center.x, center.y - p.last_center.y);
                        if let Some(pp) = &mut self.pinch {
                            pp.last_center = center;
                        }
                        call(&self.on_pinch, PinchUpdate { scale: dist / p.start_dist, center, delta });
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }
                let axis = self.pan_axis;
                if let Some(pan) = self.pan.as_mut().filter(|p| p.id == *id) {
                    if !Self::pan_step(pan, axis, *position, &self.on_pan_start, &self.on_pan_update) {
                        return EventResult::Ignored;
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::TouchEnd { id, .. } => {
                if self.finger(*id).is_none() {
                    return EventResult::Ignored;
                }
                self.fingers.retain(|(i, _)| i != id);
                if let Some(p) = self.pinch {
                    if p.a == *id || p.b == *id {
                        self.pinch = None;
                        if let Some(cb) = &self.on_pinch_end {
                            if let Ok(mut f) = cb.lock() {
                                f();
                            }
                        }
                    }
                }
                if self.pan.as_ref().is_some_and(|p| p.id == *id) {
                    let pan = self.pan.take().unwrap();
                    let foreign = pan.foreign;
                    self.pan_finish(pan);
                    if foreign {
                        return EventResult::Ignored;
                    }
                }
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }
}

impl Element for GestureDetectorElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(gd) = widget.as_any().downcast_ref::<GestureDetector>() {
            self.on_click = gd.on_click.clone();
            self.on_click_at = gd.on_click_at.clone();
            self.on_click_with_bounds = gd.on_click_with_bounds.clone();
            self.on_double_click = gd.on_double_click.clone();
            self.on_hover_change = gd.on_hover_change.clone();
            self.on_mouse_down = gd.on_mouse_down.clone();
            self.on_mouse_up = gd.on_mouse_up.clone();
            self.on_back = gd.on_back.clone();
            self.on_secondary_click = gd.on_secondary_click.clone();
            self.on_middle_click = gd.on_middle_click.clone();
            self.on_long_press = gd.on_long_press.clone();
            self.on_pan_start = gd.on_pan_start.clone();
            self.on_pan_update = gd.on_pan_update.clone();
            self.on_pan_end = gd.on_pan_end.clone();
            self.on_swipe = gd.on_swipe.clone();
            self.on_pinch = gd.on_pinch.clone();
            self.on_pinch_end = gd.on_pinch_end.clone();
            self.pan_axis = gd.pan_axis;
            self.pan_mouse = gd.pan_mouse;
            self.cursor = gd.cursor;
        }
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
        self.bounds = Rect::new(Point::zero(), size);
        size
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if matches!(event, Event::TouchStart { .. } | Event::TouchMove { .. } | Event::TouchEnd { .. }) {
            if !self.wants_touch() {
                return EventResult::Ignored;
            }
            return self.handle_touch(event, ctx);
        }
        match event {
            Event::LongPress { position } => {
                if let (Some(cb), true) = (&self.on_long_press, self.bounds.contains(*position)) {
                    // Удержание не должно закончиться ещё и щелчком.
                    self.pressed = false;
                    if let Ok(mut f) = cb.lock() {
                        f(*position);
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseMove(pos) if self.mouse_pan.is_some() => {
                let axis = self.pan_axis;
                let pan = self.mouse_pan.as_mut().unwrap();
                Self::pan_step(pan, axis, *pos, &self.on_pan_start, &self.on_pan_update);
                if pan.active {
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::MouseMove(pos) => {
                let inside = self.bounds.contains(*pos);
                if inside != self.hovered {
                    self.hovered = inside;
                    if let Some(ref cb) = self.on_hover_change {
                        if let Ok(mut f) = cb.lock() {
                            f(inside);
                        }
                    }
                    ctx.request_paint();
                }
                if inside {
                    ctx.set_cursor(self.cursor);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position }
                if matches!(button, MouseButton::Right | MouseButton::Middle) =>
            {
                let cb = if *button == MouseButton::Right { &self.on_secondary_click } else { &self.on_middle_click };
                if let (Some(cb), true) = (cb, self.bounds.contains(*position)) {
                    if let Ok(mut f) = cb.lock() {
                        f(*position);
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    self.pressed = true;
                    if self.pan_mouse && self.wants_pan() {
                        self.mouse_pan = Some(PanState::new(u64::MAX, *position));
                    }
                    if let Some(ref cb) = self.on_mouse_down {
                        if let Ok(mut f) = cb.lock() {
                            f(*position);
                        }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, position } => {
                let dragged = match self.mouse_pan.take() {
                    Some(pan) if *button == MouseButton::Left => self.pan_finish(pan),
                    other => {
                        self.mouse_pan = other;
                        false
                    }
                };
                if dragged {
                    self.pressed = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if *button == MouseButton::Left && self.pressed {
                    self.pressed = false;
                    if let Some(ref cb) = self.on_mouse_up {
                        if let Ok(mut f) = cb.lock() {
                            f(*position);
                        }
                    }
                    if self.bounds.contains(*position) {
                        if let Some(ref cb) = self.on_click {
                            if let Ok(mut f) = cb.lock() {
                                f();
                            }
                        }
                        if let Some(ref cb) = self.on_click_at {
                            if let Ok(mut f) = cb.lock() {
                                f(*position);
                            }
                        }
                        if let Some(ref cb) = self.on_click_with_bounds {
                            if let Ok(mut f) = cb.lock() {
                                f(*position, self.bounds);
                            }
                        }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::DoubleClick { position, .. } => {
                if self.bounds.contains(*position) {
                    if let Some(ref cb) = self.on_double_click {
                        if let Ok(mut f) = cb.lock() {
                            f();
                        }
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::BackPressed => {
                if let Some(ref cb) = self.on_back {
                    if let Ok(mut f) = cb.lock() {
                        if f() {
                            return EventResult::Handled;
                        }
                    }
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] {
        match self.child_id {
            Some(ref id) => std::slice::from_ref(id),
            None => &[],
        }
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }
    /// Итоговый размер из раскладки. Собственный `layout` знает только
    /// ограничения, и при неограниченной высоте (так контейнер измеряет
    /// ребёнка, прежде чем раздать ему место по flex-grow) он даёт ноль —
    /// область оставалась нулевой, и нажатия сквозь неё не проходили.
    fn set_content_size(&mut self, size: Size) {
        self.bounds.size = size;
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
        if let Some(node) = tree.elements.get(&self.id) {
            if let Some(first) = node.children.first() {
                self.child_id = Some(*first);
            }
        }
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }
    fn get_classes(&self) -> &[String] {
        &self.classes
    }
    fn element_type_name(&self) -> &str {
        "GestureDetector"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(c) = self.mss.cursor {
            self.cursor = c;
        }
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
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }
}

impl StyledElement for GestureDetectorElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {}
    fn classes(&self) -> &[String] {
        &self.classes
    }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestHarness;
    use crate::widgets::{DecoratedBox, ScrollView};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn counter() -> Arc<AtomicU32> {
        Arc::new(AtomicU32::new(0))
    }

    #[test]
    fn horizontal_pan_and_swipe() {
        let swipes = counter();
        let pans = counter();
        let (s, p) = (swipes.clone(), pans.clone());
        let gd = GestureDetector::new()
            .pan_axis(PanAxis::Horizontal)
            .on_pan_update(move |_| {
                p.fetch_add(1, Ordering::SeqCst);
            })
            .on_swipe(move |dir, _| {
                assert_eq!(dir, SwipeDirection::Left);
                s.fetch_add(1, Ordering::SeqCst);
            })
            .child(DecoratedBox::new());
        let mut h = TestHarness::new(Box::new(gd));
        h.layout(400.0, 400.0);
        h.touch_down(1, Point::new(300.0, 200.0));
        for i in 1..=10 {
            // Скорость считается по времени — бросок 20 px за 5 мс.
            std::thread::sleep(std::time::Duration::from_millis(5));
            h.touch_move(1, Point::new(300.0 - i as f32 * 20.0, 200.0));
        }
        h.touch_up(1);
        assert!(pans.load(Ordering::SeqCst) >= 9);
        assert_eq!(swipes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn foreign_axis_goes_to_parent_scroll() {
        let pans = counter();
        let p = pans.clone();
        let content = crate::widgets::Column::new().child(
            GestureDetector::new()
                .pan_axis(PanAxis::Horizontal)
                .on_pan_update(move |_| {
                    p.fetch_add(1, Ordering::SeqCst);
                })
                .child(crate::widgets::Column::new().height(2000.0)),
        );
        let mut h = TestHarness::new(Box::new(ScrollView::new().vertical().child(content)));
        h.layout(400.0, 400.0);
        h.touch_down(1, Point::new(200.0, 300.0));
        for i in 1..=10 {
            h.touch_move(1, Point::new(200.0, 300.0 - i as f32 * 20.0));
        }
        h.touch_up(1);
        assert_eq!(pans.load(Ordering::SeqCst), 0, "вертикальный жест — не наш");
        let sv = h.find_by_type_name("ScrollView")[0];
        let off = h.tree.get(sv).unwrap().scroll_offset();
        assert!(off.y > 100.0, "прокрутка подхватила жест: {off:?}");
    }

    #[test]
    fn long_press_and_tap() {
        let longs = counter();
        let clicks = counter();
        let (l, c) = (longs.clone(), clicks.clone());
        let gd = GestureDetector::new()
            .on_long_press(move |_| {
                l.fetch_add(1, Ordering::SeqCst);
            })
            .on_click(move || {
                c.fetch_add(1, Ordering::SeqCst);
            })
            .child(DecoratedBox::new());
        let mut h = TestHarness::new(Box::new(gd));
        h.layout(200.0, 200.0);
        h.touch_down(1, Point::new(50.0, 50.0));
        h.touch_up(1);
        assert_eq!(clicks.load(Ordering::SeqCst), 1);
        crate::input::set_touch_config(crate::input::TouchConfig {
            long_press: std::time::Duration::ZERO,
            ..Default::default()
        });
        h.touch_down(2, Point::new(150.0, 150.0));
        assert!(h.touch_poll());
        h.touch_up(2);
        crate::input::set_touch_config(Default::default());
        assert_eq!(longs.load(Ordering::SeqCst), 1);
        assert_eq!(clicks.load(Ordering::SeqCst), 1, "удержание — не щелчок");
    }

    #[test]
    fn pinch_scale() {
        let last = Arc::new(Mutex::new(1.0f32));
        let l = last.clone();
        let gd = GestureDetector::new()
            .on_pinch(move |p| {
                *l.lock().unwrap() = p.scale;
            })
            .child(DecoratedBox::new());
        let mut h = TestHarness::new(Box::new(gd));
        h.layout(400.0, 400.0);
        h.touch_down(1, Point::new(150.0, 200.0));
        h.touch_down(2, Point::new(250.0, 200.0));
        h.touch_move(1, Point::new(100.0, 200.0));
        h.touch_move(2, Point::new(300.0, 200.0));
        h.touch_up(1);
        h.touch_up(2);
        assert!((*last.lock().unwrap() - 2.0).abs() < 0.01);
    }
}
