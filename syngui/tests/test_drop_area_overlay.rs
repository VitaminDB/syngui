//! Прозрачный слой-DropArea поверх содержимого (полосы у краёв док-области):
//! `priority` — перебивает более глубокие цели, `accept_at` — берёт перенос
//! только в своей части, `pointer_transparent` — не отнимает клики. И
//! подсветка не залипает, когда бросок взяла другая цель.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use syngui::core::{Point, Rect};
use syngui::input::{DragData, Event};
use syngui::testing::*;
use syngui::widget::DragState;
use syngui::widgets::overlay::{DropArea, Draggable};
use syngui::widgets::{Column, Stack, StackFit, Text};

#[derive(Default, Clone)]
struct C {
    click: Arc<AtomicUsize>,
    under_drop: Arc<AtomicUsize>,
    edge_drop: Arc<AtomicUsize>,
    edge_over: Arc<AtomicUsize>,
    edge_leave: Arc<AtomicUsize>,
}

const EDGE: f32 = 20.0;

fn build() -> (TestHarness, C) {
    let c = C::default();
    let (click, under_drop) = (c.click.clone(), c.under_drop.clone());
    let content = Column::new()
        .child(
            Draggable::new("other", "")
                .on_click(move || {
                    click.fetch_add(1, Ordering::SeqCst);
                })
                .child(Text::new("Кнопка под слоем")),
        )
        .child(
            DropArea::new()
                .accept_types(vec!["tab".into()])
                .placeholder("")
                .on_drop(move |_| {
                    under_drop.fetch_add(1, Ordering::SeqCst);
                })
                .child(Text::new("Группа вкладок")),
        );
    let (d, o, l) = (c.edge_drop.clone(), c.edge_over.clone(), c.edge_leave.clone());
    let edge = DropArea::new()
        .accept_types(vec!["tab".into()])
        .placeholder("")
        .priority(1)
        .pointer_transparent()
        .accept_at(|p, _| p.x < EDGE)
        .on_drop(move |_| {
            d.fetch_add(1, Ordering::SeqCst);
        })
        .on_drag_over(move |_| {
            o.fetch_add(1, Ordering::SeqCst);
        })
        .on_drag_leave(move || {
            l.fetch_add(1, Ordering::SeqCst);
        })
        .child(Stack::new());
    let mut h = TestHarness::new(Box::new(Stack::new().fit(StackFit::Expand).child(content).child(edge)));
    h.layout(400.0, 300.0);
    (h, c)
}

fn bounds_of(h: &TestHarness, ty: &str, i: usize) -> Rect {
    h.element_bounds(h.find_by_type_name(ty)[i])
}

fn mid(r: Rect, x: f32) -> Point {
    Point::new(r.origin.x + x, r.origin.y + r.size.height / 2.0)
}

#[test]
fn clicks_pass_through_overlay() {
    let (mut h, c) = build();
    let btn = bounds_of(&h, "Draggable", 0);
    assert!(btn.size.width > 0.0);
    // и в полосе у края, и вне её
    h.send_events(&click_at(mid(btn, 5.0)));
    h.send_events(&click_at(mid(btn, btn.size.width / 2.0)));
    assert_eq!(c.click.load(Ordering::SeqCst), 2, "слой не должен забирать клики");
}

#[test]
fn edge_takes_drop_only_at_edge() {
    let (mut h, c) = build();
    let under = bounds_of(&h, "DropArea", 0);
    let data = DragData::new("tab", "p", 0);
    // у края — слой (приоритет выше более глубокой группы)
    let at_edge = mid(under, 5.0);
    h.tree.dispatch_drag_event(&Event::DragMove { position: at_edge, data: data.clone() });
    assert!(c.edge_over.load(Ordering::SeqCst) >= 1);
    h.tree.dispatch_drag_event(&Event::Drop { position: at_edge, data: data.clone() });
    assert_eq!(c.edge_drop.load(Ordering::SeqCst), 1);
    assert_eq!(c.under_drop.load(Ordering::SeqCst), 0);
    // в середине — группа под слоем
    assert!(under.size.width > 2.0 * EDGE + 10.0, "{under:?}");
    let inside = mid(under, under.size.width - 5.0);
    h.tree.dispatch_drag_event(&Event::Drop { position: inside, data });
    assert_eq!(c.edge_drop.load(Ordering::SeqCst), 1);
    assert_eq!(c.under_drop.load(Ordering::SeqCst), 1);
}

#[test]
fn hint_cleared_when_other_target_takes_drop() {
    let (mut h, c) = build();
    let under = bounds_of(&h, "DropArea", 0);
    let data = DragData::new("tab", "p", 0);
    // последнее движение — у края (подсветка слоя), отпустили уже в группе
    h.tree.dispatch_drag_event(&Event::DragMove { position: mid(under, 5.0), data: data.clone() });
    assert_eq!(c.edge_leave.load(Ordering::SeqCst), 0);
    let p = mid(under, under.size.width - 5.0);
    h.tree.drag_state = Some(DragState {
        data,
        start_pos: p,
        current_pos: p,
        hover_element: None,
        drag_offset: Point::zero(),
        source_bounds: Rect::zero(),
        accepting: false,
    });
    let root = h.root_id;
    assert!(h.tree.end_drag(root, p, false));
    assert_eq!(c.under_drop.load(Ordering::SeqCst), 1);
    assert!(c.edge_leave.load(Ordering::SeqCst) >= 1, "слой должен снять подсветку");
}
