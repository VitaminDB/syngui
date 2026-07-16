use syngui::prelude::*;
use syngui::mss::StyleValue;

use super::{label, section_card, section_title};

/// A gradient sample box styled via MSS class
fn gradient_box(class: &str, label_text: &str) -> impl Widget {
    Column::new()
        .gap(4.0)
        .child(
            DecoratedBox::new()
                .style("width", 160.0_f32)
                .style("height", 80.0_f32)
                .class(class),
        )
        .child(Text::new(label_text).class("label"))
}

/// A gradient box created programmatically (not via MSS)
fn programmatic_gradient(gradient: syngui::core::Gradient, label_text: &str) -> impl Widget {
    Column::new()
        .gap(4.0)
        .child(
            DecoratedBox::new()
                .style("background-gradient", StyleValue::Gradient(gradient))
                .style("border-radius", 8.0_f32)
                .style("width", 160.0_f32)
                .style("height", 80.0_f32),
        )
        .child(Text::new(label_text).class("label"))
}

pub fn build_gradients_section() -> impl Widget {
    use syngui::core::{ColorStop, Gradient, GradientShape};

    section_card(
        Column::new()
            .gap(20.0)
            .child(section_title("Gradients"))

            // ── Linear Gradients (MSS) ──
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Linear Gradients (MSS)"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(gradient_box("gradient-horizontal", "Horizontal (90deg)"))
                            .child(gradient_box("gradient-vertical", "Vertical (180deg)"))
                            .child(gradient_box("gradient-diagonal", "Diagonal (135deg)"))
                            .child(gradient_box("gradient-to-top", "To top (0deg)"))
                    ),
            )

            // ── Multi-stop Gradients (MSS) ──
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Multi-stop Gradients (MSS)"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(gradient_box("gradient-rainbow", "Rainbow"))
                            .child(gradient_box("gradient-sunset", "Sunset"))
                            .child(gradient_box("gradient-ocean", "Ocean"))
                    ),
            )

            // ── Radial Gradients (MSS) ──
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Radial Gradients (MSS)"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(gradient_box("gradient-radial", "Circle"))
                            .child(gradient_box("gradient-radial-ellipse", "Ellipse"))
                    ),
            )

            // ── Gradients with Rounded Corners (MSS) ──
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Gradients with Rounded Corners"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(gradient_box("gradient-rounded", "Rounded 12px"))
                            .child(gradient_box("gradient-pill", "Pill shape"))
                    ),
            )

            // ── Gradients with Borders ──
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Gradients with Borders"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(gradient_box("gradient-bordered", "With border"))
                    ),
            )

            // ── Programmatic Gradients ──
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Programmatic Gradients (code)"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(programmatic_gradient(
                                Gradient::Linear {
                                    angle_deg: 45.0,
                                    stops: vec![
                                        ColorStop::new(Color::from_hex("#FF6B6B"), 0.0),
                                        ColorStop::new(Color::from_hex("#4ECDC4"), 1.0),
                                    ],
                                },
                                "Code: 45deg",
                            ))
                            .child(programmatic_gradient(
                                Gradient::Linear {
                                    angle_deg: 90.0,
                                    stops: vec![
                                        ColorStop::new(Color::from_hex("#667eea"), 0.0),
                                        ColorStop::new(Color::from_hex("#764ba2"), 0.5),
                                        ColorStop::new(Color::from_hex("#f093fb"), 1.0),
                                    ],
                                },
                                "Code: 3 stops",
                            ))
                            .child(programmatic_gradient(
                                Gradient::Radial {
                                    shape: GradientShape::Circle,
                                    center: (0.5, 0.5),
                                    stops: vec![
                                        ColorStop::new(Color::from_hex("#FFFFFF"), 0.0),
                                        ColorStop::new(Color::from_hex("#6366F1"), 1.0),
                                    ],
                                    quality: syngui::core::GRADIENT_DEFAULT_QUALITY,
                                },
                                "Code: Radial",
                            ))
                    ),
            )

            // ── Gradient Cards (practical UI) ──
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Gradient Cards (practical UI)"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(gradient_card("gradient-card-purple", "Premium Plan", "$29/mo"))
                            .child(gradient_card("gradient-card-blue", "Pro Plan", "$49/mo"))
                            .child(gradient_card("gradient-card-green", "Enterprise", "$99/mo"))
                    ),
            )
    )
}

fn gradient_card(class: &str, title: &str, price: &str) -> impl Widget {
    DecoratedBox::new()
        .style("padding", 16.0_f32)
        .style("width", 180.0_f32)
        .child(
            Column::new()
                .gap(8.0)
                .child(Text::new(title).color(Color::WHITE).bold())
                .child(Text::new(price).color(Color::WHITE.with_alpha(0.8)).style("font-size", 24.0_f32))
        )
        .class(class)
}
