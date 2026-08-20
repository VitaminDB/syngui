use syngui::prelude::*;
use syngui::widgets::*;

use super::{label, section_card, section_title};

/// Generate a colorful gradient test pattern (RGBA)
fn make_gradient(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let u = x as f32 / width as f32;
            let v = y as f32 / height as f32;
            let r = (u * 255.0) as u8;
            let g = (v * 255.0) as u8;
            let b = ((1.0 - u) * 200.0 + 55.0) as u8;
            data.extend_from_slice(&[r, g, b, 255]);
        }
    }
    data
}

/// Card wrapping an Image with a given fit mode
fn image_card(label_text: &str, fit: ImageFit) -> impl Widget {
    let key = format!("gradient-{:?}", fit);
    Card::new()
        .child(
            Column::new()
                .gap(4.0)
                .child(
                    Image::from_rgba(key, 64, 64, make_gradient(64, 64))
                        .fit(fit)
                        .style("width", 100.0_f32)
                        .style("height", 75.0_f32),
                )
                .child(Text::new(label_text).class("label")),
        )
        .style("padding", 4.0_f32)
}

/// Accordion-style section built from basic widgets: Button header that
/// toggles the signal, AnimatedSize wrapping a ShowIf for smooth height
/// collapse/expand. No dedicated Accordion widget needed.
fn build_accordion_section(title: &str, body: &str, state: RwSignal<usize>) -> impl Widget {
    let t = title.to_string();
    let b = body.to_string();
    Column::new()
        .gap(0.0)
        .child(
            Button::new(t)
                .class("secondary")
                .on_click(move || {
                    state.set(if state.get_untracked() == 0 { 1 } else { 0 });
                }),
        )
        .child(
            AnimatedSize::new(
                ShowIf::new(0, state).child(
                    DecoratedBox::new()
                        .child(Text::new(b).style("font-size", 13.0_f32))
                        .style("padding", 12.0_f32),
                ),
            )
            .axis(AnimationAxis::Height)
            .duration_ms(300),
        )
}

