use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SegmentState {
    Empty,
    Partial(f32),
    Filled,
    Disabled,
    /// Отметка события (красный сегмент с белой точкой — различим и без
    /// цвета). `pulse` — событие не просмотрено: яркость плавно пульсирует.
    Alert { pulse: bool },
}

const DEFAULT_HEIGHT: f32 = 10.0;
const DEFAULT_GAP: f32 = 2.0;
const DEFAULT_RADIUS: f32 = 2.0;
const APPEAR_DURATION_SECS: f32 = 0.18;
/// Период пульсации непросмотренной отметки.
const PULSE_PERIOD_SECS: f32 = 1.0;

pub struct SegmentedProgressBar {
    segments: Vec<SegmentState>,
    classes: Vec<String>,
}

impl SegmentedProgressBar {
    pub fn new(segments: Vec<SegmentState>) -> Self {
        Self {
            segments,
            classes: Vec::new(),
        }
    }

    pub fn from_bools(v: &[bool]) -> Self {
        Self::new(
            v.iter()
                .map(|&b| {
                    if b {
                        SegmentState::Filled
                    } else {
                        SegmentState::Empty
                    }
                })
                .collect(),
        )
    }

    pub fn from_fractions(v: &[f32]) -> Self {
        Self::new(
            v.iter()
                .map(|&f| {
                    if f >= 1.0 {
                        SegmentState::Filled
                    } else if f <= 0.0 {
                        SegmentState::Empty
                    } else {
                        SegmentState::Partial(f)
                    }
                })
                .collect(),
        )
    }

    pub fn with_disabled_from(mut self, from_idx: usize) -> Self {
        for s in self.segments.iter_mut().skip(from_idx) {
            *s = SegmentState::Disabled;
        }
        self
    }

    pub fn class(mut self, name: &str) -> Self {
        crate::widget::push_classes(&mut self.classes, name.to_string());
        self
    }
}

