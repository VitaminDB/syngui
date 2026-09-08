//! Плавающее окно нельзя потерять за краем главного.
//!
//! Регрессия: на ноутбуке с низким разрешением окно опроса (990×920)
//! оказывалось больше вьюпорта — заголовок уезжал вверх за экран, и окно
//! нельзя было ни сдвинуть, ни закрыть.

use syngui::core::{Point, Rect, Size};
use syngui::input::{Event, MouseButton};
use syngui::prelude::*;
use syngui::render::DrawCommand;
use syngui::testing::*;
use syngui::widgets::containers::Stack;
use syngui::widgets::{DecoratedBox, FloatingWindow};

/// Высота полосы заголовка в `FloatingWindow`.
const TITLE_BAR_HEIGHT: f32 = 36.0;

fn viewport() -> Size {
    Size::new(1000.0, 700.0)
}

/// Прямоугольник окна: единственный Rect с рамкой (`push_rect_bordered`).
fn window_rect(harness: &mut TestHarness) -> Rect {
    let list = harness.paint();
    list.commands()
        .iter()
        .find_map(|cmd| match cmd {
            DrawCommand::Rect { rect, border: Some(_), .. } => Some(*rect),
            _ => None,
        })
        .expect("окно не нарисовано")
}

fn open_window(size: Size, pos: RwSignal<Point>) -> TestHarness {
    let open = use_signal(true);
    let widget = Stack::new().clip(false).child(
        FloatingWindow::new("Опрос прибора")
            .is_open(open)
            .position(pos)
            .size(size)
            .child(DecoratedBox::new()),
    );
    let mut harness = TestHarness::new(Box::new(widget));
    let vp = viewport();
    harness.layout(vp.width, vp.height);
    harness
}

#[test]
fn window_larger_than_viewport_is_clamped() {
    let pos = use_signal(Point::new(40.0, 40.0));
    let mut harness = open_window(Size::new(990.0, 920.0), pos);
    let vp = viewport();
    let win = window_rect(&mut harness);

    assert!(
        win.size.width <= vp.width + 0.5 && win.size.height <= vp.height + 0.5,
        "окно {:?} больше вьюпорта {:?}",
        win.size,
        vp
    );
    let p = pos.get_untracked();
    assert!(
        p.y >= -0.5 && p.y <= vp.height - TITLE_BAR_HEIGHT,
        "заголовок вне экрана: y={}",
        p.y
    );
}

#[test]
fn position_outside_viewport_is_pulled_back() {
    let pos = use_signal(Point::new(-800.0, -400.0));
    let mut harness = open_window(Size::new(600.0, 400.0), pos);
    let vp = viewport();
    harness.layout(vp.width, vp.height);

    let p = pos.get_untracked();
    assert!(p.y >= -0.5, "окно ушло выше экрана: y={}", p.y);
    assert!(
        p.x + 600.0 >= 100.0,
        "от окна не осталось видимой полосы: x={}",
        p.x
    );
}

/// Заголовок за краем (позицию сменили после раскладки — так бывает между
/// кадрами): окно тащится за тело и возвращается на экран.
#[test]
fn body_drag_when_title_bar_is_off_screen() {
    let pos = use_signal(Point::new(100.0, 100.0));
    let mut harness = open_window(Size::new(600.0, 400.0), pos);
    harness.paint();

    pos.set(Point::new(100.0, -300.0));
    let grab = Point::new(300.0, 20.0); // тело окна, заголовок выше экрана
    harness.send_event(&Event::MouseDown {
        button: MouseButton::Left,
        position: grab,
    });
    harness.send_event(&Event::MouseMove(Point::new(320.0, 200.0)));
    harness.send_event(&Event::MouseUp {
        button: MouseButton::Left,
        position: Point::new(320.0, 200.0),
    });

    let p = pos.get_untracked();
    assert!(p.y >= -0.5, "окно не вернулось на экран: y={}", p.y);
}
