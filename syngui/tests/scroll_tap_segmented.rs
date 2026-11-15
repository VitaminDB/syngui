//! Касание сегмента в прокрученной ленте выбирает сегмент (телефон: «Плавность» в настройках
//! плеера, ниже первого экрана, не переключалась).

use std::sync::{Arc, Mutex};

use syngui::core::Point;
use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::*;

fn settle(h: &mut TestHarness) {
    h.rebuild();
    h.layout(300.0, 300.0);
}

#[test]
#[ignore = "баг не исправлен: касание сегмента после прокрутки касанием не выбирает его (телефон, «Плавность» в настройках плеера)"]
fn tap_segment_after_touch_scroll() {
    let picked = Arc::new(Mutex::new(Vec::new()));
    let p = picked.clone();
    let mut col = Column::new().gap(10.0);
    for i in 0..12 {
        col = col.child(Text::new(format!("строка {i}")).class("row"));
    }
    col = col.child(SegmentedButton::new(vec!["a".to_string(), "b".to_string(), "c".to_string()]).on_change(move |i| p.lock().unwrap().push(i)));
    for i in 0..12 {
        col = col.child(Text::new(format!("ещё {i}")));
    }
    let mut h = TestHarness::new(Box::new(ScrollView::new().vertical().child(col)));
    h.apply_mss(".row { height: 40px; }");
    settle(&mut h);
    // прокрутка пальцем вверх на 250 px
    h.touch_down(1, Point::new(150.0, 280.0));
    settle(&mut h);
    for k in 1..=10 {
        h.touch_move(1, Point::new(150.0, 280.0 - 25.0 * k as f32));
        settle(&mut h);
    }
    h.touch_up(1);
    settle(&mut h);
    for _ in 0..120 {
        h.animate(std::time::Duration::from_millis(16));
        settle(&mut h);
    }
    let id = h.find_by_type_name("SegmentedButton")[0];
    let b = h.element_bounds(id);
    eprintln!("сегменты: {b:?}");
    let at = Point::new(b.x() + b.size.width * 5.0 / 6.0, b.y() + b.size.height / 2.0);
    h.touch_down(2, at);
    settle(&mut h);
    h.touch_up(2);
    settle(&mut h);
    assert_eq!(*picked.lock().unwrap(), vec![2], "касание третьего сегмента");
}
