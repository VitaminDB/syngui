use super::BarChart;
use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::widget::context::{EventContextExt, TextMeasure};
use std::sync::Arc;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::time::Duration;

use super::super::animation::ChartAnimationState;
use super::super::math::{LinearScale, compute_ticks, format_tick_value};
use super::super::render::axis::{render_x_axis, render_y_axis, estimate_y_axis_width, AxisColors};
use super::super::render::legend::{render_legend_items, legend_height};
use super::super::render::tooltip::TooltipColors;
use super::super::types::{
    AxisConfig, BarMode, BarOrientation, BarSeries, ChartLayout, LegendConfig, LegendPosition,
    TooltipConfig, palette_color,
};

impl Widget for BarChart {
    fn create_element(&self) -> Box<dyn Element> {
        let num_series = self.bar_series.len();
        let mut anim = ChartAnimationState::default();
        anim.ensure_series_count(num_series);
        if self.animate {
            anim.start_appear();
        } else {
            anim.appear_progress = 1.0;
            anim.appear_eased = 1.0;
            anim.appear_started = true;
        }

        let resolved_colors: Vec<Color> = self
            .bar_series
            .iter()
            .enumerate()
            .map(|(i, s)| s.color.unwrap_or_else(|| palette_color(i)))
            .collect();

        Box::new(BarChartElement {
            id: ElementId::new(),
            categories: self.categories.clone(),
            bar_series: self.bar_series.clone(),
            mode: self.mode,
            orientation: self.orientation,
            x_axis: self.x_axis.clone(),
            y_axis: self.y_axis.clone(),
            legend_config: self.legend.clone(),
            tooltip_config: self.tooltip,
            animate_enabled: self.animate,
            show_value_labels: self.show_value_labels,
            bar_width: self.bar_width,
            bar_gap: self.bar_gap,
            bar_border_radius: self.bar_border_radius,
            width: self.width,
            height: self.height,
            title: self.title.clone(),
            bounds: Rect::zero(),
            layout: ChartLayout::default(),
            value_scale: LinearScale::new((0.0, 1.0), (100.0, 0.0)),
            resolved_colors,
            bar_rects: Vec::new(),
            hovered_bar: None,
            mouse_pos: None,
            anim,
            legend_rects: Vec::new(),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            mss_grid_color: None,
            mss_axis_color: None,
            mss_axis_font_size: None,
            mss_title_font_size: None,
            mss_tooltip_bg: None,
            text_measure: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool {
        other.is::<Self>()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
    fn widget_classes(&self) -> &[String] { &self.classes }
}

struct BarChartElement {
    id: ElementId,

    categories: Vec<String>,
    bar_series: Vec<BarSeries>,
    mode: BarMode,
    orientation: BarOrientation,
    x_axis: AxisConfig,
    y_axis: AxisConfig,
    legend_config: LegendConfig,
    tooltip_config: TooltipConfig,
    animate_enabled: bool,
    show_value_labels: bool,
    bar_width: f32,
    bar_gap: f32,
    bar_border_radius: f32,
    width: Option<Dimension>,
    height: Option<Dimension>,
    title: Option<String>,

    bounds: Rect,
    layout: ChartLayout,
    value_scale: LinearScale,
    resolved_colors: Vec<Color>,

    bar_rects: Vec<Vec<Rect>>,
    hovered_bar: Option<(usize, usize)>,

    mouse_pos: Option<Point>,

    anim: ChartAnimationState,

    legend_rects: Vec<Rect>,

    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,

    mss_grid_color: Option<Color>,
    mss_axis_color: Option<Color>,
    mss_axis_font_size: Option<f32>,
    mss_title_font_size: Option<f32>,
    mss_tooltip_bg: Option<Color>,

    text_measure: Option<Arc<dyn TextMeasure>>,
}

impl BarChartElement {
    fn resolve_colors(&mut self) {
        self.resolved_colors = self
            .bar_series
            .iter()
            .enumerate()
            .map(|(i, s)| s.color.unwrap_or_else(|| palette_color(i)))
            .collect();
    }

    fn compute_value_range(&self) -> (f64, f64) {
        let num_categories = self.categories.len();
        if num_categories == 0 || self.bar_series.is_empty() {
            return (0.0, 1.0);
        }

        let mut value_min = 0.0_f64;
        let mut value_max = 0.0_f64;

        match self.mode {
            BarMode::Grouped => {
                for (si, series) in self.bar_series.iter().enumerate() {
                    if !self.anim.is_series_visible(si) {
                        continue;
                    }
                    for &val in &series.data {
                        value_min = value_min.min(val);
                        value_max = value_max.max(val);
                    }
                }
            }
            BarMode::Stacked => {
                for ci in 0..num_categories {
                    let mut pos_sum = 0.0_f64;
                    let mut neg_sum = 0.0_f64;
                    for (si, series) in self.bar_series.iter().enumerate() {
                        if !self.anim.is_series_visible(si) {
                            continue;
                        }
                        let val = series.data.get(ci).copied().unwrap_or(0.0);
                        if val >= 0.0 {
                            pos_sum += val;
                        } else {
                            neg_sum += val;
                        }
                    }
                    value_max = value_max.max(pos_sum);
                    value_min = value_min.min(neg_sum);
                }
            }
        }

        if value_min >= 0.0 {
            value_min = 0.0;
        }

        let range = value_max - value_min;
        if range.abs() < 1e-12 {
            value_max = value_min + 1.0;
        } else {
            value_max += range * 0.1;
        }

        (value_min, value_max)
    }

    fn compute_layout(&mut self) {
        let padding = self.mss.padding_ltrb([16.0; 4]);
        let pad_l = padding[0];
        let pad_t = padding[1];
        let pad_r = padding[2];
        let pad_b = padding[3];

        let inner_x = self.bounds.origin.x + pad_l;
        let inner_y = self.bounds.origin.y + pad_t;
        let inner_w = (self.bounds.size.width - pad_l - pad_r).max(0.0);
        let inner_h = (self.bounds.size.height - pad_t - pad_b).max(0.0);

        let axis_font_size = self.mss_axis_font_size.unwrap_or(11.0);
        let title_font_size = self.mss_title_font_size.unwrap_or(14.0);
        let legend_font_size = 12.0;

        let title_h = if self.title.is_some() {
            title_font_size + 8.0
        } else {
            0.0
        };

        let legend_h = legend_height(self.legend_config.position, legend_font_size);

        let (legend_top, legend_bottom) = match self.legend_config.position {
            LegendPosition::Top => (legend_h, 0.0),
            LegendPosition::Bottom => (0.0, legend_h),
            _ => (0.0, 0.0),
        };

        let (value_min, value_max) = self.compute_value_range();

        let value_min = self.y_axis.min.unwrap_or(value_min);
        let value_max = self.y_axis.max.unwrap_or(value_max);

        match self.orientation {
            BarOrientation::Vertical => {
                let x_axis_h = axis_font_size + 8.0;
                let y_axis_w = estimate_y_axis_width(&self.y_axis, value_min, value_max, axis_font_size, self.text_measure.as_ref());

                let plot_x = inner_x + y_axis_w;
                let plot_y = inner_y + title_h + legend_top;
                let plot_w = (inner_w - y_axis_w).max(0.0);
                let plot_h = (inner_h - title_h - legend_top - legend_bottom - x_axis_h).max(0.0);

                self.layout = ChartLayout {
                    title_rect: Rect::new(Point::new(inner_x, inner_y), Size::new(inner_w, title_h)),
                    plot_rect: Rect::new(Point::new(plot_x, plot_y), Size::new(plot_w, plot_h)),
                    _x_axis_rect: Rect::new(
                        Point::new(plot_x, plot_y + plot_h),
                        Size::new(plot_w, x_axis_h),
                    ),
                    _y_axis_rect: Rect::new(Point::new(inner_x, plot_y), Size::new(y_axis_w, plot_h)),
                    legend_rect: Rect::new(
                        Point::new(
                            inner_x,
                            if legend_top > 0.0 {
                                inner_y + title_h
                            } else {
                                plot_y + plot_h + x_axis_h
                            },
                        ),
                        Size::new(inner_w, legend_h),
                    ),
                };

                self.value_scale = LinearScale::new(
                    (value_min, value_max),
                    (plot_h, 0.0),
                );

                self.compute_bar_rects_vertical();
            }
            BarOrientation::Horizontal => {
                let x_axis_h = axis_font_size + 8.0;
                let cat_axis_w = self
                    .categories
                    .iter()
                    .map(|c| super::super::render::estimate_text_width(c, axis_font_size, self.text_measure.as_ref()))
                    .fold(0.0_f32, f32::max)
                    .max(20.0) + 8.0;

                let plot_x = inner_x + cat_axis_w;
                let plot_y = inner_y + title_h + legend_top;
                let plot_w = (inner_w - cat_axis_w).max(0.0);
                let plot_h = (inner_h - title_h - legend_top - legend_bottom - x_axis_h).max(0.0);

                self.layout = ChartLayout {
                    title_rect: Rect::new(Point::new(inner_x, inner_y), Size::new(inner_w, title_h)),
                    plot_rect: Rect::new(Point::new(plot_x, plot_y), Size::new(plot_w, plot_h)),
                    _x_axis_rect: Rect::new(
                        Point::new(plot_x, plot_y + plot_h),
                        Size::new(plot_w, x_axis_h),
                    ),
                    _y_axis_rect: Rect::new(Point::new(inner_x, plot_y), Size::new(cat_axis_w, plot_h)),
                    legend_rect: Rect::new(
                        Point::new(
                            inner_x,
                            if legend_top > 0.0 {
                                inner_y + title_h
                            } else {
                                plot_y + plot_h + x_axis_h
                            },
                        ),
                        Size::new(inner_w, legend_h),
                    ),
                };

                self.value_scale = LinearScale::new(
                    (value_min, value_max),
                    (0.0, plot_w),
                );

                self.compute_bar_rects_horizontal();
            }
        }
    }

    fn compute_bar_rects_vertical(&mut self) {
        let plot = &self.layout.plot_rect;
        let num_categories = self.categories.len();
        let num_series = self.bar_series.len();
        if num_categories == 0 || num_series == 0 {
            self.bar_rects = Vec::new();
            return;
        }

        let cat_width = plot.size.width / num_categories as f32;
        let zero_y = self.value_scale.map(0.0);

        self.bar_rects = vec![vec![Rect::zero(); num_categories]; num_series];

        match self.mode {
            BarMode::Grouped => {
                let group_width = cat_width * self.bar_width;
                let single_bar_width = if num_series > 1 {
                    group_width / num_series as f32 * (1.0 - self.bar_gap)
                } else {
                    group_width
                };
                let gap_width = if num_series > 1 {
                    group_width / num_series as f32 * self.bar_gap
                } else {
                    0.0
                };

                for si in 0..num_series {
                    for ci in 0..num_categories {
                        let val = self.bar_series[si].data.get(ci).copied().unwrap_or(0.0);
                        let val_y = self.value_scale.map(val);

                        let group_start_x = ci as f32 * cat_width + (cat_width - group_width) * 0.5;
                        let bar_x = group_start_x + si as f32 * (single_bar_width + gap_width);

                        let (rect_y, rect_h) = if val >= 0.0 {
                            (val_y, (zero_y - val_y).max(0.0))
                        } else {
                            (zero_y, (val_y - zero_y).max(0.0))
                        };

                        self.bar_rects[si][ci] = Rect::new(
                            Point::new(plot.origin.x + bar_x, plot.origin.y + rect_y),
                            Size::new(single_bar_width, rect_h),
                        );
                    }
                }
            }
            BarMode::Stacked => {
                let bar_w = cat_width * self.bar_width;

                for ci in 0..num_categories {
                    let mut pos_acc = 0.0_f64;
                    let mut neg_acc = 0.0_f64;

                    for si in 0..num_series {
                        let val = self.bar_series[si].data.get(ci).copied().unwrap_or(0.0);
                        let bar_x = ci as f32 * cat_width + (cat_width - bar_w) * 0.5;

                        if val >= 0.0 {
                            let base = pos_acc;
                            let top = base + val;
                            let top_y = self.value_scale.map(top);
                            let base_y = self.value_scale.map(base);
                            let rect_h = (base_y - top_y).max(0.0);

                            self.bar_rects[si][ci] = Rect::new(
                                Point::new(plot.origin.x + bar_x, plot.origin.y + top_y),
                                Size::new(bar_w, rect_h),
                            );
                            pos_acc = top;
                        } else {
                            let base = neg_acc;
                            let bottom = base + val;
                            let base_y = self.value_scale.map(base);
                            let bottom_y = self.value_scale.map(bottom);
                            let rect_h = (bottom_y - base_y).max(0.0);

                            self.bar_rects[si][ci] = Rect::new(
                                Point::new(plot.origin.x + bar_x, plot.origin.y + base_y),
                                Size::new(bar_w, rect_h),
                            );
                            neg_acc = bottom;
                        }
                    }
                }
            }
        }
    }

    fn compute_bar_rects_horizontal(&mut self) {
        let plot = &self.layout.plot_rect;
        let num_categories = self.categories.len();
        let num_series = self.bar_series.len();
        if num_categories == 0 || num_series == 0 {
            self.bar_rects = Vec::new();
            return;
        }

        let cat_height = plot.size.height / num_categories as f32;
        let zero_x = self.value_scale.map(0.0);

        self.bar_rects = vec![vec![Rect::zero(); num_categories]; num_series];

        match self.mode {
            BarMode::Grouped => {
                let group_height = cat_height * self.bar_width;
                let single_bar_height = if num_series > 1 {
                    group_height / num_series as f32 * (1.0 - self.bar_gap)
                } else {
                    group_height
                };
                let gap_height = if num_series > 1 {
                    group_height / num_series as f32 * self.bar_gap
                } else {
                    0.0
                };

                for si in 0..num_series {
                    for ci in 0..num_categories {
                        let val = self.bar_series[si].data.get(ci).copied().unwrap_or(0.0);
                        let val_x = self.value_scale.map(val);

                        let group_start_y = ci as f32 * cat_height + (cat_height - group_height) * 0.5;
                        let bar_y = group_start_y + si as f32 * (single_bar_height + gap_height);

                        let (rect_x, rect_w) = if val >= 0.0 {
                            (zero_x, (val_x - zero_x).max(0.0))
                        } else {
                            (val_x, (zero_x - val_x).max(0.0))
                        };

                        self.bar_rects[si][ci] = Rect::new(
                            Point::new(plot.origin.x + rect_x, plot.origin.y + bar_y),
                            Size::new(rect_w, single_bar_height),
                        );
                    }
                }
            }
            BarMode::Stacked => {
                let bar_h = cat_height * self.bar_width;

                for ci in 0..num_categories {
                    let mut pos_acc = 0.0_f64;
                    let mut neg_acc = 0.0_f64;

                    for si in 0..num_series {
                        let val = self.bar_series[si].data.get(ci).copied().unwrap_or(0.0);
                        let bar_y = ci as f32 * cat_height + (cat_height - bar_h) * 0.5;

                        if val >= 0.0 {
                            let base = pos_acc;
                            let top = base + val;
                            let base_x = self.value_scale.map(base);
                            let top_x = self.value_scale.map(top);
                            let rect_w = (top_x - base_x).max(0.0);

                            self.bar_rects[si][ci] = Rect::new(
                                Point::new(plot.origin.x + base_x, plot.origin.y + bar_y),
                                Size::new(rect_w, bar_h),
                            );
                            pos_acc = top;
                        } else {
                            let base = neg_acc;
                            let bottom = base + val;
                            let bottom_x = self.value_scale.map(bottom);
                            let base_x = self.value_scale.map(base);
                            let rect_w = (base_x - bottom_x).max(0.0);

                            self.bar_rects[si][ci] = Rect::new(
                                Point::new(plot.origin.x + bottom_x, plot.origin.y + bar_y),
                                Size::new(rect_w, bar_h),
                            );
                            neg_acc = bottom;
                        }
                    }
                }
            }
        }
    }

    fn axis_colors(&self) -> AxisColors {
        let default = AxisColors::default();
        AxisColors {
            grid_color: self.mss_grid_color
                .or(self.mss.color.map(|c| c.with_alpha(0.15)))
                .unwrap_or(default.grid_color),
            axis_color: self.mss_axis_color
                .or(self.mss.color.map(|c| c.with_alpha(0.4)))
                .unwrap_or(default.axis_color),
            label_color: self.mss.color.map(|c| c.with_alpha(0.6)).unwrap_or(default.label_color),
            title_color: self.mss.color.unwrap_or(default.title_color),
            axis_font_size: self.mss_axis_font_size.unwrap_or(default.axis_font_size),
            title_font_size: self.mss_title_font_size.unwrap_or(default.title_font_size),
        }
    }

    fn tooltip_colors(&self) -> TooltipColors {
        let default = TooltipColors::default();
        TooltipColors {
            background: self.mss_tooltip_bg.unwrap_or(default.background),
            border_color: default.border_color,
            text_color: default.text_color,
            font_size: 12.0,
        }
    }

    fn hit_test_bar(&self, pos: Point) -> Option<(usize, usize)> {
        for (si, series_rects) in self.bar_rects.iter().enumerate() {
            if !self.anim.is_series_visible(si) {
                continue;
            }
            for (ci, rect) in series_rects.iter().enumerate() {
                if rect.contains(pos) {
                    return Some((si, ci));
                }
            }
        }
        None
    }

    fn render_bar_tooltip(
        &self,
        list: &mut DisplayList,
        mouse: Point,
        si: usize,
        ci: usize,
    ) {
        let tc = self.tooltip_colors();
        let opacity = self.anim.tooltip_opacity;
        if opacity < 0.01 {
            return;
        }

        let cat_name = self.categories.get(ci).map(|s| s.as_str()).unwrap_or("?");
        let series_name = self.bar_series.get(si).map(|s| s.name.as_str()).unwrap_or("?");
        let value = self.bar_series.get(si).and_then(|s| s.data.get(ci)).copied().unwrap_or(0.0);
        let value_str = format_tick_value(value);

        let line1 = cat_name.to_string();
        let line2 = format!("{}: {}", series_name, value_str);

        let line_height = tc.font_size + 4.0;
        let padding = 8.0;
        let max_text_width = super::super::render::estimate_text_width(&line1, tc.font_size, self.text_measure.as_ref())
            .max(super::super::render::estimate_text_width(&line2, tc.font_size, self.text_measure.as_ref()))
            .max(40.0);
        let tooltip_width = max_text_width + padding * 2.0;
        let tooltip_height = 2.0 * line_height + padding * 2.0;

        let offset_x = 12.0;
        let offset_y = -12.0;

        let mut x = mouse.x + offset_x;
        let mut y = mouse.y + offset_y - tooltip_height;

        if x + tooltip_width > self.bounds.origin.x + self.bounds.size.width {
            x = mouse.x - offset_x - tooltip_width;
        }
        if y < self.bounds.origin.y {
            y = mouse.y + offset_y + 16.0;
        }

        x = x.max(self.bounds.origin.x);
        y = y.max(self.bounds.origin.y);

        let tooltip_rect = Rect::new(Point::new(x, y), Size::new(tooltip_width, tooltip_height));

        list.push_shadow(
            tooltip_rect,
            Color::new(0.0, 0.0, 0.0, 0.2 * opacity),
            8.0,
            (0.0, 2.0),
            [6.0; 4],
        );
        list.push_rect(
            tooltip_rect,
            tc.background.with_alpha(opacity * 0.95),
            [6.0; 4],
        );

        let text_rect1 = Rect::new(
            Point::new(x + padding, y + padding),
            Size::new(max_text_width, line_height),
        );
        list.push_text(&line1, text_rect1, tc.text_color.with_alpha(opacity * 0.7), tc.font_size);

        let color = self.resolved_colors.get(si).copied().unwrap_or(Color::WHITE);
        let text_rect2 = Rect::new(
            Point::new(x + padding, y + padding + line_height),
            Size::new(max_text_width, line_height),
        );
        list.push_text(&line2, text_rect2, color.with_alpha(opacity), tc.font_size);
    }
}

impl Element for BarChartElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<BarChart>() {
            self.categories = w.categories.clone();
            self.bar_series = w.bar_series.clone();
            self.mode = w.mode;
            self.orientation = w.orientation;
            self.x_axis = w.x_axis.clone();
            self.y_axis = w.y_axis.clone();
            self.legend_config = w.legend.clone();
            self.tooltip_config = w.tooltip;
            self.animate_enabled = w.animate;
            self.show_value_labels = w.show_value_labels;
            self.bar_width = w.bar_width;
            self.bar_gap = w.bar_gap;
            self.bar_border_radius = w.bar_border_radius;
            self.width = w.width;
            self.height = w.height;
            self.title = w.title.clone();

            self.anim.ensure_series_count(self.bar_series.len());
            self.resolve_colors();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self
            .mss
            .width
            .or(self.width)
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(constraints.max_width.min(600.0))
            .min(constraints.max_width);

        let h = self
            .mss
            .height
            .or(self.height)
            .map(|d| d.resolve(constraints.max_height))
            .unwrap_or(400.0)
            .min(constraints.max_height);

        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        self.resolve_colors();
        self.compute_layout();

        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let plot = &self.layout.plot_rect;
        let border_radius = self.mss.border_radius_resolved(self.bounds.size.width, 0.0);

        if let Some(bg_color) = self.mss.background_color {
            list.push_rect(self.bounds, bg_color, border_radius);
        }

        if let Some(ref shadows) = self.mss.box_shadow {
            for shadow in shadows.0.iter() {
                list.push_shadow(
                    self.bounds,
                    shadow.color,
                    shadow.blur_radius,
                    (shadow.offset_x, shadow.offset_y),
                    border_radius,
                );
            }
        }

        if let Some(ref title) = self.title {
            let title_font = self.mss_title_font_size.unwrap_or(14.0);
            let title_color = self.mss.color.unwrap_or(Color::from_hex("#1e293b"));
            list.push_text_centered(title, self.layout.title_rect, title_color, title_font);
        }

        let axis_colors = self.axis_colors();
        let axis_font_size = axis_colors.axis_font_size;

        match self.orientation {
            BarOrientation::Vertical => {
                let mut ctx = CanvasContext::new(Point::zero(), Size::zero());
                render_y_axis(
                    list,
                    &mut ctx,
                    &self.layout,
                    &self.y_axis,
                    &self.value_scale,
                    &axis_colors,
                );
            }
            BarOrientation::Horizontal => {
                let num_categories = self.categories.len();
                if num_categories > 0 {
                    let cat_height = plot.size.height / num_categories as f32;
                    let label_color = axis_colors.label_color;
                    for (i, cat) in self.categories.iter().enumerate() {
                        let cy = plot.origin.y + i as f32 * cat_height + cat_height * 0.5;
                        let label_width = 50.0;
                        let label_rect = Rect::new(
                            Point::new(plot.origin.x - label_width - 4.0, cy - axis_font_size * 0.5),
                            Size::new(label_width, axis_font_size + 2.0),
                        );
                        list.push_text_aligned(
                            cat,
                            label_rect,
                            label_color,
                            axis_font_size,
                            crate::mss::TextAlign::RIGHT,
                            crate::mss::TextDecoration::None,
                            400,
                        );
                    }
                }

                if self.y_axis.show_axis_line {
                    let line_rect = Rect::new(
                        Point::new(plot.origin.x - 1.0, plot.origin.y),
                        Size::new(1.0, plot.size.height),
                    );
                    list.push_rect(line_rect, axis_colors.axis_color, [0.0; 4]);
                }
            }
        }

        list.push_clip(Rect::new(plot.origin, plot.size));

        match self.orientation {
            BarOrientation::Vertical => {
                if self.y_axis.show_grid {
                    let ticks = compute_ticks(
                        self.value_scale.domain.0,
                        self.value_scale.domain.1,
                        self.y_axis.tick_count,
                    );
                    for &tick_val in &ticks {
                        let y = self.value_scale.map(tick_val);
                        if y >= 0.0 && y <= plot.size.height {
                            let line_rect = Rect::new(
                                Point::new(plot.origin.x, plot.origin.y + y - 0.5),
                                Size::new(plot.size.width, 1.0),
                            );
                            list.push_rect(line_rect, axis_colors.grid_color, [0.0; 4]);
                        }
                    }
                }
            }
            BarOrientation::Horizontal => {
                if self.x_axis.show_grid {
                    let ticks = compute_ticks(
                        self.value_scale.domain.0,
                        self.value_scale.domain.1,
                        self.x_axis.tick_count,
                    );
                    for &tick_val in &ticks {
                        let x = self.value_scale.map(tick_val);
                        if x >= 0.0 && x <= plot.size.width {
                            let line_rect = Rect::new(
                                Point::new(plot.origin.x + x - 0.5, plot.origin.y),
                                Size::new(1.0, plot.size.height),
                            );
                            list.push_rect(line_rect, axis_colors.grid_color, [0.0; 4]);
                        }
                    }
                }
            }
        }

        let appear = self.anim.appear_eased;
        let num_categories = self.categories.len();

        for (si, series_rects) in self.bar_rects.iter().enumerate() {
            let opacity = self.anim.series_opacity(si);
            if opacity < 0.01 {
                continue;
            }

            let base_color = self.resolved_colors.get(si).copied().unwrap_or(Color::from_hex("#888888"));
            let color = base_color.with_alpha(opacity);

            for (ci, rect) in series_rects.iter().enumerate() {
                if ci >= num_categories {
                    continue;
                }

                let animated_rect = match self.orientation {
                    BarOrientation::Vertical => {
                        let val = self.bar_series[si].data.get(ci).copied().unwrap_or(0.0);
                        let animated_h = rect.size.height * appear;
                        if val >= 0.0 {
                            Rect::new(
                                Point::new(rect.origin.x, rect.origin.y + rect.size.height - animated_h),
                                Size::new(rect.size.width, animated_h),
                            )
                        } else {
                            Rect::new(
                                Point::new(rect.origin.x, rect.origin.y),
                                Size::new(rect.size.width, animated_h),
                            )
                        }
                    }
                    BarOrientation::Horizontal => {
                        let val = self.bar_series[si].data.get(ci).copied().unwrap_or(0.0);
                        let animated_w = rect.size.width * appear;
                        if val >= 0.0 {
                            Rect::new(
                                Point::new(rect.origin.x, rect.origin.y),
                                Size::new(animated_w, rect.size.height),
                            )
                        } else {
                            Rect::new(
                                Point::new(rect.origin.x + rect.size.width - animated_w, rect.origin.y),
                                Size::new(animated_w, rect.size.height),
                            )
                        }
                    }
                };

                let is_hovered = self.hovered_bar == Some((si, ci));
                let bar_color = if is_hovered {
                    color.lighten(0.15)
                } else {
                    color
                };

                let radius = self.bar_border_radius;
                let corner_radii = [radius; 4];
                list.push_rect(animated_rect, bar_color, corner_radii);
            }
        }

        if self.show_value_labels && appear > 0.5 {
            let label_color = self.mss.color.unwrap_or(Color::from_hex("#1e293b"));
            let label_font = (axis_font_size - 1.0).max(8.0);

            for (si, series_rects) in self.bar_rects.iter().enumerate() {
                if self.anim.series_opacity(si) < 0.5 {
                    continue;
                }
                for (ci, rect) in series_rects.iter().enumerate() {
                    if ci >= num_categories {
                        continue;
                    }
                    let val = self.bar_series[si].data.get(ci).copied().unwrap_or(0.0);
                    let label = format_tick_value(val);
                    let label_w = super::super::render::estimate_text_width(&label, label_font, self.text_measure.as_ref());

                    match self.orientation {
                        BarOrientation::Vertical => {
                            let label_x = rect.origin.x + rect.size.width * 0.5 - label_w * 0.5;
                            let label_y = if val >= 0.0 {
                                rect.origin.y + rect.size.height - rect.size.height * appear - label_font - 2.0
                            } else {
                                rect.origin.y + rect.size.height * appear + 2.0
                            };
                            let label_rect = Rect::new(
                                Point::new(label_x, label_y),
                                Size::new(label_w, label_font + 2.0),
                            );
                            list.push_text_centered(&label, label_rect, label_color, label_font);
                        }
                        BarOrientation::Horizontal => {
                            let label_y = rect.origin.y + rect.size.height * 0.5 - label_font * 0.5;
                            let label_x = if val >= 0.0 {
                                rect.origin.x + rect.size.width * appear + 2.0
                            } else {
                                rect.origin.x + rect.size.width - rect.size.width * appear - label_w - 2.0
                            };
                            let label_rect = Rect::new(
                                Point::new(label_x, label_y),
                                Size::new(label_w, label_font + 2.0),
                            );
                            list.push_text(&label, label_rect, label_color, label_font);
                        }
                    }
                }
            }
        }

        list.pop_clip();

        match self.orientation {
            BarOrientation::Vertical => {
                let num_cats = self.categories.len();
                if num_cats > 0 {
                    let cat_width = plot.size.width / num_cats as f32;
                    let label_color = axis_colors.label_color;
                    for (i, cat) in self.categories.iter().enumerate() {
                        let x = plot.origin.x + i as f32 * cat_width + cat_width * 0.5;
                        let label_w = super::super::render::estimate_text_width(cat, axis_font_size, self.text_measure.as_ref()).max(20.0);
                        let label_rect = Rect::new(
                            Point::new(x - label_w * 0.5, plot.origin.y + plot.size.height + 4.0),
                            Size::new(label_w, axis_font_size + 4.0),
                        );
                        list.push_text_centered(cat, label_rect, label_color, axis_font_size);
                    }
                }

                if self.x_axis.show_axis_line {
                    let y = plot.origin.y + plot.size.height;
                    let line_rect = Rect::new(
                        Point::new(plot.origin.x, y),
                        Size::new(plot.size.width, 1.0),
                    );
                    list.push_rect(line_rect, axis_colors.axis_color, [0.0; 4]);
                }
            }
            BarOrientation::Horizontal => {
                let mut ctx = CanvasContext::new(Point::zero(), Size::zero());
                render_x_axis(
                    list,
                    &mut ctx,
                    &self.layout,
                    &self.x_axis,
                    &self.value_scale,
                    &axis_colors,
                );
            }
        }

        if self.legend_config.position != LegendPosition::None && self.bar_series.len() > 1 {
            let legend_font = 12.0;
            let label_color = self.mss.color.map(|c| c.with_alpha(0.6)).unwrap_or(Color::from_hex("#64748b"));
            let names: Vec<&str> = self.bar_series.iter().map(|s| s.name.as_str()).collect();
            render_legend_items(
                list,
                &self.layout.legend_rect,
                &names,
                &self.resolved_colors,
                &self.anim.series_visibility,
                legend_font,
                label_color,
                self.text_measure.as_ref(),
            );
        }

        if self.tooltip_config.enabled {
            if let (Some(mouse), Some((si, ci))) = (self.mouse_pos, self.hovered_bar) {
                self.render_bar_tooltip(list, mouse, si, ci);
            }
        }
    }

    fn handle_event(
        &mut self,
        event: &Event,
        ctx: &mut crate::widget::context::EventContext,
    ) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if !self.bounds.contains(*pos) {
                    if self.hovered_bar.is_some() {
                        self.hovered_bar = None;
                        self.mouse_pos = None;
                        ctx.request_paint();
                    }
                    return EventResult::Ignored;
                }

                self.mouse_pos = Some(*pos);

                let new_hover = self.hit_test_bar(*pos);
                if new_hover != self.hovered_bar {
                    self.hovered_bar = new_hover;
                    self.anim.hover_point = new_hover;
                    ctx.request_paint();
                }

                EventResult::Handled
            }

            Event::MouseDown { button, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }

                if *button == MouseButton::Left {
                    for (i, rect) in self.legend_rects.iter().enumerate() {
                        if rect.contains(*position) {
                            self.anim.toggle_series(i);
                            self.compute_layout();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                }

                EventResult::Handled
            }

            _ => EventResult::Ignored,
        }
    }

    /// Первый кадр запускает появление, дальше тик нужен, пока идёт
    /// анимация появления, показа/скрытия рядов или подсветки точки.
    fn wants_animate_tick(&self) -> bool {
        (self.animate_enabled && !self.anim.appear_started) || self.anim.is_animating()
    }

    fn animate(&mut self, dt: Duration) -> bool {
        self.anim.tick(dt)
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
        self.compute_layout();
    }

    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags |= flags;
    }

