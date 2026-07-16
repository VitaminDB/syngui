use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widget::context::TextMeasure;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use super::animation::ChartAnimationState;
use super::math::{polar_to_cartesian, regular_polygon_points};
use super::render::estimate_text_width;
use super::render::legend::{render_legend_items, legend_height};
use super::render::tooltip::TooltipColors;
use super::types::{
    LegendConfig, LegendPosition, RadarGridShape, RadarIndicator, RadarSeries,
    TooltipConfig, palette_color,
};

pub struct RadarChart {
    indicators: Vec<RadarIndicator>,
    radar_series: Vec<RadarSeries>,
    grid_shape: RadarGridShape,
    grid_levels: usize,
    legend: LegendConfig,
    tooltip: TooltipConfig,
    animate: bool,
    start_angle_deg: f32,
    width: Option<Dimension>,
    height: Option<Dimension>,
    title: Option<String>,
    classes: Vec<String>,
}

impl RadarChart {
    pub fn new() -> Self {
        Self {
            indicators: Vec::new(),
            radar_series: Vec::new(),
            grid_shape: RadarGridShape::default(),
            grid_levels: 5,
            legend: LegendConfig::default(),
            tooltip: TooltipConfig::default(),
            animate: true,
            start_angle_deg: 90.0,
            width: None,
            height: None,
            title: None,
            classes: Vec::new(),
        }
    }

    pub fn indicator(mut self, ind: RadarIndicator) -> Self {
        self.indicators.push(ind);
        self
    }

    pub fn indicators(mut self, inds: Vec<RadarIndicator>) -> Self {
        self.indicators = inds;
        self
    }

    pub fn radar_series(mut self, series: RadarSeries) -> Self {
        self.radar_series.push(series);
        self
    }

    pub fn grid_shape(mut self, shape: RadarGridShape) -> Self {
        self.grid_shape = shape;
        self
    }

