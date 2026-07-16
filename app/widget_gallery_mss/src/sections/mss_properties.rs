use syngui::prelude::*;

use super::{label, section_card, section_title};

pub fn build_mss_properties_section() -> impl Widget {
    Column::new()
        .gap(20.0)
        .child(build_margin_demo())
        .child(build_border_demo())
        .child(build_font_weight_demo())
        .child(build_text_align_demo())
        .child(build_text_vertical_align_demo())
        .child(build_text_decoration_demo())
        .child(build_opacity_demo())
        .child(build_overflow_demo())
        .child(build_cursor_demo())
        .child(build_font_family_demo())
        .child(build_transition_demo())
}

fn build_margin_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("margin"))
            .child(label("Boxes with different margins inside a container"))
            .child(
                DecoratedBox::new().style("background-color", Color::from_hex("#f1f5f9"))
                    .style("border-radius", 8.0_f32)
                    .style("padding", 4.0_f32)
                    .child(
                        Row::new()
                            .gap(0.0)
                            .child(
                                DecoratedBox::new().style("background-color", Color::from_hex("#3b82f6"))
                                    .style("width", 80.0_f32)
                                    .style("height", 50.0_f32)
                                    .style("border-radius", 6.0_f32)
                                    .child(Text::new("0px").color(Color::WHITE).style("font-size", 11.0_f32))
                                    .class("mss-margin-none"),
                            )
                            .child(
                                DecoratedBox::new().style("background-color", Color::from_hex("#8b5cf6"))
                                    .style("width", 80.0_f32)
                                    .style("height", 50.0_f32)
                                    .style("border-radius", 6.0_f32)
                                    .child(Text::new("8px").color(Color::WHITE).style("font-size", 11.0_f32))
                                    .class("mss-margin-sm"),
                            )
                            .child(
                                DecoratedBox::new().style("background-color", Color::from_hex("#ec4899"))
                                    .style("width", 80.0_f32)
                                    .style("height", 50.0_f32)
                                    .style("border-radius", 6.0_f32)
                                    .child(Text::new("16px").color(Color::WHITE).style("font-size", 11.0_f32))
                                    .class("mss-margin-md"),
                            )
                            .child(
                                DecoratedBox::new().style("background-color", Color::from_hex("#f59e0b"))
                                    .style("width", 80.0_f32)
                                    .style("height", 50.0_f32)
                                    .style("border-radius", 6.0_f32)
                                    .child(Text::new("24px").color(Color::WHITE).style("font-size", 11.0_f32))
                                    .class("mss-margin-lg"),
                            ),
                    ),
            )
            .child(label("margin: 0 | 8px | 16px | 24px")),
    )
}

fn build_border_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("border / border-width / border-color"))
            .child(label("Boxes with different border styles"))
            .child(
                Row::new()
                    .gap(12.0)
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#ffffff"))
                            .style("width", 120.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("1px solid").style("font-size", 12.0_f32))
                            .class("mss-border-thin"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#ffffff"))
                            .style("width", 120.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("2px blue").style("font-size", 12.0_f32))
                            .class("mss-border-blue"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#ffffff"))
                            .style("width", 120.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("3px red").style("font-size", 12.0_f32))
                            .class("mss-border-red"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#ffffff"))
                            .style("width", 120.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 12.0_f32)
                            .child(Text::new("3px green").style("font-size", 12.0_f32))
                            .class("mss-border-green"),
                    ),
            ),
    )
}

fn build_font_weight_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("font-weight"))
            .child(label("Text with different font weights"))
            .child(
                Column::new()
                    .gap(6.0)
                    .child(Text::new("Normal text (font-weight: 400)").class("mss-fw-normal"))
                    .child(Text::new("Bold text (font-weight: 700)").class("mss-fw-bold"))
                    .child(Text::new("Bold text (font-weight: bold)").class("mss-fw-bold-keyword"))
                    .child(
                        Row::new()
                            .gap(16.0)
                            .child(Text::new("Normal").class("mss-fw-normal"))
                            .child(Text::new("Bold").class("mss-fw-bold")),
                    ),
            ),
    )
}

fn build_text_align_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("text-align"))
            .child(label("Text alignment within a fixed-width container"))
            .child(
                Column::new()
                    .gap(8.0)
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#f8fafc"))
                            .style("width", 400.0_f32)
                            .style("height", 32.0_f32)
                            .style("border-radius", 4.0_f32)
                            .child(Text::new("Left aligned text").class("mss-align-left"))
                            .class("mss-border-thin"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#f8fafc"))
                            .style("width", 400.0_f32)
                            .style("height", 32.0_f32)
                            .style("border-radius", 4.0_f32)
                            .child(Text::new("Center aligned text").class("mss-align-center"))
                            .class("mss-border-thin"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#f8fafc"))
                            .style("width", 400.0_f32)
                            .style("height", 32.0_f32)
                            .style("border-radius", 4.0_f32)
                            .child(Text::new("Right aligned text").class("mss-align-right"))
                            .class("mss-border-thin"),
                    ),
            ),
    )
}

