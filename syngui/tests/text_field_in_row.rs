//! Поле без явной ширины в строке: строка меряет детей без предела, и поле
//! брало бесконечную ширину — строка схлопывалась до заглушки, подсказка
//! вылезала за рамку. Теперь у поля разумная ширина по умолчанию, а
//! `flex: 1` (как `flex-grow: 1`) растягивает его на остаток строки.

use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::Page;

fn tree(flex: bool) -> Box<dyn Widget> {
    let field = TextField::with_text("").placeholder("/path/to/video.mp4");
    let field: Box<dyn Widget> = if flex { Box::new(field.style("flex", 1.0_f32)) } else { Box::new(field) };
    let row = Row::new().gap(8.0).child(field).child(Button::new("Открыть"));
    Box::new(Page::new().vertical().child(DecoratedBox::new().child(Column::new().child(row)).class("card")))
}

#[test]
fn field_without_width_stays_finite() {
    let mut h = TestHarness::new(tree(false));
    h.apply_mss(".card { padding: 20px; }");
    h.rebuild();
    h.layout(1000.0, 600.0);
    let tf = h.find_by_type_name("TextField")[0];
    let w = h.element_bounds(tf).size.width;
    assert!(w.is_finite() && w > 100.0 && w < 400.0, "ширина поля: {w}");
}

#[test]
fn flex_field_fills_row() {
    let mut h = TestHarness::new(tree(true));
    h.apply_mss(".card { padding: 20px; }");
    h.rebuild();
    h.layout(1000.0, 600.0);
    let tf = h.find_by_type_name("TextField")[0];
    let row = h.find_by_type_name("Row")[0];
    let (fw, rw) = (h.element_bounds(tf).size.width, h.element_bounds(row).size.width);
    assert!(rw > 900.0, "строка на всю ширину: {rw}");
    assert!(fw > 700.0 && fw < rw, "поле занимает остаток строки: {fw} из {rw}");
}
