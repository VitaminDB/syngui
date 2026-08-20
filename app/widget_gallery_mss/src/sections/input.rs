use syngui::prelude::*;
use syngui::widgets::*;

use super::{label, section_card, section_title};

pub fn build_input_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Input"))
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Text Field"))
                    .child(
                        TextField::with_text("Hello SYNGUI!")
                            .placeholder("Type here...")
                            .class("input-width"),
                    )
                    .child(
                        TextField::new()
                            .placeholder("Placeholder text...")
                            .class("input-width"),
                    )
                    .child(
                        TextField::new()
                            .placeholder("Read-only field")
                            .read_only(true)
                            .class("input-width"),
                    )
                    .child(
                        TextField::new()
                            .placeholder("Disabled field")
                            .disabled(true)
                            .class("input-width"),
                    ),
            )
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Multiline Text Edit"))
                    .child(
                        MultilineTextEdit::new()
                            .text("fn main() {\n    println!(\"Hello, SYNGUI!\");\n    let x = 42;\n    // Edit me!\n}")
                            .rows(5)
                            .show_line_numbers(true),
                    ),
            )
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Slider"))
                    .child(Slider::new().value(0.65).class("slider-width")),
            )
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Progress Bars"))
                    .child(
                        ProgressBar::with_value(0.75)
                            .class("progress-width")
                            .class("progress-height-sm"),
                    )
                    .child(
                        ProgressBar::with_value(0.42)
                            .show_percentage()
                            .class("progress-width")
                            .class("progress-height-md"),
                    ),
            )
            // New widgets
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("SpinBox"))
                    .child(
                        SpinBox::new()
                            .value(5.0)
                            .range(0.0, 100.0)
                            .step(1.0)
                            .width(150.0),
                    ),
            )
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Combobox"))
                    .child(
                        Combobox::new(vec![
                            DropdownItem::simple("Rust"),
                            DropdownItem::simple("Python"),
                            DropdownItem::simple("Go"),
                            DropdownItem::simple("TypeScript"),
                            DropdownItem::simple("C++"),
                        ])
                        .placeholder("Select language...")
                        .width(250.0),
                    ),
            )
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Autocomplete"))
                    .child(
                        Autocomplete::new(vec![
                            "Moscow".into(),
                            "Madrid".into(),
                            "Munich".into(),
                            "Milan".into(),
                            "Montreal".into(),
                            "Melbourne".into(),
                            "London".into(),
                            "Los Angeles".into(),
                            "Lima".into(),
                            "Paris".into(),
                            "Prague".into(),
                            "Porto".into(),
                        ])
                        .placeholder("Start typing a city...")
                        .width(250.0),
                    ),
            )
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Date Picker"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            // Плейсхолдер и формат берутся из локали.
                            .child(DatePicker::new().width(160.0))
                            .child(DatePicker::new().today().width(160.0))
                            .child(
                                DatePicker::new()
                                    .locale(CalendarLocale::english())
                                    .width(160.0),
                            ),
                    ),
            )
            // TimePicker
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Time Picker"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(
                                TimePicker::new()
                                    .selected(Time::new(14, 30))
                                    .width(200.0),
                            )
                            .child(
                                TimePicker::new()
                                    .placeholder("12h mode...")
                                    .use_24h(false)
                                    .width(200.0),
                            ),
                    ),
            )
            // ColorPicker
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Color Picker"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(
                                ColorPicker::new()
                                    .color(ColorValue::new(59, 130, 246))
                                    .width(260.0),
                            )
                            .child(
                                ColorPicker::new()
                                    .color(ColorValue::new(239, 68, 68))
                                    .width(260.0),
                            ),
                    ),
            ),
    )
}
