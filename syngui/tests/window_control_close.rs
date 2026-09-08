//! Кнопка окна с интерактивным содержимым: клик обязан дойти до
//! `WindowControl`, даже если ребёнок пометил нажатие обработанным.
//!
//! Живой случай (08.09.2026): диалог подтверждения выхода в synthos —
//! `WindowControl::close()` с `Button` внутри. `Button` возвращает
//! `Handled` на MouseDown и без `on_click`, а дети получают события
//! раньше родителя, поэтому «Выйти» не закрывал окно, а «Отмена»
//! (обычная кнопка с `on_click`) работала.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use syngui::core::Point;
use syngui::input::{Event, MouseButton};
use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::overlay::{PortalAnchor, WindowControl};

/// Клик по кнопке внутри `WindowControl` закрывает окно.
#[test]
fn button_inside_window_control_closes_the_window() {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let widget = Row::new().main_axis_alignment(MainAxisAlignment::End).child(
        WindowControl::close()
            .on_activate(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
            .child(Button::new("Выйти")),
    );

    let mut h = TestHarness::new(Box::new(widget));
    h.layout(400.0, 200.0);
    let btn = h.find_by_type_name("Button");
    assert_eq!(btn.len(), 1, "кнопка не построилась");
    let r = h.element_bounds(btn[0]);
    let at = Point::new(r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0);

    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: at });
    assert_eq!(hits.load(Ordering::SeqCst), 1, "on_activate не вызван");
    assert!(h.tree.window_close_request, "закрытие окна не запрошено");
}

/// То же в модальном оверлее — так кнопка стоит в диалоге выхода.
#[test]
fn window_control_works_inside_a_modal_portal() {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let open = use_signal(true);
    let widget = Stack::new().child(
        Portal::new()
            .is_open(open)
            .modal(true)
            .backdrop(true)
            .anchor(PortalAnchor::Center)
            .child(
                Row::new()
                    .gap(10.0)
                    .child(Button::new("Отмена"))
                    .child(
                        WindowControl::close()
                            .on_activate(move || {
                                counter.fetch_add(1, Ordering::SeqCst);
                            })
                            .child(Button::new("Выйти")),
                    ),
            ),
    );

    let mut h = TestHarness::new(Box::new(widget));
    h.layout(800.0, 600.0);
    let buttons = h.find_by_type_name("Button");
    assert_eq!(buttons.len(), 2, "в диалоге ожидались две кнопки");
    let r = h.element_bounds(buttons[1]);
    let at = Point::new(r.origin.x + r.size.width / 2.0, r.origin.y + r.size.height / 2.0);

    h.send_event(&Event::MouseDown { button: MouseButton::Left, position: at });
    assert_eq!(hits.load(Ordering::SeqCst), 1, "on_activate не вызван в модалке");
    assert!(h.tree.window_close_request, "закрытие окна не запрошено из модалки");
}
