//! `Draggable` вокруг `ToolButton`: события идут от самого глубокого
//! элемента к корню, поэтому обычная кнопка «заклеймивает» MouseDown, и
//! `Draggable` не получает ни клика, ни старта перетаскивания. В режиме
//! `press_passthrough` кнопка отдаёт нажатия наверх.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use syngui::core::Point;
use syngui::input::{Event, MouseButton};
use syngui::testing::*;
use syngui::widgets::overlay::{Draggable, DropArea};
use syngui::widgets::ToolButton;

fn build(passthrough: bool) -> (TestHarness, Arc<AtomicUsize>) {
    let clicks = Arc::new(AtomicUsize::new(0));
    let clicks_cb = clicks.clone();
    let mut btn = ToolButton::new("\u{e5cd}");
    if passthrough {
        btn = btn.press_passthrough();
    }
    let widget = Draggable::new("tile", "a")
        .on_click(move || {
            clicks_cb.fetch_add(1, Ordering::SeqCst);
        })
        .child(DropArea::new().accept_types(vec!["tile".to_string()]).child(btn));
    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout(200.0, 100.0);
    (harness, clicks)
}

fn button_center(harness: &TestHarness) -> Point {
    let ids = harness.find_by_type_name("ToolButton");
    assert_eq!(ids.len(), 1, "ожидалась одна кнопка");
    let b = harness.element_bounds(ids[0]);
    assert!(b.size.width > 0.0 && b.size.height > 0.0, "кнопка без размера: {b:?}");
    Point::new(b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0)
}

#[test]
fn passthrough_button_lets_draggable_click() {
    let (mut harness, clicks) = build(true);
    let at = button_center(&harness);
    harness.send_events(&[
        Event::MouseDown { button: MouseButton::Left, position: at },
        Event::MouseUp { button: MouseButton::Left, position: at },
    ]);
    assert_eq!(clicks.load(Ordering::SeqCst), 1, "клик должен дойти до Draggable::on_click");
}

#[test]
fn passthrough_button_lets_draggable_start_drag() {
    let (mut harness, clicks) = build(true);
    let at = button_center(&harness);
    harness.send_events(&[
        Event::MouseDown { button: MouseButton::Left, position: at },
        Event::MouseMove(Point::new(at.x + 12.0, at.y + 12.0)),
    ]);
    assert!(harness.tree.drag_state.is_some(), "смещение больше порога должно начать drag");
    harness.send_event(&Event::MouseUp {
        button: MouseButton::Left,
        position: Point::new(at.x + 12.0, at.y + 12.0),
    });
    assert_eq!(clicks.load(Ordering::SeqCst), 0, "после drag клик не срабатывает");
}

#[test]
fn plain_button_swallows_the_press() {
    // Контракт, ради которого существует press_passthrough: обычная кнопка
    // внутри Draggable забирает нажатие себе.
    let (mut harness, clicks) = build(false);
    let at = button_center(&harness);
    harness.send_events(&[
        Event::MouseDown { button: MouseButton::Left, position: at },
        Event::MouseUp { button: MouseButton::Left, position: at },
    ]);
    assert_eq!(clicks.load(Ordering::SeqCst), 0);
}
