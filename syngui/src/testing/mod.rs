use crate::input::{Event, EventResult, Key, MouseButton};
use crate::core::{Point, Size, Rect};
use crate::layout::Constraints;
use crate::widget::{ElementId, ElementTree, Widget};
use crate::signal;

pub struct TestHarness {
    pub tree: ElementTree,
    pub root_id: ElementId,
}

impl TestHarness {
    pub fn new(widget: Box<dyn Widget>) -> Self {
        signal::init_main_thread();
        let mut tree = ElementTree::new();
        let element = widget.create_element();
        let root_id = tree.insert(element, None);
        widget.mount(&mut tree, root_id);
        tree.set_root(root_id);
        Self { tree, root_id }
    }

    pub fn layout(&mut self, width: f32, height: f32) -> Size {
        self.tree.viewport_size = Size::new(width, height);
        self.tree.layout(self.root_id, Constraints::tight(Size::new(width, height)))
    }

    pub fn layout_loose(&mut self, width: f32, height: f32) -> Size {
        self.tree.viewport_size = Size::new(width, height);
        self.tree.layout(self.root_id, Constraints::loose(Size::new(width, height)))
    }

    pub fn rebuild(&mut self) {
        self.tree.rebuild_if_needed(self.root_id);
        signal::drain_and_run_effects();
    }

    pub fn send_event(&mut self, event: &Event) -> EventResult {
        self.tree.handle_event(self.root_id, event)
    }

    pub fn send_events(&mut self, events: &[Event]) {
        for event in events {
            self.tree.handle_event(self.root_id, event);
        }
    }

    pub fn root_size(&self) -> Size {
        self.tree.get(self.root_id)
            .map(|e| e.bounds().size)
            .unwrap_or(Size::zero())
    }

    pub fn element_bounds(&self, id: ElementId) -> Rect {
        self.tree.get(id)
            .map(|e| e.bounds())
            .unwrap_or(Rect::zero())
    }

    pub fn find_by_type_name(&self, name: &str) -> Vec<ElementId> {
        let mut results = Vec::new();
        self.walk_tree(self.root_id, &mut |id, elem| {
            if elem.element_type_name() == name {
                results.push(id);
            }
        });
        results
    }

    pub fn apply_mss(&mut self, source: &str) -> crate::mss::StyleEngine {
        let stylesheet = crate::mss::parse_stylesheet_str(source)
            .expect("test mss must parse");
        let engine = crate::mss::StyleEngine::new(stylesheet);
        crate::mss::cascade::apply_styles_to_tree(&mut self.tree, &engine);
        engine
    }

    pub fn apply_mss_dirty(&mut self, source: &str) -> crate::mss::StyleEngine {
        let stylesheet = crate::mss::parse_stylesheet_str(source)
            .expect("test mss must parse");
        let engine = crate::mss::StyleEngine::new(stylesheet);
        crate::mss::cascade::apply_styles_dirty(&mut self.tree, &engine);
        engine
    }

    pub fn apply_styles(&mut self, engine: &crate::mss::StyleEngine) {
        crate::mss::cascade::apply_styles_to_tree(&mut self.tree, engine);
    }

    pub fn apply_styles_dirty(&mut self, engine: &crate::mss::StyleEngine) -> bool {
        crate::mss::cascade::apply_styles_dirty(&mut self.tree, engine)
    }

    pub fn element_mss(&self, id: ElementId) -> Option<&crate::mss::MssFields> {
        self.tree.get(id).and_then(|el| el.mss())
    }

    pub fn set_classes(&mut self, id: ElementId, classes: Vec<String>) {
        if let Some(node) = self.tree.elements.get_mut(&id) {
            node.element.set_classes(classes);
            node.styles_dirty = true;
        }
    }

    pub fn find_by_class(&self, class: &str) -> Vec<ElementId> {
        let mut results = Vec::new();
        self.walk_tree(self.root_id, &mut |id, elem| {
            if elem.get_classes().iter().any(|c| c == class) {
                results.push(id);
            }
        });
        results
    }

    pub fn element_count(&self) -> usize {
        let mut count = 0;
        self.walk_tree(self.root_id, &mut |_, _| count += 1);
        count
    }

    fn walk_tree(&self, id: ElementId, f: &mut dyn FnMut(ElementId, &dyn crate::widget::Element)) {
        if let Some(elem) = self.tree.get(id) {
            f(id, elem.as_ref());
            let children: Vec<ElementId> = self.tree.children_of(id).to_vec();
            for child_id in children {
                self.walk_tree(child_id, f);
            }
        }
    }
}

pub fn click_at(point: Point) -> Vec<Event> {
    vec![
        Event::MouseMove(point),
        Event::MouseDown { button: MouseButton::Left, position: point },
        Event::MouseUp { button: MouseButton::Left, position: point },
    ]
}

pub fn type_text(text: &str) -> Vec<Event> {
    text.chars().map(Event::CharInput).collect()
}

pub fn press_key(key: Key) -> Vec<Event> {
    vec![Event::KeyDown(key), Event::KeyUp(key)]
}

#[macro_export]
macro_rules! assert_size {
    ($harness:expr, $w:expr, $h:expr) => {{
        let size = $harness.root_size();
        assert!(
            (size.width - $w as f32).abs() < 1.0 && (size.height - $h as f32).abs() < 1.0,
            "Expected size ({}, {}), got ({}, {})",
            $w, $h, size.width, size.height
        );
    }};
}

#[macro_export]
macro_rules! assert_bounds {
    ($harness:expr, $id:expr, $x:expr, $y:expr, $w:expr, $h:expr) => {{
        let bounds = $harness.element_bounds($id);
        assert!(
            (bounds.origin.x - $x as f32).abs() < 1.0
                && (bounds.origin.y - $y as f32).abs() < 1.0
                && (bounds.size.width - $w as f32).abs() < 1.0
                && (bounds.size.height - $h as f32).abs() < 1.0,
            "Expected bounds ({}, {}, {}, {}), got ({}, {}, {}, {})",
            $x, $y, $w, $h,
            bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height
        );
    }};
}
