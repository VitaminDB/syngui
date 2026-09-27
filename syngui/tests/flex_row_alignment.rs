//! `Flex` в направлении Row учитывает `cross_axis_alignment` и
//! `main_axis_alignment` (раньше подставлялись Start/Start): панель
//! syndesktop с апплетами разной высоты стояла прижатой к верху.

use syngui::prelude::*;
use syngui::testing::*;

const MSS: &str = "
.bar { width: 400px; height: 60px; }
.tall { width: 40px; height: 40px; }
.short { width: 40px; height: 20px; }
";

fn bar(cross: CrossAxisAlignment, main: MainAxisAlignment) -> TestHarness {
    let widget = DecoratedBox::new().class("bar").child(
        Flex::new()
            .direction(FlexDirection::Row)
            .cross_axis_alignment(cross)
            .main_axis_alignment(main)
            .class("flex")
            .child(DecoratedBox::new().class("tall"))
            .child(DecoratedBox::new().class("short")),
    );
    let mut h = TestHarness::new(Box::new(widget));
    h.apply_mss(MSS);
    h.layout(800.0, 600.0);
    h
}

#[test]
fn row_cross_center() {
    let h = bar(CrossAxisAlignment::Center, MainAxisAlignment::Start);
    let flex = h.element_bounds(h.find_by_class("flex")[0]);
    let tall = h.element_bounds(h.find_by_class("tall")[0]);
    let short = h.element_bounds(h.find_by_class("short")[0]);
    let mid = |r: Rect| r.origin.y + r.size.height / 2.0;
    assert!((mid(tall) - mid(short)).abs() < 0.5, "tall {tall:?} short {short:?}");
    assert!((mid(short) - mid(flex)).abs() < 0.5, "flex {flex:?} short {short:?}");
}

#[test]
fn row_cross_start_is_default_behaviour() {
    let h = bar(CrossAxisAlignment::Start, MainAxisAlignment::Start);
    let tall = h.element_bounds(h.find_by_class("tall")[0]);
    let short = h.element_bounds(h.find_by_class("short")[0]);
    assert_eq!(tall.origin.y, short.origin.y);
}

#[test]
fn row_main_end() {
    let h = bar(CrossAxisAlignment::Start, MainAxisAlignment::End);
    let flex = h.element_bounds(h.find_by_class("flex")[0]);
    let short = h.element_bounds(h.find_by_class("short")[0]);
    assert!(
        (short.origin.x + short.size.width - (flex.origin.x + flex.size.width)).abs() < 0.5,
        "flex {flex:?} short {short:?}"
    );
}
