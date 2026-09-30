//! MSS `width` у Grid: колонки делят явную ширину, а не всю доступную
//! (цифровая панель экрана блокировки расползалась по ширине экрана).

use syngui::prelude::*;
use syngui::testing::*;

#[test]
fn grid_honours_explicit_width() {
    let mut grid = Grid::new(3).gap(10.0);
    for _ in 0..6 {
        grid = grid.child(DecoratedBox::new().class("key"));
    }
    let widget = Stack::new().fit(StackFit::Expand).child(Column::new().cross_axis_alignment(CrossAxisAlignment::Center).child(grid.class("pad")));
    let mut h = TestHarness::new(Box::new(widget));
    let engine = h.apply_mss(".pad { width: 320px; } .key { width: 100px; height: 50px; }");
    for _ in 0..3 {
        h.frame(Some(&engine), 800.0, 600.0);
    }
    let pad = h.element_bounds(h.find_by_class("pad")[0]);
    let keys = h.find_by_class("key");
    let k2 = h.element_bounds(keys[2]);
    assert!((pad.size.width - 320.0).abs() < 1.0, "сетка {pad:?}");
    assert!((pad.origin.x - 240.0).abs() < 1.0, "сетка по центру {pad:?}");
    assert!((k2.origin.x - 460.0).abs() < 1.0, "третья кнопка {k2:?}");
}
