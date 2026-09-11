//! Подсказка `ToolButton` обязана гаснуть, когда курсор ушёл с кнопки.
//!
//! Регрессия (synthos, 08.09.2026): подсказки плиток рейла оставались
//! висеть поверх панели после быстрого увода курсора. Корень — в
//! `dispatch_mouse_move`: элементу, выпавшему из hit-path (перекрыл сосед
//! в `Stack`, ползунок `ScrollView`, overlay), «уход» слался реальной
//! позицией курсора, всё ещё лежащей в его bounds, — он оставался «под
//! курсором» навсегда, hover замерзал, таймер подсказки дотикивал.

use std::time::Duration;

use syngui::core::Point;
use syngui::input::{Event, MouseButton};
use syngui::prelude::*;
use syngui::render::DrawCommand;
use syngui::testing::*;
use syngui::widgets::containers::{Positioned, Stack};

const TIP: &str = "Подсказка";
const DELAY: Duration = Duration::from_millis(600);

/// Кнопка 40×40 в левом верхнем углу; бейдж 20×20 поверх её правой
/// половины — как счётчик терминалов на плитке рейла.
fn harness(passthrough: bool) -> (TestHarness, ElementId) {
    let mut btn = ToolButton::new("★").tooltip(TIP);
    if passthrough {
        btn = btn.press_passthrough();
    }
    let widget = Stack::new()
        .clip(false)
        .child(btn.class("btn"))
        .child(Positioned::new(DecoratedBox::new().class("badge")).at(20.0, 0.0));
    let mut h = TestHarness::new(Box::new(widget));
    h.apply_mss(".btn { width: 40px; height: 40px; } .badge { width: 20px; height: 20px; }");
    h.layout(300.0, 200.0);
    let btn_id = h.find_by_type_name("ToolButton")[0];
    let b = h.element_bounds(btn_id);
    assert_eq!(
        (b.size.width, b.size.height),
        (40.0, 40.0),
        "стили кнопки применились: {b:?}"
    );
    (h, btn_id)
}

fn tip_drawn(h: &mut TestHarness) -> bool {
    h.paint()
        .overlay_commands()
        .iter()
        .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_str() == TIP))
}

const ON_BUTTON: Point = Point::new(8.0, 30.0);
const ON_BADGE: Point = Point::new(30.0, 8.0);
const FAR: Point = Point::new(250.0, 150.0);

#[test]
fn tooltip_shows_after_delay_and_hides_on_leave() {
    let (mut h, btn) = harness(false);
    h.send_event(&Event::MouseMove(ON_BUTTON));
    assert!(!tip_drawn(&mut h), "до задержки подсказки нет");
    h.animate(DELAY);
    assert!(tip_drawn(&mut h), "после 500 мс подсказка видна");
    h.send_event(&Event::MouseMove(FAR));
    assert!(!tip_drawn(&mut h), "ушли — погасла");
    assert!(!h.is_animating(btn), "таймер не тикает без курсора");
}

#[test]
fn leaving_before_delay_never_shows_tooltip() {
    let (mut h, btn) = harness(false);
    h.send_event(&Event::MouseMove(ON_BUTTON));
    h.animate(Duration::from_millis(200));
    h.send_event(&Event::MouseMove(FAR));
    h.animate(DELAY);
    assert!(!tip_drawn(&mut h));
    assert!(!h.is_animating(btn));
}

/// Курсор ушёл с кнопки через перекрывающий её бейдж: кнопка выпадает из
/// hit-path, пока точка ещё внутри её bounds. Подсказка обязана погаснуть
/// и не появиться позже.
#[test]
fn tooltip_hides_when_cursor_leaves_through_overlapping_sibling() {
    let (mut h, btn) = harness(false);
    h.send_event(&Event::MouseMove(ON_BUTTON));
    h.animate(DELAY);
    assert!(tip_drawn(&mut h));
    h.send_event(&Event::MouseMove(ON_BADGE));
    h.send_event(&Event::MouseMove(FAR));
    assert!(
        !tip_drawn(&mut h),
        "подсказка висит после ухода через бейдж"
    );
    h.animate(DELAY);
    assert!(!tip_drawn(&mut h), "таймер дотикал без курсора");
    assert!(!h.is_animating(btn));
}

/// То же до истечения задержки: без синтетического «ухода» таймер дотикал
/// бы и показал подсказку над пустым местом.
#[test]
fn pending_tooltip_is_cancelled_when_button_drops_out_of_hit_path() {
    let (mut h, btn) = harness(false);
    h.send_event(&Event::MouseMove(ON_BUTTON));
    h.animate(Duration::from_millis(200));
    h.send_event(&Event::MouseMove(ON_BADGE));
    h.send_event(&Event::MouseMove(FAR));
    h.animate(DELAY);
    assert!(!tip_drawn(&mut h));
    assert!(!h.is_animating(btn));
}

/// Сквозное нажатие (плитки рейла): клик гасит подсказку до следующего
/// наведения, хотя само событие кнопка не забирает.
#[test]
fn passthrough_press_hides_tooltip() {
    let (mut h, _btn) = harness(true);
    h.send_event(&Event::MouseMove(ON_BUTTON));
    h.animate(DELAY);
    assert!(tip_drawn(&mut h));
    h.send_event(&Event::MouseDown {
        button: MouseButton::Left,
        position: ON_BUTTON,
    });
    assert!(!tip_drawn(&mut h));
    h.animate(DELAY);
    assert!(
        !tip_drawn(&mut h),
        "после клика подсказка не возвращается, пока курсор на кнопке"
    );
    h.send_event(&Event::MouseMove(FAR));
    h.send_event(&Event::MouseMove(ON_BUTTON));
    h.animate(DELAY);
    assert!(tip_drawn(&mut h), "новое наведение — снова показывается");
}
