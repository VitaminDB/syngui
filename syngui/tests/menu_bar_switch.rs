//! Строка меню: пока меню открыто, наведение на соседний заголовок
//! переключает меню, щелчок по другому заголовку открывает его сразу
//! (без «холостого» щелчка, закрывающего первое), щелчок по открытому — закрывает.

use std::time::Duration;

use syngui::core::Point;
use syngui::RectExt;
use syngui::input::Event;
use syngui::testing::*;
use syngui::widgets::containers::Stack;
use syngui::widgets::overlay::MenuItem;
use syngui::widgets::MenuBar;

fn frame(h: &mut TestHarness) {
    h.frame(None, 800.0, 600.0);
    h.paint();
    h.animate(Duration::from_millis(16));
    h.frame(None, 800.0, 600.0);
    h.paint();
}

/// Видимые меню: заголовки открытых (по классу `open`).
fn open_titles(h: &TestHarness) -> usize {
    h.find_by_class("open").len()
}

#[test]
fn hover_and_click_switch_menus() {
    let bar = MenuBar::new()
        .menu("Файл", || vec![MenuItem::new("open", "Открыть")])
        .menu("Правка", || vec![MenuItem::new("undo", "Отменить")])
        .build();
    let mut h = TestHarness::new(Box::new(Stack::new().child(bar)));
    frame(&mut h);
    let items: Vec<_> = h.find_by_class("menubar-item");
    assert_eq!(items.len(), 2);
    let (r0, r1) = (h.element_bounds(items[0]), h.element_bounds(items[1]));
    let c0 = Point::new(r0.x() + r0.size.width / 2.0, r0.y() + r0.size.height / 2.0);
    let c1 = Point::new(r1.x() + r1.size.width / 2.0, r1.y() + r1.size.height / 2.0);

    h.send_events(&click_at(c0));
    frame(&mut h);
    let open0 = h.find_by_class("open");
    assert_eq!(open0.len(), 1, "щелчок открывает «Файл»");
    assert_eq!(h.element_bounds(open0[0]), r0);

    // наведение на «Правка» при открытом меню — переключение
    h.send_event(&Event::MouseMove(c1));
    frame(&mut h);
    let open1 = h.find_by_class("open");
    assert_eq!(open1.len(), 1, "меню осталось открытым");
    assert_eq!(h.element_bounds(open1[0]), r1, "наведение переключило на «Правка»");

    // щелчок по открытому заголовку — закрыть
    h.send_events(&click_at(c1));
    frame(&mut h);
    assert_eq!(open_titles(&h), 0, "щелчок по открытому заголовку закрывает меню");

    // открыть «Правка», затем нажатие на «Файл» без наведения (палец/перо) —
    // сразу «Файл», без холостого щелчка, закрывающего первое меню
    h.send_events(&click_at(c1));
    frame(&mut h);
    h.send_events(&click_at(c1)[1..]);
    frame(&mut h);
    assert_eq!(open_titles(&h), 0);
    h.send_events(&click_at(c1));
    frame(&mut h);
    h.send_events(&click_at(c0)[1..]);
    frame(&mut h);
    let open = h.find_by_class("open");
    assert_eq!(open.len(), 1, "один щелчок по другому заголовку открывает его");
    assert_eq!(h.element_bounds(open[0]), r0);

    // щелчок мимо строки и меню — закрывает
    h.send_events(&click_at(Point::new(600.0, 500.0)));
    frame(&mut h);
    assert_eq!(open_titles(&h), 0, "щелчок снаружи закрывает меню");
}
