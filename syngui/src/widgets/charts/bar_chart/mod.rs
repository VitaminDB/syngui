mod element;

use crate::mss::Dimension;
use super::types::{AxisConfig, BarMode, BarOrientation, BarSeries, LegendConfig, LegendPosition, TooltipConfig};

pub struct BarChart {
    categories: Vec<String>,
    bar_series: Vec<BarSeries>,
    mode: BarMode,
    orientation: BarOrientation,
    x_axis: AxisConfig,
    y_axis: AxisConfig,
    legend: LegendConfig,
    tooltip: TooltipConfig,
    animate: bool,
    show_value_labels: bool,
    bar_width: f32,
    bar_gap: f32,
    bar_border_radius: f32,
    width: Option<Dimension>,
    height: Option<Dimension>,
    title: Option<String>,
    classes: Vec<String>,
}

impl BarChart {
    pub fn new() -> Self {
        Self {
            categories: Vec::new(),
            bar_series: Vec::new(),
            mode: BarMode::Grouped,
            orientation: BarOrientation::Vertical,
            x_axis: AxisConfig::default(),
            y_axis: AxisConfig::default(),
            legend: LegendConfig::default(),
            tooltip: TooltipConfig::default(),
            animate: true,
            show_value_labels: false,
            bar_width: 0.6,
            bar_gap: 0.1,
            bar_border_radius: 0.0,
            width: None,
            height: None,
            title: None,
            classes: Vec::new(),
        }
    }

    pub fn category(mut self, name: impl Into<String>) -> Self {
        self.categories.push(name.into());
        self
    }

    pub fn categories(mut self, cats: Vec<String>) -> Self {
        self.categories = cats;
        self
    }

    pub fn bar_series(mut self, series: BarSeries) -> Self {
        self.bar_series.push(series);
        self
    }

    pub fn mode(mut self, mode: BarMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn orientation(mut self, orientation: BarOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn x_axis(mut self, config: AxisConfig) -> Self {
        self.x_axis = config;
        self
    }

    pub fn y_axis(mut self, config: AxisConfig) -> Self {
        self.y_axis = config;
        self
    }

    pub fn legend(mut self, position: LegendPosition) -> Self {
        self.legend = LegendConfig::new(position);
        self
    }

    pub fn tooltip(mut self, enabled: bool) -> Self {
        self.tooltip = TooltipConfig::enabled(enabled);
        self
    }

    pub fn tooltip_config(mut self, config: TooltipConfig) -> Self {
        self.tooltip = config;
        self
    }

    pub fn animate(mut self, enabled: bool) -> Self {
        self.animate = enabled;
        self
    }

    pub fn value_labels(mut self, show: bool) -> Self {
        self.show_value_labels = show;
        self
    }

    pub fn bar_width(mut self, f: f32) -> Self {
        self.bar_width = f.clamp(0.1, 1.0);
        self
    }

    pub fn bar_gap(mut self, f: f32) -> Self {
        self.bar_gap = f.clamp(0.0, 0.5);
        self
    }

    pub fn bar_radius(mut self, f: f32) -> Self {
        self.bar_border_radius = f.max(0.0);
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self.height = Some(Dimension::Px(h));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(Dimension::Px(h));
        self
    }

    pub fn title(mut self, text: impl Into<String>) -> Self {
        self.title = Some(text.into());
        self
    }

    pub fn class(mut self, cls: impl Into<String>) -> Self {
        self.classes.push(cls.into());
        self
    }
}

impl Default for BarChart {
    fn default() -> Self {
        Self::new()
    }
}
