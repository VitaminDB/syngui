use crate::core::{Color, Rect};
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
}

impl DataPoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl From<(f64, f64)> for DataPoint {
    fn from((x, y): (f64, f64)) -> Self {
        Self { x, y }
    }
}

impl From<(f32, f32)> for DataPoint {
    fn from((x, y): (f32, f32)) -> Self {
        Self { x: x as f64, y: y as f64 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineStyle {
    Solid,
    Dashed { dash: f32, gap: f32 },
    Dotted,
}

impl Default for LineStyle {
    fn default() -> Self {
        Self::Solid
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointShape {
    Circle,
    Square,
    Triangle,
    Diamond,
}

impl Default for PointShape {
    fn default() -> Self {
        Self::Circle
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AreaFill {
    pub opacity: f32,
}

impl AreaFill {
    pub fn new(opacity: f32) -> Self {
        Self { opacity: opacity.clamp(0.0, 1.0) }
    }
}

#[derive(Debug, Clone)]
pub struct SeriesStyle {
    pub color: Option<Color>,
    pub line_width: f32,
    pub line_style: LineStyle,
    pub smooth: bool,
    pub show_points: bool,
    pub point_size: f32,
    pub point_shape: PointShape,
    pub area_fill: Option<AreaFill>,
    pub visual_map: Option<Vec<VisualMapPiece>>,
}

impl Default for SeriesStyle {
    fn default() -> Self {
        Self {
            color: None,
            line_width: 2.0,
            line_style: LineStyle::Solid,
            smooth: false,
            show_points: true,
            point_size: 4.0,
            point_shape: PointShape::Circle,
            area_fill: None,
            visual_map: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VisualMapPiece {
    pub gt: f64,
    pub lte: f64,
    pub color: Color,
}

impl VisualMapPiece {
    pub fn new(gt: f64, lte: f64, color: impl Into<Color>) -> Self {
        Self { gt, lte, color: color.into() }
    }

    pub fn contains(&self, value: f64) -> bool {
        value > self.gt && value <= self.lte
    }
}

#[derive(Debug, Clone)]
pub struct MarkLine {
    pub value: f64,
    pub label: Option<String>,
    pub color: Option<Color>,
    pub dashed: bool,
}

impl MarkLine {
    pub fn new(value: f64) -> Self {
        Self { value, label: None, color: None, dashed: true }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn solid(mut self) -> Self {
        self.dashed = false;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Series {
    pub name: String,
    pub data: Vec<DataPoint>,
    pub style: SeriesStyle,
}

impl Series {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            data: Vec::new(),
            style: SeriesStyle::default(),
        }
    }

    pub fn data(mut self, data: impl IntoIterator<Item = impl Into<DataPoint>>) -> Self {
        self.data = data.into_iter().map(Into::into).collect();
        self
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.style.color = Some(color.into());
        self
    }

    pub fn line_width(mut self, width: f32) -> Self {
        self.style.line_width = width;
        self
    }

    pub fn dashed(mut self) -> Self {
        self.style.line_style = LineStyle::Dashed { dash: 8.0, gap: 4.0 };
        self
    }

    pub fn dashed_custom(mut self, dash: f32, gap: f32) -> Self {
        self.style.line_style = LineStyle::Dashed { dash, gap };
        self
    }

    pub fn dotted(mut self) -> Self {
        self.style.line_style = LineStyle::Dotted;
        self
    }

    pub fn smooth(mut self, enabled: bool) -> Self {
        self.style.smooth = enabled;
        self
    }

    pub fn show_points(mut self, show: bool) -> Self {
        self.style.show_points = show;
        self
    }

    pub fn point_size(mut self, size: f32) -> Self {
        self.style.point_size = size;
        self
    }

    pub fn point_shape(mut self, shape: PointShape) -> Self {
        self.style.point_shape = shape;
        self
    }

    pub fn area_fill(mut self, opacity: f32) -> Self {
        self.style.area_fill = Some(AreaFill::new(opacity));
        self
    }

    pub fn style(mut self, style: SeriesStyle) -> Self {
        self.style = style;
        self
    }

    pub fn visual_map(mut self, pieces: Vec<VisualMapPiece>) -> Self {
        self.style.visual_map = Some(pieces);
        self
    }
}

#[derive(Clone)]
pub struct AxisConfig {
    pub title: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub tick_count: usize,
    pub format_fn: Option<Arc<dyn Fn(f64) -> String + Send + Sync>>,
    pub show_grid: bool,
    pub show_axis_line: bool,
    pub inverse: bool,
}

impl Default for AxisConfig {
    fn default() -> Self {
        Self {
            title: None,
            min: None,
            max: None,
            tick_count: 5,
            format_fn: None,
            show_grid: true,
            show_axis_line: true,
            inverse: false,
        }
    }
}

impl std::fmt::Debug for AxisConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AxisConfig")
            .field("title", &self.title)
            .field("min", &self.min)
            .field("max", &self.max)
            .field("tick_count", &self.tick_count)
            .field("show_grid", &self.show_grid)
            .finish()
    }
}

impl AxisConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn min(mut self, min: f64) -> Self {
        self.min = Some(min);
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        self.max = Some(max);
        self
    }

    pub fn tick_count(mut self, count: usize) -> Self {
        self.tick_count = count;
        self
    }

    pub fn format(mut self, f: impl Fn(f64) -> String + Send + Sync + 'static) -> Self {
        self.format_fn = Some(Arc::new(f));
        self
    }

    pub fn grid(mut self, show: bool) -> Self {
        self.show_grid = show;
        self
    }

    pub fn axis_line(mut self, show: bool) -> Self {
        self.show_axis_line = show;
        self
    }

    pub fn inverse(mut self, inv: bool) -> Self {
        self.inverse = inv;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LegendPosition {
    Top,
    Bottom,
    Left,
    Right,
    None,
}

impl Default for LegendPosition {
    fn default() -> Self {
        Self::Bottom
    }
}

#[derive(Debug, Clone)]
pub struct LegendConfig {
    pub position: LegendPosition,
}

impl Default for LegendConfig {
    fn default() -> Self {
        Self {
            position: LegendPosition::Bottom,
        }
    }
}

impl LegendConfig {
    pub fn new(position: LegendPosition) -> Self {
        Self { position }
    }

    pub fn none() -> Self {
        Self { position: LegendPosition::None }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TooltipConfig {
    pub enabled: bool,
    pub shared: bool,
}

impl Default for TooltipConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            shared: true,
        }
    }
}

impl TooltipConfig {
    pub fn enabled(enabled: bool) -> Self {
        Self { enabled, shared: true }
    }

    pub fn disabled() -> Self {
        Self { enabled: false, shared: false }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ChartLayout {
    pub title_rect: Rect,
    pub plot_rect: Rect,
    pub _x_axis_rect: Rect,
    pub _y_axis_rect: Rect,
    pub legend_rect: Rect,
}

impl Default for ChartLayout {
    fn default() -> Self {
        Self {
            title_rect: Rect::zero(),
            plot_rect: Rect::zero(),
            _x_axis_rect: Rect::zero(),
            _y_axis_rect: Rect::zero(),
            legend_rect: Rect::zero(),
        }
    }
}

pub const DEFAULT_PALETTE: [&str; 10] = [
    "#5470c6", "#91cc75", "#fac858", "#ee6666", "#73c0de",
    "#3ba272", "#fc8452", "#9a60b4", "#ea7ccc", "#48b8d0",
];

pub fn palette_color(index: usize) -> Color {
    Color::from_hex(DEFAULT_PALETTE[index % DEFAULT_PALETTE.len()])
}

#[derive(Debug, Clone)]
pub struct PieSlice {
    pub label: String,
    pub value: f64,
    pub color: Option<Color>,
}

impl PieSlice {
    pub fn new(label: impl Into<String>, value: f64) -> Self {
        Self { label: label.into(), value, color: None }
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PieLabelPosition {
    Inside,
    Outside,
    None,
}

impl Default for PieLabelPosition {
    fn default() -> Self { Self::Outside }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BarMode {
    Grouped,
    Stacked,
}

impl Default for BarMode {
    fn default() -> Self { Self::Grouped }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BarOrientation {
    Vertical,
    Horizontal,
}

impl Default for BarOrientation {
    fn default() -> Self { Self::Vertical }
}

#[derive(Debug, Clone)]
pub struct BarSeries {
    pub name: String,
    pub data: Vec<f64>,
    pub color: Option<Color>,
}

impl BarSeries {
    pub fn new(name: impl Into<String>, data: Vec<f64>) -> Self {
        Self { name: name.into(), data, color: None }
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct RadarIndicator {
    pub name: String,
    pub max: f64,
}

impl RadarIndicator {
    pub fn new(name: impl Into<String>, max: f64) -> Self {
        Self { name: name.into(), max }
    }
}

#[derive(Debug, Clone)]
pub struct RadarSeries {
    pub name: String,
    pub data: Vec<f64>,
    pub color: Option<Color>,
    pub area_opacity: f32,
    pub show_points: bool,
    pub line_width: f32,
}

impl RadarSeries {
    pub fn new(name: impl Into<String>, data: Vec<f64>) -> Self {
        Self {
            name: name.into(),
            data,
            color: None,
            area_opacity: 0.2,
            show_points: true,
            line_width: 2.0,
        }
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn area_opacity(mut self, opacity: f32) -> Self {
        self.area_opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn show_points(mut self, show: bool) -> Self {
        self.show_points = show;
        self
    }

    pub fn line_width(mut self, width: f32) -> Self {
        self.line_width = width;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RadarGridShape {
    Polygon,
    Circle,
}

impl Default for RadarGridShape {
    fn default() -> Self { Self::Polygon }
}
