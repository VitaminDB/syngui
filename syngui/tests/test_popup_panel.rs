use syngui::core::{Point, Rect, Size};
use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::containers::Stack;
use syngui::widgets::{PopupAnchor, PopupPanel};

#[test]
fn popup_content_sits_under_the_anchor() {
    let open = use_signal(false);
    let anchor = use_signal(Rect::new(Point::new(240.0, 120.0), Size::new(160.0, 44.0)));

    let widget = Stack::new().child(
        PopupPanel::new()
            .is_open(open)
            .anchor_rect(anchor)
            .anchor(PopupAnchor::BottomStart)
            .min_width(300.0)
            .max_width(420.0)
            .child(
                DecoratedBox::new()
                    .style("width", 400.0_f32)
                    .style("height", 220.0_f32),
            ),
    );

    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout(1000.0, 800.0);

    open.set(true);
    harness.layout(1000.0, 800.0);

    let boxes = harness.find_by_type_name("DecoratedBox");
    assert_eq!(boxes.len(), 1, "ожидалось содержимое попапа");

    let content = harness.element_bounds(boxes[0]);
    assert!(
        (content.origin.x - 240.0).abs() < 1.0 && (content.origin.y - 164.0).abs() < 1.0,
        "содержимое должно стоять под якорем, получено {:?}",
        content.origin
    );
}
