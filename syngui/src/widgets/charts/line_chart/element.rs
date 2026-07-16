use super::LineChart;
use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult};
use crate::layout::Constraints;
use crate::mss::MssFields;
use crate::mss::{ComputedStyle, Dimension};
use crate::render::DisplayList;
use crate::widget::context::{EventContextExt, TextMeasure};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};

use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;
use std::time::Duration;

use super::super::animation::ChartAnimationState;
use super::super::math::{data_extent, nearest_point_index, LinearScale};
use super::super::render::{axis, legend, series, tooltip};
use super::super::types::*;

impl Widget for LineChart {
    fn create_element(&self) -> Box<dyn Element> {
        let mut anim = ChartAnimationState::default();
        anim.ensure_series_count(self.series.len());
        if self.animate {
            anim.start_appear();
        } else {
            anim.appear_progress = 1.0;
            anim.appear_eased = 1.0;
            anim.appear_started = true;
        }

        Box::new(LineChartElement {
            id: ElementId::new(),
            series: self.series.clone(),
            x_axis: self.x_axis.clone(),
            y_axis: self.y_axis.clone(),
            legend_config: self.legend.clone(),
            tooltip_config: self.tooltip,
            animate_enabled: self.animate,
            zoom_enabled: self.zoom_enabled,
            title: self.title.clone(),
            mark_lines: self.mark_lines.clone(),
            on_point_click: self.on_point_click.clone(),
            width: self.width,
            height: self.height,
            bounds: Rect::zero(),
            layout: ChartLayout::default(),
            x_scale: LinearScale::new((0.0, 1.0), (0.0, 100.0)),
            y_scale: LinearScale::new((0.0, 1.0), (100.0, 0.0)),
            resolved_colors: Vec::new(),
            screen_points: Vec::new(),
            mouse_pos: None,
            dragging: false,
            drag_start: None,
            drag_pan_start: (0.0, 0.0),
            anim,
            legend_rects: Vec::new(),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            mss_grid_color: None,
            mss_axis_color: None,
            mss_axis_font_size: None,
            mss_title_font_size: None,
            mss_legend_font_size: None,
            mss_tooltip_bg: None,
            mss_tooltip_border: None,
            mss_animation_duration: None,
            mss_point_size: None,
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

struct LineChartElement {
    id: ElementId,

    series: Vec<Series>,
    x_axis: AxisConfig,
    y_axis: AxisConfig,
    legend_config: LegendConfig,
    tooltip_config: TooltipConfig,
    animate_enabled: bool,
    zoom_enabled: bool,
    title: Option<String>,
    mark_lines: Vec<MarkLine>,
    on_point_click: Option<Arc<Mutex<dyn FnMut(usize, usize, &DataPoint) + Send>>>,

    width: Option<Dimension>,
    height: Option<Dimension>,
    bounds: Rect,

    layout: ChartLayout,
    x_scale: LinearScale,
    y_scale: LinearScale,
    resolved_colors: Vec<Color>,
    screen_points: Vec<Vec<(f32, f32)>>,

    mouse_pos: Option<Point>,
    dragging: bool,
    drag_start: Option<Point>,
    drag_pan_start: (f64, f64),

    anim: ChartAnimationState,

    legend_rects: Vec<Rect>,

    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,

    mss_grid_color: Option<Color>,
    mss_axis_color: Option<Color>,
    mss_axis_font_size: Option<f32>,
    mss_title_font_size: Option<f32>,
    mss_legend_font_size: Option<f32>,
    mss_tooltip_bg: Option<Color>,
    mss_tooltip_border: Option<Color>,
    mss_animation_duration: Option<f32>,
    mss_point_size: Option<f32>,

    text_measure: Option<Arc<dyn TextMeasure>>,
}

impl LineChartElement {
    fn resolve_colors(&mut self) {
        self.resolved_colors = self
            .series
            .iter()
            .enumerate()
            .map(|(i, s)| s.style.color.unwrap_or_else(|| palette_color(i)))
            .collect();
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
        let legend_font_size = self.mss_legend_font_size.unwrap_or(12.0);

        let title_h = if self.title.is_some() {
            title_font_size + 8.0
        } else {
            0.0
        };

        let legend_h = legend::legend_height(self.legend_config.position, legend_font_size);

        let x_axis_h = axis_font_size
            + 8.0
            + if self.x_axis.title.is_some() {
                title_font_size + 4.0
            } else {
                0.0
            };

        let visible: Vec<bool> = (0..self.series.len())
            .map(|i| self.anim.is_series_visible(i))
            .collect();

        let (x_min, x_max, y_min, y_max) = data_extent(&self.series, &visible);

        let (x_min, x_max, y_min, y_max) = self.apply_zoom_pan(x_min, x_max, y_min, y_max);

        let x_min = self.x_axis.min.unwrap_or(x_min);
        let x_max = self.x_axis.max.unwrap_or(x_max);
        let y_min = self.y_axis.min.unwrap_or(y_min);
        let y_max = self.y_axis.max.unwrap_or(y_max);

        let y_axis_w = axis::estimate_y_axis_width(&self.y_axis, y_min, y_max, axis_font_size, self.text_measure.as_ref());

        let (legend_top, legend_bottom) = match self.legend_config.position {
            LegendPosition::Top => (legend_h, 0.0),
            LegendPosition::Bottom => (0.0, legend_h),
            _ => (0.0, 0.0),
        };

        let has_visual_map = self.series.iter().any(|s| s.style.visual_map.is_some());
        let visual_map_w = if has_visual_map { 100.0 } else { 0.0 };

        let plot_x = inner_x + y_axis_w;
        let plot_y = inner_y + title_h + legend_top;
        let plot_w = (inner_w - y_axis_w - visual_map_w).max(0.0);
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

        self.x_scale = LinearScale::new((x_min, x_max), (0.0, plot_w));
        self.y_scale = if self.y_axis.inverse {
            LinearScale::new((y_min, y_max), (0.0, plot_h))
        } else {
            LinearScale::new((y_min, y_max), (plot_h, 0.0))
        };

        self.compute_screen_points();
    }

    fn apply_zoom_pan(
        &self,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
    ) -> (f64, f64, f64, f64) {
        if self.anim.zoom_level == 1.0 && self.anim.pan_offset == (0.0, 0.0) {
            return (x_min, x_max, y_min, y_max);
        }

        let x_range = x_max - x_min;
        let y_range = y_max - y_min;
        let zoom = self.anim.zoom_level as f64;

        let zoomed_x_range = x_range / zoom;
        let zoomed_y_range = y_range / zoom;

        let x_center = (x_min + x_max) * 0.5 + self.anim.pan_offset.0;
        let y_center = (y_min + y_max) * 0.5 + self.anim.pan_offset.1;

        (
            x_center - zoomed_x_range * 0.5,
            x_center + zoomed_x_range * 0.5,
            y_center - zoomed_y_range * 0.5,
            y_center + zoomed_y_range * 0.5,
        )
    }

    fn compute_screen_points(&mut self) {
        self.screen_points = self
            .series
            .iter()
            .map(|s| {
                s.data
                    .iter()
                    .map(|dp| {
                        let x = self.x_scale.map(dp.x);
                        let y = self.y_scale.map(dp.y);
                        (x, y)
                    })
                    .collect()
            })
            .collect();
    }

    fn axis_colors(&self) -> axis::AxisColors {
        let default = axis::AxisColors::default();
        axis::AxisColors {
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

    fn tooltip_colors(&self) -> tooltip::TooltipColors {
        let default = tooltip::TooltipColors::default();
        tooltip::TooltipColors {
            background: self.mss_tooltip_bg.unwrap_or(default.background),
            border_color: self.mss_tooltip_border.unwrap_or(default.border_color),
            text_color: default.text_color,
            font_size: self.mss_legend_font_size.unwrap_or(default.font_size),
        }
    }

    fn find_nearest_point(&self, mouse: Point) -> Option<(usize, usize)> {
        let plot = &self.layout.plot_rect;

        let rel_x = mouse.x - plot.origin.x;
        let rel_y = mouse.y - plot.origin.y;

        if rel_x < -10.0
            || rel_x > plot.size.width + 10.0
            || rel_y < -10.0
            || rel_y > plot.size.height + 10.0
        {
            return None;
        }

        let data_x = self.x_scale.invert(rel_x);

        let mut best: Option<(usize, usize, f64)> = None;

        for (si, s) in self.series.iter().enumerate() {
            if !self.anim.is_series_visible(si) {
                continue;
            }
            if let Some(pi) = nearest_point_index(&s.data, data_x) {
                let dp = &s.data[pi];
                let sx = self.x_scale.map(dp.x);
                let sy = self.y_scale.map(dp.y);
                let dist = ((rel_x - sx) as f64).powi(2) + ((rel_y - sy) as f64).powi(2);
                if let Some((_, _, best_dist)) = best {
                    if dist < best_dist {
                        best = Some((si, pi, dist));
                    }
                } else {
                    best = Some((si, pi, dist));
                }
            }
        }

        best.filter(|(_, _, d)| *d < 900.0)
            .map(|(si, pi, _)| (si, pi))
    }
}

impl Element for LineChartElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<LineChart>() {
            self.series = w.series.clone();
            self.x_axis = w.x_axis.clone();
            self.y_axis = w.y_axis.clone();
            self.legend_config = w.legend.clone();
            self.tooltip_config = w.tooltip;
            self.animate_enabled = w.animate;
            self.zoom_enabled = w.zoom_enabled;
            self.title = w.title.clone();
            self.mark_lines = w.mark_lines.clone();
            self.on_point_click = w.on_point_click.clone();
            self.width = w.width;
            self.height = w.height;

            self.anim.ensure_series_count(self.series.len());
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

        {
            let mut ctx = CanvasContext::new(Point::zero(), Size::zero());
            axis::render_y_axis(
                list,
                &mut ctx,
                &self.layout,
                &self.y_axis,
                &self.y_scale,
                &axis_colors,
            );
        }

        list.push_clip(Rect::new(plot.origin, plot.size));

        {
            let mut ctx = CanvasContext::new(plot.origin, plot.size);

            axis::render_x_axis(
                list,
                &mut ctx,
                &self.layout,
                &self.x_axis,
                &self.x_scale,
                &axis_colors,
            );

            axis::render_y_axis(
                list,
                &mut ctx,
                &self.layout,
                &self.y_axis,
                &self.y_scale,
                &axis_colors,
            );

            let has_visual_map = self.series.iter().any(|s| s.style.visual_map.is_some());

            for ml in &self.mark_lines {
                let y = self.y_scale.map(ml.value);
                if y >= 0.0 && y <= plot.size.height {
                    let ml_color = ml.color.unwrap_or(axis_colors.axis_color.with_alpha(0.5));
                    if ml.dashed {
                        ctx.save();
                        ctx.set_color(ml_color);
                        ctx.set_stroke_width(1.0);
                        let dash_segments = crate::widgets::charts::math::segment_dashed(
                            &[(0.0, y), (plot.size.width, y)],
                            6.0, 4.0,
                        );
                        for seg in &dash_segments {
                            ctx.draw_polyline(seg);
                        }
                        ctx.restore();
                    } else {
                        ctx.save();
                        ctx.set_color(ml_color);
                        ctx.set_stroke_width(1.0);
                        ctx.draw_line(0.0, y, plot.size.width, y);
                        ctx.restore();
                    }
                    if !has_visual_map {
                        let label_text = ml.label.as_deref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| crate::widgets::charts::math::format_tick_value(ml.value));
                        let label_rect = Rect::new(
                            Point::new(plot.origin.x + plot.size.width + 4.0, plot.origin.y + y - 6.0),
                            Size::new(40.0, 12.0),
                        );
                        list.push_text(&label_text, label_rect, ml_color, axis_colors.axis_font_size);
                    }
                }
            }

            let appear = self.anim.appear_eased;

            for (i, s) in self.series.iter().enumerate() {
                let opacity = self.anim.series_opacity(i);
                if opacity < 0.01 {
                    continue;
                }
                if let Some(ref fill) = s.style.area_fill {
                    if let Some(pts) = self.screen_points.get(i) {
                        let color = self.resolved_colors[i].with_alpha(opacity);
                        let (d_min, d_max) = self.y_scale.domain;
                        let baseline_y = if !self.y_axis.inverse && d_min < 0.0 && d_max > 0.0 {
                            self.y_scale.map(0.0)
                        } else if self.y_axis.inverse {
                            self.y_scale.map(d_max)
                        } else {
                            self.y_scale.map(d_min)
                        };
                        series::render_area(&mut ctx, pts, baseline_y, color, fill, appear, s.style.smooth);
                    }
                }
            }

            ctx.flush(list);

            for (i, s) in self.series.iter().enumerate() {
                let opacity = self.anim.series_opacity(i);
                if opacity < 0.01 {
                    continue;
                }
                if let Some(pts) = self.screen_points.get(i) {
                    let color = self.resolved_colors[i].with_alpha(opacity);
                    if s.style.visual_map.is_some() {
                        let mut line_ctx = CanvasContext::new(plot.origin, plot.size);
                        series::render_line(&mut line_ctx, pts, &s.data, &s.style, color, appear);
                        line_ctx.flush(list);
                    } else {
                        series::render_line_gpu(
                            list, pts, &s.style, color, appear, plot.origin, s.style.smooth,
                        );
                    }
                }
            }
        }

        for (i, s) in self.series.iter().enumerate() {
            let opacity = self.anim.series_opacity(i);
            if opacity < 0.01 || !s.style.show_points {
                continue;
            }
            if let Some(pts) = self.screen_points.get(i) {
                let color = self.resolved_colors[i].with_alpha(opacity);
                let hover_idx =
                    self.anim
                        .hover_point
                        .and_then(|(si, pi)| if si == i { Some(pi) } else { None });
                let style = if let Some(ps) = self.mss_point_size {
                    let mut st = s.style.clone();
                    st.point_size = ps;
                    st
                } else {
                    s.style.clone()
                };
                series::render_points(
                    list,
                    pts,
                    &style,
                    color,
                    hover_idx,
                    self.anim.hover_t,
                    self.anim.appear_eased,
                    plot.origin,
                );
            }
        }

        list.pop_clip();

        {
            let mut ctx = CanvasContext::new(Point::zero(), Size::zero());
            axis::render_x_axis(
                list,
                &mut ctx,
                &self.layout,
                &self.x_axis,
                &self.x_scale,
                &axis_colors,
            );
        }

        if self.legend_config.position != LegendPosition::None && self.series.len() > 1 {
            let legend_font = self.mss_legend_font_size.unwrap_or(12.0);
            let label_color = self.mss.color.map(|c| c.with_alpha(0.6)).unwrap_or(Color::from_hex("#64748b"));
            legend::render_legend(
                list,
                &self.layout.legend_rect,
                &self.series,
                &self.resolved_colors,
                &self.anim.series_visibility,
                legend_font,
                label_color,
                self.text_measure.as_ref(),
            );
        }

        for s in &self.series {
            if let Some(ref vm) = s.style.visual_map {
                let label_color = self.mss.color.map(|c| c.with_alpha(0.6)).unwrap_or(Color::from_hex("#64748b"));
                let font_size = self.mss_legend_font_size.unwrap_or(11.0);
                series::render_visual_map_legend(list, vm, &self.layout.plot_rect, font_size, label_color);
                break;
            }
        }

        if let Some(hover) = self.anim.hover_point {
            if let Some(s) = self.series.get(hover.0) {
                if let Some(dp) = s.data.get(hover.1) {
                    let x_pixel = self.x_scale.map(dp.x);
                    let crosshair_color = self.mss_grid_color.unwrap_or(Color::from_hex("#94a3b8"));
                    tooltip::render_crosshair(
                        list,
                        &self.layout.plot_rect,
                        x_pixel,
                        self.anim.tooltip_opacity,
                        crosshair_color,
                    );
                }
            }
        }

        if self.tooltip_config.enabled {
            if let (Some(mouse), Some(hover)) = (self.mouse_pos, self.anim.hover_point) {
                if self.anim.tooltip_opacity > 0.01 {
                    let tc = self.tooltip_colors();
                    tooltip::render_tooltip(
                        list,
                        mouse,
                        &self.bounds,
                        &self.series,
                        &self.resolved_colors,
                        &self.anim.series_visibility,
                        hover,
                        self.tooltip_config.shared,
                        &self.x_scale,
                        &self.y_scale,
                        self.anim.tooltip_opacity,
                        &tc,
                        &self.x_axis.format_fn,
                        &self.y_axis.format_fn,
                        self.text_measure.as_ref(),
                    );
                }
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
                    if self.anim.hover_point.is_some() {
                        self.anim.hover_point = None;
                        ctx.request_paint();
                    }
                    return EventResult::Ignored;
                }

                self.mouse_pos = Some(*pos);

                if self.dragging {
                    if let Some(start) = self.drag_start {
                        let dx = pos.x - start.x;
                        let dy = pos.y - start.y;
                        let plot_w = self.layout.plot_rect.size.width;
                        let plot_h = self.layout.plot_rect.size.height;
                        let x_range = self.x_scale.domain.1 - self.x_scale.domain.0;
                        let y_range = self.y_scale.domain.1 - self.y_scale.domain.0;

                        self.anim.pan_offset.0 =
                            self.drag_pan_start.0 - (dx as f64 / plot_w as f64) * x_range;
                        self.anim.pan_offset.1 =
                            self.drag_pan_start.1 + (dy as f64 / plot_h as f64) * y_range;

                        self.compute_layout();
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                let new_hover = self.find_nearest_point(*pos);
                if new_hover != self.anim.hover_point {
                    self.anim.hover_point = new_hover;
                    ctx.request_paint();
                }

                if self.layout.plot_rect.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Crosshair);
                }

                EventResult::Handled
            }

            Event::MouseDown { button, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }

                if *button == crate::input::MouseButton::Left {
                    for (i, rect) in self.legend_rects.iter().enumerate() {
                        if rect.contains(*position) {
                            self.anim.toggle_series(i);
                            self.compute_layout();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }

                    if let Some((si, pi)) = self.anim.hover_point {
                        if let Some(dp) = self.series.get(si).and_then(|s| s.data.get(pi)) {
                            if let Some(ref cb) = self.on_point_click {
                                if let Ok(mut f) = cb.lock() {
                                    f(si, pi, dp);
                                }
                            }
                            return EventResult::Handled;
                        }
                    }

                    if self.zoom_enabled && self.layout.plot_rect.contains(*position) {
                        self.dragging = true;
                        self.drag_start = Some(*position);
                        self.drag_pan_start = self.anim.pan_offset;
                        ctx.set_cursor(CursorIcon::Grabbing);
                        return EventResult::Handled;
                    }
                }

                EventResult::Handled
            }

            Event::MouseUp { button, .. } => {
                if *button == crate::input::MouseButton::Left && self.dragging {
                    self.dragging = false;
                    self.drag_start = None;
                    ctx.set_cursor(CursorIcon::Crosshair);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }

            Event::MouseWheel {
                delta,
                delta_x: _,
                position,
            } => {
                if !self.zoom_enabled || !self.layout.plot_rect.contains(*position) {
                    return EventResult::Ignored;
                }

                let rel_x = position.x - self.layout.plot_rect.origin.x;
                let rel_y = position.y - self.layout.plot_rect.origin.y;
                let focal_x = self.x_scale.invert(rel_x);
                let focal_y = self.y_scale.invert(rel_y);

                let zoom_factor = if *delta > 0.0 { 1.15 } else { 1.0 / 1.15 };
                let view_center = (
                    (self.x_scale.domain.0 + self.x_scale.domain.1) / 2.0,
                    (self.y_scale.domain.0 + self.y_scale.domain.1) / 2.0,
                );
                self.anim.zoom_at(focal_x, focal_y, zoom_factor, view_center);
                self.compute_layout();
                ctx.request_paint();

                EventResult::Handled
            }

            _ => EventResult::Ignored,
        }
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
        "LineChart"
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
        if let Some(v) = style.get("legend-font-size").and_then(|v| v.as_px()) {
            self.mss_legend_font_size = Some(v);
        }
        if let Some(c) = style.get("tooltip-background").and_then(|v| v.as_color()) {
            self.mss_tooltip_bg = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(c) = style.get("tooltip-border-color").and_then(|v| v.as_color()) {
            self.mss_tooltip_border = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(v) = style.get("animation-duration").and_then(|v| v.as_px()) {
            self.mss_animation_duration = Some(v);
            self.anim.set_appear_duration_ms(v);
        }
        if let Some(v) = style.get("point-size").and_then(|v| v.as_px()) {
            self.mss_point_size = Some(v);
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
                label: Some(format!("Line chart with {} series", self.series.len())),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for LineChartElement {
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
