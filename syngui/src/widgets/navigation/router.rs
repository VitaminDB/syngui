use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::EventContext;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Debug, Clone)]
pub struct Router {
    current: String,
    history: Vec<String>,
    history_index: usize,
    routes: Vec<String>,
    changed: Arc<AtomicBool>,
}

impl Router {
    pub fn new(routes: Vec<impl Into<String>>, initial: impl Into<String>) -> Self {
        let initial = initial.into();
        Self {
            current: initial.clone(),
            history: vec![initial],
            history_index: 0,
            routes: routes.into_iter().map(Into::into).collect(),
            changed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    pub fn changed_flag(&self) -> Arc<AtomicBool> {
        self.changed.clone()
    }

    pub fn navigate(&mut self, route: impl Into<String>) {
        let route = route.into();
        if route == self.current {
            return;
        }
        self.history.truncate(self.history_index + 1);
        self.history.push(route.clone());
        self.history_index = self.history.len() - 1;
        self.current = route;
        self.changed.store(true, Ordering::Relaxed);
    }

    pub fn back(&mut self) -> bool {
        if self.history_index > 0 {
            self.history_index -= 1;
            self.current = self.history[self.history_index].clone();
            self.changed.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn forward(&mut self) -> bool {
        if self.history_index + 1 < self.history.len() {
            self.history_index += 1;
            self.current = self.history[self.history_index].clone();
            self.changed.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    pub fn routes(&self) -> &[String] {
        &self.routes
    }

    pub fn history(&self) -> &[String] {
        &self.history
    }

    pub fn history_index(&self) -> usize {
        self.history_index
    }
}

type RouteBuilder = Arc<dyn Fn() -> Box<dyn Widget> + Send + Sync>;

pub struct RouterView {
    router: Arc<Mutex<Router>>,
    builders: Vec<(String, RouteBuilder)>,
    handle_back: bool,
}

impl RouterView {
    pub fn new(router: Arc<Mutex<Router>>) -> Self {
        Self {
            router,
            builders: Vec::new(),
            handle_back: true,
        }
    }

    pub fn route(
        mut self,
        key: impl Into<String>,
        builder: impl Fn() -> Box<dyn Widget> + Send + Sync + 'static,
    ) -> Self {
        self.builders.push((key.into(), Arc::new(builder)));
        self
    }

    /// Обрабатывать ли [`Event::BackPressed`] самим RouterView (по умолчанию —
    /// да: шаг назад по истории). Приложение, которое держит вокруг роутера
    /// собственное состояние (подсветка навигации, крошки) и обрабатывает
    /// «назад» само, выключает встроенную обработку, иначе RouterView перехватит
    /// событие раньше и состояние приложения разъедется с историей.
    pub fn handle_back(mut self, on: bool) -> Self {
        self.handle_back = on;
        self
    }
}

impl Widget for RouterView {
    fn create_element(&self) -> Box<dyn Element> {
        let router_lock = self.router.lock().unwrap();
        let active_key = router_lock.current().to_string();
        let changed_flag = router_lock.changed_flag();
        drop(router_lock);

        Box::new(RouterViewElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            router: self.router.clone(),
            builders: self.builders.clone(),
            active_key,
            pending_rebuild: false,
            changed_flag,
            handle_back: self.handle_back,
            classes: Vec::new(),
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
        let active_key = self.router.lock().unwrap().current().to_string();
        if let Some((_, builder)) = self.builders.iter().find(|(k, _)| k == &active_key) {
            let child_widget = builder();
            let child_element = child_widget.create_element();
            let child_id = tree.insert_with_type_id(
                child_element,
                Some(parent_id),
                child_widget.as_any().type_id(),
            );
            child_widget.mount(tree, child_id);
        }
    }
}

struct RouterViewElement {
    id: ElementId,
    bounds: Rect,
    router: Arc<Mutex<Router>>,
    builders: Vec<(String, RouteBuilder)>,
    active_key: String,
    pending_rebuild: bool,
    changed_flag: Arc<AtomicBool>,
    handle_back: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl Element for RouterViewElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(rv) = widget.as_any().downcast_ref::<RouterView>() {
            self.router = rv.router.clone();
            self.builders = rv.builders.clone();
            let router_lock = rv.router.lock().unwrap();
            self.changed_flag = router_lock.changed_flag();
            drop(router_lock);
            self.active_key = self.router.lock().unwrap().current().to_string();
            self.handle_back = rv.handle_back;
            self.pending_rebuild = true;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn manages_own_children(&self) -> bool {
        true
    }

    fn animate(&mut self, _dt: Duration) -> bool {
        if self.changed_flag.swap(false, Ordering::Relaxed) {
            let current = self.router.lock().unwrap().current().to_string();
            if current != self.active_key {
                self.active_key = current;
                self.pending_rebuild = true;
                return true;
            }
        }
        false
    }

    fn wants_animate_tick(&self) -> bool {
        true
    }

    fn needs_rebuild(&self) -> bool {
        self.pending_rebuild
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        if let Some((_, builder)) = self.builders.iter().find(|(k, _)| k == &self.active_key) {
            vec![builder()]
        } else {
            vec![]
        }
    }

    fn clear_rebuild(&mut self) {
        self.pending_rebuild = false;
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
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Stack { expand: false }
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {
    }

    fn handle_event(&mut self, event: &Event, _ctx: &mut EventContext) -> EventResult {
        if self.handle_back && matches!(event, Event::BackPressed) {
            if let Ok(mut r) = self.router.lock() {
                if r.can_go_back() {
                    r.back();
                    return EventResult::Handled;
                }
            }
        }
        EventResult::Ignored
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

    fn element_type_name(&self) -> &str { "RouterView" }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
    fn get_classes(&self) -> &[String] {
        &self.classes
    }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
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

impl StyledElement for RouterViewElement {
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
    use crate::signal::use_signal;
    use crate::testing::TestHarness;
    use crate::widget::Text;
    use crate::widgets::Reactive;

    #[test]
    fn keeps_active_route_when_parent_rebuilds() {
        let router = Arc::new(Mutex::new(Router::new(vec!["a", "b"], "a")));
        let tick = use_signal(0u32);
        let inner = router.clone();
        let widget = Reactive::new(move || {
            let _ = tick.get();
            vec![Box::new(
                RouterView::new(inner.clone())
                    .route("a", || Box::new(Text::new("A")))
                    .route("b", || Box::new(Text::new("B"))),
            ) as Box<dyn Widget>]
        });
        let mut harness = TestHarness::new(Box::new(widget));
        harness.rebuild();
        assert_eq!(harness.find_by_type_name("Text").len(), 1);
        tick.set(1);
        harness.rebuild();
        assert_eq!(harness.find_by_type_name("Text").len(), 1);
        router.lock().unwrap().navigate("b");
        harness.rebuild();
        assert_eq!(harness.find_by_type_name("Text").len(), 1);
    }
}
