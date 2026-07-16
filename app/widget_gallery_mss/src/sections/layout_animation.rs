use syngui::animation::Easing;
use syngui::prelude::*;
use syngui::widgets::*;

use super::{label, section_card, section_title};

pub fn build_layout_animation_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(24.0)
            .child(section_title("Layout Animation"))
            .child(build_expand_collapse_demo())
            .child(build_content_resize_demo())
            .child(build_axis_demo())
            .child(build_easing_demo())
            .child(build_accordion_demo()),
    )
}

// ── Expand / Collapse ────────────────────────────────────────────────

fn build_expand_collapse_demo() -> impl Widget {
    let expanded = use_signal(0usize); // 0=visible, 1=hidden

    Column::new()
        .gap(8.0)
        .child(label("Expand / Collapse"))
        .child(
            Text::new("AnimatedSize wraps ShowIf — height smoothly animates to/from zero.")
                .class("label"),
        )
        .child(
            Button::new("Toggle Panel").on_click(move || {
                expanded.set(if expanded.get_untracked() == 0 { 1 } else { 0 });
            }),
        )
        .child(
            AnimatedSize::new(
                ShowIf::new(0, expanded).child(
                    DecoratedBox::new()
                        .child(
                            Column::new()
                                .gap(8.0)
                                .child(
                                    Text::new(
                                        "This panel expands and collapses smoothly.",
                                    )
                                    .class("label"),
                                )
                                .child(
                                    Text::new(
                                        "AnimatedSize detects when its child's natural size\nchanges and animates the transition.",
                                    )
                                    .class("label"),
                                )
                                .child(
                                    Row::new()
                                        .gap(8.0)
                                        .child(colored_box("layout-anim-box", 40.0, 40.0))
                                        .child(colored_box("layout-anim-box-green", 40.0, 40.0))
                                        .child(colored_box("layout-anim-box-purple", 40.0, 40.0)),
                                ),
                        )
                        .class("layout-anim-panel"),
                ),
            )
            .duration_ms(400)
            .easing(Easing::EaseOutCubic)
            .axis(AnimationAxis::Height),
        )
}

fn colored_box(class: &str, w: f32, h: f32) -> impl Widget {
    DecoratedBox::new()
        .style("width", w)
        .style("height", h)
        .class(class)
}

// ── Content Resize ───────────────────────────────────────────────────

fn build_content_resize_demo() -> impl Widget {
    let content_idx = use_signal(0usize);

    Column::new()
        .gap(8.0)
        .child(label("Content Resize"))
        .child(
            Text::new("Click to cycle content — AnimatedSize smoothly transitions both axes.")
                .class("label"),
        )
        .child(
            Button::new("Cycle Content").on_click(move || {
                content_idx.set((content_idx.get_untracked() + 1) % 3);
            }),
        )
        .child(
            AnimatedSize::new(
                Stack::new()
                    .child(ShowIf::new(0, content_idx).child(
                        DecoratedBox::new()
                            .style("width", 150.0_f32)
                            .style("height", 40.0_f32)
                            .child(Text::new("Small content").class("label"))
                            .class("layout-anim-content"),
                    ))
                    .child(ShowIf::new(1, content_idx).child(
                        DecoratedBox::new()
                            .style("width", 250.0_f32)
                            .style("height", 70.0_f32)
                            .child(
                                Column::new()
                                    .gap(4.0)
                                    .child(Text::new("Medium content with more text").class("label"))
                                    .child(Text::new("Second line here").class("label")),
                            )
                            .class("layout-anim-content"),
                    ))
                    .child(ShowIf::new(2, content_idx).child(
                        DecoratedBox::new()
                            .style("width", 350.0_f32)
                            .style("height", 150.0_f32)
                            .child(
                                Column::new()
                                    .gap(4.0)
                                    .child(Text::new("Large content block").class("label"))
                                    .child(Text::new("With multiple lines of text").class("label"))
                                    .child(Text::new("And even more content below").class("label"))
                                    .child(
                                        Row::new()
                                            .gap(8.0)
                                            .child(colored_box("layout-anim-box", 60.0, 60.0))
                                            .child(colored_box(
                                                "layout-anim-box-green",
                                                60.0,
                                                60.0,
                                            )),
                                    ),
                            )
                            .class("layout-anim-content"),
                    )),
            )
            .duration_ms(350)
            .easing(Easing::EaseOutCubic),
        )
}

// ── Axis Comparison ──────────────────────────────────────────────────

