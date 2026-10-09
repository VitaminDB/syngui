//! `MultiTouch` — сырые касания каждого пальца с его id: нажал, ведёт, отпустил. Для элементов,
//! которыми управляют несколькими пальцами сразу и независимо (экранный контроллер: стик одним
//! пальцем, кнопки другими; клавиши синтезатора). Жесты (тап, панорама, щипок) — у
//! [`GestureDetector`](super::GestureDetector); здесь их нет.
//!
//! ```ignore
//! MultiTouch::new()
//!     .on_touch(|t: TouchPoint| hit(t.position)) // true — палец взят: его движения и отпускание придут сюда
//!     .child(view)
//! ```
//!
//! Палец, взятый на `Down`, ведётся и за границами виджета (захват по пальцу в дереве). Мышь
//! (левая кнопка, не синтезированная из касания) — палец [`MOUSE_ID`]: проверять на компьютере.

use super::IntoWidget;
use crate::core::sync::Mutex;
use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;

/// id «пальца» мыши.
pub const MOUSE_ID: u64 = u64::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Down,
    Move,
    Up,
    /// Касание отменено (элемент убран, палец отобрали): отпустить без действия.
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchPoint {
    pub id: u64,
    pub phase: TouchPhase,
    /// Точка в координатах виджета (0,0 — его левый верхний угол).
    pub position: Point,
    /// Размер виджета.
    pub size: Size,
}

type TouchCb = Arc<Mutex<dyn FnMut(TouchPoint) -> bool + Send>>;

pub struct MultiTouch {
    on_touch: Option<TouchCb>,
    mouse: bool,
    child: Option<Box<dyn Widget>>,
    classes: Vec<String>,
}

impl Default for MultiTouch {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiTouch {
    pub fn new() -> Self {
        Self { on_touch: None, mouse: true, child: None, classes: Vec::new() }
    }

    /// Касание пальца. На `Down` вернуть `true` — палец взят (иначе событие идёт дальше, ниже
    /// по дереву); на остальных фазах результат не важен.
    pub fn on_touch(mut self, cb: impl FnMut(TouchPoint) -> bool + Send + 'static) -> Self {
        self.on_touch = Some(Arc::new(Mutex::new(cb)));
        self
    }

    /// Мышь как палец [`MOUSE_ID`] (по умолчанию да).
    pub fn mouse(mut self, on: bool) -> Self {
        self.mouse = on;
        self
    }