    fn clear_dirty(&mut self, flags: DirtyFlags) {
        self.dirty_flags.remove(flags);
    }

    fn is_dirty(&self, flags: DirtyFlags) -> bool {
        self.dirty_flags.contains(flags)
    }

    fn id(&self) -> ElementId {
        self.id
    }

    fn set_id(&mut self, id: ElementId) {
        self.id = id;
    }

    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "BarChart"
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);

        if let Some(c) = style.get("grid-color").and_then(|v| v.as_color()) {
            self.mss_grid_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(c) = style.get("axis-color").and_then(|v| v.as_color()) {
            self.mss_axis_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(v) = style.get("axis-font-size").and_then(|v| v.as_px()) {
            self.mss_axis_font_size = Some(v);
        }
        if let Some(v) = style.get("title-font-size").and_then(|v| v.as_px()) {
            self.mss_title_font_size = Some(v);
        }
        if let Some(c) = style.get("tooltip-background").and_then(|v| v.as_color()) {
            self.mss_tooltip_bg = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(v) = style.get("animation-duration").and_then(|v| v.as_px()) {
            self.anim.set_appear_duration_ms(v);
        }

        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Group,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(format!(
                    "Bar chart with {} series and {} categories",
                    self.bar_series.len(),
                    self.categories.len(),
                )),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for BarChartElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
