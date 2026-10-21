//! Касания пальцем у виджетов, которые сами `Touch*` не разбирают:
//! перетаскивание ручек (`Element::drags_with_touch`) и прокрутка колесом
//! того, что листается только колесом (списки, редакторы).

use std::sync::{Arc, Mutex};

use syngui::core::Point;
use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::containers::Stack;

fn settle(h: &mut TestHarness, w: f32, hgt: f32) {
    h.rebuild();
    h.layout(w, hgt);
}

fn drag(h: &mut TestHarness, from: Point, to: Point, steps: usize, w: f32, hgt: f32) {
    h.touch_down(1, from);
    settle(h, w, hgt);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        h.touch_move(
            1,
            Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t),
        );
        settle(h, w, hgt);
    }
    h.touch_up(1);
    settle(h, w, hgt);
}

#[test]
fn tick_slider_follows_finger() {
    use syngui::widgets::input::TickSlider;
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = log.clone();
    let widget = Stack::new().child(
        TickSlider::new()
            .horizontal()
            .range(0.0, 100.0)
            .width(200.0)
            .on_change(move |v| l.lock().unwrap().push(v)),
    );
    let mut h = TestHarness::new(Box::new(widget));
    h.layout(400.0, 100.0);
    settle(&mut h, 400.0, 100.0);
    let id = h.find_by_type_name("TickSlider")[0];
    let b = h.element_bounds(id);
    let y = b.y() + b.size.height / 2.0;
    drag(
        &mut h,
        Point::new(b.x() + b.size.width * 0.1, y),
        Point::new(b.x() + b.size.width * 0.8, y),
        6,
        400.0,
        100.0,
    );
    let v = log.lock().unwrap().clone();
    assert!(v.len() >= 4, "значение меняется по ходу пальца, а не только в конце: {v:?}");
    let last = *v.last().unwrap();
    assert!(last > 60.0, "ползунок доехал за пальцем: {v:?}");
}

#[test]
fn tick_slider_tap_sets_value_once() {
    use syngui::widgets::input::TickSlider;
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = log.clone();
    let widget = Stack::new().child(
        TickSlider::new()
            .horizontal()
            .range(0.0, 100.0)
            .width(200.0)
            .on_change(move |v| l.lock().unwrap().push(v)),
    );
    let mut h = TestHarness::new(Box::new(widget));
    h.layout(400.0, 100.0);
    settle(&mut h, 400.0, 100.0);
    let id = h.find_by_type_name("TickSlider")[0];
    let b = h.element_bounds(id);
    let p = Point::new(b.x() + b.size.width * 0.5, b.y() + b.size.height / 2.0);
    h.touch_down(1, p);
    settle(&mut h, 400.0, 100.0);
    assert_eq!(log.lock().unwrap().len(), 1, "значение ставится сразу при касании");
    h.touch_up(1);
    settle(&mut h, 400.0, 100.0);
    assert_eq!(
        log.lock().unwrap().len(),
        1,
        "отпускание не синтезирует второй щелчок"
    );
}

#[test]
fn split_view_divider_drags_by_finger() {
    use syngui::widgets::containers::SplitView;
    use syngui::widgets::DecoratedBox;
    let ratio = use_signal(0.5f32);
    let widget = SplitView::new(DecoratedBox::new(), DecoratedBox::new())
        .initial_ratio(0.5)
        .ratio_signal(ratio);
    let mut h = TestHarness::new(Box::new(widget));
    h.layout(400.0, 200.0);
    settle(&mut h, 400.0, 200.0);
    let id = h.find_by_type_name("SplitView")[0];
    let b = h.element_bounds(id);
    let x0 = b.x() + b.size.width * 0.5;
    drag(
        &mut h,
        Point::new(x0, 100.0),
        Point::new(x0 - 100.0, 100.0),
        5,
        400.0,
        200.0,
    );
    let r = ratio.get_untracked();
    assert!(r < 0.35, "разделитель уехал за пальцем влево: ratio {r}");
}

#[test]
fn list_view_scrolls_by_finger() {
    use syngui::widgets::data::{ListItem, ListView};
    let picked = Arc::new(Mutex::new(Vec::new()));
    let p = picked.clone();
    let items = (0..40).map(|i| ListItem::new(format!("строка {i}"))).collect();
    let widget = Stack::new().child(
        ListView::new(items)
            .item_height(40.0)
            .height(200.0)
            .width(300.0)
            .on_select(move |i| p.lock().unwrap().push(i)),
    );
    let mut h = TestHarness::new(Box::new(widget));
    h.layout(400.0, 400.0);
    settle(&mut h, 400.0, 400.0);
    let id = h.find_by_type_name("ListView")[0];
    let b = h.element_bounds(id);
    let at = Point::new(b.x() + 50.0, b.y() + 20.0);

    h.touch_down(1, at);
    h.touch_up(1);
    settle(&mut h, 400.0, 400.0);
    let first = *picked.lock().unwrap().last().expect("тап выбирает строку");

    // Пальцем вверх на три строки (из-под тапа, чтобы был запас по высоте).
    let from = Point::new(at.x, b.y() + 170.0);
    drag(&mut h, from, Point::new(from.x, from.y - 120.0), 8, 400.0, 400.0);
    // Ждём, пока не уйдёт двойной тап.
    std::thread::sleep(std::time::Duration::from_millis(350));
    h.touch_down(1, at);
    h.touch_up(1);
    settle(&mut h, 400.0, 400.0);
    let after = *picked.lock().unwrap().last().unwrap();
    assert_eq!(after, first + 3, "список пролистан пальцем на три строки");
}
