use syngui::prelude::*;

use super::{label, section_card, section_title};

pub fn build_selection_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Selection"))
            .child(
                Column::new().gap(8.0).child(label("Checkboxes")).child(
                    Row::new()
                        .gap(16.0)
                        .child(Checkbox::checked(true).label("Enable notifications"))
                        .child(Checkbox::new().label("Auto-update"))
                        .child(Checkbox::new().label("Remember me")),
                ),
            )
            .child(
                Column::new().gap(8.0).child(label("Toggles")).child(
                    Row::new()
                        .gap(16.0)
                        .child(Toggle::new().on(true))
                        .child(Toggle::new())
                        .child(Toggle::new().disabled(true)),
                ),
            )
            .child(
                Column::new().gap(8.0).child(label("Dropdown")).child(
                    Dropdown::new()
                        .placeholder("Select language...")
                        .item(DropdownItem::simple("Rust").icon("🦀"))
                        .item(DropdownItem::simple("Python").icon("🐍"))
                        .item(DropdownItem::simple("Go").icon("🐹"))
                        .item(DropdownItem::simple("TypeScript").icon("📘"))
                        .item(DropdownItem::simple("C++").icon("⚡"))
                        .selected("Rust")
                        .class("dropdown-width"),
                ),
            )
            .child({
                let group = RadioGroup::new("theme").selected("light");
                Column::new().gap(8.0).child(label("Radio Buttons")).child(
                    Column::new()
                        .gap(6.0)
                        .child(RadioButton::new("light", &group).label("Light Theme"))
                        .child(RadioButton::new("dark", &group).label("Dark Theme"))
                        .child(RadioButton::new("system", &group).label("System Default")),
                )
            })
            .child(Column::new().gap(8.0).child(label("Option Group")).child(
                OptionGroup::new(vec![
                    OptionItem::new("pacman", "pacman").description("extra/zig 0.14.1, 48 МБ").badge("рекомендуется"),
                    OptionItem::new("aur", "AUR").description("zig-bin через yay"),
                    OptionItem::new("download", "Скачать").description("ziglang.org/download"),
                    OptionItem::new("none", "Недоступно").disabled(true),
                ])
                .selected("pacman")
                .on_change(|v| println!("OptionGroup: {v}")),
            ))
            // New: Multiselect
            .child(
                Column::new().gap(8.0).child(label("Multiselect")).child(
                    Multiselect::new(vec![
                        DropdownItem::simple("Apple"),
                        DropdownItem::simple("Banana"),
                        DropdownItem::simple("Cherry"),
                        DropdownItem::simple("Grape"),
                        DropdownItem::simple("Orange"),
                    ])
                    .selected(vec![0, 2])
                    .placeholder("Select fruits...")
                    .width(250.0),
                ),
            ),
    )
}
