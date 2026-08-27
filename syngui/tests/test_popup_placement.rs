//! Всплывающие слои обязаны оставаться в пределах окна.
//!
//! Регрессия: меню выбора языка в нав-рейле (`PopupAnchor::Position`,
//! кнопка внизу окна) уезжало за нижнюю границу — переворот считался от
//! пустого `anchor_rect`, а прижатия к краю не было вовсе.

use syngui::core::{Point, Rect, Size};
use syngui::prelude::*;
use syngui::render::DisplayList;
use syngui::testing::*;
use syngui::widgets::containers::Stack;
use syngui::widgets::{MenuItem, PopupMenu};

const MENU_ITEM_HEIGHT: f32 = 32.0;
const MENU_PADDING: f32 = 4.0;

fn items(n: usize) -> Vec<MenuItem> {
    (0..n)
        .map(|i| MenuItem::new(format!("id{i}"), format!("Пункт {i}")))
        .collect()
}

fn menu_height(n: usize) -> f32 {
    n as f32 * MENU_ITEM_HEIGHT + MENU_PADDING * 2.0
}

/// Открывает меню в точке `at` и возвращает нарисованные прямоугольники.
fn drawn_rects(item_count: usize, at: Point, viewport: Size) -> Vec<Rect> {
    let open = use_signal(false);
    let pos = use_signal(Point::zero());

    let widget = Stack::new().clip(false).child(
        PopupMenu::new()
            .items(items(item_count))
            .is_open(open)
            .position(pos),
    );

    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout(viewport.width, viewport.height);

    pos.set(at);
    open.set(true);
    harness.layout(viewport.width, viewport.height);

    let mut list = DisplayList::new();
    list.set_surface_size(viewport);
    harness.tree.build_display_list(
        harness.root_id,
        &mut list,
        Rect::new(Point::zero(), viewport),
    );

    list.commands()
        .iter()
        .filter_map(|cmd| match cmd {
            syngui::render::DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

fn assert_inside(rects: &[Rect], viewport: Size, what: &str) {
    assert!(!rects.is_empty(), "{what}: меню ничего не нарисовало");
    for r in rects {
        assert!(
            r.origin.y >= -0.5 && r.origin.y + r.size.height <= viewport.height + 0.5,
            "{what}: прямоугольник {:?} вышел за окно высотой {}",
            r,
            viewport.height
        );
        assert!(
            r.origin.x >= -0.5 && r.origin.x + r.size.width <= viewport.width + 0.5,
            "{what}: прямоугольник {:?} вышел за окно шириной {}",
            r,
            viewport.width
        );
    }
}

#[test]
fn menu_opened_near_the_bottom_flips_up() {
    let viewport = Size::new(600.0, 400.0);
    let height = menu_height(6); // 200
    let at = Point::new(40.0, 340.0); // 340 + 200 > 400, но 340 - 200 >= 0

    let rects = drawn_rects(6, at, viewport);
    assert_inside(&rects, viewport, "переворот вверх");

    let top = rects.iter().map(|r| r.origin.y).fold(f32::MAX, f32::min);
    assert!(
        (top - (at.y - height)).abs() < 1.0,
        "меню должно раскрыться вверх до точки открытия, верх получился {top}"
    );
}

#[test]
fn menu_taller_than_both_sides_is_pinned_to_the_edge() {
    let viewport = Size::new(600.0, 300.0);
    let height = menu_height(8); // 264
    let at = Point::new(40.0, 200.0); // вниз не влезает, вверх (200-264) тоже

    let rects = drawn_rects(8, at, viewport);
    assert_inside(&rects, viewport, "прижатие к нижнему краю");

    let top = rects.iter().map(|r| r.origin.y).fold(f32::MAX, f32::min);
    assert!(
        (top - (viewport.height - height)).abs() < 1.0,
        "меню должно прижаться к низу окна, верх получился {top}"
    );
}

#[test]
fn menu_that_fits_keeps_the_requested_position() {
    let viewport = Size::new(600.0, 400.0);
    let at = Point::new(40.0, 60.0);

    let rects = drawn_rects(6, at, viewport);
    assert_inside(&rects, viewport, "обычное раскрытие вниз");

    let top = rects.iter().map(|r| r.origin.y).fold(f32::MAX, f32::min);
    assert!(
        (top - at.y).abs() < 1.0,
        "меню должно стоять там, где его открыли, верх получился {top}"
    );
}
