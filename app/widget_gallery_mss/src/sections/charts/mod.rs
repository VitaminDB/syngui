//! Charts section with sub-sidebar for chart type navigation.

mod line_chart_demos;
mod gauge_chart_demos;
mod pie_chart_demos;
mod bar_chart_demos;
mod radar_chart_demos;

use syngui::prelude::*;
use syngui::widgets::*;
use std::sync::Arc;
use syngui::core::sync::Mutex;
/// Build the Charts page with a sub-sidebar (three-panel layout).
pub fn build_charts_section() -> impl Widget {
    let chart_type = use_signal(0usize);

    let route_keys = ["line-chart", "gauge-chart", "pie-chart", "bar-chart", "radar-chart"];
    let router = Arc::new(Mutex::new(Router::new(
        route_keys.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        "line-chart",
    )));

    Row::new()
        .gap(0.0)
        // Left sub-sidebar for chart types
        .child(
            Sidebar::new()
                .class("charts-sidebar")
                .child(
                    DecoratedBox::new().class("grow").child(
                        ListView::new(vec![
                            ListItem::new("Line Chart").icon("📈"),
                            ListItem::new("Gauge Chart").icon("⏱"),
                            ListItem::new("Pie Chart").icon("🥧"),
                            ListItem::new("Bar Chart").icon("📊"),
                            ListItem::new("Radar Chart").icon("🕸"),
                        ])
                        .selection_mode(SelectionMode::Single)
                        .selected(vec![chart_type.get()])
                        .on_select({
                            let r = router.clone();
                            move |idx| {
                                if let Some(key) = route_keys.get(idx) {
                                    r.lock().unwrap().navigate(*key);
                                }
                            }
                        })
                    )
                ),
        )
        // Right content area
        .child(
            DecoratedBox::new().class("grow").child(
                RouterView::new(router)
                    .route("line-chart", || {
                        Box::new(
                            Page::new()
                                .vertical()
                                .scrollbar_policy(ScrollbarPolicy::Auto)
                                .child(line_chart_demos::build_line_chart_demos())
                                .style("padding", 24.0_f32)
                                .class("content"),
                        )
                    })
                    .route("gauge-chart", || {
                        Box::new(
                            Page::new()
                                .vertical()
                                .scrollbar_policy(ScrollbarPolicy::Auto)
                                .child(gauge_chart_demos::build_gauge_chart_demos())
                                .style("padding", 24.0_f32)
                                .class("content"),
                        )
                    })
                    .route("pie-chart", || {
                        Box::new(
                            Page::new()
                                .vertical()
                                .scrollbar_policy(ScrollbarPolicy::Auto)
                                .child(pie_chart_demos::build_pie_chart_demos())
                                .style("padding", 24.0_f32)
                                .class("content"),
                        )
                    })
                    .route("bar-chart", || {
                        Box::new(
                            Page::new()
                                .vertical()
                                .scrollbar_policy(ScrollbarPolicy::Auto)
                                .child(bar_chart_demos::build_bar_chart_demos())
                                .style("padding", 24.0_f32)
                                .class("content"),
                        )
                    })
                    .route("radar-chart", || {
                        Box::new(
                            Page::new()
                                .vertical()
                                .scrollbar_policy(ScrollbarPolicy::Auto)
                                .child(radar_chart_demos::build_radar_chart_demos())
                                .style("padding", 24.0_f32)
                                .class("content"),
                        )
                    }),
            ),
        )
}
