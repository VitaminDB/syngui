//! Render benchmarks — DisplayList construction from element trees

use std::hint::black_box;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use syngui::core::{Color, Rect, Size};
use syngui::layout::Constraints;
use syngui::render::DisplayList;
use syngui::widget::{Text, Widget, ElementTree, ElementId, WidgetExt};
use syngui::widgets::{Column, DecoratedBox, Row};

// ── Helpers ─────────────────────────────────────────────────────────────

fn build_and_layout(widget: &dyn Widget) -> (ElementTree, ElementId) {
    let mut tree = ElementTree::new();
    let root_elem = widget.create_element();
    let root_id = tree.insert(root_elem, None);
    widget.mount(&mut tree, root_id);
    let constraints = Constraints::loose(Size::new(1280.0, 720.0));
    tree.layout(root_id, constraints);
    (tree, root_id)
}

/// Flat column of colored containers with text
fn build_colored_list(n: usize) -> Box<dyn Widget> {
    let mut col = Column::new().gap(4.0);
    for i in 0..n {
        col = col.child(
            DecoratedBox::new()
                .style("width", 200.0_f32)
                .style("height", 40.0_f32)
                .style("background-color", Color::from_hex("#3B82F6"))
                .child(Text::new(format!("Item {i}")))
        );
    }
    Box::new(col)
}

/// Grid of containers (generates many rects)
fn build_rect_grid(rows: usize, cols: usize) -> Box<dyn Widget> {
    let mut column = Column::new().gap(2.0);
    for r in 0..rows {
        let mut row = Row::new().gap(2.0);
        for c in 0..cols {
            let hue = ((r * cols + c) as f32 / (rows * cols) as f32 * 360.0) as u32;
            let color = Color::from_hex(&format!("#{:02x}{:02x}{:02x}", hue % 256, 128, 200));
            row = row.child(
                DecoratedBox::new()
                    .style("width", 30.0_f32)
                    .style("height", 20.0_f32)
                    .style("background-color", color)
            );
        }
        column = column.child(row);
    }
    Box::new(column)
}

/// Realistic UI (header + card list + footer)
fn build_realistic_ui() -> Box<dyn Widget> {
    let mut main_col = Column::new().gap(16.0);

    // Header
    main_col = main_col.child(
        DecoratedBox::new()
            .style("height", 48.0_f32)
            .style("background-color", Color::from_hex("#1F2937"))
            .child(
                Row::new()
                    .gap(12.0)
                    .child(Text::new("Dashboard").color(Color::from_hex("#FFFFFF")).style("font-size", 20.0_f32))
                    .child(Text::new("Settings").color(Color::from_hex("#9CA3AF")))
            )
    );

    // Content cards
    for i in 0..20 {
        main_col = main_col.child(
            DecoratedBox::new()
                .style("background-color", Color::from_hex("#FFFFFF"))
                .style("padding", 12.0_f32)
                .child(
                    Column::new()
                        .gap(4.0)
                        .child(Text::new(format!("Card Title {i}")))
                        .child(Text::new("Lorem ipsum dolor sit amet, consectetur adipiscing elit.").color(Color::from_hex("#6B7280")).style("font-size", 12.0_f32))
                )
        );
    }

    // Footer
    main_col = main_col.child(
        DecoratedBox::new()
            .style("height", 32.0_f32)
            .style("background-color", Color::from_hex("#F3F4F6"))
            .child(Text::new("Footer text").style("font-size", 12.0_f32))
    );

    Box::new(main_col)
}

// ── Benchmarks ──────────────────────────────────────────────────────────

fn bench_display_list_flat(c: &mut Criterion) {
    let mut group = c.benchmark_group("render/display_list_flat");
    for n in [10, 50, 100, 500, 1000, 2000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let widget = build_colored_list(n);
            let (tree, root_id) = build_and_layout(widget.as_ref());
            let viewport = Rect::new(euclid::point2(0.0, 0.0), Size::new(1280.0, 720.0));
            b.iter(|| {
                let mut list = DisplayList::new();
                list.set_surface_size(Size::new(1280.0, 720.0));
                tree.build_display_list(root_id, &mut list, viewport);
                black_box(list.commands().len())
            });
        });
    }
    group.finish();
}

fn bench_display_list_grid(c: &mut Criterion) {
    let mut group = c.benchmark_group("render/display_list_grid");
    for (rows, cols) in [(10, 10), (20, 20), (10, 50), (20, 50), (40, 50)] {
        let label = format!("{rows}x{cols}");
        group.bench_with_input(BenchmarkId::new("size", &label), &(rows, cols), |b, &(rows, cols)| {
            let widget = build_rect_grid(rows, cols);
            let (tree, root_id) = build_and_layout(widget.as_ref());
            let viewport = Rect::new(euclid::point2(0.0, 0.0), Size::new(1280.0, 720.0));
            b.iter(|| {
                let mut list = DisplayList::new();
                list.set_surface_size(Size::new(1280.0, 720.0));
                tree.build_display_list(root_id, &mut list, viewport);
                black_box(list.commands().len())
            });
        });
    }
    group.finish();
}

fn bench_display_list_realistic(c: &mut Criterion) {
    c.bench_function("render/display_list_realistic", |b| {
        let widget = build_realistic_ui();
        let (tree, root_id) = build_and_layout(widget.as_ref());
        let viewport = Rect::new(euclid::point2(0.0, 0.0), Size::new(1280.0, 720.0));
        b.iter(|| {
            let mut list = DisplayList::new();
            list.set_surface_size(Size::new(1280.0, 720.0));
            tree.build_display_list(root_id, &mut list, viewport);
            black_box(list.commands().len())
        });
    });
}

fn bench_display_list_reuse(c: &mut Criterion) {
    c.bench_function("render/display_list_reuse", |b| {
        let widget = build_realistic_ui();
        let (tree, root_id) = build_and_layout(widget.as_ref());
        let viewport = Rect::new(euclid::point2(0.0, 0.0), Size::new(1280.0, 720.0));
        let mut list = DisplayList::new();
        list.set_surface_size(Size::new(1280.0, 720.0));
        b.iter(|| {
            list.clear();
            tree.build_display_list(root_id, &mut list, viewport);
            black_box(list.commands().len())
        });
    });
}

criterion_group!(
    benches,
    bench_display_list_flat,
    bench_display_list_grid,
    bench_display_list_realistic,
    bench_display_list_reuse,
);
criterion_main!(benches);
