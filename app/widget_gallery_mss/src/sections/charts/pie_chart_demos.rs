//! Pie chart demo page: 5 examples showcasing different pie/donut chart features.

use syngui::prelude::*;
use syngui::widgets::*;
use syngui::widgets::charts::{PieSlice, PieLabelPosition, LegendPosition};

use crate::sections::{section_card, section_title, label};

/// Build all pie chart demos.
pub fn build_pie_chart_demos() -> impl Widget {
    Column::new()
        .gap(24.0)
        .child(section_card(build_basic_demo()))
        .child(section_card(build_donut_demo()))
        .child(section_card(build_labels_demo()))
        .child(section_card(build_interactive_demo()))
        .child(section_card(build_multi_donut_demo()))
}

// ─── 1. Basic Pie ──────────────────────────────────────────────────────────

fn build_basic_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Basic Pie Chart"))
        .child(label("Simple pie chart with 5 slices and auto-assigned colors"))
        .child(
            PieChart::new()
                .title("Market Share")
                .slice(PieSlice::new("Chrome", 65.0))
                .slice(PieSlice::new("Safari", 18.0))
                .slice(PieSlice::new("Firefox", 8.0))
                .slice(PieSlice::new("Edge", 5.0))
                .slice(PieSlice::new("Other", 4.0))
                .legend(LegendPosition::Bottom)
                .animate(true)
                .size(400.0, 400.0)
                .class("pie-chart"),
        )
}

// ─── 2. Donut Chart ────────────────────────────────────────────────────────

fn build_donut_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Donut Chart"))
        .child(label("Pie chart with inner radius creating a donut shape"))
        .child(
            Row::new()
                .gap(24.0)
                .child(
                    PieChart::new()
                        .title("Revenue")
                        .slice(PieSlice::new("Product A", 42.0).color(Color::from_hex("#5470c6")))
                        .slice(PieSlice::new("Product B", 28.0).color(Color::from_hex("#91cc75")))
                        .slice(PieSlice::new("Product C", 18.0).color(Color::from_hex("#fac858")))
                        .slice(PieSlice::new("Product D", 12.0).color(Color::from_hex("#ee6666")))
                        .donut(0.55)
                        .legend(LegendPosition::Bottom)
                        .animate(true)
                        .size(350.0, 380.0)
                        .class("pie-chart"),
                )
                .child(
                    PieChart::new()
                        .title("Expenses")
                        .slice(PieSlice::new("Salaries", 55.0).color(Color::from_hex("#73c0de")))
                        .slice(PieSlice::new("R&D", 25.0).color(Color::from_hex("#3ba272")))
                        .slice(PieSlice::new("Marketing", 12.0).color(Color::from_hex("#fc8452")))
                        .slice(PieSlice::new("Other", 8.0).color(Color::from_hex("#9a60b4")))
                        .donut(0.55)
                        .legend(LegendPosition::Bottom)
                        .animate(true)
                        .size(350.0, 380.0)
                        .class("pie-chart"),
                ),
        )
}

// ─── 3. Labels & Percentages ───────────────────────────────────────────────

fn build_labels_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Labels & Percentages"))
        .child(label("Outside labels with percentage display and leader lines"))
        .child(
            Row::new()
                .gap(24.0)
                .child(
                    PieChart::new()
                        .title("Inside Labels")
                        .slice(PieSlice::new("Direct", 335.0))
                        .slice(PieSlice::new("Email", 310.0))
                        .slice(PieSlice::new("Affiliate", 234.0))
                        .slice(PieSlice::new("Video", 135.0))
                        .slice(PieSlice::new("Search", 1548.0))
                        .label_position(PieLabelPosition::Inside)
                        .show_percentage(true)
                        .legend(LegendPosition::Bottom)
                        .animate(true)
                        .size(380.0, 400.0)
                        .class("pie-chart"),
                )
                .child(
                    PieChart::new()
                        .title("Outside Labels")
                        .slice(PieSlice::new("Direct", 335.0))
                        .slice(PieSlice::new("Email", 310.0))
                        .slice(PieSlice::new("Affiliate", 234.0))
                        .slice(PieSlice::new("Video", 135.0))
                        .slice(PieSlice::new("Search", 1548.0))
                        .label_position(PieLabelPosition::Outside)
                        .show_percentage(true)
                        .legend(LegendPosition::None)
                        .animate(true)
                        .size(380.0, 400.0)
                        .class("pie-chart"),
                ),
        )
}

// ─── 4. Interactive ────────────────────────────────────────────────────────

fn build_interactive_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Interactive Pie"))
        .child(label("Hover to explode slices, legend click to toggle visibility"))
        .child(
            PieChart::new()
                .title("Traffic Sources")
                .slice(PieSlice::new("Organic Search", 40.0).color(Color::from_hex("#5470c6")))
                .slice(PieSlice::new("Direct", 25.0).color(Color::from_hex("#91cc75")))
                .slice(PieSlice::new("Social", 15.0).color(Color::from_hex("#fac858")))
                .slice(PieSlice::new("Referral", 12.0).color(Color::from_hex("#ee6666")))
                .slice(PieSlice::new("Email", 8.0).color(Color::from_hex("#73c0de")))
                .donut(0.0)
                .show_percentage(true)
                .label_position(PieLabelPosition::Outside)
                .legend(LegendPosition::Bottom)
                .tooltip(true)
                .animate(true)
                .size(450.0, 430.0)
                .class("pie-chart"),
        )
}

// ─── 5. Multi-Donut Comparison ─────────────────────────────────────────────

fn build_multi_donut_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Multi-Donut Comparison"))
        .child(label("Multiple small donut charts for dashboard-style comparisons"))
        .child(
            Row::new()
                .gap(16.0)
                .child(
                    PieChart::new()
                        .title("Q1")
                        .slice(PieSlice::new("Won", 72.0).color(Color::from_hex("#91cc75")))
                        .slice(PieSlice::new("Lost", 28.0).color(Color::from_hex("#ee6666")))
                        .donut(0.65)
                        .label_position(PieLabelPosition::None)
                        .legend(LegendPosition::None)
                        .animate(true)
                        .size(200.0, 220.0)
                        .class("pie-chart"),
                )
                .child(
                    PieChart::new()
                        .title("Q2")
                        .slice(PieSlice::new("Won", 58.0).color(Color::from_hex("#91cc75")))
                        .slice(PieSlice::new("Lost", 42.0).color(Color::from_hex("#ee6666")))
                        .donut(0.65)
                        .label_position(PieLabelPosition::None)
                        .legend(LegendPosition::None)
                        .animate(true)
                        .size(200.0, 220.0)
                        .class("pie-chart"),
                )
                .child(
                    PieChart::new()
                        .title("Q3")
                        .slice(PieSlice::new("Won", 85.0).color(Color::from_hex("#91cc75")))
                        .slice(PieSlice::new("Lost", 15.0).color(Color::from_hex("#ee6666")))
                        .donut(0.65)
                        .label_position(PieLabelPosition::None)
                        .legend(LegendPosition::None)
                        .animate(true)
                        .size(200.0, 220.0)
                        .class("pie-chart"),
                ),
        )
}
