//! Integration tests for layout constraint propagation.

use syngui::testing::*;
use syngui::prelude::*;

#[test]
#[ignore]
fn column_with_gap_sizes_correctly() {
    let widget = Column::new()
        .gap(10.0)
        .child(DecoratedBox::new().style("width", 100.0_f32).style("height", 50.0_f32))
        .child(DecoratedBox::new().style("width", 100.0_f32).style("height", 50.0_f32));

    let mut harness = TestHarness::new(Box::new(widget));
    let size = harness.layout_loose(800.0, 600.0);

    // Two children of height 50 + gap 10 = 110
    assert!((size.height - 110.0).abs() < 1.0,
        "Expected height ~110, got {}", size.height);
}

#[test]
#[ignore]
fn row_with_gap_sizes_correctly() {
    let widget = Row::new()
        .gap(10.0)
        .child(DecoratedBox::new().style("width", 100.0_f32).style("height", 50.0_f32))
        .child(DecoratedBox::new().style("width", 100.0_f32).style("height", 50.0_f32));

    let mut harness = TestHarness::new(Box::new(widget));
    let size = harness.layout_loose(800.0, 600.0);

    // Two children of width 100 + gap 10 = 210
    assert!((size.width - 210.0).abs() < 1.0,
        "Expected width ~210, got {}", size.width);
}

#[test]
#[ignore]
fn padding_adds_to_child_size() {
    let widget = Padding::all(20.0)
        .child(DecoratedBox::new().style("width", 100.0_f32).style("height", 50.0_f32));

    let mut harness = TestHarness::new(Box::new(widget));
    let size = harness.layout_loose(800.0, 600.0);

    assert!((size.width - 140.0).abs() < 1.0,
        "Expected width ~140, got {}", size.width);
    assert!((size.height - 90.0).abs() < 1.0,
        "Expected height ~90, got {}", size.height);
}

#[test]
#[ignore]
fn decorated_box_with_explicit_size() {
    let widget = DecoratedBox::new().style("width", 200.0_f32).style("height", 100.0_f32);

    let mut harness = TestHarness::new(Box::new(widget));
    let size = harness.layout_loose(800.0, 600.0);

    assert!((size.width - 200.0).abs() < 1.0);
    assert!((size.height - 100.0).abs() < 1.0);
}

#[test]
fn find_elements_by_type() {
    let widget = Column::new()
        .child(Text::new("Hello"))
        .child(Text::new("World"))
        .child(DecoratedBox::new().style("width", 50.0_f32).style("height", 50.0_f32));

    let mut harness = TestHarness::new(Box::new(widget));
    harness.layout(800.0, 600.0);

    let texts = harness.find_by_type_name("Text");
    assert_eq!(texts.len(), 2, "Expected 2 Text elements, found {}", texts.len());

    let containers = harness.find_by_type_name("DecoratedBox");
    assert_eq!(containers.len(), 1, "Expected 1 DecoratedBox, found {}", containers.len());
}

#[test]
fn element_count_matches_tree() {
    let widget = Column::new()
        .child(Text::new("A"))
        .child(Row::new()
            .child(Text::new("B"))
            .child(Text::new("C"))
        );

    let harness = TestHarness::new(Box::new(widget));
    let count = harness.element_count();

    // Column + Text(A) + Row + Text(B) + Text(C) = 5
    assert_eq!(count, 5, "Expected 5 elements, found {}", count);
}
