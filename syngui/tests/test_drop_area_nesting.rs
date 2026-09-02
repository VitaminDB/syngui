//! Вложенные DropArea: drag-события дерева идут самой глубокой цели под
//! курсором, и только если она их не взяла — следующей. Иначе карточка
//! доски, лежащей внутри редактора страницы, при дропе уходила бы и в
//! колонку, и на страницу.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use syngui::core::Point;
use syngui::input::{DragData, Event};
use syngui::testing::*;
use syngui::widgets::overlay::DropArea;
use syngui::widgets::{Column, Text};

struct Counters {
    outer: Arc<AtomicUsize>,
    inner: Arc<AtomicUsize>,
    outer_over: Arc<AtomicUsize>,
    inner_leave: Arc<AtomicUsize>,
}

fn build(inner_types: Vec<&str>, outer_types: Vec<&str>) -> (TestHarness, Counters) {
    let c = Counters {
        outer: Arc::new(AtomicUsize::new(0)),
        inner: Arc::new(AtomicUsize::new(0)),
        outer_over: Arc::new(AtomicUsize::new(0)),
        inner_leave: Arc::new(AtomicUsize::new(0)),
    };
    let (outer, inner, outer_over, inner_leave) =
        (c.outer.clone(), c.inner.clone(), c.outer_over.clone(), c.inner_leave.clone());
    let widget = DropArea::new()
        .accept_types(outer_types.into_iter().map(String::from).collect())
        .on_drop(move |_| {
            outer.fetch_add(1, Ordering::SeqCst);
        })
        .on_drag_over(move |_| {
            outer_over.fetch_add(1, Ordering::SeqCst);
        })
        .child(
            Column::new()
                .child(Text::new("Верхняя зона внешней области"))
                .child(
                    DropArea::new()
                        .accept_types(inner_types.into_iter().map(String::from).collect())
                        .on_drop(move |_| {
                            inner.fetch_add(1, Ordering::SeqCst);
                        })
                        .on_drag_leave(move || {
                            inner_leave.fetch_add(1, Ordering::SeqCst);
                        })
                        .child(Text::new("Внутренняя область")),
                )
                .child(Text::new("Нижняя зона внешней области")),
        );
    let mut h = TestHarness::new(Box::new(widget));
    h.layout(400.0, 300.0);
    (h, c)
}

fn areas(h: &TestHarness) -> (Point, Point) {
    let ids = h.find_by_type_name("DropArea");
    assert_eq!(ids.len(), 2);
    let outer = h.element_bounds(ids[0]);
    let inner = h.element_bounds(ids[1]);
    assert!(inner.size.height > 0.0 && outer.size.height > inner.size.height);
    let in_inner = Point::new(inner.origin.x + 10.0, inner.origin.y + inner.size.height / 2.0);
    let in_outer_only = Point::new(outer.origin.x + 10.0, outer.origin.y + 2.0);
    (in_inner, in_outer_only)
}

#[test]
fn deepest_area_takes_the_drop_alone() {
    let (mut h, c) = build(vec!["card"], vec!["card"]);
    let (in_inner, in_outer_only) = areas(&h);
    let data = DragData::new("card", "k1", 0);
    h.tree.dispatch_drag_event(&Event::DragMove { position: in_inner, data: data.clone() });
    h.tree.dispatch_drag_event(&Event::Drop { position: in_inner, data: data.clone() });
    assert_eq!(c.inner.load(Ordering::SeqCst), 1, "внутренняя область должна принять дроп");
    assert_eq!(c.outer.load(Ordering::SeqCst), 0, "внешняя область дроп получать не должна");
    // Движение над внутренней — внешняя не «над», on_drag_over не зовётся.
    assert_eq!(c.outer_over.load(Ordering::SeqCst), 0);

    h.tree.dispatch_drag_event(&Event::DragMove { position: in_outer_only, data: data.clone() });
    assert_eq!(c.inner_leave.load(Ordering::SeqCst), 1, "уход с внутренней области — DragLeave ей");
    assert!(c.outer_over.load(Ordering::SeqCst) >= 1);
    h.tree.dispatch_drag_event(&Event::Drop { position: in_outer_only, data });
    assert_eq!(c.outer.load(Ordering::SeqCst), 1);
    assert_eq!(c.inner.load(Ordering::SeqCst), 1);
}

#[test]
fn drop_falls_through_to_the_next_area_by_type() {
    // Внутренняя принимает только «card», внешняя — всё: файл над внутренней
    // достаётся внешней.
    let (mut h, c) = build(vec!["card"], vec![]);
    let (in_inner, _) = areas(&h);
    let data = DragData::new("file", "/tmp/a.png", 0);
    h.tree.dispatch_drag_event(&Event::Drop { position: in_inner, data });
    assert_eq!(c.inner.load(Ordering::SeqCst), 0);
    assert_eq!(c.outer.load(Ordering::SeqCst), 1);
}
