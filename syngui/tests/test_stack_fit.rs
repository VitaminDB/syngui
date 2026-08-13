use syngui::prelude::*;
use syngui::testing::*;
use syngui::widgets::containers::{Stack, StackFit};

fn overlay() -> Column {
    Column::new()
        .main_axis_alignment(MainAxisAlignment::End)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .child(
            DecoratedBox::new()
                .style("width", 120.0_f32)
                .style("height", 60.0_f32),
        )
}

#[test]
fn stack_expand_stretches_children() {
    let widget = Stack::new().fit(StackFit::Expand).child(overlay());

    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout_loose(800.0, 600.0);

    let columns = harness.find_by_type_name("Column");
    assert_eq!(columns.len(), 1, "ожидалась одна колонка-оверлей");

    let bounds = harness.element_bounds(columns[0]);
    assert!(
        (bounds.size.height - 600.0).abs() < 1.0,
        "при StackFit::Expand оверлей должен занимать всю высоту, получено {}",
        bounds.size.height
    );
    assert!(
        (bounds.size.width - 800.0).abs() < 1.0,
        "при StackFit::Expand оверлей должен занимать всю ширину, получено {}",
        bounds.size.width
    );
}

#[test]
fn stack_loose_keeps_children_intrinsic() {
    let widget = Stack::new().child(overlay());

    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout_loose(800.0, 600.0);

    let columns = harness.find_by_type_name("Column");
    assert_eq!(columns.len(), 1, "ожидалась одна колонка-оверлей");

    let bounds = harness.element_bounds(columns[0]);
    assert!(
        bounds.size.height < 200.0,
        "при StackFit::Loose высота оверлея должна быть по контенту, получено {}",
        bounds.size.height
    );
}
