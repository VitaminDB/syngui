pub(crate) mod animation;
pub mod bar_chart;
pub mod gauge_chart;
pub mod line_chart;
pub mod math;
pub mod pie_chart;
pub mod radar_chart;
pub(crate) mod render;
pub mod types;

pub use bar_chart::BarChart;
pub use gauge_chart::{GaugeChart, GaugeSegment};
pub use line_chart::LineChart;
pub use pie_chart::PieChart;
pub use radar_chart::RadarChart;
pub use types::{
    AreaFill, AxisConfig, BarLineSeries, BarMode, BarOrientation, BarSeries, DataPoint,
    LegendConfig, LegendPosition, LineStyle, MarkLine, PieLabelPosition, PieSlice, PointShape,
    RadarGridShape, RadarIndicator, RadarSeries, Series, SeriesStyle, TooltipConfig,
    VisualMapPiece, DEFAULT_PALETTE,
};
