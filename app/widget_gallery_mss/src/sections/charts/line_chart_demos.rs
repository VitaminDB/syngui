//! Line chart demo page: 8 examples showcasing different features.

use syngui::prelude::*;
use syngui::widgets::*;
use syngui::widgets::charts::{AxisConfig, Series, LegendPosition, VisualMapPiece};

use crate::sections::{section_card, section_title, label};

/// Build all line chart demos.
pub fn build_line_chart_demos() -> impl Widget {
    Column::new()
        .gap(24.0)
        .child(section_card(build_basic_demo()))
        .child(section_card(build_multi_series_demo()))
        .child(section_card(build_smooth_vs_straight_demo()))
        .child(section_card(build_area_chart_demo()))
        .child(section_card(build_line_styles_demo()))
        .child(section_card(build_interactive_demo()))
        .child(section_card(build_large_dataset_demo()))
        .child(section_card(build_custom_format_demo()))
        .child(section_card(build_rainfall_evaporation_demo()))
        .child(section_card(build_beijing_aqi_demo()))
}

// ─── 1. Basic Line Chart ───────────────────────────────────────────────────

fn build_basic_demo() -> impl Widget {
    let data: Vec<(f64, f64)> = vec![
        (1.0, 820.0), (2.0, 932.0), (3.0, 901.0), (4.0, 934.0),
        (5.0, 1290.0), (6.0, 1330.0), (7.0, 1320.0), (8.0, 1520.0),
        (9.0, 1210.0), (10.0, 1150.0), (11.0, 1320.0), (12.0, 1480.0),
    ];

    Column::new()
        .gap(12.0)
        .child(section_title("Basic Line Chart"))
        .child(label("Single series with 12 data points, auto-scaled axes"))
        .child(
            LineChart::new()
                .title("Monthly Sales")
                .series(
                    Series::new("Sales")
                        .data(data)
                        .color(Color::from_hex("#3b82f6"))
                        .point_size(5.0),
                )
                .x_axis(AxisConfig::new().title("Month").grid(true))
                .y_axis(AxisConfig::new().title("Units"))
                .legend(LegendPosition::None)
                .size(700.0, 350.0)
                .class("line-chart"),
        )
}

// ─── 2. Multi-Series ───────────────────────────────────────────────────────

fn build_multi_series_demo() -> impl Widget {
    let months: Vec<f64> = (1..=12).map(|x| x as f64).collect();

    let revenue: Vec<(f64, f64)> = months.iter().zip(
        [150.0, 230.0, 224.0, 218.0, 335.0, 447.0, 510.0, 520.0, 601.0, 580.0, 620.0, 690.0].iter()
    ).map(|(&x, &y)| (x, y)).collect();

    let costs: Vec<(f64, f64)> = months.iter().zip(
        [120.0, 160.0, 190.0, 200.0, 220.0, 280.0, 310.0, 350.0, 360.0, 380.0, 410.0, 440.0].iter()
    ).map(|(&x, &y)| (x, y)).collect();

    let profit: Vec<(f64, f64)> = revenue.iter().zip(costs.iter())
        .map(|((x, r), (_, c))| (*x, r - c))
        .collect();

    Column::new()
        .gap(12.0)
        .child(section_title("Multi-Series Chart"))
        .child(label("Three series with interactive legend (click to toggle)"))
        .child(
            LineChart::new()
                .title("Financial Overview")
                .series(
                    Series::new("Revenue")
                        .data(revenue)
                        .color(Color::from_hex("#22c55e"))
                        .smooth(true)
                        .line_width(2.5),
                )
                .series(
                    Series::new("Costs")
                        .data(costs)
                        .color(Color::from_hex("#ef4444"))
                        .smooth(true)
                        .line_width(2.5),
                )
                .series(
                    Series::new("Profit")
                        .data(profit)
                        .color(Color::from_hex("#3b82f6"))
                        .smooth(true)
                        .line_width(2.5),
                )
                .x_axis(
                    AxisConfig::new()
                        .title("Month")
                        .format(|v| {
                            let names = ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun",
                                         "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
                            names.get(v as usize).unwrap_or(&"").to_string()
                        }),
                )
                .y_axis(AxisConfig::new().title("$ (thousands)"))
                .legend(LegendPosition::Bottom)
                .tooltip(true)
                .size(700.0, 400.0)
                .class("line-chart"),
        )
}

