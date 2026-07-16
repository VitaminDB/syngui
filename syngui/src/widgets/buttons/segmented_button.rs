use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Segment {
    pub label: String,
    pub icon: Option<String>,
}

impl Segment {
    pub fn new(label: impl Into<String>) -> Self {
        Self { label: label.into(), icon: None }
    }

    pub fn with_icon(label: impl Into<String>, icon: impl Into<String>) -> Self {
        Self { label: label.into(), icon: Some(icon.into()) }
    }

    pub fn icon_only(icon: impl Into<String>) -> Self {
        Self { label: String::new(), icon: Some(icon.into()) }
    }
}

impl From<String> for Segment {
    fn from(s: String) -> Self { Segment::new(s) }
}

impl From<&str> for Segment {
    fn from(s: &str) -> Self { Segment::new(s) }
}

pub struct SegmentedButton {
    segments: Vec<Segment>,
    selected: usize,
    disabled: bool,
    on_change: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
}

impl SegmentedButton {
    pub fn new(segments: Vec<impl Into<Segment>>) -> Self {
        Self {
            segments: segments.into_iter().map(|s| s.into()).collect(),
            selected: 0,
            disabled: false,
            on_change: None,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(usize) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Widget for SegmentedButton {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(SegmentedButtonElement {
            id: ElementId::new(),
            segments: self.segments.clone(),
            selected: self.selected,
            disabled: self.disabled,
            on_change: self.on_change.clone(),
            bounds: Rect::zero(),
            hovered_index: None,
            pressed_index: None,
            focused: false,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
            segment_padding: 16.0,
            segment_widths: Vec::new(),
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

pub struct SegmentedButtonElement {
    id: ElementId,
    segments: Vec<Segment>,
    selected: usize,
    disabled: bool,
    on_change: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    bounds: Rect,
    hovered_index: Option<usize>,
    pressed_index: Option<usize>,
    focused: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
    segment_padding: f32,
    segment_widths: Vec<f32>,
}

impl SegmentedButtonElement {
    fn index_at_x(&self, x: f32) -> Option<usize> {
        let local_x = x - self.bounds.x();
        if local_x < 0.0 || local_x >= self.bounds.size.width || self.segments.is_empty() {
            return None;
        }
        let mut acc = 0.0;
        for (i, &w) in self.segment_widths.iter().enumerate() {
            acc += w;
            if local_x < acc {
                return Some(i);
            }
        }
        Some(self.segments.len() - 1)
    }
}

impl Element for SegmentedButtonElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(sb) = widget.as_any().downcast_ref::<SegmentedButton>() {
            self.segments = sb.segments.clone();
            self.selected = sb.selected;
            self.disabled = sb.disabled;
            self.on_change = sb.on_change.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let height = self.mss.height.map(|d| d.resolve(constraints.max_height)).unwrap_or(36.0);
        let font_size = self.mss.font_size_or(14.0);
        let bold = self.mss.font_weight_or(400) >= 700;
        let h_padding = self.segment_padding;
        let icon_extra = font_size * 1.2 + 6.0;

        let content_widths: Vec<f32> = self.segments.iter().map(|seg| {
            let text_w = if seg.label.is_empty() {
                0.0
            } else {
                self.text_measure.as_ref()
                    .map(|tm| tm.measure_text_width_styled(&seg.label, font_size, seg.label.chars().count(), bold, self.mss.font_family.as_deref()))
                    .unwrap_or(seg.label.chars().count() as f32 * font_size * 0.6)
            };
            let extra = if seg.icon.is_some() { icon_extra } else { 0.0 };
            text_w + extra + h_padding
        }).collect();

        let intrinsic: f32 = content_widths.iter().sum();

        let width = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            intrinsic.max(constraints.min_width)
        };

        if intrinsic > 0.0 {
            let scale = width / intrinsic;
            self.segment_widths = content_widths.iter().map(|w| w * scale).collect();
        } else {
            let eq = if self.segments.is_empty() { 0.0 } else { width / self.segments.len() as f32 };
            self.segment_widths = vec![eq; self.segments.len()];
        }

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let count = self.segments.len();
        if count == 0 {
            return;
        }

        let base_bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let base_fg = self.mss.color.unwrap_or(Color::from_hex("#374151"));
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let accent = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let font_size = self.mss.font_size_or(14.0);
        let font_weight = self.mss.font_weight_or(400);
        let outer_radius = self.mss.border_radius_uniform(self.bounds.size.width.min(self.bounds.size.height), 8.0);
        let border_width = self.mss.border_width_or(1.0);

        if border_width > 0.0 {
            list.push_rect_bordered(
                self.bounds,
                Color::TRANSPARENT,
                [outer_radius; 4],
                Border { width: border_width, color: border_color },
            );
        }

        let bw = border_width;
        let inner = Rect::new(
            Point::new(self.bounds.x() + bw, self.bounds.y() + bw),
            Size::new(
                (self.bounds.size.width - bw * 2.0).max(0.0),
                (self.bounds.size.height - bw * 2.0).max(0.0),
            ),
        );
        let inner_radius = (outer_radius - bw).max(0.0);

        let icon_size = font_size * 1.2;
        let icon_gap = 6.0;

        let mut seg_x = inner.x();
        for i in 0..count {
            let seg_w = if i < self.segment_widths.len() {
                self.segment_widths[i] - bw * 2.0 / count as f32
            } else {
                inner.size.width / count as f32
            };
            let seg_rect = Rect::new(
                Point::new(seg_x, inner.y()),
                Size::new(seg_w, inner.size.height),
            );

            let radius = if count == 1 {
                [inner_radius; 4]
            } else if i == 0 {
                [inner_radius, 0.0, 0.0, inner_radius]
            } else if i == count - 1 {
                [0.0, inner_radius, inner_radius, 0.0]
            } else {
                [0.0; 4]
            };

            let is_dark_bg = (base_bg.r + base_bg.g + base_bg.b) / 3.0 < 0.5;
            let hover_bg = if is_dark_bg {
                base_bg.lighten(0.12)
            } else {
                base_bg.darken(0.08)
            };
            let bg = if self.disabled {
                base_bg.darken(0.02)
            } else if i == self.selected {
                accent
            } else if self.pressed_index == Some(i) || self.hovered_index == Some(i) {
                hover_bg
            } else {
                base_bg
            };

            list.push_rect(seg_rect, bg, radius);

            let text_col = if self.disabled {
                base_fg.with_alpha(0.4)
            } else if i == self.selected {
                Color::WHITE
            } else {
                base_fg
            };

            let seg = &self.segments[i];
            let has_icon = seg.icon.is_some();
            let has_text = !seg.label.is_empty();

            if has_icon && has_text {
                let icon_str = seg.icon.as_deref().unwrap();
                let text_w = self.text_measure.as_ref()
                    .map(|tm| tm.measure_text_width_styled(&seg.label, font_size, seg.label.chars().count(), font_weight >= 700, self.mss.font_family.as_deref()))
                    .unwrap_or(seg.label.chars().count() as f32 * font_size * 0.6);
                let total_w = icon_size + icon_gap + text_w;
                let start_x = seg_rect.x() + (seg_rect.width() - total_w) / 2.0;

                let icon_y = seg_rect.y() + (seg_rect.height() - icon_size) / 2.0;
                let icon_rect = Rect::new(
                    Point::new(start_x, icon_y),
                    Size::new(icon_size, icon_size),
                );
                list.push_text_centered(icon_str, icon_rect, text_col, icon_size);

                let text_y = seg_rect.y() + (seg_rect.height() - font_size) / 2.0;
                let text_rect = Rect::new(
                    Point::new(start_x + icon_size + icon_gap, text_y),
                    Size::new(text_w, font_size),
                );
                list.push_text_styled(
                    &seg.label, text_rect, text_col, font_size,
                    crate::mss::TextAlign::LEFT, crate::mss::TextDecoration::None,
                    font_weight, self.mss.font_family.clone(),
                );
            } else if has_icon {
                let icon_str = seg.icon.as_deref().unwrap();
                let icon_y = seg_rect.y() + (seg_rect.height() - icon_size) / 2.0;
                let icon_rect = Rect::new(
                    Point::new(seg_rect.x() + (seg_rect.width() - icon_size) / 2.0, icon_y),
                    Size::new(icon_size, icon_size),
                );
                list.push_text_centered(icon_str, icon_rect, text_col, icon_size);
            } else {
                list.push_text_styled(
                    &seg.label, seg_rect, text_col, font_size,
                    crate::mss::TextAlign::CENTER, crate::mss::TextDecoration::None,
                    font_weight, self.mss.font_family.clone(),
                );
            }

            seg_x += seg_w;

            if i < count - 1 {
                let next_is_selected = i + 1 == self.selected;
                let current_is_selected = i == self.selected;
                if !current_is_selected && !next_is_selected {
                    let div_rect = Rect::new(
                        Point::new(seg_x - 0.5, inner.y() + 4.0),
                        Size::new(1.0, inner.size.height - 8.0),
                    );
                    list.push_rect(div_rect, border_color.with_alpha(0.5), [0.0; 4]);
                }
            }
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled {
            return EventResult::Ignored;
        }
        match event {
            Event::MouseMove(pos) => {
                let was = self.hovered_index;
                if self.bounds.contains(*pos) {
                    self.hovered_index = self.index_at_x(pos.x);
                    ctx.set_cursor(CursorIcon::Pointer);
                } else {
                    self.hovered_index = None;
                }
                if self.hovered_index != was {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hovered_index.is_some() {
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position }
                if *button == MouseButton::Left && self.bounds.contains(*position) =>
            {
                self.pressed_index = self.index_at_x(position.x);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::MouseUp { button, position }
                if *button == MouseButton::Left && self.pressed_index.is_some() =>
            {
                let pressed = self.pressed_index.take();
                if self.bounds.contains(*position) {
                    if let Some(idx) = self.index_at_x(position.x) {
                        if Some(idx) == pressed && idx != self.selected {
                            self.selected = idx;
                            if let Some(ref cb) = self.on_change {
                                if let Ok(mut f) = cb.lock() {
                                    f(idx);
                                }
                            }
                        }
                    }
                }
                ctx.request_paint();
                EventResult::Handled
            }
            Event::FocusGained => {
                self.focused = true;
                ctx.request_paint();
                EventResult::Handled
            }
            Event::FocusLost => {
                self.focused = false;
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(Key::Left) if self.focused && !self.segments.is_empty() => {
                if self.selected > 0 {
                    self.selected -= 1;
                    if let Some(ref cb) = self.on_change {
                        if let Ok(mut f) = cb.lock() { f(self.selected); }
                    }
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::KeyDown(Key::Right) if self.focused && !self.segments.is_empty() => {
                if self.selected + 1 < self.segments.len() {
                    self.selected += 1;
                    if let Some(ref cb) = self.on_change {
                        if let Ok(mut f) = cb.lock() { f(self.selected); }
                    }
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::KeyDown(Key::Enter) | Event::KeyDown(Key::Space) if self.focused => {
                if let Some(ref cb) = self.on_change {
                    if let Ok(mut f) = cb.lock() { f(self.selected); }
                }
                ctx.request_paint();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        (None, self.mss.height.map(|d| d.resolve(f32::INFINITY)))
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

    fn element_type_name(&self) -> &str { "SegmentedButton" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(v) = style.get("segment-padding").and_then(|v| v.as_px()) {
            self.segment_padding = v;
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
        let labels: Vec<&str> = self.segments.iter().map(|s| s.label.as_str()).collect();
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::RadioButton,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                focused: self.focused,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: Some(format!("Segmented button: {}", labels.join(", "))),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for SegmentedButtonElement {
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
