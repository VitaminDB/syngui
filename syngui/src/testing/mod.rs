use std::time::Duration;

use web_time::Instant;

use crate::a11y::{A11yTree, FocusManager, NullAdapter};
use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::signal;
use crate::widget::{ElementId, ElementTree, Widget};

pub struct TestHarness {
    pub tree: ElementTree,
    pub root_id: ElementId,
    /// Дерево доступности и порядок Tab для [`frame`](Self::frame): живут
    /// между кадрами, как у окна приложения. Заводятся первым кадром.
    frame_a11y: Option<Box<(A11yTree, FocusManager)>>,
    /// Дерево пересобиралось — следующий кадр синхронизирует a11y.
    a11y_dirty: bool,
}

/// Время фаз одного кадра [`TestHarness::frame`].
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameTimings {
    /// Проходы пересборки (без стилей — они в `styles`).
    pub rebuild: Duration,
    /// Стили новых и изменённых элементов после пересборок
    /// (`apply_styles_dirty`), в обоих местах кадра.
    pub styles: Duration,
    /// Эффекты (`drain_and_run_effects`).
    pub effects: Duration,
    /// Раскладка вместе с пересборкой и раскладкой, которые она заявила.
    pub layout: Duration,
    /// Синхронизация a11y и порядка Tab.
    pub a11y: Duration,
    /// Display list; здесь же подсветка кода `MarkdownView`.
    pub paint: Duration,
    /// Сколько проходов цикла пересборки что-то пересобрали (до 8).
    pub rebuild_rounds: u32,
    /// После раскладки понадобилась пересборка, и дерево раскладывалось
    /// второй раз.
    pub relayout: bool,
    /// Команд в display list (с оверлеями).
    pub commands: usize,
}

impl FrameTimings {
    pub fn total(&self) -> Duration {
        self.rebuild + self.styles + self.effects + self.layout + self.a11y + self.paint
    }
}

impl TestHarness {
    pub fn new(widget: Box<dyn Widget>) -> Self {
        signal::allow_signal_reads_on_this_thread();
        let mut tree = ElementTree::new();
        let element = widget.create_element();
        let root_id = tree.insert(element, None);
        widget.mount(&mut tree, root_id);
        tree.set_root(root_id);
        Self {
            tree,
            root_id,
            frame_a11y: None,
            a11y_dirty: true,
        }
    }

    pub fn layout(&mut self, width: f32, height: f32) -> Size {
        self.tree.viewport_size = Size::new(width, height);
        self.tree
            .layout(self.root_id, Constraints::tight(Size::new(width, height)))
    }

    pub fn layout_loose(&mut self, width: f32, height: f32) -> Size {
        self.tree.viewport_size = Size::new(width, height);
        self.tree
            .layout(self.root_id, Constraints::loose(Size::new(width, height)))
    }

    /// Прогнать `update` корневого элемента новым виджетом — как делает
    /// Reactive при пересборке поддерева с теми же элементами.
    pub fn update_widget(&mut self, widget: Box<dyn Widget>) {
        let mut ctx = crate::widget::context::UpdateContext::new(self.root_id);
        if let Some(element) = self.tree.get_mut(self.root_id) {
            element.update(widget.as_ref(), &mut ctx);
        }
        // Как в дереве после update: элемент, запросивший пересборку,
        // попадает в реестр — иначе `rebuild` его не увидит.
        self.tree.sync_registries_for(self.root_id);
    }

    pub fn rebuild(&mut self) {
        self.tree.rebuild_if_needed(self.root_id);
        signal::drain_and_run_effects();
    }

    /// Полный кадр приложения без GPU — как `AppHandler::render`
    /// (`app/handler/render.rs`): до 8 проходов пересборки, после каждого —
    /// стили новых элементов (`apply_styles_dirty`, если передан `engine`);
    /// эффекты; раскладка по свободным ограничениям окна; пересборка и
    /// повторная раскладка, если раскладка их заявила; синхронизация a11y и
    /// порядка Tab, если дерево пересобиралось; display list и overlay-стек.
    ///
    /// Анимации кадр не тикает: в приложении это делает цикл событий, в
    /// тесте — [`animate`](Self::animate). Атласа шрифтов без GPU нет —
    /// измеритель текста тест ставит сам (`tree.text_measure`), иначе текст
    /// меряется оценкой.
    pub fn frame(
        &mut self,
        engine: Option<&crate::mss::StyleEngine>,
        width: f32,
        height: f32,
    ) -> FrameTimings {
        let root = self.root_id;
        let mut t = FrameTimings::default();

        let started = Instant::now();
        let mut any_rebuilt = false;
        for _ in 0..8 {
            if !self.tree.rebuild_if_needed(root) {
                break;
            }
            any_rebuilt = true;
            t.rebuild_rounds += 1;
            if let Some(engine) = engine {
                let styled = Instant::now();
                crate::mss::cascade::apply_styles_dirty(&mut self.tree, engine);
                t.styles += styled.elapsed();
            }
        }
        if any_rebuilt {
            self.tree.force_full_measure = true;
            self.a11y_dirty = true;
        }
        t.rebuild = started.elapsed().saturating_sub(t.styles);

        let started = Instant::now();
        signal::drain_and_run_effects();
        t.effects = started.elapsed();

        let started = Instant::now();
        let size = Size::new(width, height);
        self.tree.viewport_size = size;
        crate::viewport::publish(size);
        self.tree.set_pixel_snap_scale(0.0);
        let constraints = Constraints::new(0.0, width, 0.0, height);
        self.tree.layout(root, constraints);
        let mut relayout_styles = Duration::ZERO;
        if self.tree.rebuild_if_needed(root) {
            if let Some(engine) = engine {
                let styled = Instant::now();
                crate::mss::cascade::apply_styles_dirty(&mut self.tree, engine);
                relayout_styles = styled.elapsed();
                t.styles += relayout_styles;
            }
            self.tree.force_full_measure = true;
            self.tree.layout(root, constraints);
            self.a11y_dirty = true;
            t.relayout = true;
        }
        self.tree.force_full_measure = false;
        t.layout = started.elapsed().saturating_sub(relayout_styles);

        let started = Instant::now();
        if self.a11y_dirty {
            let (a11y, focus) = &mut **self.frame_a11y.get_or_insert_with(|| {
                Box::new((A11yTree::new(Box::new(NullAdapter)), FocusManager::new()))
            });
            a11y.sync(&self.tree, root);
            focus.rebuild_tab_order(&self.tree, root);
            self.a11y_dirty = false;
        }
        t.a11y = started.elapsed();

        let started = Instant::now();
        let mut list = crate::render::DisplayList::new();
        list.set_surface_size(size);
        list.set_scale_factor(1.0);
        self.tree
            .build_display_list(root, &mut list, Rect::new(Point::zero(), size));
        self.tree.build_drag_overlay(&mut list);
        self.tree.sync_overlay_stack();
        t.paint = started.elapsed();
        let stats = list.stats();
        t.commands = stats.command_count + stats.overlay_command_count;
        t
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
        self.tree
            .get(self.root_id)
            .map(|e| e.bounds().size)
            .unwrap_or(Size::zero())
    }

