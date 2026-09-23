//! Окно с высотой «по содержимому» не бывает выше экрана: середина с
//! `flex-shrink` сжимается и прокручивается, кнопки внизу остаются видны.
//!
//! Регрессия: диалог группового изменения на экране 1366×768 уводил
//! кнопку «Применить» за нижний край.

use syngui::core::{Point, Size};
use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::containers::Stack;
use syngui::widgets::{DecoratedBox, FloatingWindow, ScrollView};

const MSS: &str = ".shrink { flex-shrink: 1; }";

fn rows(n: usize) -> Column {
    let mut col = Column::new();
    for _ in 0..n {
        col = col.child(DecoratedBox::new().style("height", 40.0_f32));
    }
    col
}

fn dialog(body_rows: usize, window: Size) -> TestHarness {
    let open = use_signal(true);
    let pos = use_signal(Point::new(0.0, 0.0));
    let content = Column::new()
        .child(DecoratedBox::new().class("head").style("height", 30.0_f32))
        .child(
            DecoratedBox::new()
                .class("shrink body")
                .clip(true)
                .child(ScrollView::new().vertical().child(rows(body_rows))),
        )
        .child(
            DecoratedBox::new()
                .class("buttons")
                .style("height", 40.0_f32),
        );
    let widget = Stack::new().clip(false).child(
        FloatingWindow::new("Диалог")
            .is_open(open)
            .position(pos)
            .size(window)
            .child(content),
    );
    let mut h = TestHarness::new(Box::new(widget));
    let engine = h.apply_mss(MSS);
    h.apply_styles(&engine);
    h.layout(800.0, 500.0);
    h
}

fn bounds(h: &TestHarness, class: &str) -> syngui::core::Rect {
    h.element_bounds(h.find_by_class(class)[0])
}

#[test]
fn tall_auto_dialog_keeps_buttons_on_screen() {
    let h = dialog(30, Size::new(400.0, 0.0));
    let buttons = bounds(&h, "buttons");
    assert!(
        buttons.y() + buttons.size.height <= 500.0 + 0.5,
        "кнопки ушли за край: {buttons:?}"
    );
    let body = bounds(&h, "body");
    assert!(
        body.size.height < 30.0 * 40.0,
        "середина не сжалась: {body:?}"
    );
}

#[test]
fn short_auto_dialog_keeps_natural_height() {
    let h = dialog(3, Size::new(400.0, 0.0));
    let body = bounds(&h, "body");
    assert!(
        (body.size.height - 120.0).abs() < 1.0,
        "короткий диалог не должен растягиваться: {body:?}"
    );
}

#[test]
fn fixed_height_window_taller_than_viewport_fits_content() {
    let h = dialog(30, Size::new(400.0, 900.0));
    let buttons = bounds(&h, "buttons");
    assert!(
        buttons.y() + buttons.size.height <= 500.0 + 0.5,
        "кнопки окна с явной высотой ушли за край: {buttons:?}"
    );
}
