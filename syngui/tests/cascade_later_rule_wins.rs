//! Каскад для потомков: правило выбирается по классам предка, и смена
//! классов предка обязана пересчитать стиль поддерева.

use syngui::prelude::*;
use syngui::testing::*;

const MSS: &str = "
.icon { icon-color: #101010; }
.pill-active .icon { icon-color: #ff0000; }
.pill-focused .icon { icon-color: #ffffff; }
";

#[test]
fn later_descendant_rule_wins_over_earlier() {
    let widget = DecoratedBox::new()
        .class("pill pill-active pill-focused")
        .child(Icon::new("x").class("icon"));
    let mut h = TestHarness::new(Box::new(widget));
    h.apply_mss(MSS);
    h.layout(400.0, 200.0);

    let icon = h.find_by_class("icon")[0];
    let color = h
        .element_mss(icon)
        .and_then(|m| m.icon_color)
        .expect("icon-color должен быть задан");
    assert_eq!(
        (color.r, color.g, color.b),
        (1.0, 1.0, 1.0),
        "должно выиграть позднее правило .pill-focused .icon (белый)"
    );
}

#[test]
fn ancestor_class_change_restyles_descendant() {
    let widget = DecoratedBox::new()
        .class("pill pill-active")
        .child(Row::new().child(Icon::new("x").class("icon")));
    let mut h = TestHarness::new(Box::new(widget));
    let engine = h.apply_mss(MSS);
    h.layout(400.0, 200.0);

    let icon = h.find_by_class("icon")[0];
    let color = |h: &TestHarness| {
        let c = h.element_mss(icon).and_then(|m| m.icon_color).unwrap();
        (c.r, c.g, c.b)
    };
    assert_eq!(color(&h), (1.0, 0.0, 0.0), "пока предок только .pill-active");

    let pill = h.find_by_class("pill")[0];
    h.set_classes(
        pill,
        vec![
            "pill".to_string(),
            "pill-active".to_string(),
            "pill-focused".to_string(),
        ],
    );
    h.apply_styles_dirty(&engine);
    h.layout(400.0, 200.0);
    assert_eq!(
        color(&h),
        (1.0, 1.0, 1.0),
        "после появления .pill-focused у предка иконка должна стать белой"
    );
}

#[test]
fn later_rule_wins_through_intermediate_container() {
    let widget = DecoratedBox::new()
        .class("pill pill-active pill-focused")
        .child(Row::new().child(Icon::new("x").class("icon")));
    let mut h = TestHarness::new(Box::new(widget));
    h.apply_mss(MSS);
    h.layout(400.0, 200.0);

    let icon = h.find_by_class("icon")[0];
    let c = h.element_mss(icon).and_then(|m| m.icon_color).unwrap();
    assert_eq!(
        (c.r, c.g, c.b),
        (1.0, 1.0, 1.0),
        "правило-потомок должно доставать иконку и через контейнер"
    );
}
