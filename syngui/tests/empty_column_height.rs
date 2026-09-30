//! Пустая колонка (список без элементов, например уведомлений) в свободных
//! ограничениях не должна занимать всю доступную высоту — иначе она
//! выталкивает соседей (низ экрана блокировки уезжал за край).

use syngui::prelude::*;
use syngui::testing::*;

#[test]
fn empty_column_is_zero_height() {
    let widget = Stack::new().fit(StackFit::Expand).child(
        Column::new()
            .main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .child(Reactive::new(|| {
                vec![Box::new(
                    Column::new()
                        .child(DecoratedBox::new().class("box"))
                        .child(Reactive::new(|| vec![Box::new(Column::new().class("empty")) as Box<dyn Widget>])),
                ) as Box<dyn Widget>]
            }))
            .child(DecoratedBox::new().class("bottom")),
    );
    let mut h = TestHarness::new(Box::new(widget));
    let engine = h.apply_mss(".box { width: 40px; height: 40px; } .bottom { width: 40px; height: 20px; }");
    for _ in 0..3 {
        h.frame(Some(&engine), 400.0, 800.0);
    }
    let empty = h.element_bounds(h.find_by_class("empty")[0]);
    let bottom = h.element_bounds(h.find_by_class("bottom")[0]);
    assert!(empty.size.height < 1.0, "пустая колонка {empty:?}");
    assert!((bottom.origin.y - 780.0).abs() < 1.0, "низ {bottom:?}");
}
