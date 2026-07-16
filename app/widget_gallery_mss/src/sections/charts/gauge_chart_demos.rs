//! Gauge chart demo page: 6 examples showcasing different gauge styles.

use syngui::prelude::*;
use syngui::widgets::*;
use syngui::widgets::charts::GaugeSegment;

use crate::sections::{section_card, section_title, label};

/// Build all gauge chart demos.
pub fn build_gauge_chart_demos() -> impl Widget {
    Column::new()
        .gap(24.0)
        .child(section_card(build_basic_demo()))
        .child(section_card(build_segments_demo()))
        .child(section_card(build_speedometer_demo()))
        .child(section_card(build_temperature_demo()))
        .child(section_card(build_multi_gauge_demo()))
        .child(section_card(build_custom_angles_demo()))
}

// ─── 1. Basic Gauge ────────────────────────────────────────────────────────

fn build_basic_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Basic Gauge"))
        .child(label("Simple gauge showing a single value with default styling"))
        .child(
            Row::new()
                .gap(24.0)
                .child(
                    GaugeChart::new()
                        .value(72.0)
                        .title("Score")
                        .size(280.0, 280.0)
                        .class("gauge-chart"),
                )
                .child(
                    GaugeChart::new()
                        .value(38.0)
                        .title("Progress")
                        .format(|v| format!("{:.0}%", v))
                        .size(280.0, 280.0)
                        .class("gauge-chart"),
                ),
        )
}

// ─── 2. Colored Segments ───────────────────────────────────────────────────

fn build_segments_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Colored Segments"))
        .child(label("Gauge with color-coded zones: good (green), warning (yellow), danger (red)"))
        .child(
            Row::new()
                .gap(24.0)
                .child(
                    GaugeChart::new()
                        .value(65.0)
                        .segment(GaugeSegment::new(0.0, 40.0, Color::from_hex("#22c55e")))
                        .segment(GaugeSegment::new(40.0, 70.0, Color::from_hex("#f59e0b")))
                        .segment(GaugeSegment::new(70.0, 100.0, Color::from_hex("#ef4444")))
                        .title("Risk Level")
                        .size(300.0, 300.0)
                        .class("gauge-chart"),
                )
                .child(
                    GaugeChart::new()
                        .value(25.0)
                        .segment(GaugeSegment::new(0.0, 30.0, Color::from_hex("#ef4444")))
                        .segment(GaugeSegment::new(30.0, 60.0, Color::from_hex("#f59e0b")))
                        .segment(GaugeSegment::new(60.0, 100.0, Color::from_hex("#22c55e")))
                        .title("Battery")
                        .format(|v| format!("{:.0}%", v))
                        .size(300.0, 300.0)
                        .class("gauge-chart"),
                ),
        )
}

// ─── 3. Speedometer ────────────────────────────────────────────────────────

fn build_speedometer_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Speedometer"))
        .child(label("Automotive-style gauge with 0–220 km/h range and fine divisions"))
        .child(
            GaugeChart::new()
                .value(120.0)
                .min(0.0)
                .max(220.0)
                .tick_count(11)
                .minor_ticks(4)
                .segment(GaugeSegment::new(0.0, 60.0, Color::from_hex("#22c55e")))
                .segment(GaugeSegment::new(60.0, 120.0, Color::from_hex("#3b82f6")))
                .segment(GaugeSegment::new(120.0, 180.0, Color::from_hex("#f59e0b")))
                .segment(GaugeSegment::new(180.0, 220.0, Color::from_hex("#ef4444")))
                .format(|v| format!("{:.0}", v))
                .title("km/h")
                .size(350.0, 350.0)
                .class("gauge-chart"),
        )
}

// ─── 4. Temperature ────────────────────────────────────────────────────────

fn build_temperature_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Temperature Gauge"))
        .child(label("Temperature range from -20°C to +50°C with blue-to-red segments"))
        .child(
            GaugeChart::new()
                .value(22.5)
                .min(-20.0)
                .max(50.0)
                .tick_count(7)
                .minor_ticks(4)
                .segment(GaugeSegment::new(-20.0, 0.0, Color::from_hex("#3b82f6")))
                .segment(GaugeSegment::new(0.0, 15.0, Color::from_hex("#06b6d4")))
                .segment(GaugeSegment::new(15.0, 25.0, Color::from_hex("#22c55e")))
                .segment(GaugeSegment::new(25.0, 35.0, Color::from_hex("#f59e0b")))
                .segment(GaugeSegment::new(35.0, 50.0, Color::from_hex("#ef4444")))
                .format(|v| format!("{:.0}°", v))
                .title("°C")
                .size(320.0, 320.0)
                .class("gauge-chart"),
        )
}

