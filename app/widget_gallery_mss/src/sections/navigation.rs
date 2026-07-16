use syngui::prelude::*;
use syngui::widgets::*;
use syngui::signal::use_signal;

use super::{label, section_card, section_title};

pub fn build_navigation_section() -> impl Widget {
    let tab_state = use_signal(0usize);

    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Navigation"))
            .child(
                Column::new().gap(8.0).child(label("Tab Bar")).child(
                    TabBar::new()
                        .tab(Tab::new("Dashboard", 0, &tab_state).icon("📊"))
                        .tab(Tab::new("Settings", 1, &tab_state).icon("⚙"))
                        .tab(Tab::new("Profile", 2, &tab_state).icon("👤"))
                        .tab(Tab::new("Help", 3, &tab_state)),
                ),
            )
            .child(
                Column::new().gap(8.0).child(label("Toolbar")).child(
                    Toolbar::with_title("File Editor")
                        .child(ToolButton::new("📁"))
                        .child(ToolButton::new("💾"))
                        .child(ToolButton::new("✂"))
                        .child(ToolButton::new("📋"))
                        .child(ToolButton::new("↩")),
                ),
            )
            .child(
                Column::new().gap(8.0).child(label("Breadcrumb")).child(
                    Breadcrumb::new()
                        .item("Home")
                        .item("Documents")
                        .item("Projects")
                        .item("SYNGUI")
                        .separator(">"),
                ),
            )
            .child({
                // TabView = TabBar + ShowIf composition. Dedicated TabView
                // widget was removed; use the pattern below for tabbed content.
                let selected = use_signal(0usize);
                Column::new().gap(8.0).child(label("Tab View")).child(
                    Column::new()
                        .gap(8.0)
                        .child(
                            TabBar::new()
                                .tab(Tab::new("Overview", 0, &selected))
                                .tab(Tab::new("Details", 1, &selected))
                                .tab(Tab::new("Settings", 2, &selected)),
                        )
                        .child(
                            ShowIf::new(0, selected).child(
                                Text::new("Welcome to SYNGUI!")
                                    .class("text-primary")
                                    .class("tab-text"),
                            ),
                        )
                        .child(
                            ShowIf::new(1, selected).child(
                                Text::new("Detailed information here.")
                                    .class("text-primary")
                                    .class("tab-text"),
                            ),
                        )
                        .child(
                            ShowIf::new(2, selected).child(
                                Text::new("Configuration panel.")
                                    .class("text-primary")
                                    .class("tab-text"),
                            ),
                        ),
                )
            })
            // New: Pagination
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Pagination"))
                    .child(Pagination::new(10, 3).max_visible(7)),
            )
            // ── Stepper variants ───────────────────────────────────
            .child(
                Column::new().gap(8.0)
                    .child(label("Stepper — Pill"))
                    .child(
                        Stepper::new()
                            .step("Business type", Some("Supporting text"))
                            .step("Business Detail", Some("Supporting text"))
                            .step("Your details", Some("Supporting text"))
                            .step("Verification", Some("Supporting text"))
                            .current(1)
                            .class("pill"),
                    ),
            )
            .child(
                Column::new().gap(8.0)
                    .child(label("Stepper — Radio"))
                    .child(
                        Stepper::new()
                            .step("Business type", None)
                            .step("Business detail", None)
                            .step("Your Details", None)
                            .step("Bank details", None)
                            .step("Statement", None)
                            .step("Verification", None)
                            .current(1)
                            .class("radio"),
                    ),
            )
            .child(
                Column::new().gap(8.0)
                    .child(label("Stepper — Numbered"))
                    .child(
                        Stepper::new()
                            .step("Business type", Some("Support Text"))
                            .step("Business details", Some("Support Text"))
                            .step("Your details", Some("Support Text"))
                            .step("Bank details", Some("Support Text"))
                            .step("Statement", Some("Support Text"))
                            .current(1)
                            .class("numbered"),
                    ),
            )
            .child(
                Column::new().gap(8.0)
                    .child(label("Stepper — Icon"))
                    .child(
                        Stepper::new()
                            .step_with_icon("Personal info", "📋", Some("Support text"))
                            .step_with_icon("Social accounts", "🔗", Some("Support text"))
                            .step_with_icon("Integrations", "⚙", Some("Support text"))
                            .step_with_icon("Payment info", "💳", Some("Support text"))
                            .current(1)
                            .class("icon"),
                    ),
            )
            .child(
                Column::new().gap(8.0)
                    .child(label("Stepper — Status"))
                    .child(
                        Stepper::new()
                            .step_with_status("Step 1", "Personal info", "Completed")
                            .step_with_status("Step 2", "Social accounts", "In Progress")
                            .step_with_status("Step 3", "Payment info", "Pending")
                            .step_with_status("Step 4", "Payment info", "Pending")
                            .current(1)
                            .class("status"),
                    ),
            ),
    )
}
