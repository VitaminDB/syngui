//! Grid::min_column_width — число колонок по ширине.

use syngui::testing::*;
use syngui::widgets::{DecoratedBox, Grid, Text};

fn grid() -> Grid {
    let mut g = Grid::new(1).min_column_width(200.0).gap(10.0);
    for i in 0..6 {
        g = g.child(DecoratedBox::new().child(Text::new(format!("ячейка {i}"))));
    }
    g
}

fn columns_at(width: f32) -> usize {
    let mut h = TestHarness::new(Box::new(grid()));
    h.layout(width, 600.0);
    let ids = h.find_by_type_name("DecoratedBox");
    let mut xs: Vec<i32> = ids.iter().map(|&id| h.element_bounds(id).origin.x as i32).collect();
    xs.sort();
    xs.dedup();
    xs.len()
}

#[test]
fn columns_follow_width() {
    // (650 + 10) / 210 = 3.14 → 3
    assert_eq!(columns_at(650.0), 3);
    assert_eq!(columns_at(1300.0), 6);
    assert_eq!(columns_at(420.0), 2);
    // уже одной колонки — всё равно одна
    assert_eq!(columns_at(150.0), 1);
}
