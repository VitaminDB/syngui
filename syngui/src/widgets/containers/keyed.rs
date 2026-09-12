//! Строка списка с ключом и версией.
//!
//! Дерево syngui сверяет детей по позиции, поэтому вставка сообщения в ленту
//! пересоздавала все пузырьки вместе с их состоянием. `Keyed` даёт ребёнку
//! ключ ([`Widget::widget_key`]) — по нему дерево узнаёт строку на новой
//! позиции — и версию: пока она та же, сборщик строки не вызывается вовсе.
//! То есть пересборка списка не стоит ни разбора markdown, ни создания
//! элементов для строк, которые не менялись.
//!
//! Сигналы `Keyed` отслеживает как [`Reactive`](super::reactive::Reactive):
//! если строка читает сигнал (раскрытие блока, стрим-хвост), её пересоберёт
//! этот сигнал, не трогая соседей.

use crate::core::{Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::render::DisplayList;
use crate::signal;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, LayoutHint, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;

type RowBuilder = Arc<dyn Fn() -> Box<dyn Widget> + Send + Sync>;

pub struct Keyed {
    key: u64,
    version: u64,
    builder: RowBuilder,
}

impl Keyed {
    /// `key` — кто это (переживает вставку и удаление соседей),
    /// `version` — что показывать (меняется, когда строку надо собрать
    /// заново).
    pub fn new(
        key: u64,
        version: u64,
        builder: impl Fn() -> Box<dyn Widget> + Send + Sync + 'static,
    ) -> Self {
        Self {
            key,
            version,
            builder: Arc::new(builder),
        }
    }

    pub fn from_arc(key: u64, version: u64, builder: RowBuilder) -> Self {
        Self {
            key,
            version,
            builder,
        }
    }
}

impl Widget for Keyed {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(KeyedElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            child_ids: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            key: self.key,
            version: self.version,
            builder: self.builder.clone(),
            mounted: false,
            version_changed: false,
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

    fn widget_key(&self) -> Option<u64> {
        Some(self.key)
    }
}

struct KeyedElement {
    id: ElementId,
    bounds: Rect,
    child_ids: Vec<ElementId>,
    dirty_flags: DirtyFlags,
    key: u64,
    version: u64,
    builder: RowBuilder,
    mounted: bool,
    version_changed: bool,
}

impl Element for KeyedElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<Keyed>() {
            // Сборщик берём свежий всегда: в нём живёт снимок данных, из
            // которого строится строка. Иначе пересборка по сигналу
            // построила бы строку из устаревшего снимка.
            self.builder = w.builder.clone();
            self.key = w.key;
            if self.version != w.version {
                self.version = w.version;
                self.version_changed = true;
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = constraints.min_width.max(0.0);
        let h = constraints.min_height.max(0.0);
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn layout_hint(&self) -> LayoutHint {
        LayoutHint::Loose
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {}

    fn handle_event(
        &mut self,
        _event: &Event,
        _ctx: &mut crate::widget::context::EventContext,
    ) -> EventResult {
        EventResult::Ignored
    }

    fn passthrough_hit_test(&self) -> bool {
        true
    }

    fn children(&self) -> &[ElementId] {
        &self.child_ids
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
        "Keyed"
    }

    fn manages_own_children(&self) -> bool {
        true
    }

    fn needs_rebuild(&self) -> bool {
        !self.mounted || self.version_changed || signal::is_element_dirty(self.id)
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        signal::begin_tracking(self.id);
        signal::begin_element_scope(self.id);
        let child = (self.builder)();
        signal::end_element_scope();
        signal::end_tracking();
        vec![child]
    }

    fn clear_rebuild(&mut self) {
        self.mounted = true;
        self.version_changed = false;
        signal::clear_element_dirty(self.id);
    }
}

impl Drop for KeyedElement {
    fn drop(&mut self) {
        signal::cleanup_element(self.id);
    }
}

#[cfg(all(test, feature = "testing"))]
mod tests {
    use super::Keyed;
    use crate::perf::counters::snapshot;
    use crate::prelude::*;
    use crate::signal::use_signal;
    use crate::testing::TestHarness;
    use crate::widgets::containers::reactive::Reactive;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Лента из строк `(ключ, версия)`; счётчик считает, сколько строк
    /// действительно собрано.
    fn feed(
        rows: RwSignal<Vec<(u64, u64)>>,
        builds: Arc<AtomicUsize>,
    ) -> (TestHarness, ) {
        let h = TestHarness::new(Box::new(Reactive::new(move || -> Vec<Box<dyn Widget>> {
            rows.get()
                .into_iter()
                .map(|(key, version)| {
                    let builds = builds.clone();
                    Box::new(Keyed::new(key, version, move || {
                        builds.fetch_add(1, Ordering::Relaxed);
                        Box::new(Text::new(format!("строка {key}/{version}"))) as Box<dyn Widget>
                    })) as Box<dyn Widget>
                })
                .collect()
        })));
        (h,)
    }

    fn ids(h: &TestHarness) -> Vec<ElementId> {
        h.find_by_type_name("Keyed")
    }

    /// Вставка в середину не трогает соседей: их элементы те же, собрана
    /// только новая строка. При позиционной сверке пересобрались бы все.
    #[test]
    fn insert_in_the_middle_keeps_neighbours() {
        crate::signal::allow_signal_reads_on_this_thread();
        let rows = use_signal(vec![(1u64, 1u64), (2, 1), (3, 1)]);
        let builds = Arc::new(AtomicUsize::new(0));
        let (mut h,) = feed(rows, builds.clone());
        h.rebuild();
        h.layout(600.0, 400.0);
        let before = ids(&h);
        assert_eq!(before.len(), 3);
        assert_eq!(builds.swap(0, Ordering::Relaxed), 3);

        let start = snapshot();
        rows.set(vec![(1, 1), (9, 1), (2, 1), (3, 1)]);
        h.rebuild();
        h.layout(600.0, 400.0);

        let after = ids(&h);
        assert_eq!(after.len(), 4);
        assert_eq!(builds.load(Ordering::Relaxed), 1, "собрана только новая строка");
        assert_eq!(after[0], before[0]);
        assert_eq!(after[2], before[1], "сосед переехал вместе со своим элементом");
        assert_eq!(after[3], before[2]);
        let d = snapshot().since(&start);
        assert_eq!(d.elements_removed, 0, "соседей не удаляли");
    }

    /// Перестановка строк не создаёт и не удаляет элементы.
    #[test]
    fn reorder_moves_elements_without_recreating() {
        crate::signal::allow_signal_reads_on_this_thread();
        let rows = use_signal(vec![(1u64, 1u64), (2, 1), (3, 1)]);
        let builds = Arc::new(AtomicUsize::new(0));
        let (mut h,) = feed(rows, builds.clone());
        h.rebuild();
        h.layout(600.0, 400.0);
        let before = ids(&h);
        builds.store(0, Ordering::Relaxed);

        let start = snapshot();
        rows.set(vec![(3, 1), (1, 1), (2, 1)]);
        h.rebuild();
        h.layout(600.0, 400.0);
        let after = ids(&h);

        assert_eq!(after, vec![before[2], before[0], before[1]]);
        assert_eq!(builds.load(Ordering::Relaxed), 0, "строки не пересобирались");
        let d = snapshot().since(&start);
        assert_eq!((d.elements_created, d.elements_removed), (0, 0));
    }

    /// Меняется версия одной строки — пересобирается только она.
    #[test]
    fn version_change_rebuilds_only_its_row() {
        crate::signal::allow_signal_reads_on_this_thread();
        let rows = use_signal(vec![(1u64, 1u64), (2, 1), (3, 1)]);
        let builds = Arc::new(AtomicUsize::new(0));
        let (mut h,) = feed(rows, builds.clone());
        h.rebuild();
        h.layout(600.0, 400.0);
        let before = ids(&h);
        builds.store(0, Ordering::Relaxed);

        rows.set(vec![(1, 1), (2, 2), (3, 1)]);
        h.rebuild();
        h.layout(600.0, 400.0);

        assert_eq!(builds.load(Ordering::Relaxed), 1);
        assert_eq!(ids(&h), before, "элементы строк остались теми же");
    }

    /// Удаление строки уносит её элемент и не трогает остальные.
    #[test]
    fn removing_a_row_keeps_the_rest() {
        crate::signal::allow_signal_reads_on_this_thread();
        let rows = use_signal(vec![(1u64, 1u64), (2, 1), (3, 1)]);
        let builds = Arc::new(AtomicUsize::new(0));
        let (mut h,) = feed(rows, builds.clone());
        h.rebuild();
        h.layout(600.0, 400.0);
        let before = ids(&h);
        builds.store(0, Ordering::Relaxed);

        rows.set(vec![(1, 1), (3, 1)]);
        h.rebuild();
        h.layout(600.0, 400.0);

        assert_eq!(ids(&h), vec![before[0], before[2]]);
        assert_eq!(builds.load(Ordering::Relaxed), 0);
    }

    /// Дубликаты ключей — обратно к позиционной сверке, без паники.
    #[test]
    fn duplicate_keys_fall_back_to_positional() {
        crate::signal::allow_signal_reads_on_this_thread();
        let rows = use_signal(vec![(1u64, 1u64), (1, 2)]);
        let builds = Arc::new(AtomicUsize::new(0));
        let (mut h,) = feed(rows, builds.clone());
        h.rebuild();
        h.layout(600.0, 400.0);
        assert_eq!(ids(&h).len(), 2);

        rows.set(vec![(1, 3), (1, 4)]);
        h.rebuild();
        h.layout(600.0, 400.0);
        assert_eq!(ids(&h).len(), 2);
    }

    /// Ключ есть не у всех детей — сверка остаётся позиционной, иначе
    /// безключевые пересоздавались бы на каждой сборке.
    #[test]
    fn mixed_children_stay_positional() {
        crate::signal::allow_signal_reads_on_this_thread();
        let gen = use_signal(0u64);
        let builds = Arc::new(AtomicUsize::new(0));
        let b = builds.clone();
        let mut h = TestHarness::new(Box::new(Reactive::new(move || -> Vec<Box<dyn Widget>> {
            let n = gen.get();
            let b = b.clone();
            vec![
                Box::new(Text::new(format!("шапка {n}"))) as Box<dyn Widget>,
                Box::new(Keyed::new(7, 1, move || {
                    b.fetch_add(1, Ordering::Relaxed);
                    Box::new(Text::new("строка")) as Box<dyn Widget>
                })),
            ]
        })));
        h.rebuild();
        h.layout(600.0, 400.0);
        let before = ids(&h);
        builds.store(0, Ordering::Relaxed);

        gen.set(1);
        h.rebuild();
        h.layout(600.0, 400.0);

        // Позиционная сверка: типы совпали, элементы те же, версия строки
        // не изменилась — сборщик не звали.
        assert_eq!(ids(&h), before);
        assert_eq!(builds.load(Ordering::Relaxed), 0);
    }
}
