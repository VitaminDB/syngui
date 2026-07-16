//! Layout benchmarks — measure_recursive + position_recursive performance

use std::hint::black_box;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use syngui::core::{Color, Size};
use syngui::layout::Constraints;
use syngui::widget::{Text, Widget, ElementTree, DirtyFlags, ElementId, WidgetExt};
use syngui::widgets::{Column, DecoratedBox, Row, Padding};

// ── Helpers ─────────────────────────────────────────────────────────────

/// Build a flat Column with `n` Text children
fn build_flat_column(n: usize) -> (ElementTree, ElementId) {
    let mut col = Column::new().gap(4.0);
    for i in 0..n {
        col = col.child(Text::new(format!("Item {i}")));
    }
    let mut tree = ElementTree::new();
    let root_elem = col.create_element();
    let root_id = tree.insert(root_elem, None);
    col.mount(&mut tree, root_id);
    (tree, root_id)
}

/// Build a flat Row with `n` DecoratedBox children
fn build_flat_row(n: usize) -> (ElementTree, ElementId) {
    let mut row = Row::new().gap(8.0);
    for _ in 0..n {
        row = row.child(DecoratedBox::new().style("width", 60.0_f32).style("height", 40.0_f32).style("background-color", Color::from_hex("#3B82F6")));
    }
    let mut tree = ElementTree::new();
    let root_elem = row.create_element();
    let root_id = tree.insert(root_elem, None);
    row.mount(&mut tree, root_id);
    (tree, root_id)
}

/// Build a deep nested tree: Column > Column > ... > Text (depth levels)
fn build_deep_tree(depth: usize) -> (ElementTree, ElementId) {
    fn make_widget(depth: usize) -> Box<dyn Widget> {
        if depth == 0 {
            Box::new(Text::new("Leaf"))
        } else {
            Box::new(
                Column::new()
                    .gap(2.0)
                    .child(Text::new(format!("Level {depth}")))
                    .children(vec![make_widget(depth - 1)])
            )
        }
    }

    let widget = make_widget(depth);
    let mut tree = ElementTree::new();
    let root_elem = widget.create_element();
    let root_id = tree.insert(root_elem, None);
    widget.mount(&mut tree, root_id);
    (tree, root_id)
}

/// Build a wide+deep grid-like tree: Column of Rows of DecoratedBoxes
fn build_grid_tree(rows: usize, cols: usize) -> (ElementTree, ElementId) {
    let mut column = Column::new().gap(4.0);
    for _ in 0..rows {
        let mut row = Row::new().gap(4.0);
        for _ in 0..cols {
            row = row.child(
                DecoratedBox::new()
                    .style("width", 80.0_f32)
                    .style("height", 32.0_f32)
                    .style("background-color", Color::from_hex("#E5E7EB"))
            );
        }
        column = column.child(row);
    }
    let mut tree = ElementTree::new();
    let root_elem = column.create_element();
    let root_id = tree.insert(root_elem, None);
    column.mount(&mut tree, root_id);
    (tree, root_id)
}

/// Build a mixed widget tree simulating a real app UI
fn build_realistic_ui() -> (ElementTree, ElementId) {
    let mut main_col = Column::new().gap(16.0);

    // Header row
    main_col = main_col.child(
        Row::new()
            .gap(12.0)
            .child(Text::new("App Title").style("font-size", 24.0_f32))
            .child(DecoratedBox::new().style("width", 1.0_f32).style("height", 24.0_f32))
            .child(Text::new("Menu 1"))
            .child(Text::new("Menu 2"))
            .child(Text::new("Menu 3"))
    );

    // Content: 3 columns of cards
    let mut content_row = Row::new().gap(16.0);
    for col_idx in 0..3 {
        let mut card_col = Column::new().gap(8.0);
        card_col = card_col.child(Text::new(format!("Column {}", col_idx + 1)).style("font-size", 18.0_f32));
        for item_idx in 0..10 {
            card_col = card_col.child(
                Padding::all(8.0).child(
                    Column::new()
                        .gap(4.0)
                        .child(Text::new(format!("Card {item_idx}")))
                        .child(Text::new("Description text here").style("font-size", 12.0_f32))
                )
            );
        }
        content_row = content_row.child(card_col);
    }
    main_col = main_col.child(content_row);

    // Footer
    main_col = main_col.child(
        Row::new()
            .gap(8.0)
            .child(Text::new("Footer"))
            .child(Text::new("v1.0.0"))
    );

    let mut tree = ElementTree::new();
    let root_elem = main_col.create_element();
    let root_id = tree.insert(root_elem, None);
    main_col.mount(&mut tree, root_id);
    (tree, root_id)
}

