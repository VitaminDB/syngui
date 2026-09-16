//! Слои внутри обрезанного бокса начинаются там же, где сам бокс.
//!
//! Фон экрана в приложениях собран так: распорка слева, рядом бокс с
//! `overflow: hidden`, внутри — Stack из картинки и затемняющих слоёв. Если
//! слой съедет от края бокса хоть на полпикселя, из-под него по краю видно
//! то, что он должен закрывать.

use syngui::prelude::*;
use syngui::testing::*;

const MSS: &str = "
.gap { width: 480px; height: 100%; }
.box { width: 1440px; height: 918px; overflow: hidden; }
.layer { width: 100%; height: 100%; }
";

#[test]
fn stack_layers_start_at_box_origin() {
    let widget = Row::new()
        .child(DecoratedBox::new().class("gap"))
        .child(
            DecoratedBox::new().class("box").child(
                Stack::new()
                    .fit(StackFit::Expand)
                    .child(DecoratedBox::new().class("layer"))
                    .child(DecoratedBox::new().class("layer")),
            ),
        );
    let mut h = TestHarness::new(Box::new(widget));
    h.apply_mss(MSS);
    h.layout(1920.0, 1080.0);

    let boxed = h.element_bounds(h.find_by_class("box")[0]);
    assert_eq!((boxed.origin.x, boxed.size.width), (480.0, 1440.0));

    for id in h.find_by_class("layer") {
        let layer = h.element_bounds(id);
        assert_eq!(
            (layer.origin.x, layer.size.width),
            (boxed.origin.x, boxed.size.width),
            "слой должен совпадать с боксом по горизонтали"
        );
    }
}
