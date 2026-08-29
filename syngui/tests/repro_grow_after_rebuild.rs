//! `.grow` (flex-grow) обязан пережить перестройку реактивного поддерева.

use syngui::prelude::*;
use syngui::testing::TestHarness;

const MSS: &str = ".grow { flex-grow: 1; }";

fn build(reactive: bool) -> TestHarness {
    let content: Box<dyn Widget> = if reactive {
        Box::new(DecoratedBox::new().child(move || Text::new("заголовок")))
    } else {
        Box::new(DecoratedBox::new().child(Text::new("заголовок")))
    };
    let row = Row::new()
        .gap(6.0)
        .child(
            DecoratedBox::new()
                .class("grow")
                .child(Stack::new().fit(StackFit::Expand).children(vec![content])),
        )
        .child(DecoratedBox::new().style("width", 32.0_f32).style("height", 32.0_f32));
    let mut h = TestHarness::new(Box::new(row));
    let engine = h.apply_mss(MSS);
    h.apply_styles(&engine);
    h.layout(400.0, 52.0);
    h.rebuild();
    h.apply_styles(&engine);
    h.layout(400.0, 52.0);
    h
}

fn grow_width(h: &TestHarness) -> f32 {
    let id = h.find_by_class("grow")[0];
    h.element_bounds(id).size.width
}

#[test]
fn static_content_grows() {
    let h = build(false);
    assert!((grow_width(&h) - 362.0).abs() < 1.0, "got {}", grow_width(&h));
}

#[test]
fn reactive_content_grows_too() {
    let h = build(true);
    assert!((grow_width(&h) - 362.0).abs() < 1.0, "got {}", grow_width(&h));
}

/// Растягивать flex-ребёнка нужно только по главной оси. По поперечной
/// `Stack(StackFit::Expand)` передаёт `min_height` как «можешь занять всё»;
/// если Loose исполнит это буквально, содержимое перестанет центрироваться
/// и заголовок панели уедет к верхнему краю шапки.
#[test]
fn reactive_content_stays_centered_vertically() {
    let h = build(true);
    let id = h.find_by_class("grow")[0];
    let b = h.element_bounds(id);
    assert!(
        b.size.height < 52.0,
        "по высоте содержимое должно остаться натуральным, а не занять всю шапку: {}",
        b.size.height
    );
}