// ─── 3. Smooth vs Straight ─────────────────────────────────────────────────

fn build_smooth_vs_straight_demo() -> impl Widget {
    let data: Vec<(f64, f64)> = vec![
        (0.0, 5.0), (1.0, 20.0), (2.0, 36.0), (3.0, 10.0),
        (4.0, 30.0), (5.0, 40.0), (6.0, 15.0), (7.0, 35.0),
    ];

    Column::new()
        .gap(12.0)
        .child(section_title("Smooth vs Straight Lines"))
        .child(label("Comparison of Catmull-Rom smooth interpolation vs straight segments"))
        .child(
            Row::new()
                .gap(16.0)
                .child(
                    LineChart::new()
                        .title("Straight")
                        .series(
                            Series::new("Data")
                                .data(data.clone())
                                .color(Color::from_hex("#8b5cf6"))
                                .smooth(false)
                                .point_size(5.0),
                        )
                        .legend(LegendPosition::None)
                        .size(340.0, 280.0)
                        .class("line-chart"),
                )
                .child(
                    LineChart::new()
                        .title("Smooth")
                        .series(
                            Series::new("Data")
                                .data(data)
                                .color(Color::from_hex("#8b5cf6"))
                                .smooth(true)
                                .point_size(5.0),
                        )
                        .legend(LegendPosition::None)
                        .size(340.0, 280.0)
                        .class("line-chart"),
                ),
        )
}

// ─── 4. Area Chart ─────────────────────────────────────────────────────────

fn build_area_chart_demo() -> impl Widget {
    let data1: Vec<(f64, f64)> = vec![
        (1.0, 120.0), (2.0, 200.0), (3.0, 150.0), (4.0, 80.0),
        (5.0, 170.0), (6.0, 220.0), (7.0, 190.0), (8.0, 230.0),
    ];
    let data2: Vec<(f64, f64)> = vec![
        (1.0, 60.0), (2.0, 90.0), (3.0, 120.0), (4.0, 50.0),
        (5.0, 80.0), (6.0, 110.0), (7.0, 150.0), (8.0, 130.0),
    ];

    Column::new()
        .gap(12.0)
        .child(section_title("Area Chart"))
        .child(label("Lines with semi-transparent area fill"))
        .child(
            LineChart::new()
                .title("Network Traffic")
                .series(
                    Series::new("Download")
                        .data(data1)
                        .color(Color::from_hex("#06b6d4"))
                        .smooth(true)
                        .area_fill(0.15)
                        .show_points(false)
                        .line_width(2.5),
                )
                .series(
                    Series::new("Upload")
                        .data(data2)
                        .color(Color::from_hex("#f59e0b"))
                        .smooth(true)
                        .area_fill(0.15)
                        .show_points(false)
                        .line_width(2.5),
                )
                .x_axis(AxisConfig::new().title("Hour"))
                .y_axis(AxisConfig::new().title("Mbps"))
                .legend(LegendPosition::Bottom)
                .tooltip(true)
                .size(700.0, 350.0)
                .class("line-chart"),
        )
}

// ─── 5. Line Styles ────────────────────────────────────────────────────────

