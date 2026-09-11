//! Контейнер, выросший сверх содержимого (растяжение родителем или
//! `min-height`), обязан отдать детям определённую высоту: flex-ребёнок
//! получает остаток, `Stack(Expand)` заполняет всё.

use syngui::prelude::*;
use syngui::testing::TestHarness;
use syngui::widgets::ScrollView;

const MSS: &str = ".grow { flex-grow: 1; } .floor { min-height: 160; } .card { padding: 10; }";

fn height_of(h: &TestHarness, class: &str) -> f32 {
    let id = h.find_by_class(class)[0];
    h.element_bounds(id).size.height
}

#[test]
fn stretched_card_gives_extra_height_to_its_flex_child() {
    let card = DecoratedBox::new().class("card").child(
        Column::new()
            .child(DecoratedBox::new().style("height", 20.0_f32))
            .child(DecoratedBox::new().class("grow")),
    );
    let tall = DecoratedBox::new()
        .style("width", 40.0_f32)
        .style("height", 300.0_f32);
    let row = Row::new()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .child(card)
        .child(tall);

    let mut h = TestHarness::new(Box::new(ScrollView::new().vertical().child(row)));
    let engine = h.apply_mss(MSS);
    h.apply_styles(&engine);
    h.layout(400.0, 600.0);

    let grown = height_of(&h, "grow");
    assert!(
        (grown - 260.0).abs() < 1.0,
        "flex-ребёнок растянутой карточки должен занять остаток 260, а не {grown}"
    );
}

#[test]
fn min_height_box_is_definite_for_an_expanding_stack() {
    let floor = DecoratedBox::new().class("floor").child(
        Stack::new()
            .fit(StackFit::Expand)
            .child(DecoratedBox::new().class("inner")),
    );

    let mut h = TestHarness::new(Box::new(
        ScrollView::new()
            .vertical()
            .child(Column::new().child(floor)),
    ));
    let engine = h.apply_mss(MSS);
    h.apply_styles(&engine);
    h.layout(400.0, 600.0);

    let inner = height_of(&h, "inner");
    assert!(
        (inner - 160.0).abs() < 1.0,
        "Stack(Expand) внутри бокса с min-height должен занять 160, а не {inner}"
    );
}

#[test]
fn content_taller_than_min_height_is_not_squeezed() {
    let floor = DecoratedBox::new().class("floor").child(
        DecoratedBox::new()
            .class("inner")
            .style("height", 240.0_f32),
    );

    let mut h = TestHarness::new(Box::new(
        ScrollView::new()
            .vertical()
            .child(Column::new().child(floor)),
    ));
    let engine = h.apply_mss(MSS);
    h.apply_styles(&engine);
    h.layout(400.0, 600.0);

    assert!((height_of(&h, "floor") - 240.0).abs() < 1.0);
    assert!((height_of(&h, "inner") - 240.0).abs() < 1.0);
}