fn build_text_vertical_align_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("text-vertical-align"))
            .child(label("Vertical text alignment within a fixed-height container"))
            .child(
                Column::new()
                    .gap(8.0)
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#f8fafc"))
                            .style("width", 400.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 4.0_f32)
                            .child(Text::new("Top aligned text").class("mss-valign-top"))
                            .class("mss-border-thin"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#f8fafc"))
                            .style("width", 400.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 4.0_f32)
                            .child(Text::new("Center aligned text (default)").class("mss-valign-center"))
                            .class("mss-border-thin"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#f8fafc"))
                            .style("width", 400.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 4.0_f32)
                            .child(Text::new("Bottom aligned text").class("mss-valign-bottom"))
                            .class("mss-border-thin"),
                    ),
            ),
    )
}

fn build_text_decoration_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("text-decoration"))
            .child(label("Text with underline and line-through decorations"))
            .child(
                Column::new()
                    .gap(8.0)
                    .child(Text::new("Normal text (no decoration)").style("font-size", 15.0_f32))
                    .child(Text::new("Underlined text").class("mss-underline").style("font-size", 15.0_f32))
                    .child(Text::new("Strikethrough text").class("mss-line-through").style("font-size", 15.0_f32))
                    .child(
                        Row::new()
                            .gap(16.0)
                            .child(Text::new("Link-style text").color(Color::from_hex("#3b82f6")).class("mss-underline").style("font-size", 14.0_f32))
                            .child(Text::new("Deleted price: $99").color(Color::from_hex("#94a3b8")).class("mss-line-through").style("font-size", 14.0_f32))
                            .child(Text::new("New price: $49").color(Color::from_hex("#22c55e")).class("mss-fw-bold").style("font-size", 14.0_f32)),
                    ),
            ),
    )
}

fn build_opacity_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("opacity"))
            .child(label("Containers with varying opacity levels"))
            .child(
                Row::new()
                    .gap(12.0)
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#3b82f6"))
                            .style("width", 100.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("100%").color(Color::WHITE).style("font-size", 13.0_f32))
                            .class("mss-opacity-100"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#3b82f6"))
                            .style("width", 100.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("75%").color(Color::WHITE).style("font-size", 13.0_f32))
                            .class("mss-opacity-75"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#3b82f6"))
                            .style("width", 100.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("50%").color(Color::WHITE).style("font-size", 13.0_f32))
                            .class("mss-opacity-50"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#3b82f6"))
                            .style("width", 100.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("25%").color(Color::WHITE).style("font-size", 13.0_f32))
                            .class("mss-opacity-25"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#3b82f6"))
                            .style("width", 100.0_f32)
                            .style("height", 60.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("10%").color(Color::WHITE).style("font-size", 13.0_f32))
                            .class("mss-opacity-10"),
                    ),
            ),
    )
}

fn overflow_boxes() -> Row {
    Row::new()
        .gap(4.0)
        .child(DecoratedBox::new().style("background-color", Color::from_hex("#f59e0b")).style("width", 40.0_f32).style("height", 35.0_f32).style("border-radius", 4.0_f32)
            .child(Text::new("A").color(Color::WHITE).style("font-size", 13.0_f32)))
        .child(DecoratedBox::new().style("background-color", Color::from_hex("#ef4444")).style("width", 40.0_f32).style("height", 35.0_f32).style("border-radius", 4.0_f32)
            .child(Text::new("B").color(Color::WHITE).style("font-size", 13.0_f32)))
        .child(DecoratedBox::new().style("background-color", Color::from_hex("#3b82f6")).style("width", 40.0_f32).style("height", 35.0_f32).style("border-radius", 4.0_f32)
            .child(Text::new("C").color(Color::WHITE).style("font-size", 13.0_f32)))
        .child(DecoratedBox::new().style("background-color", Color::from_hex("#10b981")).style("width", 40.0_f32).style("height", 35.0_f32).style("border-radius", 4.0_f32)
            .child(Text::new("D").color(Color::WHITE).style("font-size", 13.0_f32)))
        .child(DecoratedBox::new().style("background-color", Color::from_hex("#8b5cf6")).style("width", 40.0_f32).style("height", 35.0_f32).style("border-radius", 4.0_f32)
            .child(Text::new("E").color(Color::WHITE).style("font-size", 13.0_f32)))
}

fn build_overflow_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("overflow"))
            .child(label("5 boxes (220px total) inside a 160px container"))
            .child(
                Row::new()
                    .gap(24.0)
                    .child(
                        Column::new()
                            .gap(4.0)
                            .child(label("overflow: visible (default)"))
                            .child(
                                DecoratedBox::new().style("background-color", Color::from_hex("#fef3c7"))
                                    .style("width", 160.0_f32)
                                    .style("height", 50.0_f32)
                                    .style("border-radius", 8.0_f32)
                                    .child(overflow_boxes())
                                    .class("mss-border-thin"),
                            ),
                    )
                    .child(
                        Column::new()
                            .gap(4.0)
                            .child(label("overflow: hidden"))
                            .child(
                                DecoratedBox::new().style("background-color", Color::from_hex("#dbeafe"))
                                    .style("width", 160.0_f32)
                                    .style("height", 50.0_f32)
                                    .style("border-radius", 8.0_f32)
                                    .child(overflow_boxes())
                                    .class("mss-overflow-hidden"),
                            ),
                    ),
            ),
    )
}

