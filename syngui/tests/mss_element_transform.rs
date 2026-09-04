//! Регрессия: `transform` доезжает до элементов, которые хранят `MssFields`.
//!
//! Рендер берёт трансформ из `Element::mss()` (`widget/tree/render.rs`). Часть
//! виджетов держала `MssFields`, но геттер не переопределяла — и `transform`
//! на них молча ничего не делал, хотя правило разбиралось без ошибок.

use syngui::prelude::*;
use syngui::testing::*;

fn pushes_transform(h: &mut TestHarness) -> bool {
    h.paint()
        .commands()
        .iter()
        .any(|c| matches!(c, DrawCommand::PushTransform(_)))
}

#[test]
fn chip_receives_mss_transform() {
    // Chip внутри Column: корневой элемент рисуется в обход проверки
    // `mss()` (`ElementTree::build_display_list`), трансформ читается
    // только у детей.
    let widget: Box<dyn Widget> = Box::new(Column::new().child(Chip::new("tag")));
    let mut h = TestHarness::new(widget);
    h.apply_mss("Chip { transform: rotate(15deg); }");
    h.layout(200.0, 60.0);

    assert!(pushes_transform(&mut h), "transform на Chip должен уехать в display list");
}

#[test]
fn chip_without_transform_rule_pushes_nothing() {
    let widget: Box<dyn Widget> = Box::new(Column::new().child(Chip::new("tag")));
    let mut h = TestHarness::new(widget);
    h.apply_mss("Chip { background-color: #EEEEEE; }");
    h.layout(200.0, 60.0);

    assert!(!pushes_transform(&mut h), "без правила лишнего трансформа быть не должно");
}