fn build_line_styles_demo() -> impl Widget {
    let data1: Vec<(f64, f64)> = (0..10).map(|x| (x as f64, (x as f64 * 0.8).sin() * 30.0 + 50.0)).collect();
    let data2: Vec<(f64, f64)> = (0..10).map(|x| (x as f64, (x as f64 * 0.8 + 1.0).sin() * 25.0 + 45.0)).collect();
    let data3: Vec<(f64, f64)> = (0..10).map(|x| (x as f64, (x as f64 * 0.8 + 2.0).sin() * 20.0 + 40.0)).collect();

    Column::new()
        .gap(12.0)
        .child(section_title("Line Styles"))
        .child(label("Solid, dashed, and dotted lines"))
        .child(
            LineChart::new()
                .title("Line Styles")
                .series(
                    Series::new("Solid")
                        .data(data1)
                        .color(Color::from_hex("#3b82f6"))
                        .line_width(2.5),
                )
                .series(
                    Series::new("Dashed")
                        .data(data2)
                        .color(Color::from_hex("#ef4444"))
                        .dashed()
                        .line_width(2.5),
                )
                .series(
                    Series::new("Dotted")
                        .data(data3)
                        .color(Color::from_hex("#22c55e"))
                        .dotted()
                        .line_width(2.5),
                )
                .legend(LegendPosition::Bottom)
                .tooltip(true)
                .size(700.0, 350.0)
                .class("line-chart"),
        )
}

// ─── 6. Interactive (Zoom/Pan) ─────────────────────────────────────────────

fn build_interactive_demo() -> impl Widget {
    let data: Vec<(f64, f64)> = (0..50)
        .map(|x| {
            let xf = x as f64 * 0.3;
            (xf, (xf * 0.5).sin() * 30.0 + xf * 2.0 + 10.0)
        })
        .collect();

    Column::new()
        .gap(12.0)
        .child(section_title("Interactive Chart (Zoom & Pan)"))
        .child(label("Scroll to zoom, drag to pan. 50 data points with trend."))
        .child(
            LineChart::new()
                .title("Zoomable Data")
                .series(
                    Series::new("Value")
                        .data(data)
                        .color(Color::from_hex("#8b5cf6"))
                        .smooth(true)
                        .point_size(3.0)
                        .line_width(2.0),
                )
                .legend(LegendPosition::None)
                .tooltip(true)
                .zoom(true)
                .size(700.0, 350.0)
                .class("line-chart"),
        )
}

// ─── 7. Large Dataset ──────────────────────────────────────────────────────

fn build_large_dataset_demo() -> impl Widget {
    let data: Vec<(f64, f64)> = (0..1000)
        .map(|x| {
            let xf = x as f64 * 0.01;
            let noise = ((x * 7919) % 100) as f64 / 100.0 * 5.0 - 2.5;
            (xf, (xf * 3.0).sin() * 20.0 + xf * 5.0 + noise)
        })
        .collect();

    Column::new()
        .gap(12.0)
        .child(section_title("Large Dataset (1000 Points)"))
        .child(label("Demonstrates performance with 1000 data points"))
        .child(
            LineChart::new()
                .title("High-Frequency Data")
                .series(
                    Series::new("Signal")
                        .data(data)
                        .color(Color::from_hex("#06b6d4"))
                        .show_points(false)
                        .line_width(1.5),
                )
                .legend(LegendPosition::None)
                .tooltip(true)
                .animate(true)
                .size(700.0, 350.0)
                .class("line-chart"),
        )
}

// ─── 8. Custom Formatting ──────────────────────────────────────────────────

fn build_custom_format_demo() -> impl Widget {
    let data: Vec<(f64, f64)> = vec![
        (2020.0, 45000.0), (2021.0, 52000.0), (2022.0, 61000.0),
        (2023.0, 58000.0), (2024.0, 72000.0), (2025.0, 85000.0),
    ];

    Column::new()
        .gap(12.0)
        .child(section_title("Custom Axis Formatting"))
        .child(label("Custom format functions for X (year) and Y (currency) axes"))
        .child(
            LineChart::new()
                .title("Annual Revenue")
                .series(
                    Series::new("Revenue")
                        .data(data)
                        .color(Color::from_hex("#22c55e"))
                        .smooth(true)
                        .area_fill(0.1)
                        .point_size(6.0)
                        .line_width(3.0),
                )
                .x_axis(
                    AxisConfig::new()
                        .title("Year")
                        .format(|v| format!("{:.0}", v))
                        .grid(false),
                )
                .y_axis(
                    AxisConfig::new()
                        .title("Revenue")
                        .format(|v| {
                            if v >= 1000.0 {
                                format!("${:.0}K", v / 1000.0)
                            } else {
                                format!("${:.0}", v)
                            }
                        }),
                )
                .legend(LegendPosition::None)
                .tooltip(true)
                .size(700.0, 350.0)
                .class("line-chart"),
        )
}

