use syngui::prelude::*;
use syngui::widgets::*;

use super::{label, section_card, section_title};

pub fn build_containers_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Containers"))
            // SplitView Horizontal
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("SplitView — Horizontal"))
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#F9FAFB"))
                            .style("border-width", 1.0_f32).style("border-color", Color::from_hex("#E5E7EB"))
                            .style("border-radius", 8.0_f32)
                            .child(
                                SplitView::new(
                                    // Left panel
                                    DecoratedBox::new().style("background-color", Color::from_hex("#EFF6FF"))
                                        .child(
                                            Column::new()
                                                .gap(8.0)
                                                .child(Text::new("Left Panel").class("label"))
                                                .child(Text::new("Drag the divider to resize").class("label")),
                                        ),
                                    // Right panel
                                    DecoratedBox::new().style("background-color", Color::from_hex("#F0FDF4"))
                                        .child(
                                            Column::new()
                                                .gap(8.0)
                                                .child(Text::new("Right Panel").class("label"))
                                                .child(Text::new("Flexible content area").class("label")),
                                        ),
                                )
                                .direction(SplitDirection::Horizontal)
                                .initial_ratio(0.4)
                                .min_size(80.0)
                                .divider_width(6.0),
                            )
                            .class("split-demo-h"),
                    ),
            )
            // SplitView Vertical
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("SplitView — Vertical"))
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#F9FAFB"))
                            .style("border-width", 1.0_f32).style("border-color", Color::from_hex("#E5E7EB"))
                            .style("border-radius", 8.0_f32)
                            .child(
                                SplitView::new(
                                    // Top panel
                                    DecoratedBox::new().style("background-color", Color::from_hex("#FEF3C7"))
                                        .child(Text::new("Top Panel").class("label")),
                                    // Bottom panel
                                    DecoratedBox::new().style("background-color", Color::from_hex("#FCE7F3"))
                                        .child(Text::new("Bottom Panel").class("label")),
                                )
                                .direction(SplitDirection::Vertical)
                                .initial_ratio(0.35)
                                .min_size(60.0),
                            )
                            .class("split-demo-v"),
                    ),
            )
            // TransformBox demos
            .child(section_title("TransformBox"))
            .child(label("Interactive resize, rotate, and move handles (Figma-style)."))
            // TransformBox with DecoratedBox
            .child({
                let active1 = use_signal(true);
                let pos1 = use_signal(Point::new(20.0, 20.0));
                let size1 = use_signal(Size::new(200.0, 120.0));
                let rot1 = use_signal(0.0_f32);
                Column::new()
                    .gap(8.0)
                    .child(label("TransformBox + DecoratedBox"))
                    .child(
                        DecoratedBox::new()
                            .style("background-color", Color::from_hex("#F9FAFB"))
                            .style("border-width", 1.0_f32)
                            .style("border-color", Color::from_hex("#E5E7EB"))
                            .style("border-radius", 8.0_f32)
                            .style("width", 800.0_f32)
                            .style("height", 400.0_f32)
                            .clip(true)
                            .child(
                                TransformBox::new()
                                    .active(active1)
                                    .position(pos1)
                                    .size_signal(size1)
                                    .rotation(rot1)
                                    .initial_size(280.0, 160.0)
                                    .child(
                                        DecoratedBox::new()
                                            .style("background-color", Color::from_hex("#DBEAFE"))
                                            .style("border-radius", 8.0_f32)
                                            .style("padding", 16.0_f32)
                                            .child(
                                                Column::new()
                                                    .gap(4.0)
                                                    .child(Text::new("Decorated Card").class("label"))
                                                    .child(Text::new("Drag body to move").class("label"))
                                                    .child(Text::new("Corners to resize").class("label"))
                                                    .child(Text::new("Top handle to rotate").class("label")),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        Row::new().gap(12.0)
                            .child(Toggle::new().on(true).on_change(move |v| active1.set(v)))
                            .child(label("Active")),
                    )
                    .child(move || {
                        let p = pos1.get();
                        let s = size1.get();
                        let r = rot1.get();
                        Text::new(&format!(
                            "pos: ({:.0}, {:.0})  size: {:.0}x{:.0}  rotation: {:.1}\u{00b0}",
                            p.x, p.y, s.width, s.height, r,
                        )).class("label")
                    })
            })
            // TransformBox with Button
            .child({
                let active2 = use_signal(true);
                Column::new()
                    .gap(8.0)
                    .child(label("TransformBox + Button"))
                    .child(
                        DecoratedBox::new()
                            .style("background-color", Color::from_hex("#F9FAFB"))
                            .style("border-width", 1.0_f32)
                            .style("border-color", Color::from_hex("#E5E7EB"))
                            .style("border-radius", 8.0_f32)
                            .style("width", 800.0_f32)
                            .style("height", 200.0_f32)
                            .clip(true)
                            .child(
                                TransformBox::new()
                                    .active(active2)
                                    .initial_size(160.0, 48.0)
                                    .child(Button::new("Click Me")),
                            ),
                    )
                    .child(
                        Row::new().gap(12.0)
                            .child(Toggle::new().on(true).on_change(move |v| active2.set(v)))
                            .child(label("Active")),
                    )
            })
            // TransformBox with TextField
            .child({
                let active3 = use_signal(true);
                Column::new()
                    .gap(8.0)
                    .child(label("TransformBox + TextField"))
                    .child(
                        DecoratedBox::new()
                            .style("background-color", Color::from_hex("#F9FAFB"))
                            .style("border-width", 1.0_f32)
                            .style("border-color", Color::from_hex("#E5E7EB"))
                            .style("border-radius", 8.0_f32)
                            .style("width", 800.0_f32)
                            .style("height", 200.0_f32)
                            .clip(true)
                            .child(
                                TransformBox::new()
                                    .active(active3)
                                    .initial_size(250.0, 40.0)
                                    .rotatable(false)
                                    .child(
                                        TextField::new()
                                            .placeholder("Type here..."),
                                    ),
                            ),
                    )
                    .child(
                        Row::new().gap(12.0)
                            .child(Toggle::new().on(true).on_change(move |v| active3.set(v)))
                            .child(label("Active")),
                    )
            }),
    )
}
