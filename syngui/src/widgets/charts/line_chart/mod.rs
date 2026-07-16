mod element;

use crate::mss::Dimension;
use std::sync::Arc;
use crate::core::sync::Mutex;

use super::types::{AxisConfig, DataPoint, LegendConfig, LegendPosition, MarkLine, Series, TooltipConfig};

pub struct LineChart {
    pub(super) series: Vec<Series>,
    pub(super) x_axis: AxisConfig,
    pub(super) y_axis: AxisConfig,
    pub(super) legend: LegendConfig,
    pub(super) tooltip: TooltipConfig,
    pub(super) animate: bool,
    pub(super) zoom_enabled: bool,
    pub(super) width: Option<Dimension>,
    pub(super) height: Option<Dimension>,
    pub(super) title: Option<String>,
    pub(super) mark_lines: Vec<MarkLine>,
    pub(super) classes: Vec<String>,
    pub(super) on_point_click: Option<Arc<Mutex<dyn FnMut(usize, usize, &DataPoint) + Send>>>,
}

impl LineChart {
    pub fn new() -> Self {
        Self {
            series: Vec::new(),
            x_axis: AxisConfig::default(),
            y_axis: AxisConfig::default(),
            legend: LegendConfig::default(),
            tooltip: TooltipConfig::default(),
            animate: true,
            zoom_enabled: false,
            width: None,
            height: None,
            title: None,
            mark_lines: Vec::new(),
            classes: Vec::new(),
            on_point_click: None,
        }
    }

    pub fn series(mut self, s: Series) -> Self {
        self.series.push(s);
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

    pub fn zoom(mut self, enabled: bool) -> Self {
        self.zoom_enabled = enabled;
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

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn mark_line(mut self, ml: MarkLine) -> Self {
        self.mark_lines.push(ml);
        self
    }

    pub fn mark_lines(mut self, values: &[f64]) -> Self {
        for &v in values {
            self.mark_lines.push(MarkLine::new(v));
        }
        self
    }

    pub fn class(mut self, cls: impl Into<String>) -> Self {
        self.classes.push(cls.into());
        self
    }

    pub fn on_point_click(
        mut self,
        f: impl FnMut(usize, usize, &DataPoint) + Send + 'static,
    ) -> Self {
        self.on_point_click = Some(Arc::new(Mutex::new(f)));
        self
    }
}

impl Default for LineChart {
    fn default() -> Self {
        Self::new()
    }
}
