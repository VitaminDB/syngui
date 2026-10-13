//! `ScrollView` с `height: fit-content` — по содержимому, но не выше доступного.

use syngui::prelude::*;
use syngui::testing::TestHarness;

const MSS: &str = ".fit { height: fit-content; } .box { max-height: 400px; }";

fn scroll_height(content: f32) -> f32 {
    syngui::signal::allow_signal_reads_on_this_thread();
    let root = Column::new().child(
        DecoratedBox::new().class("box").child(
            ScrollView::new()
                .vertical()
                .class("fit")
                .child(DecoratedBox::new().style("height", content)),
        ),
    );
    let mut h = TestHarness::new(Box::new(root));
    let engine = h.apply_mss(MSS);
    for _ in 0..2 {
        h.frame(Some(&engine), 300.0, 600.0);
    }
    h.element_bounds(h.find_by_class("fit")[0]).size.height
}

#[test]
fn short_content_hugs() {
    assert_eq!(scroll_height(100.0), 100.0);
}

#[test]
fn tall_content_is_capped() {
    assert_eq!(scroll_height(1000.0), 400.0);
}