    pub fn child<M>(mut self, child: impl IntoWidget<M>) -> Self {
        self.child = Some(child.into_widget());
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }
}

impl Widget for MultiTouch {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MultiTouchElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            on_touch: self.on_touch.clone(),
            mouse: self.mouse,
            fingers: Vec::new(),
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
        self.child.as_ref().map(|c| vec![c.as_ref() as &dyn Widget]).unwrap_or_default()
    }
    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct MultiTouchElement {
    id: ElementId,
    bounds: Rect,
    on_touch: Option<TouchCb>,
    mouse: bool,
    /// Взятые пальцы.
    fingers: Vec<u64>,
    child_id: Option<ElementId>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl MultiTouchElement {
    fn call(&self, id: u64, phase: TouchPhase, pos: Point) -> bool {
        let Some(cb) = &self.on_touch else { return false };
        let local = Point::new(pos.x - self.bounds.origin.x, pos.y - self.bounds.origin.y);
        let t = TouchPoint { id, phase, position: local, size: self.bounds.size };
        cb.lock().map(|mut f| f(t)).unwrap_or(false)
    }

    fn down(&mut self, id: u64, pos: Point, ctx: &mut EventContext) -> EventResult {
        if !self.bounds.contains(pos) || self.fingers.contains(&id) {
            return EventResult::Ignored;
        }
        if self.call(id, TouchPhase::Down, pos) {
            self.fingers.push(id);
            ctx.request_paint();
            return EventResult::Handled;
        }
        EventResult::Ignored
    }

    fn moved(&mut self, id: u64, pos: Point, ctx: &mut EventContext) -> EventResult {
        if !self.fingers.contains(&id) {
            return EventResult::Ignored;
        }
        self.call(id, TouchPhase::Move, pos);
        ctx.request_paint();
        EventResult::Handled
    }

    fn up(&mut self, id: u64, pos: Point, ctx: &mut EventContext) -> EventResult {
        let Some(i) = self.fingers.iter().position(|f| *f == id) else { return EventResult::Ignored };
        self.fingers.swap_remove(i);
        self.call(id, TouchPhase::Up, pos);
        ctx.request_paint();
        EventResult::Handled
    }
}

impl Element for MultiTouchElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<MultiTouch>() {
            self.on_touch = w.on_touch.clone();
            self.mouse = w.mouse;
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = if constraints.max_width.is_finite() { constraints.max_width } else { 0.0 };
        let h = if constraints.max_height.is_finite() { constraints.max_height } else { 0.0 };
        let size = Size::new(w, h);
        self.bounds = Rect::new(Point::zero(), size);
        size
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Padding { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::TouchStart { id, position } => self.down(*id, *position, ctx),
            Event::TouchMove { id, position } => self.moved(*id, *position, ctx),
            Event::TouchEnd { id, position } => self.up(*id, *position, ctx),
            Event::MouseDown { button: MouseButton::Left, position } if self.mouse && !crate::input::is_synthesized_mouse() => {
                self.down(MOUSE_ID, *position, ctx)
            }
            Event::MouseMove(position) if self.fingers.contains(&MOUSE_ID) => self.moved(MOUSE_ID, *position, ctx),
            Event::MouseUp { button: MouseButton::Left, position } if self.fingers.contains(&MOUSE_ID) => {
                self.up(MOUSE_ID, *position, ctx)
            }
            // Синтезированные из касания щелчки — не наши: пальцы уже пришли касаниями.
            Event::MouseDown { .. } | Event::MouseUp { .. } if !self.fingers.is_empty() => EventResult::Handled,
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
        "MultiTouch"
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
}

/// Элемент убран с пальцами на нём (контроллер спрятали посреди нажатия) — отпустить их.
impl Drop for MultiTouchElement {
    fn drop(&mut self) {
        for id in std::mem::take(&mut self.fingers) {
            self.call(id, TouchPhase::Cancel, self.bounds.origin);
        }
    }
}

impl StyledElement for MultiTouchElement {
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
    use crate::widgets::Text;

    /// Два пальца независимо: каждый получает свои движения, отпускание одного не трогает другой;
    /// палец, ушедший за границы, ведётся дальше.
    #[test]
    fn fingers_are_independent() {
        let log = Arc::new(std::sync::Mutex::new(Vec::<(u64, TouchPhase)>::new()));
        let l = log.clone();
        let w = MultiTouch::new()
            .on_touch(move |t| {
                l.lock().unwrap().push((t.id, t.phase));
                true
            })
            .child(Text::new("x"));
        let mut h = TestHarness::new(Box::new(w));
        h.layout(200.0, 200.0);
        let p = |x, y| Point::new(x, y);
        h.send_event(&Event::TouchStart { id: 1, position: p(10.0, 10.0) });
        h.send_event(&Event::TouchStart { id: 2, position: p(150.0, 150.0) });
        h.send_event(&Event::TouchMove { id: 1, position: p(20.0, 10.0) });
        h.send_event(&Event::TouchEnd { id: 2, position: p(150.0, 150.0) });
        h.send_event(&Event::TouchMove { id: 1, position: p(400.0, 10.0) });
        h.send_event(&Event::TouchEnd { id: 1, position: p(400.0, 10.0) });
        let got = log.lock().unwrap().clone();
        use TouchPhase::*;
        assert_eq!(got, vec![(1, Down), (2, Down), (1, Move), (2, Up), (1, Move), (1, Up)]);
    }
}
