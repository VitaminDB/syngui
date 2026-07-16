//! Radar chart demo page: 5 examples showcasing different radar chart features.

use syngui::prelude::*;
use syngui::widgets::*;
use syngui::widgets::charts::{RadarIndicator, RadarSeries, RadarGridShape, LegendPosition};

use crate::sections::{section_card, section_title, label};

/// Build all radar chart demos.
pub fn build_radar_chart_demos() -> impl Widget {
    Column::new()
        .gap(24.0)
        .child(section_card(build_basic_demo()))
        .child(section_card(build_multi_series_demo()))
        .child(section_card(build_grid_shape_demo()))
        .child(section_card(build_custom_indicators_demo()))
        .child(section_card(build_styled_demo()))
}

// ─── 1. Basic Radar ────────────────────────────────────────────────────────

fn build_basic_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Basic Radar Chart"))
        .child(label("Single series with 5 indicators"))
        .child(
            RadarChart::new()
                .title("Skill Assessment")
                .indicator(RadarIndicator::new("Attack", 100.0))
                .indicator(RadarIndicator::new("Defense", 100.0))
                .indicator(RadarIndicator::new("Speed", 100.0))
                .indicator(RadarIndicator::new("Stamina", 100.0))
                .indicator(RadarIndicator::new("Technique", 100.0))
                .radar_series(
                    RadarSeries::new("Player A", vec![85.0, 70.0, 92.0, 65.0, 78.0])
                        .color(Color::from_hex("#5470c6"))
                        .area_opacity(0.3),
                )
                .legend(LegendPosition::None)
                .animate(true)
                .size(450.0, 420.0)
                .class("radar-chart"),
        )
}

// ─── 2. Multi-Series ───────────────────────────────────────────────────────

fn build_multi_series_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Multi-Series Radar"))
        .child(label("Comparing two players across 6 dimensions"))
        .child(
            RadarChart::new()
                .title("Player Comparison")
                .indicator(RadarIndicator::new("Attack", 100.0))
                .indicator(RadarIndicator::new("Defense", 100.0))
                .indicator(RadarIndicator::new("Speed", 100.0))
                .indicator(RadarIndicator::new("Stamina", 100.0))
                .indicator(RadarIndicator::new("Technique", 100.0))
                .indicator(RadarIndicator::new("Intelligence", 100.0))
                .radar_series(
                    RadarSeries::new("Player A", vec![85.0, 70.0, 92.0, 65.0, 78.0, 88.0])
                        .color(Color::from_hex("#5470c6"))
                        .area_opacity(0.2),
                )
                .radar_series(
                    RadarSeries::new("Player B", vec![72.0, 88.0, 68.0, 90.0, 85.0, 62.0])
                        .color(Color::from_hex("#ee6666"))
                        .area_opacity(0.2),
                )
                .legend(LegendPosition::Bottom)
                .tooltip(true)
                .animate(true)
                .size(500.0, 470.0)
                .class("radar-chart"),
        )
}

// ─── 3. Circle vs Polygon Grid ─────────────────────────────────────────────

