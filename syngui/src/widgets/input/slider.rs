use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

pub struct Slider {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub disabled: bool,
    pub width: Option<Dimension>,
    pub vertical: bool,
    pub bipolar: bool,
    pub on_change: Option<Arc<Mutex<dyn FnMut(f32) + Send>>>,
}

impl Slider {
    pub fn new() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            disabled: false,
            width: None,
            vertical: false,
            bipolar: false,
            on_change: None,
        }
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value;
        self
    }

    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(Dimension::Px(width));
        self
    }

    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    pub fn bipolar(mut self) -> Self {
        self.bipolar = true;
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(f32) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Default for Slider {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Slider {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(SliderElement {
            id: ElementId::new(),
            value: self.value,
            min: self.min,
            max: self.max,
            step: self.step,
            disabled: self.disabled,
            width: self.width,
            vertical: self.vertical,
            bipolar: self.bipolar,
            bounds: Rect::zero(),
            track_bounds: Rect::zero(),
            dragging: false,
            hover: false,
            focused: false,
            on_change: self.on_change.clone(),
            classes: Vec::new(),
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

pub struct SliderElement {
    id: ElementId,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    disabled: bool,
    width: Option<Dimension>,
    vertical: bool,
    bipolar: bool,
    bounds: Rect,
    track_bounds: Rect,
    dragging: bool,
    hover: bool,
    focused: bool,
    on_change: Option<Arc<Mutex<dyn FnMut(f32) + Send>>>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
}

impl SliderElement {
    fn value_to_pos(&self, value: f32) -> f32 {
        let range = self.max - self.min;
        if self.vertical {
            if range <= 0.0 {
                return self.track_bounds.y() + self.track_bounds.size.height;
            }
            let percent = (value - self.min) / range;
            self.track_bounds.y() + (1.0 - percent) * self.track_bounds.size.height
        } else {
            if range <= 0.0 {
                return self.track_bounds.x();
            }
            let percent = (value - self.min) / range;
            self.track_bounds.x() + percent * self.track_bounds.size.width
        }
    }

    fn pos_to_value(&self, pos: f32) -> f32 {
        let range = self.max - self.min;
        let raw_value = if self.vertical {
            if range <= 0.0 || self.track_bounds.size.height <= 0.0 {
                return self.min;
            }
            let percent =
                ((self.track_bounds.y() + self.track_bounds.size.height - pos)
                    / self.track_bounds.size.height)
                    .clamp(0.0, 1.0);
            self.min + percent * range
        } else {
            if range <= 0.0 || self.track_bounds.size.width <= 0.0 {
                return self.min;
            }
            let percent =
                ((pos - self.track_bounds.x()) / self.track_bounds.size.width).clamp(0.0, 1.0);
            self.min + percent * range
        };

        if self.step > 0.0 {
            (raw_value / self.step).round() * self.step
        } else {
            raw_value
        }
    }

    fn trigger_change(&mut self) {
        if let Some(ref callback) = self.on_change {
            if let Ok(mut cb) = callback.lock() {
                cb(self.value);
            }
        }
    }
}

impl Element for SliderElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(slider) = widget.as_any().downcast_ref::<Slider>() {
            self.value = slider.value;
            self.min = slider.min;
            self.max = slider.max;
            self.step = slider.step;
            self.disabled = slider.disabled;
            self.width = slider.width;
            self.vertical = slider.vertical;
            self.bipolar = slider.bipolar;
            self.on_change = slider.on_change.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        if self.vertical {
            let mss_width = self.mss.width.map(|d| d.resolve(constraints.max_width));
            let width = mss_width.unwrap_or(24.0).min(constraints.max_width);
            let mss_height = self.mss.height.map(|d| d.resolve(constraints.max_height));
            let height = mss_height
                .unwrap_or(120.0)
                .min(constraints.max_height);

            self.bounds = Rect::new(Point::zero(), Size::new(width, height));

            let track_w = self.mss.min_width.map(|d| d.resolve(width)).unwrap_or(4.0);
            self.track_bounds = Rect::new(
                Point::new((width - track_w) / 2.0, 8.0),
                Size::new(track_w, height - 16.0),
            );

            Size::new(width, height)
        } else {
            let mss_height = self.mss.height.map(|d| d.resolve(constraints.max_height));
            let height = mss_height.unwrap_or(24.0);
            let width = self
                .width
                .map(|d| d.resolve(constraints.max_width))
                .unwrap_or(constraints.max_width)
                .min(constraints.max_width);

            self.bounds = Rect::new(Point::zero(), Size::new(width, height));

            let track_height = self.mss.min_height.map(|d| d.resolve(height)).unwrap_or(4.0);
            self.track_bounds = Rect::new(
                Point::new(8.0, (height - track_height) / 2.0),
                Size::new(width - 16.0, track_height),
            );

            Size::new(width, height)
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let track_base = self.mss.background_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let fill_base = self.mss.color.unwrap_or(Color::from_hex("#3B82F6"));
        let track_color = if self.disabled {
            track_base.darken(0.1)
        } else {
            track_base
        };
        let fill_color = if self.disabled { track_base.darken(0.2) } else { fill_base };

        let track_radius_basis = if self.vertical {
            self.track_bounds.size.width
        } else {
            self.track_bounds.size.height
        };
        let track_radius = self
            .mss
            .border_radius
            .map(|r| r.map(|d| d.resolve(track_radius_basis)))
            .unwrap_or([2.0; 4]);

        if let Some(ref gradient) = self.mss.background_gradient {
            list.push_gradient_rect(self.track_bounds, gradient.clone(), track_radius);
        } else {
            list.push_rect(self.track_bounds, track_color, track_radius);
        }

        let thumb_pos = self.value_to_pos(self.value);

        if self.mss.background_gradient.is_none() {
            let bipolar_ok = self.bipolar && self.min < 0.0 && self.max > 0.0;
            if self.vertical {
                if bipolar_ok {
                    let zero_y = self.value_to_pos(0.0);
                    let (top, bottom) = if thumb_pos <= zero_y {
                        (thumb_pos, zero_y)
                    } else {
                        (zero_y, thumb_pos)
                    };
                    let h = bottom - top;
                    if h > 0.0 {
                        let fill_rect = Rect::new(
                            Point::new(self.track_bounds.x(), top),
                            Size::new(self.track_bounds.size.width, h),
                        );
                        list.push_rect(fill_rect, fill_color, track_radius);
                    }
                } else {
                    let bottom = self.track_bounds.y() + self.track_bounds.size.height;
                    let h = bottom - thumb_pos;
                    if h > 0.0 {
                        let fill_rect = Rect::new(
                            Point::new(self.track_bounds.x(), thumb_pos),
                            Size::new(self.track_bounds.size.width, h),
                        );
                        list.push_rect(fill_rect, fill_color, track_radius);
                    }
                }
            } else if bipolar_ok {
                let zero_x = self.value_to_pos(0.0);
                let (left, right) = if thumb_pos <= zero_x {
                    (thumb_pos, zero_x)
                } else {
                    (zero_x, thumb_pos)
                };
                let w = right - left;
                if w > 0.0 {
                    let fill_rect = Rect::new(
                        Point::new(left, self.track_bounds.y()),
                        Size::new(w, self.track_bounds.size.height),
                    );
                    list.push_rect(fill_rect, fill_color, track_radius);
                }
            } else {
                let fill_width = thumb_pos - self.track_bounds.x();
                if fill_width > 0.0 {
                    let fill_rect = Rect::new(
                        self.track_bounds.origin,
                        Size::new(fill_width, self.track_bounds.size.height),
                    );
                    list.push_rect(fill_rect, fill_color, track_radius);
                }
            }
        }

        let thumb_color = if self.disabled {
            track_base.darken(0.2)
        } else {
            self.mss.accent_color.unwrap_or(Color::WHITE)
        };
        let thumb_border_color = self.mss.border_color.unwrap_or(fill_color);
        let thumb_border = if self.disabled {
            track_color
        } else if self.hover || self.dragging {
            thumb_border_color.darken(0.1)
        } else {
            thumb_border_color
        };
        let thumb_border_width = self.mss.border_width.unwrap_or(2.0);

        if self.vertical {
            let thumb_w = self.mss.max_width.map(|d| d.resolve(0.0)).unwrap_or(18.0);
            let thumb_h: f32 = 6.0;
            let thumb_rect = Rect::new(
                Point::new(
                    self.bounds.x() + (self.bounds.size.width - thumb_w) / 2.0,
                    thumb_pos - thumb_h / 2.0,
                ),
                Size::new(thumb_w, thumb_h),
            );
            let radii = [thumb_h / 2.0; 4];
            list.push_shadow(thumb_rect, Color::new(0.0, 0.0, 0.0, 0.15), 2.0, (0.0, 1.0), radii);
            list.push_rect_bordered(
                thumb_rect,
                thumb_color,
                radii,
                Border { width: thumb_border_width, color: thumb_border },
            );
        } else {
            let thumb_size = self.mss.max_height.map(|d| d.resolve(0.0)).unwrap_or(16.0);
            let thumb_rect = Rect::new(
                Point::new(
                    thumb_pos - thumb_size / 2.0,
                    self.bounds.y() + (self.bounds.size.height - thumb_size) / 2.0,
                ),
                Size::new(thumb_size, thumb_size),
            );
            let thumb_radius = thumb_size / 2.0;
            list.push_shadow(thumb_rect, Color::new(0.0, 0.0, 0.0, 0.15), 2.0, (0.0, 1.0), [thumb_radius; 4]);
            list.push_rect_bordered(
                thumb_rect,
                thumb_color,
                [thumb_radius; 4],
                Border { width: thumb_border_width, color: thumb_border },
            );
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled {
            return EventResult::Ignored;
        }

        match event {
            Event::MouseMove(pos) => {
                let was_hover = self.hover;
                self.hover = self.bounds.contains(*pos);
                if self.hover { ctx.set_cursor(CursorIcon::Pointer); }

                if self.dragging {
                    ctx.set_cursor(CursorIcon::Grabbing);
                    let drag_axis = if self.vertical { pos.y } else { pos.x };
                    let new_value = self.pos_to_value(drag_axis);
                    if (new_value - self.value).abs() > 0.001 {
                        self.value = new_value.clamp(self.min, self.max);
                        self.trigger_change();
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.hover != was_hover {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hover { return EventResult::Handled; }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    self.dragging = true;
                    let drag_axis = if self.vertical { position.y } else { position.x };
                    self.value = self.pos_to_value(drag_axis).clamp(self.min, self.max);
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } => {
                if *button == MouseButton::Left && self.dragging {
                    self.dragging = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::Down) | Event::KeyDown(Key::Left) => {
                if self.focused {
                    self.value = (self.value - self.step).max(self.min);
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::Up) | Event::KeyDown(Key::Right) => {
                if self.focused {
                    self.value = (self.value + self.step).min(self.max);
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::Home) => {
                if self.focused {
                    self.value = self.min;
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::End) => {
                if self.focused {
                    self.value = self.max;
                    self.trigger_change();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
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
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
        if self.vertical {
            let track_w = self.track_bounds.size.width;
            self.track_bounds.origin =
                Point::new(pos.x + (self.bounds.size.width - track_w) / 2.0, pos.y + 8.0);
        } else {
            self.track_bounds.origin = Point::new(pos.x + 8.0, pos.y + 10.0);
        }
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

    fn element_type_name(&self) -> &str { "Slider" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = self.mss.width { self.width = Some(w); }
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
            role: crate::a11y::Role::Slider,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                focused: self.focused,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                value: Some(format!("{:.1}", self.value)),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for SliderElement {
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

    fn direct(s: &Slider) -> SliderElement {
        SliderElement {
            id: ElementId::new(),
            value: s.value,
            min: s.min,
            max: s.max,
            step: s.step,
            disabled: s.disabled,
            width: s.width,
            vertical: s.vertical,
            bipolar: s.bipolar,
            bounds: Rect::zero(),
            track_bounds: Rect::zero(),
            dragging: false,
            hover: false,
            focused: false,
            on_change: s.on_change.clone(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
        }
    }

    #[test]
    fn vertical_slider_layout_uses_height() {
        let s = Slider::new().vertical().range(-1.0, 1.0).value(0.0);
        let mut elem = direct(&s);
        let constraints = Constraints::tight(Size::new(200.0, 100.0));
        let size = elem.layout(constraints);
        assert!(size.height > 50.0, "vertical должен занять height: got {size:?}");
        assert!(size.width <= 50.0, "vertical width — толщина карты: got {size:?}");
    }

    #[test]
    fn vertical_value_to_pos_inverted() {
        let s = Slider::new().vertical().range(0.0, 100.0).value(0.0);
        let mut elem = direct(&s);
        elem.layout(Constraints::tight(Size::new(24.0, 120.0)));
        let pos_min = elem.value_to_pos(0.0);
        let pos_max = elem.value_to_pos(100.0);
        assert!(pos_max < pos_min, "max value должен быть выше: max_y={pos_max} min_y={pos_min}");
    }

    #[test]
    fn horizontal_value_to_pos_normal() {
        let s = Slider::new().range(0.0, 100.0).value(50.0);
        let mut elem = direct(&s);
        elem.layout(Constraints::tight(Size::new(200.0, 24.0)));
        let pos_min = elem.value_to_pos(0.0);
        let pos_max = elem.value_to_pos(100.0);
        assert!(pos_min < pos_max, "min value слева: min_x={pos_min} max_x={pos_max}");
    }

    #[test]
    fn pos_to_value_roundtrip_vertical() {
        let s = Slider::new().vertical().range(-10.0, 10.0).value(0.0).step(0.0);
        let mut elem = direct(&s);
        elem.layout(Constraints::tight(Size::new(24.0, 120.0)));
        for v in [-10.0_f32, -5.0, 0.0, 5.0, 10.0] {
            let pos = elem.value_to_pos(v);
            let back = elem.pos_to_value(pos);
            assert!(
                (back - v).abs() < 0.5,
                "vertical roundtrip {v} → pos={pos} → {back}"
            );
        }
    }

    #[test]
    fn bipolar_requires_min_lt_zero_lt_max() {
        let s = Slider::new().range(-18.0, 18.0).value(6.0).bipolar();
        let elem = direct(&s);
        assert!(elem.bipolar);
        assert!(elem.min < 0.0 && elem.max > 0.0);
    }
}