impl Widget for SegmentedProgressBar {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(SegmentedProgressBarElement {
            id: ElementId::new(),
            segments: self.segments.clone(),
            bounds: Rect::zero(),
            appear_t: 0.0,
            pulse_t: 0.0,
            classes: self.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
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
}

pub struct SegmentedProgressBarElement {
    id: ElementId,
    segments: Vec<SegmentState>,
    bounds: Rect,
    appear_t: f32,
    /// Фаза пульсации отметок, секунды.
    pulse_t: f32,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl SegmentedProgressBarElement {
    fn has_pulse(&self) -> bool {
        self.segments
            .iter()
            .any(|s| matches!(s, SegmentState::Alert { pulse: true }))
    }
}

impl Element for SegmentedProgressBarElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<SegmentedProgressBar>() {
            if self.segments != w.segments {
                self.segments = w.segments.clone();
                self.mark_dirty(DirtyFlags::RENDER);
            }
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = self
            .mss
            .width
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(constraints.max_width)
            .min(constraints.max_width);
        let height = self
            .mss
            .height
            .map(|d| d.resolve(constraints.max_height))
            .unwrap_or(DEFAULT_HEIGHT)
            .min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let n = self.segments.len();
        if n == 0 || self.bounds.size.width <= 0.0 || self.bounds.size.height <= 0.0 {
            return;
        }

        let gap = self.mss.gap.unwrap_or(DEFAULT_GAP).max(0.0);
        let radius = self
            .mss
            .border_radius_uniform(self.bounds.size.height, DEFAULT_RADIUS);

        let accent = self
            .mss
            .accent_color
            .unwrap_or_else(|| Color::from_hex("#22c55e"));
        let track = self
            .mss
            .background_color
            .unwrap_or_else(|| Color::from_hex("#E5E7EB"));
        let neutral = self.mss.color.unwrap_or_else(|| Color::from_hex("#9CA3AF"));
        // Цвет отметки — `outline-color` (по умолчанию красный).
        let alert = self
            .mss
            .outline_color
            .unwrap_or_else(|| Color::from_hex("#ef4444"));
        // Плавная пульсация 0.35…1 вместо жёсткого мигания.
        let wave = 0.5 - 0.5 * (self.pulse_t / PULSE_PERIOD_SECS * std::f32::consts::TAU).cos();
        let pulse_alpha = 1.0 - 0.65 * wave;

        let alpha = self.appear_t.clamp(0.0, 1.0);
        if alpha <= 0.001 {
            return;
        }

        let total_gap = gap * (n.saturating_sub(1) as f32);
        let seg_w = ((self.bounds.size.width - total_gap) / n as f32).max(0.0);

        let y = self.bounds.y();
        let h = self.bounds.size.height;

        for (i, seg) in self.segments.iter().enumerate() {
            let x = self.bounds.x() + i as f32 * (seg_w + gap);
            let rect = Rect::new(Point::new(x, y), Size::new(seg_w, h));

            let color = match seg {
                SegmentState::Empty => track,
                SegmentState::Filled => accent,
                SegmentState::Partial(f) => {
                    let f = f.clamp(0.0, 1.0);
                    track.lerp(&accent, f)
                }
                SegmentState::Disabled => neutral.with_alpha(0.30),
                SegmentState::Alert { pulse } => {
                    if *pulse {
                        alert.with_alpha(alert.a * pulse_alpha)
                    } else {
                        alert
                    }
                }
            };

            let color = if alpha < 1.0 {
                color.with_alpha(color.a * alpha)
            } else {
                color
            };
            list.push_rect(rect, color, [radius; 4]);
            if matches!(seg, SegmentState::Alert { .. }) {
                let d = (seg_w.min(h) * 0.4).max(2.0);
                let dot = Rect::new(
                    Point::new(x + (seg_w - d) / 2.0, y + (h - d) / 2.0),
                    Size::new(d, d),
                );
                let white = Color::from_hex("#ffffff");
                list.push_rect(dot, white.with_alpha(white.a * alpha), [d / 2.0; 4]);
            }
        }
    }

    /// Тик нужен, пока не доиграло появление или есть пульсирующие
    /// отметки; без них полоса кадров не требует.
    fn wants_animate_tick(&self) -> bool {
        self.appear_t < 1.0 || self.has_pulse()
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let mut changed = false;
        if self.appear_t < 1.0 {
            self.appear_t = (self.appear_t + dt.as_secs_f32() / APPEAR_DURATION_SECS).min(1.0);
            changed = true;
        }
        if self.has_pulse() {
            self.pulse_t = (self.pulse_t + dt.as_secs_f32()) % PULSE_PERIOD_SECS;
            changed = true;
        }
        if changed {
            self.mark_dirty(DirtyFlags::RENDER);
        }
        changed
    }

    fn handle_event(
        &mut self,
        _event: &Event,
        _ctx: &mut crate::widget::context::EventContext,
    ) -> EventResult {
        EventResult::Ignored
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
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

    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "SegmentedProgressBar"
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn mss(&self) -> Option<&MssFields> {
        Some(&self.mss)
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
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
        let total = self.segments.len().max(1) as f32;
        let filled: f32 = self
            .segments
            .iter()
            .map(|s| match s {
                SegmentState::Filled => 1.0,
                SegmentState::Partial(f) => f.clamp(0.0, 1.0),
                _ => 0.0,
            })
            .sum();
        let pct = (filled / total * 100.0).round() as i32;
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::ProgressBar,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                value: Some(format!("{}%", pct)),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for SegmentedProgressBarElement {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bools_maps_states() {
        let w = SegmentedProgressBar::from_bools(&[true, false, true]);
        assert_eq!(w.segments[0], SegmentState::Filled);
        assert_eq!(w.segments[1], SegmentState::Empty);
        assert_eq!(w.segments[2], SegmentState::Filled);
    }

    #[test]
    fn from_fractions_classifies_extremes() {
        let w = SegmentedProgressBar::from_fractions(&[0.0, 0.5, 1.0]);
        assert_eq!(w.segments[0], SegmentState::Empty);
        assert!(matches!(w.segments[1], SegmentState::Partial(_)));
        assert_eq!(w.segments[2], SegmentState::Filled);
    }

    #[test]
    fn disabled_tail_only_touches_suffix() {
        let w = SegmentedProgressBar::from_bools(&[true; 5]).with_disabled_from(3);
        assert_eq!(w.segments[2], SegmentState::Filled);
        assert_eq!(w.segments[3], SegmentState::Disabled);
        assert_eq!(w.segments[4], SegmentState::Disabled);
    }
}
