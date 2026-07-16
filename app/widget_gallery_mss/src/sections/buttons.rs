use syngui::prelude::*;
use syngui::widgets::*;

use super::{label, section_card, section_title};

pub fn build_buttons_section() -> impl Widget {
    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Buttons"))
            .child(
                Column::new().gap(8.0).child(label("Button Styles")).child(
                    Row::new()
                        .gap(8.0)
                        .child(Button::new("Primary").class("primary"))
                        .child(Button::new("Secondary").class("secondary"))
                        .child(Button::new("Danger").class("danger"))
                        .child(Button::new("Text").class("text")),
                ),
            )
            .child(
                Column::new().gap(8.0).child(label("Disabled State")).child(
                    Row::new()
                        .gap(8.0)
                        .child(Button::new("Disabled Primary").class("primary").disabled(true))
                        .child(Button::new("Disabled Secondary").class("secondary").disabled(true)),
                ),
            )
            .child(
                Column::new().gap(8.0).child(label("Tool Buttons")).child(
                    Row::new()
                        .gap(4.0)
                        .child(ToolButton::new("🔍"))
                        .child(ToolButton::new("⚙"))
                        .child(ToolButton::new("🔔"))
                        .child(ToolButton::new("👤"))
                        .child(ToolButton::new("📁"))
                        .child(ToolButton::new("💾")),
                ),
            )
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Option Buttons (Toggle)"))
                    .child(
                        Row::new()
                            .gap(4.0)
                            .child(OptionButton::new("Bold").icon("B"))
                            .child(OptionButton::new("Italic").icon("I"))
                            .child(OptionButton::new("Underline").icon("U")),
                    ),
            )
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Segmented Button"))
                    .child(
                        SegmentedButton::new(vec!["Day", "Week", "Month"])
                            .selected(1),
                    ),
            ),
    )
}