// ─── 9. Rainfall vs Evaporation (mirrored charts) ──────────────────────────

fn build_rainfall_evaporation_demo() -> impl Widget {
    // Sampled from ECharts Rainfall vs Evaporation example (~100 points each)
    // Evaporation: time-series with spikes up to ~250 m³/s
    let evap_raw: &[f64] = &[
        0.97, 0.94, 0.94, 0.94, 0.94, 0.86, 0.86, 0.86, 0.86, 0.93,
        1.06, 1.20, 1.36, 1.49, 1.44, 1.27, 1.18, 1.11, 1.10, 1.10,
        1.05, 1.00, 0.95, 0.94, 0.94, 0.86, 0.86, 0.78, 0.78, 0.78,
        0.58, 0.58, 0.58, 0.46, 0.46, 0.46, 0.46, 0.46, 0.46, 0.67,
        1.52, 3.22, 3.28, 3.28, 2.54, 1.62, 1.31, 1.31, 1.31, 1.31,
        1.06, 0.74, 0.64, 0.64, 0.64, 0.53, 0.48, 0.46, 0.46, 0.46,
        0.46, 0.46, 0.46, 0.64, 0.78, 0.78, 0.78, 0.78, 0.78, 0.78,
        0.78, 0.78, 0.86, 0.94, 0.94, 0.86, 0.76, 0.71, 0.71, 0.71,
        0.71, 0.71, 0.71, 0.71, 0.71, 0.78, 0.94, 0.89, 0.86, 0.71,
        0.71, 0.71, 0.71, 1.14, 1.40, 1.40, 1.09, 0.78, 0.65, 0.64,
        0.64, 0.64, 0.64, 0.94, 3.07, 14.0, 82.2, 226.0, 212.0, 151.6,
        119.9, 105.4, 77.7, 50.6, 37.2, 25.5, 25.5, 22.5, 14.0, 14.2,
        17.1, 17.9, 12.7, 3.5, 2.3, 1.6, 1.3, 1.2, 1.2, 1.2,
        1.3, 1.3, 1.3, 1.3, 1.2, 0.95, 0.71, 0.71, 0.70, 0.68,
        0.66, 0.65, 0.64, 0.64, 0.64, 0.64, 0.68, 0.78, 0.78, 0.86,
        1.25, 5.40, 47.5, 252.1, 208.7, 165.3, 138.3, 106.7, 80.0, 62.5,
        51.2, 68.2, 109.0, 121.0, 100.0, 99.8, 163.6, 164.6, 131.4, 104.9,
        81.5, 64.9, 54.6, 42.7, 33.5, 27.2, 27.2, 22.8, 19.0, 18.0,
        15.8, 12.8, 10.6, 8.5, 7.2, 6.0, 4.8, 3.6, 2.9, 2.1,
        1.5, 1.1, 0.72, 0.52, 0.52, 0.45, 0.42, 0.40, 0.40, 0.40,
    ];

    // Rainfall: mostly near-zero with burst spikes
    let rain_raw: &[f64] = &[
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.02, 0.20, 0.04, 0.05, 0.08,
        0.14, 0.23, 0.13, 0.0, 0.04, 0.05, 0.13, 0.69, 0.35, 0.13,
        0.07, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.01, 0.08, 0.28, 0.64, 1.80, 0.92, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.02, 0.0,
        0.0, 0.0, 0.20, 1.03, 1.81, 1.41, 0.53, 0.94, 2.90, 0.13,
        0.0, 0.0, 0.02, 0.0, 0.0, 0.10, 0.25, 0.49, 0.85, 2.39,
        0.47, 0.0, 0.0, 0.0, 0.02, 0.11, 0.85, 0.10, 0.06, 0.0,
        0.0, 0.01, 0.07, 0.25, 0.49, 0.25, 0.33, 0.09, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.02,
        0.23, 0.0, 0.0, 0.0, 0.07, 0.50, 0.04, 0.02, 0.06, 0.19,
        0.16, 0.09, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.02, 0.0, 0.0, 0.07, 0.27,
        0.82, 0.50, 0.0, 0.0, 0.0, 0.0, 0.04, 0.22, 0.51, 0.88,
        2.83, 5.96, 6.43, 3.35, 4.20, 1.02, 0.84, 0.62, 0.19, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];

    let evaporation: Vec<(f64, f64)> = evap_raw.iter().enumerate()
        .map(|(i, &v)| (i as f64, v)).collect();
    let rainfall: Vec<(f64, f64)> = rain_raw.iter().enumerate()
        .map(|(i, &v)| (i as f64, v)).collect();

    Column::new()
        .gap(12.0)
        .child(section_title("Rainfall vs Evaporation"))
        .child(label("Two stacked charts with mirrored axes — evaporation grows up, rainfall grows down"))
        .child(
            Column::new()
                .gap(0.0)
                // Top chart: Evaporation (normal Y axis)
                .child(
                    LineChart::new()
                        .series(
                            Series::new("Evaporation")
                                .data(evaporation)
                                .color(Color::from_hex("#5470c6"))
                                .line_width(1.5)
                                .point_size(3.0)
                                .show_points(false),
                        )
                        .x_axis(AxisConfig::new().grid(true))
                        .y_axis(
                            AxisConfig::new()
                                .title("Evaporation(m³/s)")
                                .max(500.0)
                                .format(|v| format!("{:.0}", v)),
                        )
                        .legend(LegendPosition::None)
                        .tooltip(true)
                        .animate(true)
                        .size(700.0, 220.0)
                        .class("line-chart"),
                )
                // Bottom chart: Rainfall (inverted Y axis — grows downward)
                .child(
                    LineChart::new()
                        .series(
                            Series::new("Rainfall")
                                .data(rainfall)
                                .color(Color::from_hex("#91cc75"))
                                .line_width(1.5)
                                .show_points(false),
                        )
                        .x_axis(AxisConfig::new().grid(true))
                        .y_axis(
                            AxisConfig::new()
                                .title("Rainfall(mm)")
                                .inverse(true)
                                .format(|v| format!("{:.1}", v)),
                        )
                        .legend(LegendPosition::None)
                        .tooltip(true)
                        .animate(true)
                        .size(700.0, 220.0)
                        .class("line-chart"),
                ),
        )
}