    pub fn element_bounds(&self, id: ElementId) -> Rect {
        self.tree
            .get(id)
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
        let stylesheet = crate::mss::parse_stylesheet_str(source).expect("test mss must parse");
        let engine = crate::mss::StyleEngine::new(stylesheet);
        crate::mss::cascade::apply_styles_to_tree(&mut self.tree, &engine);
        engine
    }

    pub fn apply_mss_dirty(&mut self, source: &str) -> crate::mss::StyleEngine {
        let stylesheet = crate::mss::parse_stylesheet_str(source).expect("test mss must parse");
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

    /// Прогоняет отрисовку — как кадр приложения. Нужна там, где виджет
    /// узнаёт размер поверхности только при рисовании (попапы, контекстные
    /// меню) и лишь после этого попадает в overlay-стек, через который к
    /// нему приходят клики.
    pub fn paint(&mut self) -> crate::render::DisplayList {
        let mut list = crate::render::DisplayList::new();
        list.set_surface_size(self.tree.viewport_size);
        let clip = Rect::new(crate::core::Point::zero(), self.tree.viewport_size);
        self.tree.build_display_list(self.root_id, &mut list, clip);
        self.tree.sync_overlay_stack();
        list
    }

    /// Тик анимаций — как кадр приложения. Обходит только элементы,
    /// зарегистрированные в реестре анимаций.
    pub fn animate(&mut self, dt: std::time::Duration) -> bool {
        let r = self.tree.animate(self.root_id, dt);
        self.tree.rebuild_if_needed(self.root_id);
        r
    }

    /// Стоит ли элемент в реестре анимаций (получает ли `animate`).
    pub fn is_animating(&self, id: ElementId) -> bool {
        self.tree.animation_registry.contains(&id)
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
        Event::MouseDown {
            button: MouseButton::Left,
            position: point,
        },
        Event::MouseUp {
            button: MouseButton::Left,
            position: point,
        },
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
            $w,
            $h,
            size.width,
            size.height
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
            $x,
            $y,
            $w,
            $h,
            bounds.origin.x,
            bounds.origin.y,
            bounds.size.width,
            bounds.size.height
        );
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::signal::{create_effect, use_signal};
    use crate::widgets::containers::reactive::Reactive;

    /// Кадр харнесса повторяет кадр приложения: новые элементы получают
    /// стили до раскладки, а сигнал, выставленный эффектом, пересобирает
    /// ветку в том же кадре — пересборкой после раскладки. `rebuild()` так
    /// не умеет: запись эффекта он увидел бы только следующим вызовом.
    #[test]
    fn frame_styles_new_children_and_catches_effect_writes() {
        signal::allow_signal_reads_on_this_thread();
        let shown = use_signal(false);
        let label = use_signal(String::new());
        create_effect(move || {
            let text = if shown.get() { "есть" } else { "" };
            label.set(text.to_string());
        });
        let mut h = TestHarness::new(Box::new(Reactive::new(move || -> Vec<Box<dyn Widget>> {
            let mut out: Vec<Box<dyn Widget>> = Vec::new();
            if shown.get() {
                out.push(Box::new(DecoratedBox::new().class("box")));
            }
            let text = label.get();
            if !text.is_empty() {
                out.push(Box::new(Text::new(text).class("label")));
            }
            out
        })));
        let engine = h.apply_mss(".box { width: 30px; height: 40px; }");
        h.frame(Some(&engine), 200.0, 100.0);
        assert!(h.find_by_class("box").is_empty());

        shown.set(true);
        let t = h.frame(Some(&engine), 200.0, 100.0);
        let boxes = h.find_by_class("box");
        assert_eq!(boxes.len(), 1);
        let b = h.element_bounds(boxes[0]);
        assert!(
            (b.size.height - 40.0).abs() < 0.5,
            "новый элемент раскладывался без стилей: {b:?}"
        );
        assert_eq!(h.find_by_class("label").len(), 1, "запись эффекта не дошла до дерева в том же кадре");
        assert!(t.relayout, "ветку, пересобранную эффектом, кадр обязан разложить заново");
        assert!(t.rebuild_rounds >= 1);
        assert!(t.commands > 0, "текст метки должен попасть в display list");
    }
}