fn build_grid_shape_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Grid Shape Comparison"))
        .child(label("Polygon grid (default) vs circle grid"))
        .child(
            Row::new()
                .gap(24.0)
                .child(
                    RadarChart::new()
                        .title("Polygon Grid")
                        .indicator(RadarIndicator::new("Str", 100.0))
                        .indicator(RadarIndicator::new("Dex", 100.0))
                        .indicator(RadarIndicator::new("Con", 100.0))
                        .indicator(RadarIndicator::new("Int", 100.0))
                        .indicator(RadarIndicator::new("Wis", 100.0))
                        .indicator(RadarIndicator::new("Cha", 100.0))
                        .radar_series(
                            RadarSeries::new("Character", vec![75.0, 90.0, 60.0, 85.0, 50.0, 70.0])
                                .color(Color::from_hex("#91cc75"))
                                .area_opacity(0.25),
                        )
                        .grid_shape(RadarGridShape::Polygon)
                        .legend(LegendPosition::None)
                        .animate(true)
                        .size(350.0, 350.0)
                        .class("radar-chart"),
                )
                .child(
                    RadarChart::new()
                        .title("Circle Grid")
                        .indicator(RadarIndicator::new("Str", 100.0))
                        .indicator(RadarIndicator::new("Dex", 100.0))
                        .indicator(RadarIndicator::new("Con", 100.0))
                        .indicator(RadarIndicator::new("Int", 100.0))
                        .indicator(RadarIndicator::new("Wis", 100.0))
                        .indicator(RadarIndicator::new("Cha", 100.0))
                        .radar_series(
                            RadarSeries::new("Character", vec![75.0, 90.0, 60.0, 85.0, 50.0, 70.0])
                                .color(Color::from_hex("#91cc75"))
                                .area_opacity(0.25),
                        )
                        .grid_shape(RadarGridShape::Circle)
                        .legend(LegendPosition::None)
                        .animate(true)
                        .size(350.0, 350.0)
                        .class("radar-chart"),
                ),
        )
}

// ─── 4. Custom Indicators ──────────────────────────────────────────────────

fn build_custom_indicators_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Custom Indicator Ranges"))
        .child(label("Different max values per indicator for real-world metrics"))
        .child(
            RadarChart::new()
                .title("Product Evaluation")
                .indicator(RadarIndicator::new("Price ($)", 500.0))
                .indicator(RadarIndicator::new("Quality", 10.0))
                .indicator(RadarIndicator::new("Durability (yrs)", 20.0))
                .indicator(RadarIndicator::new("Design", 10.0))
                .indicator(RadarIndicator::new("Performance", 100.0))
                .radar_series(
                    RadarSeries::new("Product X", vec![350.0, 8.5, 15.0, 7.0, 85.0])
                        .color(Color::from_hex("#5470c6"))
                        .area_opacity(0.15),
                )
                .radar_series(
                    RadarSeries::new("Product Y", vec![200.0, 6.0, 8.0, 9.5, 65.0])
                        .color(Color::from_hex("#fac858"))
                        .area_opacity(0.15),
                )
                .radar_series(
                    RadarSeries::new("Product Z", vec![450.0, 9.0, 18.0, 5.0, 92.0])
                        .color(Color::from_hex("#ee6666"))
                        .area_opacity(0.15),
                )
                .grid_levels(5)
                .legend(LegendPosition::Bottom)
                .tooltip(true)
                .animate(true)
                .size(500.0, 470.0)
                .class("radar-chart"),
        )
}

// ─── 5. Styled Radar ───────────────────────────────────────────────────────

fn build_styled_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Styled Radar Chart"))
        .child(label("Different area opacities and line widths for emphasis"))
        .child(
            RadarChart::new()
                .title("Team Capabilities")
                .indicator(RadarIndicator::new("Frontend", 100.0))
                .indicator(RadarIndicator::new("Backend", 100.0))
                .indicator(RadarIndicator::new("DevOps", 100.0))
                .indicator(RadarIndicator::new("Design", 100.0))
                .indicator(RadarIndicator::new("Testing", 100.0))
                .indicator(RadarIndicator::new("PM", 100.0))
                .indicator(RadarIndicator::new("Data", 100.0))
                .radar_series(
                    RadarSeries::new("Team Alpha", vec![90.0, 85.0, 70.0, 60.0, 75.0, 80.0, 65.0])
                        .color(Color::from_hex("#5470c6"))
                        .area_opacity(0.35)
                        .line_width(3.0),
                )
                .radar_series(
                    RadarSeries::new("Team Beta", vec![60.0, 70.0, 90.0, 85.0, 80.0, 55.0, 90.0])
                        .color(Color::from_hex("#91cc75"))
                        .area_opacity(0.15)
                        .line_width(2.0),
                )
                .grid_levels(4)
                .legend(LegendPosition::Bottom)
                .tooltip(true)
                .animate(true)
                .size(500.0, 470.0)
                .class("radar-chart"),
        )
}