// ─── 10. Beijing AQI (visual map — color by Y value) ───────────────────────

fn build_beijing_aqi_demo() -> impl Widget {
    // Realistic Beijing AQI data — longer runs at each level with occasional spikes
    // Mimics real air quality patterns: periods of good/moderate air with pollution events
    #[rustfmt::skip]
    let aqi_values: &[f64] = &[
        // Jun 2014: summer, mostly moderate with a pollution event
        55.0, 48.0, 42.0, 65.0, 78.0, 95.0, 120.0, 155.0, 180.0, 210.0,
        185.0, 140.0, 105.0, 85.0, 72.0, 60.0, 45.0, 38.0, 52.0, 68.0,
        82.0, 110.0, 135.0, 95.0, 70.0, 55.0, 48.0, 42.0, 58.0, 75.0,
        // Jul 2014: hot summer, two big spikes
        88.0, 105.0, 125.0, 160.0, 200.0, 255.0, 310.0, 280.0, 220.0, 165.0,
        120.0, 90.0, 72.0, 65.0, 78.0, 92.0, 85.0, 68.0, 55.0, 62.0,
        80.0, 115.0, 150.0, 195.0, 260.0, 335.0, 290.0, 230.0, 175.0, 130.0,
        // Aug 2014: declining, some events
        95.0, 78.0, 65.0, 82.0, 108.0, 145.0, 190.0, 240.0, 300.0, 265.0,
        195.0, 140.0, 100.0, 75.0, 60.0, 48.0, 55.0, 70.0, 95.0, 130.0,
        105.0, 80.0, 62.0, 50.0, 42.0, 55.0, 72.0, 88.0, 65.0, 52.0,
        // Sep 2014: autumn, gradually worsening
        45.0, 55.0, 70.0, 85.0, 110.0, 140.0, 125.0, 98.0, 75.0, 60.0,
        72.0, 95.0, 120.0, 155.0, 190.0, 170.0, 135.0, 105.0, 85.0, 95.0,
        115.0, 145.0, 175.0, 210.0, 250.0, 285.0, 230.0, 180.0, 140.0, 110.0,
        // Oct 2014: heavy pollution season starts
        90.0, 105.0, 130.0, 165.0, 200.0, 240.0, 290.0, 350.0, 380.0, 320.0,
        260.0, 200.0, 155.0, 120.0, 95.0, 110.0, 140.0, 180.0, 220.0, 195.0,
        155.0, 120.0, 100.0, 130.0, 170.0, 215.0, 260.0, 310.0, 270.0, 210.0,
        // Nov 2014: continued heavy
        160.0, 130.0, 105.0, 85.0, 100.0, 130.0, 165.0, 200.0, 175.0, 140.0,
        110.0, 90.0, 75.0, 95.0, 125.0, 160.0, 200.0, 245.0, 290.0, 250.0,
        200.0, 160.0, 125.0, 100.0, 115.0, 145.0, 180.0, 220.0, 190.0, 150.0,
        // Dec 2014: winter peak pollution
        120.0, 95.0, 110.0, 140.0, 175.0, 215.0, 265.0, 320.0, 370.0, 340.0,
        280.0, 225.0, 175.0, 135.0, 105.0, 85.0, 70.0, 60.0, 75.0, 95.0,
        // Jan 2015: winter, improving
        80.0, 65.0, 55.0, 70.0, 90.0, 120.0, 155.0, 130.0, 100.0, 78.0,
        60.0, 50.0, 42.0, 55.0, 72.0, 95.0, 125.0, 160.0, 200.0, 175.0,
        // Feb 2015: end of winter, occasional spikes
        140.0, 110.0, 85.0, 65.0, 55.0, 48.0, 62.0, 80.0, 105.0, 90.0,
        70.0, 55.0, 45.0, 60.0, 85.0, 120.0, 165.0, 210.0, 270.0, 195.0,
    ];

    let aqi_data: Vec<(f64, f64)> = aqi_values.iter().enumerate()
        .map(|(i, &v)| (i as f64, v))
        .collect();

    // AQI color thresholds (matching ECharts example)
    let visual_map = vec![
        VisualMapPiece::new(f64::NEG_INFINITY, 50.0, Color::from_hex("#93CE07")),
        VisualMapPiece::new(50.0, 100.0, Color::from_hex("#FBDB0F")),
        VisualMapPiece::new(100.0, 150.0, Color::from_hex("#FC7D02")),
        VisualMapPiece::new(150.0, 200.0, Color::from_hex("#FD0100")),
        VisualMapPiece::new(200.0, 300.0, Color::from_hex("#AA069F")),
        VisualMapPiece::new(300.0, f64::INFINITY, Color::from_hex("#AC3B2A")),
    ];

    Column::new()
        .gap(12.0)
        .child(section_title("Beijing AQI"))
        .child(label("Single series with color varying by Y value (visual map) and horizontal mark lines"))
        .child(
            LineChart::new()
                .title("Beijing AQI")
                .series(
                    Series::new("Beijing AQI")
                        .data(aqi_data)
                        .line_width(1.5)
                        .show_points(false)
                        .visual_map(visual_map),
                )
                .x_axis(
                    AxisConfig::new()
                        .format(|v| {
                            let day = v as usize;
                            let month = 6 + day / 30;
                            let m = ((month - 1) % 12) + 1;
                            let y = 2014 + (month - 1) / 12;
                            format!("{}-{:02}", y, m)
                        })
                        .tick_count(8),
                )
                .y_axis(AxisConfig::new().format(|v| format!("{:.0}", v)))
                .mark_lines(&[50.0, 100.0, 150.0, 200.0, 300.0])
                .legend(LegendPosition::None)
                .tooltip(true)
                .size(800.0, 420.0)
                .class("line-chart"),
        )
}