    pub fn grid_levels(mut self, n: usize) -> Self {
        self.grid_levels = n.max(1);
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

    pub fn start_angle(mut self, deg: f32) -> Self {
        self.start_angle_deg = deg;
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

impl Default for RadarChart {
    fn default() -> Self { Self::new() }
}

impl Widget for RadarChart {
    fn create_element(&self) -> Box<dyn Element> {
        let num_series = self.radar_series.len();
        let resolved_colors: Vec<Color> = self.radar_series.iter().enumerate().map(|(i, s)| {
            s.color.unwrap_or_else(|| palette_color(i))
        }).collect();

        let mut anim = ChartAnimationState::default();
        anim.ensure_series_count(num_series);
        if !self.animate {
            anim.appear_progress = 1.0;
            anim.appear_eased = 1.0;
        }

        Box::new(RadarChartElement {
            id: ElementId::new(),
            indicators: self.indicators.clone(),
            radar_series: self.radar_series.clone(),
            grid_shape: self.grid_shape,
            grid_levels: self.grid_levels,
            legend_config: self.legend.clone(),
            tooltip_config: self.tooltip,
            animate_enabled: self.animate,
            start_angle: self.start_angle_deg.to_radians(),
            width: self.width,
            height: self.height,
            title: self.title.clone(),
            bounds: Rect::zero(),
            center: (0.0, 0.0),
            radius: 0.0,
            resolved_colors,
            data_points: Vec::new(),
            mouse_pos: None,
            hovered_series: None,
            anim,
            legend_rects: Vec::new(),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            mss_grid_color: None,
            mss_label_color: None,
            mss_label_font_size: None,
            text_measure: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
    fn widget_classes(&self) -> &[String] { &self.classes }
}

struct RadarChartElement {
    id: ElementId,

    indicators: Vec<RadarIndicator>,
    radar_series: Vec<RadarSeries>,
    grid_shape: RadarGridShape,
    grid_levels: usize,
    legend_config: LegendConfig,
    tooltip_config: TooltipConfig,
    animate_enabled: bool,
    start_angle: f32,
    width: Option<Dimension>,
    height: Option<Dimension>,
    title: Option<String>,

    bounds: Rect,
    center: (f32, f32),
    radius: f32,
    resolved_colors: Vec<Color>,
    data_points: Vec<Vec<(f32, f32)>>,

    mouse_pos: Option<Point>,
    hovered_series: Option<usize>,

    anim: ChartAnimationState,

    legend_rects: Vec<Rect>,

    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    mss_grid_color: Option<Color>,
    mss_label_color: Option<Color>,
    mss_label_font_size: Option<f32>,

    text_measure: Option<Arc<dyn TextMeasure>>,
}

impl RadarChartElement {
    fn compute_data_points(&self, cx: f32, cy: f32, radius: f32, appear_eased: f32) -> Vec<Vec<(f32, f32)>> {
        let n = self.indicators.len();
        if n == 0 {
            return Vec::new();
        }
        let step_angle = std::f32::consts::TAU / n as f32;

        self.radar_series.iter().enumerate().map(|(si, series)| {
            (0..n).map(|i| {
                let angle = self.start_angle + i as f32 * step_angle;
                let max_val = self.indicators[i].max;
                let val = series.data.get(i).copied().unwrap_or(0.0);
                let ratio = if max_val > 0.0 {
                    (val / max_val).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };
                let r = ratio * radius * appear_eased * self.anim.series_opacity(si);
                polar_to_cartesian(cx, cy, r, angle)
            }).collect()
        }).collect()
    }

    fn find_nearest_series(&self, mouse: Point) -> Option<usize> {
        let mut best_series = None;
        let mut best_dist = f32::MAX;
        let threshold = 30.0;
        for (si, points) in self.data_points.iter().enumerate() {
            if self.anim.series_opacity(si) < 0.1 {
                continue;
            }
            for &(px, py) in points {
                let dx = mouse.x - px;
                let dy = mouse.y - py;
                let d = (dx * dx + dy * dy).sqrt();
                if d < best_dist && d < threshold {
                    best_dist = d;
                    best_series = Some(si);
                }
            }
        }
        best_series
    }
}

impl Element for RadarChartElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<RadarChart>() {
            self.indicators = w.indicators.clone();
            self.radar_series = w.radar_series.clone();
            self.grid_shape = w.grid_shape;
            self.grid_levels = w.grid_levels;
            self.legend_config = w.legend.clone();
            self.tooltip_config = w.tooltip;
            self.animate_enabled = w.animate;
            self.start_angle = w.start_angle_deg.to_radians();
            self.width = w.width;
            self.height = w.height;
            self.title = w.title.clone();

            self.resolved_colors = w.radar_series.iter().enumerate().map(|(i, s)| {
                s.color.unwrap_or_else(|| palette_color(i))
            }).collect();

            self.anim.ensure_series_count(w.radar_series.len());
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self.mss.width
            .or(self.width)
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(350.0)
            .min(constraints.max_width);

        let h = self.mss.height
            .or(self.height)
            .map(|d| d.resolve(constraints.max_height))
            .unwrap_or(350.0)
            .min(constraints.max_height);

        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg_color = self.mss.background_color.unwrap_or(Color::WHITE);
        let border_radius = self.mss.border_radius_resolved(self.bounds.size.width, 0.0);
        let padding = self.mss.padding_ltrb([16.0; 4]);

        list.push_rect(self.bounds, bg_color, border_radius);

        if let Some(ref shadows) = self.mss.box_shadow {
            for shadow in shadows.0.iter() {
                list.push_shadow(
                    self.bounds, shadow.color, shadow.blur_radius,
                    (shadow.offset_x, shadow.offset_y), border_radius,
                );
            }
        }

        let n = self.indicators.len();
        if n < 3 {
            return;
        }

        let inner_w = self.bounds.size.width - padding[0] - padding[2];
        let inner_h = self.bounds.size.height - padding[1] - padding[3];
        let inner_x = self.bounds.origin.x + padding[0];
        let inner_y = self.bounds.origin.y + padding[1];

        let title_h = if self.title.is_some() { 24.0 } else { 0.0 };
        let legend_h = legend_height(self.legend_config.position, 12.0);
        let label_offset = 24.0;

        let chart_h = (inner_h - title_h - legend_h - label_offset * 2.0).max(60.0);
        let chart_w = (inner_w - label_offset * 2.0).max(60.0);
        let chart_size = chart_w.min(chart_h);

        let cx = inner_x + inner_w * 0.5;
        let cy = inner_y + title_h + label_offset + chart_size * 0.5;
        let radius = chart_size * 0.4;

        let grid_color = self.mss_grid_color
            .or(self.mss.color.map(|c| c.with_alpha(0.15)))
            .unwrap_or(Color::from_hex("#d1d5db"));
        let label_color = self.mss_label_color
            .or(self.mss.color.map(|c| c.with_alpha(0.6)))
            .unwrap_or(Color::from_hex("#64748b"));
        let label_font = self.mss_label_font_size.unwrap_or(11.0);
        let step_angle = std::f32::consts::TAU / n as f32;

        if let Some(ref title) = self.title {
            let title_font = 16.0;
            let title_w = estimate_text_width(title, title_font, self.text_measure.as_ref());
            let title_rect = Rect::new(
                Point::new(cx - title_w * 0.5, inner_y),
                Size::new(title_w, title_font + 4.0),
            );
            let title_color = self.mss.color.unwrap_or(Color::from_hex("#1e293b"));
            list.push_text_centered(title, title_rect, title_color, title_font);
        }

        let local_cx = cx - self.bounds.origin.x;
        let local_cy = cy - self.bounds.origin.y;

        let mut ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);
        ctx.set_color(grid_color);
        ctx.set_stroke_width(1.0);

        for level in 1..=self.grid_levels {
            let r = radius * level as f32 / self.grid_levels as f32;
            match self.grid_shape {
                RadarGridShape::Polygon => {
                    let points = regular_polygon_points(local_cx, local_cy, r, n, self.start_angle);
                    if points.len() >= 2 {
                        let mut closed = points.clone();
                        closed.push(points[0]);
                        ctx.draw_polyline(&closed);
                    }
                }
                RadarGridShape::Circle => {
                    ctx.draw_arc(local_cx, local_cy, r, 0.0, std::f32::consts::TAU);
                }
            }
        }

        for i in 0..n {
            let angle = self.start_angle + i as f32 * step_angle;
            let (ox, oy) = polar_to_cartesian(local_cx, local_cy, radius, angle);
            ctx.draw_line(local_cx, local_cy, ox, oy);
        }

        ctx.flush(list);

        for i in 0..n {
            let angle = self.start_angle + i as f32 * step_angle;
            let label_r = radius + 14.0;
            let (lx, ly) = polar_to_cartesian(cx, cy, label_r, angle);

            let name = &self.indicators[i].name;
            let text_w = estimate_text_width(name, label_font, self.text_measure.as_ref());
            let text_h = label_font + 2.0;
            let label_rect = Rect::new(
                Point::new(lx - text_w * 0.5, ly - text_h * 0.5),
                Size::new(text_w, text_h),
            );
            list.push_text_centered(name, label_rect, label_color, label_font);
        }

        let appear_eased = self.anim.appear_eased;
        let data_points = self.compute_data_points(cx, cy, radius, appear_eased);

        for (si, series) in self.radar_series.iter().enumerate() {
            let opacity = self.anim.series_opacity(si);
            if opacity < 0.01 {
                continue;
            }

            let color = self.resolved_colors.get(si).copied().unwrap_or(Color::from_hex("#5470c6"));
            let pts: Vec<(f32, f32)> = data_points.get(si).cloned().unwrap_or_default();
            if pts.is_empty() {
                continue;
            }

            let local_pts: Vec<(f32, f32)> = pts.iter().map(|&(px, py)| {
                (px - self.bounds.origin.x, py - self.bounds.origin.y)
            }).collect();

            let mut series_ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);

            let fill_color = color.with_alpha(series.area_opacity * opacity);
            series_ctx.set_color(fill_color);
            series_ctx.fill_polygon(&local_pts);

            series_ctx.set_color(color.with_alpha(opacity));
            series_ctx.set_stroke_width(series.line_width);
            let mut closed_pts = local_pts.clone();
            closed_pts.push(local_pts[0]);
            series_ctx.draw_polyline(&closed_pts);

            series_ctx.flush(list);

            if series.show_points {
                let marker_r = 3.5;
                let hovered = self.hovered_series == Some(si);
                let effective_r = if hovered { marker_r * 1.4 } else { marker_r };

                for &(px, py) in &pts {
                    let marker_rect = Rect::new(
                        Point::new(px - effective_r, py - effective_r),
                        Size::new(effective_r * 2.0, effective_r * 2.0),
                    );
                    list.push_rect(marker_rect, color.with_alpha(opacity), [effective_r; 4]);
                    let inner_r = effective_r * 0.5;
                    let inner_rect = Rect::new(
                        Point::new(px - inner_r, py - inner_r),
                        Size::new(inner_r * 2.0, inner_r * 2.0),
                    );
                    list.push_rect(inner_rect, bg_color.with_alpha(opacity), [inner_r; 4]);
                }
            }
        }

        if self.legend_config.position != LegendPosition::None && self.radar_series.len() > 1 {
            let legend_y = cy + chart_size * 0.5 + label_offset;
            let legend_rect = Rect::new(
                Point::new(inner_x, legend_y),
                Size::new(inner_w, legend_h),
            );
            let names: Vec<&str> = self.radar_series.iter().map(|s| s.name.as_str()).collect();
            let _hit_rects = render_legend_items(
                list,
                &legend_rect,
                &names,
                &self.resolved_colors,
                &self.anim.series_visibility,
                12.0,
                label_color,
                self.text_measure.as_ref(),
            );
        }

        if self.tooltip_config.enabled {
            if let (Some(mouse), Some(si)) = (self.mouse_pos, self.hovered_series) {
                let tooltip_opacity = self.anim.tooltip_opacity;
                if tooltip_opacity > 0.01 {
                    self.render_radar_tooltip(list, mouse, si, tooltip_opacity);
                }
            }
        }
    }

    fn handle_event(&mut self, event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if !self.bounds.contains(*pos) {
                    if self.mouse_pos.is_some() || self.hovered_series.is_some() {
                        self.mouse_pos = None;
                        self.hovered_series = None;
                        self.anim.hover_point = None;
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    return EventResult::Ignored;
                }

                self.mouse_pos = Some(*pos);
                let new_hover = self.find_nearest_series(*pos);
                if new_hover != self.hovered_series {
                    self.hovered_series = new_hover;
                    self.anim.hover_point = new_hover.map(|si| (si, 0));
                    self.mark_dirty(DirtyFlags::RENDER);
                }
                EventResult::Handled
            }
            Event::MouseDown { button: MouseButton::Left, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                for (i, rect) in self.legend_rects.iter().enumerate() {
                    if rect.contains(*position) {
                        self.anim.toggle_series(i);
                        self.mark_dirty(DirtyFlags::RENDER);
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if !self.anim.appear_started && self.animate_enabled {
            self.anim.start_appear();
        }

        let n = self.indicators.len();
        if n >= 3 {
            let padding = self.mss.padding_ltrb([16.0; 4]);
            let inner_w = self.bounds.size.width - padding[0] - padding[2];
            let inner_h = self.bounds.size.height - padding[1] - padding[3];
            let inner_x = self.bounds.origin.x + padding[0];
            let inner_y = self.bounds.origin.y + padding[1];

            let title_h = if self.title.is_some() { 24.0 } else { 0.0 };
            let legend_h = legend_height(self.legend_config.position, 12.0);
            let label_offset = 24.0;
            let chart_h = (inner_h - title_h - legend_h - label_offset * 2.0).max(60.0);
            let chart_w = (inner_w - label_offset * 2.0).max(60.0);
            let chart_size = chart_w.min(chart_h);

            let cx = inner_x + inner_w * 0.5;
            let cy = inner_y + title_h + label_offset + chart_size * 0.5;
            let radius = chart_size * 0.4;

            self.center = (cx, cy);
            self.radius = radius;
            self.data_points = self.compute_data_points(cx, cy, radius, self.anim.appear_eased);
        }

        self.anim.tick(dt)
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
    }

    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, tree: &mut ElementTree) { self.text_measure = tree.text_measure.clone(); }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "RadarChart" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);

        if let Some(c) = style.get("grid-color").and_then(|v| v.as_color()) {
            self.mss_grid_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(c) = style.get("label-color").and_then(|v| v.as_color()) {
            self.mss_label_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(v) = style.get("label-font-size").and_then(|v| v.as_px()) {
            self.mss_label_font_size = Some(v);
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
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Group,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(format!(
                    "Radar chart with {} indicators and {} series",
                    self.indicators.len(),
                    self.radar_series.len(),
                )),
                ..Default::default()
            },
        })
    }
}

impl RadarChartElement {
    fn render_radar_tooltip(&self, list: &mut DisplayList, mouse: Point, si: usize, opacity: f32) {
        let series = match self.radar_series.get(si) {
            Some(s) => s,
            None => return,
        };

        let colors = TooltipColors::default();
        let line_height = colors.font_size + 4.0;
        let padding = 8.0;

        let mut lines: Vec<(String, Color)> = Vec::new();
        let series_color = self.resolved_colors.get(si).copied().unwrap_or(Color::WHITE);
        lines.push((series.name.clone(), series_color));

        for (i, ind) in self.indicators.iter().enumerate() {
            let val = series.data.get(i).copied().unwrap_or(0.0);
            lines.push((format!("{}: {:.1}", ind.name, val), colors.text_color));
        }

        let max_text_width = lines
            .iter()
            .map(|(text, _)| estimate_text_width(text, colors.font_size, self.text_measure.as_ref()))
            .fold(0.0_f32, f32::max);
        let tooltip_width = max_text_width + padding * 2.0;
        let tooltip_height = lines.len() as f32 * line_height + padding * 2.0;

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

        let tooltip_rect = Rect::new(
            Point::new(x, y),
            Size::new(tooltip_width, tooltip_height),
        );

        list.push_shadow(
            tooltip_rect,
            Color::new(0.0, 0.0, 0.0, 0.2 * opacity),
            8.0,
            (0.0, 2.0),
            [6.0; 4],
        );

        list.push_rect(
            tooltip_rect,
            colors.background.with_alpha(opacity * 0.95),
            [6.0; 4],
        );

        let mut text_y = y + padding;
        for (i, (text, color)) in lines.iter().enumerate() {
            let text_rect = Rect::new(
                Point::new(x + padding, text_y),
                Size::new(max_text_width, line_height),
            );
            let text_color = if i == 0 {
                color.with_alpha(opacity)
            } else {
                color.with_alpha(opacity * 0.85)
            };
            list.push_text(text, text_rect, text_color, colors.font_size);
            text_y += line_height;
        }
    }
}

impl StyledElement for RadarChartElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
