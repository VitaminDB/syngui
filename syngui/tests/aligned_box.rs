//! MSS `justify-content` / `align-items` у DecoratedBox: ребёнок с явным
//! размером выравнивается внутри бокса, а бокс — внутри выровненного родителя.

use syngui::prelude::*;
use syngui::testing::*;

const MSS: &str = "
.layer { width: 100%; height: 100%; justify-content: center; align-items: center; }
.badge { width: 132px; height: 132px; justify-content: center; align-items: center; }
.glyph { icon-size: 72px; }
.right { width: 300px; height: 100px; justify-content: end; align-items: end; padding: 10px; }
.inner { width: 40px; height: 20px; }
";

#[test]
fn nested_center_alignment() {
    let widget = Stack::new().fit(StackFit::Expand).child(
        DecoratedBox::new().class("layer").child(
            DecoratedBox::new()
                .class("badge")
                .child(Icon::new("x").class("glyph")),
        ),
    );
    let mut h = TestHarness::new(Box::new(widget));
    h.apply_mss(MSS);
    h.layout(1920.0, 1080.0);

    let layer = h.element_bounds(h.find_by_class("layer")[0]);
    let badge = h.element_bounds(h.find_by_class("badge")[0]);
    assert_eq!(
        (
            layer.origin.x,
            layer.origin.y,
            layer.size.width,
            layer.size.height
        ),
        (0.0, 0.0, 1920.0, 1080.0),
        "layer {layer:?}"
    );
    let glyph = h.element_bounds(h.find_by_class("glyph")[0]);
    assert_eq!(
        (
            badge.origin.x,
            badge.origin.y,
            badge.size.width,
            badge.size.height
        ),
        (894.0, 474.0, 132.0, 132.0),
        "badge {badge:?}"
    );
    assert_eq!(
        (
            glyph.origin.x,
            glyph.origin.y,
            glyph.size.width,
            glyph.size.height
        ),
        (924.0, 504.0, 72.0, 72.0),
        "glyph {glyph:?}"
    );
}

#[test]
fn end_alignment_respects_padding() {
    let widget = Column::new().child(
        DecoratedBox::new()
            .class("right")
            .child(DecoratedBox::new().class("inner")),
    );
    let mut h = TestHarness::new(Box::new(widget));
    h.apply_mss(MSS);
    h.layout(800.0, 600.0);
    let inner = h.element_bounds(h.find_by_class("inner")[0]);
    assert_eq!(
        (
            inner.origin.x,
            inner.origin.y,
            inner.size.width,
            inner.size.height
        ),
        (250.0, 70.0, 40.0, 20.0),
        "inner {inner:?}"
    );
}
