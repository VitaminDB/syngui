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
use std::f32::consts::TAU;
use std::sync::Arc;
use std::time::Duration;

use super::animation::ChartAnimationState;
use super::math::polar_to_cartesian;
use super::render::estimate_text_width;
use super::render::legend::{render_legend_items, legend_height};
use super::types::{LegendConfig, LegendPosition, PieLabelPosition, PieSlice, TooltipConfig, palette_color};
use super::render::tooltip::TooltipColors;

fn arc_quads(cx: f32, cy: f32, outer_r: f32, inner_r: f32, start: f32, end: f32) -> Vec<Vec<(f32, f32)>> {
    let steps = ((end - start).abs() / 0.035).max(2.0) as usize;
    let mut quads = Vec::with_capacity(steps);

    if inner_r > 0.01 {
        for i in 0..steps {
            let t0 = i as f32 / steps as f32;
            let t1 = (i + 1) as f32 / steps as f32;
            let a0 = start + t0 * (end - start);
            let a1 = start + t1 * (end - start);
            let o0 = polar_to_cartesian(cx, cy, outer_r, a0);
            let o1 = polar_to_cartesian(cx, cy, outer_r, a1);
            let i1 = polar_to_cartesian(cx, cy, inner_r, a1);
            let i0 = polar_to_cartesian(cx, cy, inner_r, a0);
            quads.push(vec![o0, o1, i1, i0]);
        }
    } else {
        for i in 0..steps {
            let t0 = i as f32 / steps as f32;
            let t1 = (i + 1) as f32 / steps as f32;
            let a0 = start + t0 * (end - start);
            let a1 = start + t1 * (end - start);
            let p0 = polar_to_cartesian(cx, cy, outer_r, a0);
            let p1 = polar_to_cartesian(cx, cy, outer_r, a1);
            quads.push(vec![(cx, cy), p0, p1]);
        }
    }

    quads
}

fn hit_test_slice(
    mx: f32, my: f32,
    cx: f32, cy: f32,
    outer_r: f32, inner_r: f32,
    start: f32, end: f32,
) -> bool {
    let dx = mx - cx;
    let dy = -(my - cy);
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < inner_r || dist > outer_r {
        return false;
    }
    let mut angle = dy.atan2(dx);
    if angle < 0.0 { angle += TAU; }
    let mut s = start % TAU;
    if s < 0.0 { s += TAU; }
    let mut e = end % TAU;
    if e < 0.0 { e += TAU; }
    if s < e {
        angle >= s && angle <= e
    } else {
        angle >= s || angle <= e
    }
}

pub struct PieChart {
    slices: Vec<PieSlice>,
    inner_radius: f32,
    label_position: PieLabelPosition,
    show_percentage: bool,
    legend: LegendConfig,
    tooltip: TooltipConfig,
    animate: bool,
    start_angle_deg: f32,
    width: Option<Dimension>,
    height: Option<Dimension>,
    title: Option<String>,
    classes: Vec<String>,
}