// ── Benchmarks ──────────────────────────────────────────────────────────

fn bench_layout_flat_column(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout/flat_column");
    for n in [10, 50, 100, 500, 1000, 2000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let (mut tree, root_id) = build_flat_column(n);
            let constraints = Constraints::loose(Size::new(1280.0, 720.0));
            b.iter(|| {
                // Force re-layout by marking root dirty
                tree.mark_dirty(root_id, DirtyFlags::LAYOUT);
                black_box(tree.layout(root_id, constraints))
            });
        });
    }
    group.finish();
}

fn bench_layout_flat_row(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout/flat_row");
    for n in [10, 50, 100, 500, 1000, 2000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let (mut tree, root_id) = build_flat_row(n);
            let constraints = Constraints::loose(Size::new(1280.0, 720.0));
            b.iter(|| {
                tree.mark_dirty(root_id, DirtyFlags::LAYOUT);
                black_box(tree.layout(root_id, constraints))
            });
        });
    }
    group.finish();
}

fn bench_layout_deep_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout/deep_tree");
    for depth in [5, 10, 20, 50] {
        group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, &depth| {
            let (mut tree, root_id) = build_deep_tree(depth);
            let constraints = Constraints::loose(Size::new(1280.0, 720.0));
            b.iter(|| {
                tree.mark_dirty(root_id, DirtyFlags::LAYOUT);
                black_box(tree.layout(root_id, constraints))
            });
        });
    }
    group.finish();
}

fn bench_layout_grid(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout/grid");
    for (rows, cols) in [(5, 5), (10, 10), (20, 10), (10, 50), (20, 50), (40, 50)] {
        let label = format!("{rows}x{cols}");
        group.bench_with_input(BenchmarkId::new("rows_cols", &label), &(rows, cols), |b, &(rows, cols)| {
            let (mut tree, root_id) = build_grid_tree(rows, cols);
            let constraints = Constraints::loose(Size::new(1280.0, 720.0));
            b.iter(|| {
                tree.mark_dirty(root_id, DirtyFlags::LAYOUT);
                black_box(tree.layout(root_id, constraints))
            });
        });
    }
    group.finish();
}

fn bench_layout_realistic_ui(c: &mut Criterion) {
    c.bench_function("layout/realistic_ui", |b| {
        let (mut tree, root_id) = build_realistic_ui();
        let constraints = Constraints::loose(Size::new(1280.0, 720.0));
        b.iter(|| {
            tree.mark_dirty(root_id, DirtyFlags::LAYOUT);
            black_box(tree.layout(root_id, constraints))
        });
    });
}

fn bench_layout_cache_hit(c: &mut Criterion) {
    c.bench_function("layout/cache_hit", |b| {
        let (mut tree, root_id) = build_realistic_ui();
        let constraints = Constraints::loose(Size::new(1280.0, 720.0));
        // Initial layout to populate cache
        tree.layout(root_id, constraints);
        b.iter(|| {
            // No dirty flags — should be a fast cache hit
            black_box(tree.layout(root_id, constraints))
        });
    });
}

criterion_group!(
    benches,
    bench_layout_flat_column,
    bench_layout_flat_row,
    bench_layout_deep_tree,
    bench_layout_grid,
    bench_layout_realistic_ui,
    bench_layout_cache_hit,
);
criterion_main!(benches);