fn build_axis_demo() -> impl Widget {
    let idx_w = use_signal(0usize);
    let idx_h = use_signal(0usize);
    let idx_b = use_signal(0usize);

    Column::new()
        .gap(8.0)
        .child(label("Axis Comparison"))
        .child(
            Text::new("Width only / Height only / Both — same toggle, different axis.")
                .class("label"),
        )
        .child(
            Row::new()
                .gap(8.0)
                .child(
                    Button::new("Toggle W").on_click(move || {
                        idx_w.set(if idx_w.get_untracked() == 0 { 1 } else { 0 });
                    }),
                )
                .child(
                    Button::new("Toggle H").on_click(move || {
                        idx_h.set(if idx_h.get_untracked() == 0 { 1 } else { 0 });
                    }),
                )
                .child(
                    Button::new("Toggle Both").on_click(move || {
                        idx_b.set(if idx_b.get_untracked() == 0 { 1 } else { 0 });
                    }),
                ),
        )
        .child(
            Row::new()
                .gap(16.0)
                .child(
                    Column::new()
                        .gap(4.0)
                        .child(Text::new("Width only").class("label"))
                        .child(
                            AnimatedSize::new(
                                ShowIf::new(0, idx_w)
                                    .child(colored_box("layout-anim-box", 120.0, 60.0)),
                            )
                            .axis(AnimationAxis::Width)
                            .duration_ms(400),
                        ),
                )
                .child(
                    Column::new()
                        .gap(4.0)
                        .child(Text::new("Height only").class("label"))
                        .child(
                            AnimatedSize::new(
                                ShowIf::new(0, idx_h)
                                    .child(colored_box("layout-anim-box-green", 120.0, 60.0)),
                            )
                            .axis(AnimationAxis::Height)
                            .duration_ms(400),
                        ),
                )
                .child(
                    Column::new()
                        .gap(4.0)
                        .child(Text::new("Both axes").class("label"))
                        .child(
                            AnimatedSize::new(
                                ShowIf::new(0, idx_b)
                                    .child(colored_box("layout-anim-box-purple", 120.0, 60.0)),
                            )
                            .axis(AnimationAxis::Both)
                            .duration_ms(400),
                        ),
                ),
        )
}

// ── Easing Comparison ────────────────────────────────────────────────

fn build_easing_demo() -> impl Widget {
    let idx = use_signal(0usize);

    Column::new()
        .gap(8.0)
        .child(label("Easing Comparison"))
        .child(
            Text::new("Same expand/collapse with different easing functions.").class("label"),
        )
        .child(
            Button::new("Toggle All").on_click(move || {
                idx.set(if idx.get_untracked() == 0 { 1 } else { 0 });
            }),
        )
        .child(
            Row::new()
                .gap(16.0)
                .child(easing_box(
                    "EaseOutCubic",
                    Easing::EaseOutCubic,
                    "layout-anim-box",
                    idx,
                ))
                .child(easing_box(
                    "EaseOutBounce",
                    Easing::EaseOutBounce,
                    "layout-anim-box-green",
                    idx,
                ))
                .child(easing_box(
                    "EaseOutElastic",
                    Easing::EaseOutElastic,
                    "layout-anim-box-purple",
                    idx,
                ))
                .child(easing_box(
                    "EaseInOutBack",
                    Easing::EaseInOutBack,
                    "layout-anim-box-amber",
                    idx,
                )),
        )
}

fn easing_box(
    name: &str,
    easing: Easing,
    class: &str,
    idx: RwSignal<usize>,
) -> impl Widget {
    Column::new()
        .gap(4.0)
        .child(Text::new(name).class("label"))
        .child(
            AnimatedSize::new(
                ShowIf::new(0, idx).child(colored_box(class, 80.0, 80.0)),
            )
            .axis(AnimationAxis::Height)
            .easing(easing)
            .duration_ms(600),
        )
}

// ── Accordion ────────────────────────────────────────────────────────

fn build_accordion_demo() -> impl Widget {
    let s1 = use_signal(0usize); // 0=open, 1=closed
    let s2 = use_signal(1usize);
    let s3 = use_signal(1usize);

    Column::new()
        .gap(8.0)
        .child(label("Accordion"))
        .child(
            Text::new("Multiple collapsible panels with smooth layout animation.").class("label"),
        )
        .child(
            Column::new()
                .gap(2.0)
                .child(accordion_section(
                    "Section 1 — Introduction",
                    "Welcome to AnimatedSize! This widget smoothly\nanimates its layout size whenever the child's\nnatural size changes.",
                    s1,
                ))
                .child(accordion_section(
                    "Section 2 — How It Works",
                    "AnimatedSize uses LayoutHint::AnimatedSize to\nintercept the tree's measure phase. It measures\nthe child normally, then returns an interpolated\nsize that gradually approaches the target.",
                    s2,
                ))
                .child(accordion_section(
                    "Section 3 — Configuration",
                    "Configure duration, easing, clip behavior, and\nwhich axes to animate. Supports all standard\neasing functions plus spring-like CubicBezier.",
                    s3,
                )),
        )
}

fn accordion_section(
    title: &str,
    body: &str,
    state: RwSignal<usize>,
) -> impl Widget {
    let t = title.to_string();
    Column::new()
        .gap(0.0)
        .child(
            Button::new(t).on_click(move || {
                state.set(if state.get_untracked() == 0 { 1 } else { 0 });
            }),
        )
        .child(
            AnimatedSize::new(
                ShowIf::new(0, state).child(
                    DecoratedBox::new()
                        .child(Text::new(body).class("label"))
                        .class("layout-anim-accordion-body"),
                ),
            )
            .axis(AnimationAxis::Height)
            .duration_ms(300)
            .easing(Easing::EaseOutCubic),
        )
}
