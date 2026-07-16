use crate::core::canvas::CanvasContext;
use crate::core::{Color, Point, Rect, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widget::context::TextMeasure;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use super::render::estimate_text_width;

#[derive(Debug, Clone)]
pub struct GaugeSegment {
    pub from: f64,
    pub to: f64,
    pub color: Color,
}

impl GaugeSegment {
    pub fn new(from: f64, to: f64, color: impl Into<Color>) -> Self {
        Self { from, to, color: color.into() }
    }
}

pub struct GaugeChart {
    value: f64,
    min: f64,
    max: f64,
    segments: Vec<GaugeSegment>,
    start_angle_deg: f32,
    end_angle_deg: f32,
    show_needle: bool,
    show_ticks: bool,
    tick_count: usize,
    minor_tick_count: usize,
    show_labels: bool,
    show_value: bool,
    value_format: Option<Arc<dyn Fn(f64) -> String + Send + Sync>>,
    title: Option<String>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    animate: bool,
    track_width_ratio: f32,
    classes: Vec<String>,
}

impl GaugeChart {
    pub fn new() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 100.0,
            segments: Vec::new(),
            start_angle_deg: 225.0,
            end_angle_deg: -45.0,
            show_needle: true,
            show_ticks: true,
            tick_count: 10,
            minor_tick_count: 5,
            show_labels: true,
            show_value: true,
            value_format: None,
            title: None,
            width: None,
            height: None,
            animate: true,
            track_width_ratio: 0.15,
            classes: Vec::new(),
        }
    }

    pub fn value(mut self, v: f64) -> Self { self.value = v; self }

    pub fn min(mut self, v: f64) -> Self { self.min = v; self }

    pub fn max(mut self, v: f64) -> Self { self.max = v; self }

    pub fn segment(mut self, seg: GaugeSegment) -> Self {
        self.segments.push(seg);
        self
    }

    pub fn start_angle(mut self, deg: f32) -> Self { self.start_angle_deg = deg; self }

    pub fn end_angle(mut self, deg: f32) -> Self { self.end_angle_deg = deg; self }

    pub fn needle(mut self, show: bool) -> Self { self.show_needle = show; self }

    pub fn ticks(mut self, show: bool) -> Self { self.show_ticks = show; self }

    pub fn tick_count(mut self, count: usize) -> Self { self.tick_count = count; self }

    pub fn minor_ticks(mut self, count: usize) -> Self { self.minor_tick_count = count; self }

    pub fn labels(mut self, show: bool) -> Self { self.show_labels = show; self }

    pub fn show_value(mut self, show: bool) -> Self { self.show_value = show; self }

    pub fn format(mut self, f: impl Fn(f64) -> String + Send + Sync + 'static) -> Self {
        self.value_format = Some(Arc::new(f));
        self
    }

    pub fn title(mut self, t: impl Into<String>) -> Self { self.title = Some(t.into()); self }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self.height = Some(Dimension::Px(h));
        self
    }

    pub fn animate(mut self, enabled: bool) -> Self { self.animate = enabled; self }

    pub fn track_width(mut self, ratio: f32) -> Self { self.track_width_ratio = ratio; self }

    pub fn class(mut self, cls: impl Into<String>) -> Self { self.classes.push(cls.into()); self }
}

impl Default for GaugeChart {
    fn default() -> Self { Self::new() }
}

