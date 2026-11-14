//! Лента (ScrollView::follow_end) с реактивным содержимым: удалили последние
//! сообщения кнопкой внутри ленты — прокрутка не должна улетать в начало
//! (чат ИИ Hexspire: «удаляю — скролл сбрасывается к началу»).

use syngui::core::Point;
use syngui::input::{Event, MouseButton};
use syngui::prelude::*;
use syngui::testing::TestHarness;

fn offset(h: &TestHarness) -> f32 {
    let id = h.find_by_type_name("ScrollView")[0];
    h.tree.get(id).unwrap().scroll_offset().y
}

fn feed(n: RwSignal<usize>) -> ScrollView {
    ScrollView::new().vertical().follow_end(true).child(move || {
        let mut col = Column::new().gap(4.0).cross_axis_alignment(CrossAxisAlignment::Stretch);
        let count = n.get();
        for i in 0..count {
            col = col.child(syngui::widgets::visual::MarkdownView::new(format!("**сообщение {i}**\n\nтекст ответа модели, `код`\n\n- пункт")).selectable(true));
            // кнопка «удалить» у каждого сообщения: удаляет его и всё после
            col = col.child(Row::new().child(ToolButton::new("x").on_click(move || n.set(i))));
        }
        col
    })
}

/// Колесом чуть выше низа, затем удалить последнее сообщение его же кнопкой.
#[test]
fn delete_button_in_feed_keeps_scroll() {
    let n = use_signal(60usize);
    let mut h = TestHarness::new(Box::new(feed(n)));
    for _ in 0..2 {
        h.frame(None, 800.0, 600.0);
    }
    let bottom = offset(&h);
    assert!(bottom > 1000.0, "лента начинается у низа: {bottom}");
    h.send_event(&Event::MouseWheel { delta: 60.0, delta_x: 0.0, position: Point::new(100.0, 300.0) });
    h.frame(None, 800.0, 600.0);
    let up = offset(&h);
    assert!(up < bottom, "колесо сдвинуло вверх: {bottom} → {up}");

    // последняя видимая кнопка «удалить» (границы — в координатах содержимого: на экране минус смещение)
    let (idx, p) = h
        .find_by_type_name("ToolButton")
        .iter()
        .enumerate()
        .rev()
        .map(|(k, b)| {
            let r = h.tree.get(*b).unwrap().bounds();
            (k, Point::new(r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0 - up))
        })
        .find(|(_, p)| p.y > 10.0 && p.y < 590.0)
        .expect("видимая кнопка");
    h.send_events(&[
        Event::MouseMove(p),
        Event::MouseDown { button: MouseButton::Left, position: p },
        Event::MouseUp { button: MouseButton::Left, position: p },
    ]);
    for _ in 0..3 {
        h.frame(None, 800.0, 600.0);
    }
    assert_eq!(n.get_untracked(), idx, "щелчок удалил сообщение {idx} и всё после");
    let after = offset(&h);
    assert!(after > bottom - 600.0, "после удаления прокрутка улетела: была {up}, стала {after} (низ был {bottom})");
}

/// Щелчок по кнопке, пока лента катится по инерции колеса: нажатие гасит
/// инерцию, лента стоит до отпускания — щелчок засчитан, а не пропал
/// (раньше лента уезжала между нажатием и отпусканием, кнопка видела
/// отпускание мимо себя).
#[test]
fn click_during_wheel_coast_is_not_lost() {
    use std::time::Duration;
    let n = use_signal(60usize);
    let mut h = TestHarness::new(Box::new(feed(n)));
    for _ in 0..2 {
        h.frame(None, 800.0, 600.0);
    }
    // несколько щелчков колеса вверх — разгон инерции
    for _ in 0..4 {
        h.send_event(&Event::MouseWheel { delta: 120.0, delta_x: 0.0, position: Point::new(100.0, 300.0) });
    }
    h.animate(Duration::from_millis(16));
    h.frame(None, 800.0, 600.0);
    let up = offset(&h);
    let (idx, p) = h
        .find_by_type_name("ToolButton")
        .iter()
        .enumerate()
        .map(|(k, b)| {
            let r = h.tree.get(*b).unwrap().bounds();
            (k, Point::new(r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0 - up))
        })
        .find(|(_, p)| p.y > 250.0 && p.y < 350.0)
        .expect("кнопка посередине экрана");
    h.send_events(&[Event::MouseMove(p), Event::MouseDown { button: MouseButton::Left, position: p }]);
    let at_down = offset(&h);
    // пока кнопка нажата, идут кадры — инерция не должна сдвигать ленту
    for _ in 0..10 {
        h.animate(Duration::from_millis(16));
        h.frame(None, 800.0, 600.0);
    }
    assert!((offset(&h) - at_down).abs() < 1.0, "лента ехала под нажатой кнопкой: {at_down} → {}", offset(&h));
    h.send_event(&Event::MouseUp { button: MouseButton::Left, position: p });
    for _ in 0..3 {
        h.frame(None, 800.0, 600.0);
    }
    assert_eq!(n.get_untracked(), idx, "щелчок по кнопке {idx} засчитан");
}