pub fn build_visual_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Visual"))
            .child(
                Column::new().gap(8.0).child(label("Icons")).child(
                    Row::new()
                        .gap(12.0)
                        .child(Icon::new("🔔").style("icon-size", 32.0_f32))
                        .child(Icon::new("⚙").style("icon-size", 32.0_f32))
                        .child(Icon::new("❤").style("icon-size", 32.0_f32))
                        .child(Icon::new("📊").style("icon-size", 32.0_f32))
                        .child(Icon::new("🎵").style("icon-size", 32.0_f32))
                        .child(Icon::new("🌍").style("icon-size", 32.0_f32)),
                ),
            )
            .child(
                Column::new().gap(8.0).child(label("Badges")).child(
                    Row::new()
                        .gap(8.0)
                        .child(Badge::new("3").class("badge-red"))
                        .child(Badge::new("New").class("badge-blue"))
                        .child(Badge::new("99+").large().class("badge-amber"))
                        .child(Badge::dot())
                        .child(Badge::new("OK").small().class("badge-green")),
                ),
            )
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Dividers"))
                    .child(Divider::horizontal())
                    .child(Divider::horizontal().style("border-width", 2.0_f32)),
            )
            .child(
                Column::new().gap(8.0).child(label("Accordion")).child(
                    // Accordion собирается композицией Button + AnimatedSize + ShowIf.
                    // Выделенного виджета нет — см. layout_animation секцию.
                    Column::new()
                        .gap(2.0)
                        .child(build_accordion_section(
                            "Getting Started",
                            "Welcome to the accordion widget! Click headers to expand/collapse sections.",
                            use_signal(0usize),
                        ))
                        .child(build_accordion_section(
                            "Configuration",
                            "Configure animation duration, easing, and allow multiple open sections.",
                            use_signal(1usize),
                        ))
                        .child(build_accordion_section(
                            "Advanced Usage",
                            "Use `AnimatedSize` for custom collapsible layouts — see `layout_animation` section.",
                            use_signal(1usize),
                        )),
                ),
            )
            .child(
                Column::new().gap(8.0).child(label("Color Palette")).child(
                    Row::new()
                        .gap(8.0)
                        .child(
                            DecoratedBox::new()
                                .child(Text::new("Blue").class("swatch-label"))
                                .class("color-swatch-blue")
                                .class("swatch-size"),
                        )
                        .child(
                            DecoratedBox::new()
                                .child(Text::new("Red").class("swatch-label"))
                                .class("color-swatch-red")
                                .class("swatch-size"),
                        )
                        .child(
                            DecoratedBox::new()
                                .child(Text::new("Green").class("swatch-label"))
                                .class("color-swatch-green")
                                .class("swatch-size"),
                        )
                        .child(
                            DecoratedBox::new()
                                .child(Text::new("Amber").class("swatch-label"))
                                .class("color-swatch-amber")
                                .class("swatch-size"),
                        )
                        .child(
                            DecoratedBox::new()
                                .child(Text::new("Purple").class("swatch-label"))
                                .class("color-swatch-purple")
                                .class("swatch-size"),
                        ),
                ),
            )
            // New: Card
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Card"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(
                                Card::new()
                                    .child(
                                        Column::new()
                                            .gap(8.0)
                                            .child(Text::new("Basic Card").class("label"))
                                            .child(Text::new("elevation: 2").class("label")),
                                    )
                                    .style("padding", 16.0_f32),
                            )
                            .child(
                                Card::new()
                                    .child(
                                        Column::new()
                                            .gap(8.0)
                                            .child(Text::new("Elevated Card").class("label"))
                                            .child(Text::new("elevation: 8, radius: 16").class("label")),
                                    )
                                    .style("padding", 20.0_f32)
                                    .style("border-radius", 16.0_f32),
                            )
                            .child(
                                Card::new()
                                    .child(
                                        Column::new()
                                            .gap(8.0)
                                            .child(Text::new("Colored Card").class("label"))
                                            .child(Text::new("flat, green tint").class("label")),
                                    )
                                    .style("padding", 16.0_f32)
                                    .style("background-color", Color::from_hex("#E8F5E9")),
                            ),
                    ),
            )
            // New: Chip
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Chips"))
                    .child(
                        Row::new()
                            .gap(8.0)
                            .child(Chip::new("Default"))
                            .child(Chip::new("Selected").selected(true))
                            .child(Chip::new("Deletable").deletable())
                            .child(Chip::new("Colored").style("background-color", Color::from_hex("#E3F2FD")))
                            .child(Chip::new("Disabled").disabled(true)),
                    ),
            )
            // New: CircularProgress
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Circular Progress"))
                    .child(
                        Row::new()
                            .gap(16.0)
                            .child(CircularProgress::with_value(0.65).size(48.0))
                            .child(
                                CircularProgress::with_value(0.3)
                                    .size(48.0)
                                    .style("color", Color::from_hex("#4CAF50")),
                            )
                            .child(CircularProgress::new().indeterminate().size(48.0))
                            .child(
                                CircularProgress::with_value(0.9)
                                    .size(32.0)
                                    .stroke_width(3.0),
                            ),
                    ),
            )
            // New: Avatar
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Avatars"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(
                                Avatar::new()
                                    .text("AB")
                                    .size(48.0)
                                    .style("background-color", Color::from_hex("#1976D2")),
                            )
                            .child(
                                Avatar::new()
                                    .text("CD")
                                    .size(40.0)
                                    .style("background-color", Color::from_hex("#388E3C")),
                            )
                            .child(
                                Avatar::new()
                                    .text("EF")
                                    .size(32.0)
                                    .style("background-color", Color::from_hex("#D32F2F")),
                            )
                            .child(
                                Avatar::new()
                                    .text("G")
                                    .size(56.0)
                                    .style("background-color", Color::from_hex("#7B1FA2"))
                                    .style("color", Color::from_hex("#FFFFFF")),
                            ),
                    ),
            )
            // Calendar
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Calendar"))
                    .child(
                        Row::new()
                            .gap(16.0)
                            // Локаль по умолчанию — русская, открывается на
                            // текущем месяце с выделенным сегодня.
                            .child(Calendar::new())
                            .child(
                                Calendar::new()
                                    .locale(CalendarLocale::english())
                                    .show_week_numbers(true)
                                    .min_date(Date::today().add_days(-10))
                                    .max_date(Date::today().add_days(20)),
                            ),
                    ),
            )
            // Images
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Image (from_rgba)"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(image_card("Contain", ImageFit::Contain))
                            .child(image_card("Cover", ImageFit::Cover))
                            .child(image_card("Fill", ImageFit::Fill))
                            .child(image_card("None", ImageFit::None)),
                    )
                    .child(label("Tint & Opacity"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(
                                Card::new()
                                    .child(
                                        Column::new()
                                            .gap(4.0)
                                            .child(
                                                Image::from_rgba("tint-red", 64, 64, make_gradient(64, 64))
                                                    .tint(Color::new(1.0, 0.5, 0.5, 1.0))
                                                    .style("width", 100.0_f32)
                                                    .style("height", 75.0_f32),
                                            )
                                            .child(Text::new("Red tint").class("label")),
                                    )
                                    .style("padding", 4.0_f32),
                            )
                            .child(
                                Card::new()
                                    .child(
                                        Column::new()
                                            .gap(4.0)
                                            .child(
                                                Image::from_rgba("opacity-50", 64, 64, make_gradient(64, 64))
                                                    .style("width", 100.0_f32)
                                                    .style("height", 75.0_f32)
                                                    .style("opacity", 0.5_f32),
                                            )
                                            .child(Text::new("50% opacity").class("label")),
                                    )
                                    .style("padding", 4.0_f32),
                            ),
                    )
                    .child(label("Image (from file)"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(
                                Card::new()
                                    .child(
                                        Column::new()
                                            .gap(4.0)
                                            .child(
                                                Image::new("app/widget_gallery_mss/pics/nature.jpg")
                                                    .fit(ImageFit::Cover)
                                                    .style("width", 160.0_f32)
                                                    .style("height", 120.0_f32),
                                            )
                                            .child(Text::new("Nature (Cover)").class("label")),
                                    )
                                    .style("padding", 4.0_f32),
                            )
                            .child(
                                Card::new()
                                    .child(
                                        Column::new()
                                            .gap(4.0)
                                            .child(
                                                Image::new("app/widget_gallery_mss/pics/city.jpg")
                                                    .fit(ImageFit::Contain)
                                                    .style("width", 160.0_f32)
                                                    .style("height", 120.0_f32),
                                            )
                                            .child(Text::new("City (Contain)").class("label")),
                                    )
                                    .style("padding", 4.0_f32),
                            )
                            .child(
                                Card::new()
                                    .child(
                                        Column::new()
                                            .gap(4.0)
                                            .child(
                                                Image::new("app/widget_gallery_mss/pics/abstract.jpg")
                                                    .tint(Color::new(0.8, 0.9, 1.0, 1.0))
                                                    .style("width", 160.0_f32)
                                                    .style("height", 120.0_f32),
                                            )
                                            .child(Text::new("Abstract (Blue tint)").class("label")),
                                    )
                                    .style("padding", 4.0_f32),
                            ),
                    ),
            )
            // RichText
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("RichText"))
                    .child(
                        RichText::new()
                            .span("SYNGUI ", |s| s.bold().font_size(18.0).color(Color::from_hex("#3B82F6")))
                            .span("is a ", |s| s.font_size(14.0))
                            .span("modern ", |s| s.bold().font_size(14.0).color(Color::from_hex("#10B981")))
                            .span("GUI framework ", |s| s.font_size(14.0))
                            .span("for Rust", |s| s.italic().font_size(14.0).color(Color::from_hex("#8B5CF6")))
                            .span(". ", |s| s.font_size(14.0))
                            .span("Underlined text", |s| s.underline().font_size(14.0).color(Color::from_hex("#EF4444")))
                            .span(" and ", |s| s.font_size(14.0))
                            .span("bold italic", |s| s.bold().italic().font_size(14.0))
                            .span(" are supported.", |s| s.font_size(14.0))
                            .max_width(600.0),
                    )
                    .child(
                        RichText::new()
                            .span("Small ", |s| s.font_size(10.0))
                            .span("Medium ", |s| s.font_size(14.0))
                            .span("Large ", |s| s.font_size(20.0))
                            .span("Extra Large", |s| s.font_size(28.0).bold().color(Color::from_hex("#1F2937"))),
                    ),
            ),
    )
}
