use syngui::prelude::*;

use super::{label, section_card, section_title};

pub fn build_scroll_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Scroll & Layout"))
            // ScrollView demo
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("ScrollView (vertical)"))
                    .child(
                        DecoratedBox::new()
                            .child(
                                ScrollView::new()
                                    .direction(ScrollDirection::Vertical)
                                    .child(
                                        Column::new()
                                            .gap(8.0)
                                            .child(Text::new("Line 1: Lorem ipsum dolor sit amet").class("scroll-text"))
                                            .child(Text::new("Line 2: Consectetur adipiscing elit").class("scroll-text"))
                                            .child(Text::new("Line 3: Sed do eiusmod tempor").class("scroll-text"))
                                            .child(Text::new("Line 4: Incididunt ut labore").class("scroll-text"))
                                            .child(Text::new("Line 5: Et dolore magna aliqua").class("scroll-text"))
                                            .child(Text::new("Line 6: Ut enim ad minim veniam").class("scroll-text"))
                                            .child(Text::new("Line 7: Quis nostrud exercitation").class("scroll-text"))
                                            .child(Text::new("Line 8: Ullamco laboris nisi").class("scroll-text"))
                                            .child(Text::new("Line 9: Ut aliquip ex ea commodo").class("scroll-text"))
                                            .child(Text::new("Line 10: Consequat duis aute irure").class("scroll-text")),
                                    ),
                            )
                            .class("scroll-container")
                            .class("scroll-height"),
                    ),
            )
            // Layout demo
            .child(
                Column::new().gap(8.0).child(label("Row Layout")).child(
                    Row::new()
                        .gap(8.0)
                        .child(
                            DecoratedBox::new()
                                .child(Text::new("Box 1").class("layout-box-text"))
                                .class("layout-box-blue")
                                .class("layout-box-size"),
                        )
                        .child(
                            DecoratedBox::new()
                                .child(Text::new("Box 2").class("layout-box-text"))
                                .class("layout-box-amber")
                                .class("layout-box-size"),
                        )
                        .child(
                            DecoratedBox::new()
                                .child(Text::new("Box 3").class("layout-box-text"))
                                .class("layout-box-green")
                                .class("layout-box-size"),
                        ),
                ),
            )
            // Stack demo
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Stack (overlapping)"))
                    .child(
                        Stack::new()
                            .child(
                                DecoratedBox::new()
                                    .class("stack-blue")
                                    .class("stack-box-lg"),
                            )
                            .child(
                                DecoratedBox::new()
                                    .class("stack-red")
                                    .class("stack-box-md"),
                            )
                            .child(
                                DecoratedBox::new()
                                    .child(Text::new("Top").class("stack-label"))
                                    .class("stack-green")
                                    .class("stack-box-sm"),
                            ),
                    ),
            )
            // New: Grid
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Grid (3 columns)"))
                    .child(
                        Grid::new(3)
                            .gap(8.0)
                            .child(
                                Card::new().child(
                                    Column::new()
                                        .gap(4.0)
                                        .child(Text::new("Card 1").class("label"))
                                        .child(Text::new("Grid item").class("label")),
                                ).style("padding", 12.0_f32),
                            )
                            .child(
                                Card::new().child(
                                    Column::new()
                                        .gap(4.0)
                                        .child(Text::new("Card 2").class("label"))
                                        .child(Text::new("Grid item").class("label")),
                                ).style("padding", 12.0_f32),
                            )
                            .child(
                                Card::new().child(
                                    Column::new()
                                        .gap(4.0)
                                        .child(Text::new("Card 3").class("label"))
                                        .child(Text::new("Grid item").class("label")),
                                ).style("padding", 12.0_f32),
                            )
                            .child(
                                Card::new().child(
                                    Column::new()
                                        .gap(4.0)
                                        .child(Text::new("Card 4").class("label"))
                                        .child(Text::new("Grid item").class("label")),
                                ).style("padding", 12.0_f32),
                            )
                            .child(
                                Card::new().child(
                                    Column::new()
                                        .gap(4.0)
                                        .child(Text::new("Card 5").class("label"))
                                        .child(Text::new("Grid item").class("label")),
                                ).style("padding", 12.0_f32),
                            )
                            .child(
                                Card::new().child(
                                    Column::new()
                                        .gap(4.0)
                                        .child(Text::new("Card 6").class("label"))
                                        .child(Text::new("Grid item").class("label")),
                                ).style("padding", 12.0_f32),
                            ),
                    ),
            )
            // New: Carousel
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Carousel"))
                    .child(
                        Carousel::new()
                            .show_indicators(true)
                            .child(
                                DecoratedBox::new().style("background-color", Color::from_hex("#E3F2FD"))
                                    .child(
                                        Padding::all(24.0).child(
                                            Text::new("Page 1: Welcome to the Carousel").class("label"),
                                        ),
                                    )
                                    .class("section-card"),
                            )
                            .child(
                                DecoratedBox::new().style("background-color", Color::from_hex("#F3E5F5"))
                                    .child(
                                        Padding::all(24.0).child(
                                            Text::new("Page 2: Swipe or use indicators").class("label"),
                                        ),
                                    )
                                    .class("section-card"),
                            )
                            .child(
                                DecoratedBox::new().style("background-color", Color::from_hex("#E8F5E9"))
                                    .child(
                                        Padding::all(24.0).child(
                                            Text::new("Page 3: End of the carousel").class("label"),
                                        ),
                                    )
                                    .class("section-card"),
                            ),
                    ),
            ),
    )
}