impl Widget for GaugeChart {
    fn create_element(&self) -> Box<dyn Element> {
        let target = ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0) as f32;
        Box::new(GaugeChartElement {
            id: ElementId::new(),
            value: self.value,
            min: self.min,
            max: self.max,
            segments: self.segments.clone(),
            start_angle: self.start_angle_deg.to_radians(),
            end_angle: self.end_angle_deg.to_radians(),
            show_needle: self.show_needle,
            show_ticks: self.show_ticks,
            tick_count: self.tick_count,
            minor_tick_count: self.minor_tick_count,
            show_labels: self.show_labels,
            show_value: self.show_value,
            value_format: self.value_format.clone(),
            title: self.title.clone(),
            width: self.width,
            height: self.height,
            animate_enabled: self.animate,
            track_width_ratio: self.track_width_ratio,
            bounds: Rect::zero(),
            anim_progress: if self.animate { 0.0 } else { target },
            anim_target: target,
            anim_started: self.animate,
            anim_duration: 0.8,
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            mss_track_color: None,
            mss_needle_color: None,
            mss_label_color: None,
            mss_label_font_size: None,
            mss_value_font_size: None,
            text_measure: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
    fn widget_classes(&self) -> &[String] { &self.classes }
}

struct GaugeChartElement {
    id: ElementId,

    value: f64,
    min: f64,
    max: f64,
    segments: Vec<GaugeSegment>,
    start_angle: f32,
    end_angle: f32,
    show_needle: bool,
    show_ticks: bool,
    tick_count: usize,
    minor_tick_count: usize,
    show_labels: bool,
    show_value: bool,
    value_format: Option<Arc<dyn Fn(f64) -> String + Send + Sync>>,
    title: Option<String>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    animate_enabled: bool,
    track_width_ratio: f32,

    bounds: Rect,
    anim_progress: f32,
    anim_target: f32,
    anim_started: bool,
    anim_duration: f32,

    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,

    mss_track_color: Option<Color>,
    mss_needle_color: Option<Color>,
    mss_label_color: Option<Color>,
    mss_label_font_size: Option<f32>,
    mss_value_font_size: Option<f32>,

    text_measure: Option<Arc<dyn TextMeasure>>,
}

impl GaugeChartElement {
    fn value_to_angle(&self, t: f32) -> f32 {
        self.start_angle + t * (self.end_angle - self.start_angle)
    }

    fn format_value(&self, v: f64) -> String {
        if let Some(ref fmt) = self.value_format {
            fmt(v)
        } else {
            Self::format_number(v)
        }
    }

    fn format_tick_label(v: f64) -> String {
        Self::format_number(v)
    }

    fn format_number(v: f64) -> String {
        let rounded = (v * 100.0).round() / 100.0;
        if (rounded - rounded.round()).abs() < 0.01 && rounded.abs() < 100000.0 {
            format!("{:.0}", rounded)
        } else {
            format!("{:.1}", rounded)
        }
    }
}

impl Element for GaugeChartElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<GaugeChart>() {
            self.value = w.value;
            self.min = w.min;
            self.max = w.max;
            self.segments = w.segments.clone();
            self.start_angle = w.start_angle_deg.to_radians();
            self.end_angle = w.end_angle_deg.to_radians();
            self.show_needle = w.show_needle;
            self.show_ticks = w.show_ticks;
            self.tick_count = w.tick_count;
            self.minor_tick_count = w.minor_tick_count;
            self.show_labels = w.show_labels;
            self.show_value = w.show_value;
            self.value_format = w.value_format.clone();
            self.title = w.title.clone();
            self.width = w.width;
            self.height = w.height;
            self.animate_enabled = w.animate;
            self.track_width_ratio = w.track_width_ratio;

            let new_target = ((w.value - w.min) / (w.max - w.min)).clamp(0.0, 1.0) as f32;
            if (new_target - self.anim_target).abs() > 0.001 {
                self.anim_target = new_target;
                if self.animate_enabled {
                    self.anim_started = true;
                } else {
                    self.anim_progress = new_target;
                }
            }

            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self.mss.width
            .or(self.width)
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(250.0)
            .min(constraints.max_width);

        let h = self.mss.height
            .or(self.height)
            .map(|d| d.resolve(constraints.max_height))
            .unwrap_or(250.0)
            .min(constraints.max_height);

        let gauge_size = w;
        let radius = gauge_size * 0.4;
        let track_half = radius * self.track_width_ratio * 0.5;
        let cy = gauge_size * 0.5;
        let start_rad = self.start_angle;
        let end_rad = self.end_angle;
        let bottom_start = cy + (radius + track_half) * (-start_rad.sin()).max(0.0);
        let bottom_end = cy + (radius + track_half) * (-end_rad.sin()).max(0.0);
        let arc_bottom = bottom_start.max(bottom_end);
        let text_bottom = cy + radius * 0.45 + 30.0;
        let content_bottom = arc_bottom.max(text_bottom) + 8.0;
        let effective_h = content_bottom.min(h);

        self.bounds = Rect::new(Point::zero(), Size::new(w, effective_h));
        Size::new(w, effective_h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let border_radius = self.mss.border_radius_resolved(self.bounds.size.width, 0.0);
        let padding = self.mss.padding_ltrb([16.0; 4]);

        if let Some(bg_color) = self.mss.background_color {
            list.push_rect(self.bounds, bg_color, border_radius);
        }

        if let Some(ref shadows) = self.mss.box_shadow {
            for shadow in shadows.0.iter() {
                list.push_shadow(
                    self.bounds, shadow.color, shadow.blur_radius,
                    (shadow.offset_x, shadow.offset_y), border_radius,
                );
            }
        }

        let inner_w = self.bounds.size.width - padding[0] - padding[2];
        let _inner_h = self.bounds.size.height - padding[1] - padding[3];
        let inner_x = self.bounds.origin.x + padding[0];
        let inner_y = self.bounds.origin.y + padding[1];

        let gauge_size = inner_w;

        let cx = inner_x + inner_w * 0.5;
        let cy = inner_y + gauge_size * 0.5;
        let radius = gauge_size * 0.4;
        let track_width = radius * self.track_width_ratio;

        let fg_color = self.mss.color;
        let track_color = self.mss_track_color.unwrap_or_else(|| fg_color.map(|c| c.with_alpha(0.2)).unwrap_or(Color::from_hex("#e2e8f0")));
        let needle_color = self.mss_needle_color.or(fg_color).unwrap_or(Color::from_hex("#1e293b"));
        let label_color = self.mss_label_color.or(fg_color.map(|c| c.with_alpha(0.6))).unwrap_or(Color::from_hex("#64748b"));
        let scale_factor = (gauge_size / 250.0).clamp(0.5, 1.5);
        let label_font = self.mss_label_font_size.unwrap_or(10.0) * scale_factor;
        let value_font = self.mss_value_font_size.unwrap_or(28.0) * scale_factor;

        let mut ctx = CanvasContext::new(self.bounds.origin, self.bounds.size);

        let local_cx = cx - self.bounds.origin.x;
        let local_cy = cy - self.bounds.origin.y;

        ctx.set_color(track_color);
        ctx.set_stroke_width(track_width);
        ctx.draw_arc(local_cx, local_cy, radius, -self.start_angle, -self.end_angle);

        let range = self.max - self.min;
        if range > 0.0 {
            for seg in &self.segments {
                let t0 = ((seg.from - self.min) / range).clamp(0.0, 1.0) as f32;
                let t1 = ((seg.to - self.min) / range).clamp(0.0, 1.0) as f32;
                let a0 = self.value_to_angle(t0);
                let a1 = self.value_to_angle(t1);
                ctx.set_color(seg.color);
                ctx.set_stroke_width(track_width);
                ctx.draw_arc(local_cx, local_cy, radius, -a0, -a1);
            }
        }

        if self.show_ticks && self.tick_count > 0 {
            let local_cx = cx - self.bounds.origin.x;
            let local_cy = cy - self.bounds.origin.y;
            let outer_r = radius + track_width * 0.5;
            let major_len = radius * 0.1;
            let minor_len = radius * 0.05;

            for i in 0..=self.tick_count {
                let t = i as f32 / self.tick_count as f32;
                let angle = self.value_to_angle(t);
                let cos_a = angle.cos();
                let sin_a = angle.sin();

                ctx.set_color(label_color);
                ctx.set_stroke_width(1.5);
                let r0 = outer_r;
                let r1 = outer_r + major_len;
                ctx.draw_line(
                    local_cx + cos_a * r0, local_cy - sin_a * r0,
                    local_cx + cos_a * r1, local_cy - sin_a * r1,
                );

                if i < self.tick_count && self.minor_tick_count > 0 {
                    for j in 1..=self.minor_tick_count {
                        let mt = (i as f32 + j as f32 / (self.minor_tick_count + 1) as f32)
                            / self.tick_count as f32;
                        let ma = self.value_to_angle(mt);
                        let mc = ma.cos();
                        let ms = ma.sin();
                        ctx.set_stroke_width(0.8);
                        let mr1 = outer_r + minor_len;
                        ctx.draw_line(
                            local_cx + mc * r0, local_cy - ms * r0,
                            local_cx + mc * mr1, local_cy - ms * mr1,
                        );
                    }
                }
            }
        }

        if self.show_needle {
            let local_cx = cx - self.bounds.origin.x;
            let local_cy = cy - self.bounds.origin.y;
            let needle_angle = self.value_to_angle(self.anim_progress);
            let needle_len = radius * 0.85;
            let needle_half_width = radius * 0.03;
            let cos_a = needle_angle.cos();
            let sin_a = needle_angle.sin();

            let tip_x = local_cx + cos_a * needle_len;
            let tip_y = local_cy - sin_a * needle_len;

            let perp_x = sin_a * needle_half_width;
            let perp_y = cos_a * needle_half_width;
            let base1 = (local_cx + perp_x, local_cy + perp_y);
            let base2 = (local_cx - perp_x, local_cy - perp_y);

            let tail_len = radius * 0.15;
            let tail_x = local_cx - cos_a * tail_len;
            let tail_y = local_cy + sin_a * tail_len;

            ctx.set_color(needle_color);
            ctx.fill_polygon(&[
                (tip_x, tip_y),
                base1,
                (tail_x, tail_y),
                base2,
            ]);

            ctx.fill_circle(local_cx, local_cy, radius * 0.06);
        }

        ctx.flush(list);

        if self.show_labels && self.tick_count > 0 && range > 0.0 {
            let local_cx = cx - self.bounds.origin.x;
            let local_cy = cy - self.bounds.origin.y;
            let label_r = radius + track_width * 0.5 + radius * 0.1 + label_font * 0.5 + 6.0;

            for i in 0..=self.tick_count {
                let t = i as f32 / self.tick_count as f32;
                let angle = self.value_to_angle(t);
                let v = self.min + t as f64 * range;
                let label_text = Self::format_tick_label(v);

                let lx = self.bounds.origin.x + local_cx + angle.cos() * label_r;
                let ly = self.bounds.origin.y + local_cy - angle.sin() * label_r;

                let label_w = estimate_text_width(&label_text, label_font, self.text_measure.as_ref());
                let label_rect = Rect::new(
                    Point::new(lx - label_w * 0.5, ly - label_font * 0.5),
                    Size::new(label_w, label_font + 2.0),
                );
                list.push_text_centered(&label_text, label_rect, label_color, label_font);
            }
        }

        if self.show_value {
            let display_value = self.min + self.anim_progress as f64 * range;
            let value_text = self.format_value(display_value);
            let value_color = fg_color.unwrap_or(Color::from_hex("#1e293b"));

            let has_title = self.title.is_some();
            let title_font = (label_font + 2.0) * scale_factor;
            let total_text_h = value_font + if has_title { title_font + 4.0 } else { 0.0 };
            let text_start_y = cy + radius * 0.45 - total_text_h * 0.5;

            let text_w = estimate_text_width(&value_text, value_font, self.text_measure.as_ref());
            let value_rect = Rect::new(
                Point::new(cx - text_w * 0.5, text_start_y),
                Size::new(text_w, value_font + 2.0),
            );
            list.push_text_centered(&value_text, value_rect, value_color, value_font);

            if let Some(ref title) = self.title {
                let title_w = estimate_text_width(title, title_font, self.text_measure.as_ref());
                let title_rect = Rect::new(
                    Point::new(cx - title_w * 0.5, text_start_y + value_font + 4.0),
                    Size::new(title_w, title_font + 2.0),
                );
                list.push_text_centered(title, title_rect, label_color, title_font);
            }
        }
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut crate::widget::context::EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if !self.anim_started {
            return false;
        }

        let dt_s = dt.as_secs_f32();
        let speed = 1.0 / self.anim_duration;
        let diff = self.anim_target - self.anim_progress;

        if diff.abs() < 0.001 {
            self.anim_progress = self.anim_target;
            self.anim_started = false;
            return false;
        }

        let step = diff * (dt_s * speed * 4.0).min(0.15);
        self.anim_progress += step;

        true
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
    fn element_type_name(&self) -> &str { "GaugeChart" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);

        if let Some(c) = style.get("track-color").and_then(|v| v.as_color()) {
            self.mss_track_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(c) = style.get("needle-color").and_then(|v| v.as_color()) {
            self.mss_needle_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(c) = style.get("label-color").and_then(|v| v.as_color()) {
            self.mss_label_color = Some(crate::animation::transition::mss_color_to_core(c));
        }
        if let Some(v) = style.get("label-font-size").and_then(|v| v.as_px()) {
            self.mss_label_font_size = Some(v);
        }
        if let Some(v) = style.get("value-font-size").and_then(|v| v.as_px()) {
            self.mss_value_font_size = Some(v);
        }
        if let Some(v) = style.get("animation-duration").and_then(|v| v.as_px()) {
            self.anim_duration = (v / 1000.0).max(0.01);
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
                label: Some(format!("Gauge: {:.0} / {:.0}-{:.0}", self.value, self.min, self.max)),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for GaugeChartElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
