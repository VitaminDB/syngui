//! Bar chart demo page: 6 examples showcasing different bar chart features.

use syngui::prelude::*;
use syngui::widgets::*;
use syngui::widgets::charts::{AxisConfig, BarSeries, BarMode, BarOrientation, LegendPosition};

use crate::sections::{section_card, section_title, label};

/// Build all bar chart demos.
pub fn build_bar_chart_demos() -> impl Widget {
    Column::new()
        .gap(24.0)
        .child(section_card(build_basic_demo()))
        .child(section_card(build_grouped_demo()))
        .child(section_card(build_stacked_demo()))
        .child(section_card(build_horizontal_demo()))
        .child(section_card(build_value_labels_demo()))
        .child(section_card(build_styled_demo()))
}

// ─── 1. Basic Vertical Bars ────────────────────────────────────────────────

fn build_basic_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Basic Bar Chart"))
        .child(label("Single series with 6 categories"))
        .child(
            BarChart::new()
                .title("Monthly Revenue")
                .categories(vec![
                    "Jan".to_string(), "Feb".to_string(), "Mar".to_string(),
                    "Apr".to_string(), "May".to_string(), "Jun".to_string(),
                ])
                .bar_series(
                    BarSeries::new("Revenue", vec![120.0, 200.0, 150.0, 80.0, 230.0, 180.0])
                        .color(Color::from_hex("#5470c6")),
                )
                .y_axis(AxisConfig::new().title("$ (thousands)"))
                .legend(LegendPosition::None)
                .animate(true)
                .size(700.0, 400.0)
                .class("bar-chart"),
        )
}

// ─── 2. Grouped Bars ───────────────────────────────────────────────────────

fn build_grouped_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Grouped Bar Chart"))
        .child(label("Multiple series grouped side by side per category"))
        .child(
            BarChart::new()
                .title("Quarterly Sales by Region")
                .categories(vec![
                    "Q1".to_string(), "Q2".to_string(), "Q3".to_string(), "Q4".to_string(),
                ])
                .bar_series(
                    BarSeries::new("North", vec![320.0, 410.0, 380.0, 490.0])
                        .color(Color::from_hex("#5470c6")),
                )
                .bar_series(
                    BarSeries::new("South", vec![220.0, 340.0, 290.0, 380.0])
                        .color(Color::from_hex("#91cc75")),
                )
                .bar_series(
                    BarSeries::new("West", vec![180.0, 250.0, 310.0, 270.0])
                        .color(Color::from_hex("#fac858")),
                )
                .mode(BarMode::Grouped)
                .y_axis(AxisConfig::new().title("Sales ($K)"))
                .legend(LegendPosition::Top)
                .tooltip(true)
                .animate(true)
                .size(700.0, 400.0)
                .class("bar-chart"),
        )
}

// ─── 3. Stacked Bars ───────────────────────────────────────────────────────

fn build_stacked_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Stacked Bar Chart"))
        .child(label("Same data as grouped, stacked to show totals"))
        .child(
            BarChart::new()
                .title("Quarterly Sales by Region (Stacked)")
                .categories(vec![
                    "Q1".to_string(), "Q2".to_string(), "Q3".to_string(), "Q4".to_string(),
                ])
                .bar_series(
                    BarSeries::new("North", vec![320.0, 410.0, 380.0, 490.0])
                        .color(Color::from_hex("#5470c6")),
                )
                .bar_series(
                    BarSeries::new("South", vec![220.0, 340.0, 290.0, 380.0])
                        .color(Color::from_hex("#91cc75")),
                )
                .bar_series(
                    BarSeries::new("West", vec![180.0, 250.0, 310.0, 270.0])
                        .color(Color::from_hex("#fac858")),
                )
                .mode(BarMode::Stacked)
                .y_axis(AxisConfig::new().title("Total Sales ($K)"))
                .legend(LegendPosition::Top)
                .tooltip(true)
                .animate(true)
                .size(700.0, 400.0)
                .class("bar-chart"),
        )
}

// ─── 4. Horizontal Bars ────────────────────────────────────────────────────

fn build_horizontal_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Horizontal Bar Chart"))
        .child(label("Horizontal orientation for long category names"))
        .child(
            BarChart::new()
                .title("Programming Language Popularity")
                .categories(vec![
                    "Python".to_string(), "JavaScript".to_string(), "Java".to_string(),
                    "C++".to_string(), "Rust".to_string(), "Go".to_string(),
                    "TypeScript".to_string(),
                ])
                .bar_series(
                    BarSeries::new("Popularity %", vec![28.0, 22.0, 16.0, 12.0, 8.0, 7.0, 7.0])
                        .color(Color::from_hex("#73c0de")),
                )
                .orientation(BarOrientation::Horizontal)
                .x_axis(AxisConfig::new().title("Popularity (%)"))
                .legend(LegendPosition::None)
                .animate(true)
                .size(700.0, 400.0)
                .class("bar-chart"),
        )
}

// ─── 5. Value Labels ───────────────────────────────────────────────────────

fn build_value_labels_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Value Labels on Bars"))
        .child(label("Showing exact values above each bar"))
        .child(
            BarChart::new()
                .title("Team Performance")
                .categories(vec![
                    "Alice".to_string(), "Bob".to_string(), "Carol".to_string(),
                    "Dave".to_string(), "Eve".to_string(),
                ])
                .bar_series(
                    BarSeries::new("Tasks Completed", vec![45.0, 38.0, 52.0, 31.0, 47.0])
                        .color(Color::from_hex("#3ba272")),
                )
                .value_labels(true)
                .y_axis(AxisConfig::new().min(0.0))
                .legend(LegendPosition::None)
                .animate(true)
                .size(600.0, 380.0)
                .class("bar-chart"),
        )
}

// ─── 6. Styled Bars ────────────────────────────────────────────────────────

fn build_styled_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Styled Bar Chart"))
        .child(label("Custom bar radius, colors, and axis formatting"))
        .child(
            BarChart::new()
                .title("Website Traffic (thousands)")
                .categories(vec![
                    "Mon".to_string(), "Tue".to_string(), "Wed".to_string(),
                    "Thu".to_string(), "Fri".to_string(), "Sat".to_string(), "Sun".to_string(),
                ])
                .bar_series(
                    BarSeries::new("Visitors", vec![5.2, 7.8, 6.5, 8.1, 9.3, 12.5, 10.2])
                        .color(Color::from_hex("#9a60b4")),
                )
                .bar_series(
                    BarSeries::new("Page Views", vec![15.6, 23.4, 19.5, 24.3, 27.9, 37.5, 30.6])
                        .color(Color::from_hex("#ea7ccc")),
                )
                .mode(BarMode::Grouped)
                .bar_radius(4.0)
                .y_axis(AxisConfig::new().format(|v| format!("{:.0}K", v)))
                .legend(LegendPosition::Top)
                .tooltip(true)
                .animate(true)
                .size(700.0, 400.0)
                .class("bar-chart"),
        )
}