impl PieChart {
    pub fn new() -> Self {
        Self {
            slices: Vec::new(),
            inner_radius: 0.0,
            label_position: PieLabelPosition::Outside,
            show_percentage: true,
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

    pub fn slice(mut self, slice: PieSlice) -> Self {
        self.slices.push(slice);
        self
    }

    pub fn slices(mut self, slices: Vec<PieSlice>) -> Self {
        self.slices = slices;
        self
    }

    pub fn donut(mut self, ratio: f32) -> Self {
        self.inner_radius = ratio.clamp(0.0, 0.95);
        self
    }

    pub fn label_position(mut self, pos: PieLabelPosition) -> Self {
        self.label_position = pos;
        self
    }

    pub fn show_percentage(mut self, show: bool) -> Self {
        self.show_percentage = show;
        self
    }

    pub fn legend(mut self, pos: LegendPosition) -> Self {
        self.legend = LegendConfig::new(pos);
        self
    }

    pub fn tooltip(mut self, enabled: bool) -> Self {
        self.tooltip = TooltipConfig::enabled(enabled);
        self
    }

    pub fn tooltip_config(mut self, cfg: TooltipConfig) -> Self {
        self.tooltip = cfg;
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

impl Default for PieChart {
    fn default() -> Self { Self::new() }
}

impl Widget for PieChart {
    fn create_element(&self) -> Box<dyn Element> {
        let count = self.slices.len();
        let resolved_colors: Vec<Color> = self.slices.iter().enumerate().map(|(i, s)| {
            s.color.unwrap_or_else(|| palette_color(i))
        }).collect();

        let mut anim = ChartAnimationState::default();
        anim.ensure_series_count(count);
        if !self.animate {
            anim.appear_progress = 1.0;
            anim.appear_eased = 1.0;
        }

        Box::new(PieChartElement {
            id: ElementId::new(),
            slices: self.slices.clone(),
            inner_radius: self.inner_radius,
            label_position: self.label_position,
            show_percentage: self.show_percentage,
            legend_config: self.legend.clone(),
            tooltip_config: self.tooltip,
            start_angle: self.start_angle_deg.to_radians(),
            animate_enabled: self.animate,
            width: self.width,
            height: self.height,
            title: self.title.clone(),
            bounds: Rect::zero(),
            resolved_colors,
            slice_angles: Vec::new(),
            mouse_pos: None,
            hovered_slice: None,
            explode_progress: vec![0.0; count],
            anim,
            legend_rects: Vec::new(),
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
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

struct PieChartElement {
    id: ElementId,

    slices: Vec<PieSlice>,
    inner_radius: f32,
    label_position: PieLabelPosition,
    show_percentage: bool,
    legend_config: LegendConfig,
    tooltip_config: TooltipConfig,
    start_angle: f32,
    animate_enabled: bool,
    width: Option<Dimension>,
    height: Option<Dimension>,
    title: Option<String>,

    bounds: Rect,
    resolved_colors: Vec<Color>,
    slice_angles: Vec<(f32, f32)>,

    mouse_pos: Option<Point>,
    hovered_slice: Option<usize>,
    explode_progress: Vec<f32>,

    anim: ChartAnimationState,

    legend_rects: Vec<Rect>,

    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,

    mss_label_color: Option<Color>,
    mss_label_font_size: Option<f32>,

    text_measure: Option<Arc<dyn TextMeasure>>,
}

impl PieChartElement {
    fn compute_angles(&mut self) {
        let count = self.slices.len();
        self.slice_angles.clear();
        self.slice_angles.reserve(count);

        let total: f64 = self.slices.iter().enumerate()
            .filter(|(i, _)| self.anim.is_series_visible(*i))
            .map(|(_, s)| s.value.max(0.0))
            .sum();

        if total <= 0.0 || count == 0 {
            for _ in 0..count {
                self.slice_angles.push((self.start_angle, self.start_angle));
            }
            return;
        }

        let mut cumulative = 0.0_f64;
        for (i, slice) in self.slices.iter().enumerate() {
            let start_frac = cumulative / total;
            let value = if self.anim.is_series_visible(i) { slice.value.max(0.0) } else { 0.0 };
            cumulative += value;
            let end_frac = cumulative / total;

            let start_rad = self.start_angle + start_frac as f32 * TAU;
            let end_rad = self.start_angle + end_frac as f32 * TAU;
            self.slice_angles.push((start_rad, end_rad));
        }
    }

    fn pie_geometry(&self, padding: [f32; 4], legend_h: f32, title_h: f32) -> (f32, f32, f32, f32) {
        let inner_w = self.bounds.size.width - padding[0] - padding[2];
        let inner_h = self.bounds.size.height - padding[1] - padding[3] - legend_h - title_h;
        // Резерв под внешние подписи: выносная линия (+24) + строка текста.
        // 70px было чрезмерно и «схлопывало» кольцо, особенно когда снизу есть
        // легенда и по высоте остаётся мало места.
        let label_margin = match self.label_position {
            PieLabelPosition::Outside => 46.0,
            _ => 0.0,
        };
        let available = (inner_w - label_margin * 2.0).min(inner_h - label_margin * 2.0).max(20.0);
        let outer_r = available * 0.5;
        let inner_r = outer_r * self.inner_radius;

        let cx = self.bounds.origin.x + padding[0] + inner_w * 0.5;
        let cy = self.bounds.origin.y + padding[1] + title_h + (inner_h * 0.5);

        (cx, cy, outer_r, inner_r)
    }

    fn format_label(&self, slice: &PieSlice, total: f64) -> String {
        if self.show_percentage && total > 0.0 {
            let pct = (slice.value / total * 100.0).round() as i32;
            format!("{} {}%", slice.label, pct)
        } else {
            slice.label.clone()
        }
    }
}

impl Element for PieChartElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<PieChart>() {
            self.slices = w.slices.clone();
            self.inner_radius = w.inner_radius;
            self.label_position = w.label_position;
            self.show_percentage = w.show_percentage;
            self.legend_config = w.legend.clone();
            self.tooltip_config = w.tooltip;
            self.start_angle = w.start_angle_deg.to_radians();
            self.animate_enabled = w.animate;
            self.width = w.width;
            self.height = w.height;
            self.title = w.title.clone();

            let count = w.slices.len();
            self.resolved_colors = w.slices.iter().enumerate().map(|(i, s)| {
                s.color.unwrap_or_else(|| palette_color(i))
            }).collect();

            self.anim.ensure_series_count(count);

            self.explode_progress.resize(count, 0.0);

            self.compute_angles();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self.mss.width
            .or(self.width)
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(300.0)
            .min(constraints.max_width);

        let h = self.mss.height
            .or(self.height)
            .map(|d| d.resolve(constraints.max_height))
            .unwrap_or(300.0)
            .min(constraints.max_height);

        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        self.compute_angles();
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg_color = self.mss.background_color.unwrap_or(Color::TRANSPARENT);
        let border_radius = self.mss.border_radius_resolved(self.bounds.size.width, 0.0);
        let padding = self.mss.padding_ltrb([16.0; 4]);
        let label_color = self.mss_label_color
            .or(self.mss.color.map(|c| c.with_alpha(0.6)))
            .unwrap_or(Color::from_hex("#64748b"));
        let label_font = self.mss_label_font_size.unwrap_or(11.0);
        let legend_font = label_font;

        list.push_rect(self.bounds, bg_color, border_radius);

        if let Some(ref shadows) = self.mss.box_shadow {
            for shadow in shadows.0.iter() {
                list.push_shadow(
                    self.bounds, shadow.color, shadow.blur_radius,
                    (shadow.offset_x, shadow.offset_y), border_radius,
                );
            }
        }

        let title_h = if self.title.is_some() { label_font + 12.0 } else { 0.0 };
        if let Some(ref title) = self.title {
            let title_font = label_font + 4.0;
            let title_color = self.mss.color.unwrap_or(Color::from_hex("#1e293b"));
            let title_w = estimate_text_width(title, title_font, self.text_measure.as_ref());
            let title_rect = Rect::new(
                Point::new(
                    self.bounds.origin.x + (self.bounds.size.width - title_w) * 0.5,
                    self.bounds.origin.y + padding[1],
                ),
                Size::new(title_w, title_font + 2.0),
            );
            list.push_text_centered(title, title_rect, title_color, title_font);
        }

        let legend_h = legend_height(self.legend_config.position, legend_font);

        let (cx, cy, outer_r, inner_r) = self.pie_geometry(padding, legend_h, title_h);
        let local_cx = cx - self.bounds.origin.x;
        let local_cy = cy - self.bounds.origin.y;

        let total: f64 = self.slices.iter().enumerate()
            .filter(|(i, _)| self.anim.is_series_visible(*i))
            .map(|(_, s)| s.value.max(0.0))
            .sum();

        let mut ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);

        for (i, _slice) in self.slices.iter().enumerate() {
            if i >= self.slice_angles.len() { break; }
            let (start, end) = self.slice_angles[i];

            let opacity = self.anim.series_opacity(i);
            if opacity < 0.01 { continue; }

            let anim_end = start + (end - start) * self.anim.appear_eased;
            if (anim_end - start).abs() < 0.001 { continue; }

            let explode = if i < self.explode_progress.len() { self.explode_progress[i] } else { 0.0 };
            let mid_angle = (start + anim_end) * 0.5;
            let offset_x = explode * 10.0 * mid_angle.cos();
            let offset_y = explode * 10.0 * (-mid_angle.sin());
            let slice_cx = local_cx + offset_x;
            let slice_cy = local_cy + offset_y;

            let color = self.resolved_colors.get(i).copied()
                .unwrap_or_else(|| palette_color(i))
                .with_alpha(opacity);

            ctx.set_color(color);
            for quad in arc_quads(slice_cx, slice_cy, outer_r, inner_r, start, anim_end) {
                ctx.fill_polygon(&quad);
            }
        }

        ctx.flush(list);

        if self.label_position != PieLabelPosition::None && self.anim.appear_eased > 0.1 {
            list.push_clip(self.bounds);
            for (i, sl) in self.slices.iter().enumerate() {
                if i >= self.slice_angles.len() { continue; }
                let opacity = self.anim.series_opacity(i);
                if opacity < 0.01 { continue; }

                let (start, end) = self.slice_angles[i];
                let anim_end = start + (end - start) * self.anim.appear_eased;
                if (anim_end - start).abs() < 0.01 { continue; }

                let mid_angle = (start + anim_end) * 0.5;
                let label_text = self.format_label(sl, total);
                let text_w = estimate_text_width(&label_text, label_font, self.text_measure.as_ref());

                match self.label_position {
                    PieLabelPosition::Inside => {
                        let label_r = if inner_r > 0.01 {
                            (outer_r + inner_r) * 0.5
                        } else {
                            outer_r * 0.6
                        };
                        let (lx, ly) = polar_to_cartesian(cx, cy, label_r, mid_angle);
                        let label_rect = Rect::new(
                            Point::new(lx - text_w * 0.5, ly - label_font * 0.5),
                            Size::new(text_w, label_font + 2.0),
                        );
                        let text_color = Color::WHITE.with_alpha(opacity);
                        list.push_text_centered(&label_text, label_rect, text_color, label_font);
                    }
                    PieLabelPosition::Outside => {
                        let edge_r = outer_r + 8.0;
                        let label_r = outer_r + 24.0;
                        let (ex, ey) = polar_to_cartesian(cx, cy, edge_r, mid_angle);
                        let (lx, ly) = polar_to_cartesian(cx, cy, label_r, mid_angle);

                        let mut line_ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);
                        line_ctx.set_color(label_color.with_alpha(opacity * 0.5));
                        line_ctx.set_stroke_width(1.0);
                        line_ctx.draw_line(
                            ex - self.bounds.origin.x, ey - self.bounds.origin.y,
                            lx - self.bounds.origin.x, ly - self.bounds.origin.y,
                        );
                        line_ctx.flush(list);

                        let text_x = if mid_angle.cos() >= 0.0 { lx } else { lx - text_w };
                        let label_rect = Rect::new(
                            Point::new(text_x, ly - label_font * 0.5),
                            Size::new(text_w, label_font + 2.0),
                        );
                        list.push_text(&label_text, label_rect, label_color.with_alpha(opacity), label_font);
                    }
                    PieLabelPosition::None => {}
                }
            }
            list.pop_clip();
        }

        if self.legend_config.position != LegendPosition::None && !self.slices.is_empty() {
            let legend_y = match self.legend_config.position {
                LegendPosition::Top => self.bounds.origin.y + padding[1] + title_h,
                _ => self.bounds.origin.y + self.bounds.size.height - padding[3] - legend_h,
            };
            let legend_rect = Rect::new(
                Point::new(self.bounds.origin.x + padding[0], legend_y),
                Size::new(self.bounds.size.width - padding[0] - padding[2], legend_h),
            );

            let names: Vec<&str> = self.slices.iter().map(|s| s.label.as_str()).collect();
            let visibility: Vec<f32> = (0..self.slices.len())
                .map(|i| self.anim.series_opacity(i))
                .collect();

            render_legend_items(
                list,
                &legend_rect,
                &names,
                &self.resolved_colors,
                &visibility,
                legend_font,
                label_color,
                self.text_measure.as_ref(),
            );
        }

        if self.tooltip_config.enabled {
            if let (Some(mouse), Some(idx)) = (self.mouse_pos, self.hovered_slice) {
                if idx < self.slices.len() && self.anim.tooltip_opacity > 0.01 {
                    let slice = &self.slices[idx];
                    let pct = if total > 0.0 {
                        format!("{:.1}%", slice.value / total * 100.0)
                    } else {
                        "0%".to_string()
                    };
                    let value_text = format_value(slice.value);
                    let lines = vec![
                        slice.label.clone(),
                        format!("{} ({})", value_text, pct),
                    ];

                    let colors = TooltipColors::default();
                    let opacity = self.anim.tooltip_opacity;
                    let line_height = colors.font_size + 4.0;
                    let tt_padding = 8.0;
                    let max_text_w = lines.iter()
                        .map(|t| estimate_text_width(t, colors.font_size, self.text_measure.as_ref()))
                        .fold(0.0_f32, f32::max);
                    let tt_w = max_text_w + tt_padding * 2.0;
                    let tt_h = lines.len() as f32 * line_height + tt_padding * 2.0;

                    let mut tx = mouse.x + 12.0;
                    let mut ty = mouse.y - 12.0 - tt_h;

                    if tx + tt_w > self.bounds.origin.x + self.bounds.size.width {
                        tx = mouse.x - 12.0 - tt_w;
                    }
                    if ty < self.bounds.origin.y {
                        ty = mouse.y + 16.0;
                    }
                    tx = tx.max(self.bounds.origin.x);
                    ty = ty.max(self.bounds.origin.y);

                    let tt_rect = Rect::new(Point::new(tx, ty), Size::new(tt_w, tt_h));

                    list.push_shadow(
                        tt_rect,
                        Color::new(0.0, 0.0, 0.0, 0.2 * opacity),
                        8.0, (0.0, 2.0), [6.0; 4],
                    );
                    list.push_rect(tt_rect, colors.background.with_alpha(opacity * 0.95), [6.0; 4]);

                    let swatch_color = self.resolved_colors.get(idx).copied()
                        .unwrap_or(Color::from_hex("#888888"));
                    let swatch_rect = Rect::new(
                        Point::new(tx + tt_padding, ty + tt_padding + 2.0),
                        Size::new(8.0, 8.0),
                    );
                    list.push_rect(swatch_rect, swatch_color.with_alpha(opacity), [4.0; 4]);

                    let mut text_y = ty + tt_padding;
                    for (li, text) in lines.iter().enumerate() {
                        let text_x = if li == 0 { tx + tt_padding + 12.0 } else { tx + tt_padding };
                        let text_rect = Rect::new(
                            Point::new(text_x, text_y),
                            Size::new(max_text_w, line_height),
                        );
                        let text_color = if li == 0 {
                            colors.text_color.with_alpha(opacity)
                        } else {
                            colors.text_color.with_alpha(opacity * 0.7)
                        };
                        list.push_text(text, text_rect, text_color, colors.font_size);
                        text_y += line_height;
                    }
                }
            }
        }
    }

    fn handle_event(&mut self, event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                let pos = *pos;
                self.mouse_pos = Some(pos);

                if !self.bounds.contains(pos) {
                    if self.hovered_slice.is_some() {
                        self.hovered_slice = None;
                        self.anim.hover_point = None;
                        self.mark_dirty(DirtyFlags::RENDER);
                    }
                    return EventResult::Ignored;
                }

                let padding = self.mss.padding_ltrb([16.0; 4]);
                let label_font = self.mss_label_font_size.unwrap_or(11.0);
                let title_h = if self.title.is_some() { label_font + 12.0 } else { 0.0 };
                let legend_h = legend_height(self.legend_config.position, label_font);
                let (cx, cy, outer_r, inner_r) = self.pie_geometry(padding, legend_h, title_h);

                let mut found = None;
                for (i, angles) in self.slice_angles.iter().enumerate() {
                    if !self.anim.is_series_visible(i) { continue; }
                    let (start, end) = *angles;
                    let anim_end = start + (end - start) * self.anim.appear_eased;
                    if hit_test_slice(pos.x, pos.y, cx, cy, outer_r, inner_r, start, anim_end) {
                        found = Some(i);
                        break;
                    }
                }

                if found != self.hovered_slice {
                    self.hovered_slice = found;
                    self.anim.hover_point = found.map(|i| (i, 0));
                    self.mark_dirty(DirtyFlags::RENDER);
                }

                EventResult::Handled
            }
            Event::MouseDown { button, position, .. } => {
                if *button != MouseButton::Left { return EventResult::Ignored; }
                let pos = *position;
                if !self.bounds.contains(pos) { return EventResult::Ignored; }

                for (i, rect) in self.legend_rects.iter().enumerate() {
                    if rect.contains(pos) {
                        self.anim.toggle_series(i);
                        self.compute_angles();
                        self.mark_dirty(DirtyFlags::RENDER);
                        return EventResult::Handled;
                    }
                }

                EventResult::Ignored
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
        if self.animate_enabled && !self.anim.appear_started {
            self.anim.start_appear();
        }

        let mut animating = self.anim.tick(dt);

        let dt_s = dt.as_secs_f32();
        let speed = 8.0 * dt_s;
        for i in 0..self.explode_progress.len() {
            let target = if self.hovered_slice == Some(i) { 1.0 } else { 0.0 };
            let current = self.explode_progress[i];
            if (current - target).abs() > 0.001 {
                let delta = (target - current) * speed.min(0.4);
                self.explode_progress[i] = current + delta;
                animating = true;
            } else {
                self.explode_progress[i] = target;
            }
        }

        if animating {
            self.compute_angles();
        }

        animating
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
    fn element_type_name(&self) -> &str { "PieChart" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);

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
        let total: f64 = self.slices.iter().map(|s| s.value.max(0.0)).sum();
        let desc = if self.slices.is_empty() {
            "Pie chart (empty)".to_string()
        } else {
            let top = &self.slices[0];
            format!("Pie chart: {} slices, largest: {} ({:.0}%)",
                self.slices.len(), top.label, top.value / total * 100.0)
        };
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Group,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(desc),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for PieChartElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}

fn format_value(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    if (rounded - rounded.round()).abs() < 0.01 && rounded.abs() < 100000.0 {
        format!("{:.0}", rounded)
    } else {
        format!("{:.1}", rounded)
    }
}
