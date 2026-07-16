//! Event handling benchmarks — dispatch performance through element trees

use std::hint::black_box;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use syngui::core::{Color, Size};
use syngui::input::{Event, MouseButton};
use syngui::layout::Constraints;
use syngui::widget::{Text, Widget, ElementTree, ElementId, WidgetExt};
use syngui::widgets::{Column, DecoratedBox, Padding, Row};

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

/// Flat column of text items
fn build_flat_list(n: usize) -> Box<dyn Widget> {
    let mut col = Column::new().gap(4.0);
    for i in 0..n {
        col = col.child(Text::new(format!("Item {i}")));
    }
    Box::new(col)
}

/// Deep nested tree
fn build_deep_tree(depth: usize) -> Box<dyn Widget> {
    fn make_widget(depth: usize) -> Box<dyn Widget> {
        if depth == 0 {
            Box::new(DecoratedBox::new().style("width", 100.0_f32).style("height", 40.0_f32).style("background-color", Color::from_hex("#EEEEEE")))
        } else {
            Box::new(
                Padding::all(4.0).child(
                    Column::new()
                        .gap(2.0)
                        .child(Text::new(format!("Level {depth}")))
                        .children(vec![make_widget(depth - 1)])
                )
            )
        }
    }
    make_widget(depth)
}

/// Realistic app UI
fn build_realistic_ui() -> Box<dyn Widget> {
    let mut main_col = Column::new().gap(8.0);

    // 3-column layout with items
    let mut content_row = Row::new().gap(16.0);
    for _ in 0..3 {
        let mut card_col = Column::new().gap(4.0);
        for i in 0..15 {
            card_col = card_col.child(
                DecoratedBox::new()
                    .style("width", 200.0_f32)
                    .style("height", 32.0_f32)
                    .style("background-color", Color::from_hex("#F3F4F6"))
                    .child(Text::new(format!("Card {i}")))
            );
        }
        content_row = content_row.child(card_col);
    }
    main_col = main_col.child(content_row);
    Box::new(main_col)
}

// ── Benchmarks ──────────────────────────────────────────────────────────

fn bench_event_mouse_move(c: &mut Criterion) {
    let mut group = c.benchmark_group("event/mouse_move");

    for n in [10, 50, 100, 500, 1000, 2000] {
        group.bench_with_input(BenchmarkId::new("flat_list", n), &n, |b, &n| {
            let widget = build_flat_list(n);
            let (mut tree, root_id) = build_and_layout(widget.as_ref());
            let event = Event::MouseMove(euclid::point2(100.0, 200.0));
            b.iter(|| {
                black_box(tree.handle_event(root_id, &event))
            });
        });
    }
    group.finish();
}

fn bench_event_mouse_click(c: &mut Criterion) {
    let mut group = c.benchmark_group("event/mouse_click");

    for n in [10, 50, 100, 500, 1000, 2000] {
        group.bench_with_input(BenchmarkId::new("flat_list", n), &n, |b, &n| {
            let widget = build_flat_list(n);
            let (mut tree, root_id) = build_and_layout(widget.as_ref());
            let event = Event::MouseDown {
                button: MouseButton::Left,
                position: euclid::point2(100.0, 200.0),
            };
            b.iter(|| {
                black_box(tree.handle_event(root_id, &event))
            });
        });
    }
    group.finish();
}

fn bench_event_deep_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("event/deep_dispatch");

    for depth in [5, 10, 20, 50] {
        group.bench_with_input(BenchmarkId::from_parameter(depth), &depth, |b, &depth| {
            let widget = build_deep_tree(depth);
            let (mut tree, root_id) = build_and_layout(widget.as_ref());
            let event = Event::MouseMove(euclid::point2(50.0, 50.0));
            b.iter(|| {
                black_box(tree.handle_event(root_id, &event))
            });
        });
    }
    group.finish();
}

fn bench_event_realistic_ui(c: &mut Criterion) {
    let mut group = c.benchmark_group("event/realistic_ui");

    let widget = build_realistic_ui();
    let (mut tree, root_id) = build_and_layout(widget.as_ref());

    // Mouse move (visits all siblings)
    group.bench_function("mouse_move", |b| {
        let event = Event::MouseMove(euclid::point2(300.0, 200.0));
        b.iter(|| {
            black_box(tree.handle_event(root_id, &event))
        });
    });

    // Mouse click (short-circuits)
    group.bench_function("mouse_down", |b| {
        let event = Event::MouseDown {
            button: MouseButton::Left,
            position: euclid::point2(300.0, 200.0),
        };
        b.iter(|| {
            black_box(tree.handle_event(root_id, &event))
        });
    });

    // Key event — no focus set → falls back to DFS
    group.bench_function("key_down", |b| {
        let event = Event::KeyDown(syngui::input::Key::Tab);
        b.iter(|| {
            black_box(tree.handle_event(root_id, &event))
        });
    });

    // Key event — focused_element set to root → targeted dispatch, 1-element chain.
    group.bench_function("key_down_focused_root", |b| {
        let widget = build_realistic_ui();
        let (mut tree, root_id) = build_and_layout(widget.as_ref());
        tree.focused_element = Some(root_id);
        let event = Event::KeyDown(syngui::input::Key::Tab);
        b.iter(|| {
            black_box(tree.handle_event(root_id, &event))
        });
    });

    group.finish();
}

fn bench_event_miss(c: &mut Criterion) {
    c.bench_function("event/miss_outside_bounds", |b| {
        let widget = build_realistic_ui();
        let (mut tree, root_id) = build_and_layout(widget.as_ref());
        // Click outside any element bounds
        let event = Event::MouseDown {
            button: MouseButton::Left,
            position: euclid::point2(5000.0, 5000.0),
        };
        b.iter(|| {
            black_box(tree.handle_event(root_id, &event))
        });
    });
}

criterion_group!(
    benches,
    bench_event_mouse_move,
    bench_event_mouse_click,
    bench_event_deep_dispatch,
    bench_event_realistic_ui,
    bench_event_miss,
);
criterion_main!(benches);