// ─── 5. Multi-Gauge Dashboard ──────────────────────────────────────────────

fn build_multi_gauge_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Multi-Gauge Dashboard"))
        .child(label("Three small gauges in a row — CPU, Memory, Disk usage"))
        .child(
            Row::new()
                .gap(16.0)
                .child(
                    GaugeChart::new()
                        .value(67.0)
                        .segment(GaugeSegment::new(0.0, 60.0, Color::from_hex("#22c55e")))
                        .segment(GaugeSegment::new(60.0, 85.0, Color::from_hex("#f59e0b")))
                        .segment(GaugeSegment::new(85.0, 100.0, Color::from_hex("#ef4444")))
                        .format(|v| format!("{:.0}%", v))
                        .title("CPU")
                        .labels(false)
                        .tick_count(5)
                        .minor_ticks(0)
                        .size(200.0, 200.0)
                        .class("gauge-chart"),
                )
                .child(
                    GaugeChart::new()
                        .value(82.0)
                        .segment(GaugeSegment::new(0.0, 60.0, Color::from_hex("#22c55e")))
                        .segment(GaugeSegment::new(60.0, 85.0, Color::from_hex("#f59e0b")))
                        .segment(GaugeSegment::new(85.0, 100.0, Color::from_hex("#ef4444")))
                        .format(|v| format!("{:.0}%", v))
                        .title("Memory")
                        .labels(false)
                        .tick_count(5)
                        .minor_ticks(0)
                        .size(200.0, 200.0)
                        .class("gauge-chart"),
                )
                .child(
                    GaugeChart::new()
                        .value(45.0)
                        .segment(GaugeSegment::new(0.0, 60.0, Color::from_hex("#22c55e")))
                        .segment(GaugeSegment::new(60.0, 85.0, Color::from_hex("#f59e0b")))
                        .segment(GaugeSegment::new(85.0, 100.0, Color::from_hex("#ef4444")))
                        .format(|v| format!("{:.0}%", v))
                        .title("Disk")
                        .labels(false)
                        .tick_count(5)
                        .minor_ticks(0)
                        .size(200.0, 200.0)
                        .class("gauge-chart"),
                ),
        )
}

// ─── 6. Custom Angles ──────────────────────────────────────────────────────

fn build_custom_angles_demo() -> impl Widget {
    Column::new()
        .gap(12.0)
        .child(section_title("Custom Angles"))
        .child(label("Different arc spans: semi-circle (180°) and full 270° gauge"))
        .child(
            Row::new()
                .gap(24.0)
                .child(
                    GaugeChart::new()
                        .value(60.0)
                        .start_angle(180.0)
                        .end_angle(0.0)
                        .segment(GaugeSegment::new(0.0, 50.0, Color::from_hex("#3b82f6")))
                        .segment(GaugeSegment::new(50.0, 100.0, Color::from_hex("#8b5cf6")))
                        .format(|v| format!("{:.0}%", v))
                        .title("Half Circle")
                        .tick_count(5)
                        .minor_ticks(1)
                        .size(280.0, 200.0)
                        .class("gauge-chart"),
                )
                .child(
                    GaugeChart::new()
                        .value(85.0)
                        .start_angle(270.0)
                        .end_angle(-90.0)
                        .segment(GaugeSegment::new(0.0, 33.0, Color::from_hex("#06b6d4")))
                        .segment(GaugeSegment::new(33.0, 66.0, Color::from_hex("#8b5cf6")))
                        .segment(GaugeSegment::new(66.0, 100.0, Color::from_hex("#ec4899")))
                        .format(|v| format!("{:.0}%", v))
                        .title("Full 360°")
                        .tick_count(10)
                        .minor_ticks(1)
                        .size(280.0, 280.0)
                        .class("gauge-chart"),
                ),
        )
}
