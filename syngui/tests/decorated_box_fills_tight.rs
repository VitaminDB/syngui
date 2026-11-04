//! DecoratedBox без явного размера, но с `justify-content`/`align-items`,
//! внутри плотных ограничений (AspectRatio, Stack(Expand), растянутая
//! колонка) занимает всё место и выравнивает ребёнка в нём. Раньше
//! выравнивание учитывалось только при явном размере, и значок оставался
//! в левом верхнем углу.
use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::containers::{Stack, StackFit};

const MSS: &str = r#"
    .ph { justify-content: center; align-items: center; }
    .ph-end { justify-content: end; align-items: end; padding: 0px 12px 10px 0px; }
    .mark { width: 40px; height: 40px; }
"#;

fn mark() -> impl Widget {
    DecoratedBox::new().class("mark")
}

fn harness(widget: impl Widget + 'static, w: f32, h: f32) -> TestHarness {
    let mut h_ = TestHarness::new(Box::new(widget));
    h_.apply_mss(MSS);
    h_.layout_loose(w, h);
    h_
}

fn mark_bounds(h: &TestHarness) -> Rect {
    let ids = h.find_by_class("mark");
    assert_eq!(ids.len(), 1);
    h.element_bounds(ids[0])
}

#[test]
fn centered_in_aspect_ratio_stack() {
    let w = AspectRatio::new(4.0 / 3.0).child(
        Stack::new()
            .fit(StackFit::Expand)
            .child(DecoratedBox::new().class("ph").child(mark())),
    );
    let h = harness(w, 400.0, 800.0);
    let ph = h.element_bounds(h.find_by_class("ph")[0]);
    assert_eq!((ph.size.width, ph.size.height), (400.0, 300.0), "бокс заполняет родителя");
    let m = mark_bounds(&h);
    assert_eq!((m.size.width, m.size.height), (40.0, 40.0), "ребёнок своего размера");
    assert_eq!((m.origin.x, m.origin.y), (180.0, 130.0), "ребёнок по центру");
}

#[test]
fn end_aligned_with_padding_in_aspect_ratio() {
    let w = AspectRatio::new(4.0 / 3.0).child(DecoratedBox::new().class("ph-end").child(mark()));
    let h = harness(w, 400.0, 800.0);
    let m = mark_bounds(&h);
    assert_eq!((m.origin.x, m.origin.y), (400.0 - 12.0 - 40.0, 300.0 - 10.0 - 40.0));
}

#[test]
fn loose_parent_shrinks_to_child() {
    // В свободных ограничениях бокс по-прежнему равен ребёнку.
    let w = Center::new().child(DecoratedBox::new().class("ph").child(mark()));
    let h = harness(w, 400.0, 300.0);
    let ph = h.element_bounds(h.find_by_class("ph")[0]);
    assert_eq!((ph.size.width, ph.size.height), (40.0, 40.0));
}