fn build_cursor_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("cursor"))
            .child(label("Hover over boxes to see different cursor styles"))
            .child(
                Row::new()
                    .gap(12.0)
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#dbeafe"))
                            .style("width", 110.0_f32)
                            .style("height", 50.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("pointer").color(Color::from_hex("#1e40af")).style("font-size", 12.0_f32))
                            .class("mss-cursor-pointer"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#dcfce7"))
                            .style("width", 110.0_f32)
                            .style("height", 50.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("text").color(Color::from_hex("#166534")).style("font-size", 12.0_f32))
                            .class("mss-cursor-text"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#fef9c3"))
                            .style("width", 110.0_f32)
                            .style("height", 50.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("move").color(Color::from_hex("#854d0e")).style("font-size", 12.0_f32))
                            .class("mss-cursor-move"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#fce7f3"))
                            .style("width", 110.0_f32)
                            .style("height", 50.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("crosshair").color(Color::from_hex("#9d174d")).style("font-size", 12.0_f32))
                            .class("mss-cursor-crosshair"),
                    ),
            ),
    )
}

fn build_font_family_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("font-family"))
            .child(label("MSS supports font-family parsing (rendering uses system default)"))
            .child(
                Column::new()
                    .gap(6.0)
                    .child(Text::new("Default font (system)").style("font-size", 15.0_f32))
                    .child(Text::new("font-family: \"Inter\" — parsed, falls back to default").class("mss-font-inter").style("font-size", 13.0_f32))
                    .child(Text::new("font-family: monospace — parsed, falls back to default").class("mss-font-mono").style("font-size", 13.0_f32))
                    .child(label("Note: Custom font rendering is planned for a future release")),
            ),
    )
}

fn build_transition_demo() -> impl Widget {
    section_card(
        Column::new()
            .gap(12.0)
            .child(section_title("transition"))
            .child(label("Hover over boxes to see smooth CSS transitions"))
            .child(label("Different speeds"))
            .child(
                Row::new()
                    .gap(12.0)
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#dbeafe"))
                            .style("width", 130.0_f32)
                            .style("height", 55.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("100ms ease").color(Color::from_hex("#1e3a5f")).style("font-size", 11.0_f32))
                            .class("mss-transition-fast"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#d1fae5"))
                            .style("width", 130.0_f32)
                            .style("height", 55.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("300ms ease-in-out").color(Color::from_hex("#064e3b")).style("font-size", 11.0_f32))
                            .class("mss-transition-normal"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#fef3c7"))
                            .style("width", 130.0_f32)
                            .style("height", 55.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("800ms ease-out").color(Color::from_hex("#78350f")).style("font-size", 11.0_f32))
                            .class("mss-transition-slow"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#ede9fe"))
                            .style("width", 130.0_f32)
                            .style("height", 55.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("500ms bounce").color(Color::from_hex("#4c1d95")).style("font-size", 11.0_f32))
                            .class("mss-transition-bounce"),
                    ),
            )
            .child(label("Different properties"))
            .child(
                Row::new()
                    .gap(12.0)
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#fee2e2"))
                            .style("width", 160.0_f32)
                            .style("height", 55.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("opacity 400ms").color(Color::from_hex("#991b1b")).style("font-size", 11.0_f32))
                            .class("mss-transition-opacity"),
                    )
                    .child(
                        DecoratedBox::new().style("background-color", Color::from_hex("#f0f9ff"))
                            .style("width", 160.0_f32)
                            .style("height", 55.0_f32)
                            .style("border-radius", 8.0_f32)
                            .child(Text::new("border-color 300ms").color(Color::from_hex("#0c4a6e")).style("font-size", 11.0_f32))
                            .class("mss-transition-border"),
                    ),
            ),
    )
}
