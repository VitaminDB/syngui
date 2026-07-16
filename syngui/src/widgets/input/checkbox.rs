use crate::animation::transition::{AnimatedPropertyMap, ResolvedProps};
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields, TextAlign, TextDecoration};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;
use std::time::Duration;

pub struct Checkbox {
    pub checked: bool,
    pub disabled: bool,
    pub label: Option<String>,
    pub on_change: Option<Arc<Mutex<dyn FnMut(bool) + Send>>>,
}

impl Checkbox {
    pub fn new() -> Self { Self { checked: false, disabled: false, label: None, on_change: None } }
    pub fn checked(checked: bool) -> Self { Self::new().with_checked(checked) }
    pub fn with_checked(mut self, checked: bool) -> Self { self.checked = checked; self }
    pub fn label(mut self, label: impl Into<String>) -> Self { self.label = Some(label.into()); self }
    pub fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub fn on_change(mut self, callback: impl FnMut(bool) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(callback))); self
    }
}

impl Default for Checkbox {
    fn default() -> Self { Self::new() }
}

impl Widget for Checkbox {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(CheckboxElement {
            id: ElementId::new(), checked: self.checked, disabled: self.disabled,
            label: self.label.clone(), bounds: Rect::zero(), checkbox_bounds: Rect::zero(),
            hover: false, focused: false, on_change: self.on_change.clone(),
            classes: Vec::new(), dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            style_checked: None,
            text_measure: None,
            check_reveal: if self.checked { 1.0 } else { 0.0 },
        })
    }
    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct CheckboxElement {
    id: ElementId,
    checked: bool, disabled: bool, label: Option<String>,
    bounds: Rect, checkbox_bounds: Rect,
    hover: bool, focused: bool,
    on_change: Option<Arc<Mutex<dyn FnMut(bool) + Send>>>,
    classes: Vec<String>, dirty_flags: DirtyFlags,
    mss: MssFields,
    style_checked: Option<ResolvedProps>,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
    check_reveal: f32,
}

fn ease_out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t;
    1.0 - inv * inv * inv
}

impl CheckboxElement {
    fn start_transition_to_current_state(&mut self) {
        let base_bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let base_border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        let (target_bg, target_border) = if self.disabled {
            (base_bg.darken(0.05), base_border)
        } else if self.checked {
            (
                self.style_checked.as_ref().and_then(|s| s.background_color()).unwrap_or(primary),
                self.style_checked.as_ref().and_then(|s| s.border_color()).unwrap_or(primary),
            )
        } else if self.hover {
            (base_bg, primary)
        } else {
            (base_bg, base_border)
        };

        let from_bg = self.mss.transition.background_color().unwrap_or(base_bg);
        let from_bc = self.mss.transition.border_color().unwrap_or(base_border);
        let from = AnimatedPropertyMap::new()
            .with_color("background-color", from_bg)
            .with_color("border-color", from_bc);
        let to = AnimatedPropertyMap::new()
            .with_color("background-color", target_bg)
            .with_color("border-color", target_border);

        if !self.mss.transition.has_specs() {
            self.mss.transition.add_default_specs(150.0);
        }
        self.mss.transition.start_transition(&from, &to);
    }
}

impl Element for CheckboxElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(cb) = widget.as_any().downcast_ref::<Checkbox>() {
            let was_checked = self.checked;
            self.checked = cb.checked;
            self.disabled = cb.disabled;
            self.label = cb.label.clone();
            self.on_change = cb.on_change.clone();
            if self.checked != was_checked {
                self.start_transition_to_current_state();
            }
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let checkbox_size: f32 = 20.0;
        let gap = 8.0;
        let font_size = self.mss.font_size.unwrap_or(14.0);
        let bold = self.mss.font_weight.unwrap_or(400) >= 700;
        let label_width = self.label.as_ref().map(|l| {
            self.text_measure.as_ref()
                .map(|tm| tm.measure_text_width_styled(l, font_size, l.chars().count(), bold, self.mss.font_family.as_deref()))
                .unwrap_or(l.chars().count() as f32 * font_size * 0.65)
        }).unwrap_or(0.0);
        let width = checkbox_size + if self.label.is_some() { gap + label_width } else { 0.0 };
        let height = checkbox_size.max(20.0);
        self.checkbox_bounds = Rect::new(
            Point::new(0.0, (height - checkbox_size) / 2.0),
            Size::new(checkbox_size, checkbox_size),
        );
        let width = width.min(constraints.max_width);
        let height = height.min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let base_bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let base_fg = self.mss.color.unwrap_or(Color::from_hex("#374151"));
        let base_border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        let (target_bg, target_border) = if self.disabled {
            (base_bg.darken(0.05), base_border)
        } else if self.checked {
            (primary, primary)
        } else if self.hover {
            (base_bg, primary)
        } else {
            (base_bg, base_border)
        };

        let (bg_color, border_color) = if self.mss.transition.is_animating() {
            let bg = self.mss.transition.background_color().unwrap_or(target_bg);
            let bc = self.mss.transition.border_color().unwrap_or(target_border);
            (bg, bc)
        } else {
            (target_bg, target_border)
        };

        let cb_radius = self.mss.border_radius_uniform(20.0, 4.0);
        let cb_border_width = self.mss.border_width_or(2.0);
        list.push_rect_bordered(self.checkbox_bounds, bg_color, [cb_radius; 4],
            Border { width: cb_border_width, color: border_color });

        let reveal = ease_out_cubic(self.check_reveal.clamp(0.0, 1.0));
        if reveal > 0.01 {
            let base_alpha = if self.disabled { 0.4 } else { 1.0 };
            let check_alpha = base_alpha * reveal;
            let check_color = if self.disabled {
                base_fg.with_alpha(check_alpha)
            } else {
                Color::WHITE.with_alpha(check_alpha)
            };
            let scale = {
                let t = reveal;
                let overshoot = (1.0 - (2.0 * t - 1.0).abs()).max(0.0) * 0.08;
                0.6 + 0.4 * t + overshoot
            };
            let icon_size = self.checkbox_bounds.size.width * 0.8 * scale;
            let check_rect = Rect::new(
                Point::new(
                    self.checkbox_bounds.x() + (self.checkbox_bounds.size.width - icon_size) * 0.5,
                    self.checkbox_bounds.y() + (self.checkbox_bounds.size.height - icon_size) * 0.5,
                ),
                Size::new(icon_size, icon_size),
            );
            list.push_text_centered("\u{E5CA}", check_rect, check_color, icon_size);
        }

        if let Some(ref label) = self.label {
            let font_size = self.mss.font_size.unwrap_or(14.0);
            let font_weight = self.mss.font_weight.unwrap_or(400);
            let text_x = self.checkbox_bounds.x() + self.checkbox_bounds.size.width + 8.0;
            let bold = font_weight >= 700;
            let label_w = self.text_measure.as_ref()
                .map(|tm| tm.measure_text_width_styled(label, font_size, label.chars().count(), bold, self.mss.font_family.as_deref()))
                .unwrap_or(label.chars().count() as f32 * font_size * 0.65) + 4.0;
            let label_rect = Rect::new(
                Point::new(text_x, self.checkbox_bounds.y() + 2.0),
                Size::new(label_w, font_size + 2.0),
            );
            let label_color = if self.disabled { base_fg.with_alpha(0.4) } else { base_fg };
            list.push_text_styled(label, label_rect, label_color, font_size,
                TextAlign::DEFAULT, TextDecoration::None, font_weight, self.mss.font_family.clone());
        }

    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled { return EventResult::Ignored; }
        match event {
            Event::MouseMove(pos) => {
                let was_hover = self.hover;
                self.hover = self.bounds.contains(*pos);
                if self.hover { ctx.set_cursor(CursorIcon::Pointer); }
                if self.hover != was_hover { ctx.request_paint(); return EventResult::Handled; }
                if self.hover { return EventResult::Handled; }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    self.checked = !self.checked;
                    self.start_transition_to_current_state();
                    if let Some(ref callback) = self.on_change {
                        if let Ok(mut cb) = callback.lock() { cb(self.checked); }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::Enter) | Event::KeyDown(Key::Space) => {
                if self.focused {
                    self.checked = !self.checked;
                    self.start_transition_to_current_state();
                    if let Some(ref callback) = self.on_change {
                        if let Ok(mut cb) = callback.lock() { cb(self.checked); }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::FocusGained => { self.focused = true; ctx.request_paint(); EventResult::Handled }
            Event::FocusLost => { self.focused = false; ctx.request_paint(); EventResult::Handled }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let target = if self.checked { 1.0 } else { 0.0 };
        let mut changed = false;
        if (self.check_reveal - target).abs() > 1e-4 {
            let step = 6.0 * dt.as_secs_f32();
            if self.check_reveal < target {
                self.check_reveal = (self.check_reveal + step).min(target);
            } else {
                self.check_reveal = (self.check_reveal - step).max(target);
            }
            changed = true;
        }
        let trans_anim = self.mss.transition.tick(dt.as_secs_f32());
        changed || trans_anim
    }

    fn needs_repaint(&self) -> bool {
        let target = if self.checked { 1.0 } else { 0.0 };
        (self.check_reveal - target).abs() > 1e-4 || self.mss.transition.is_animating()
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
        self.checkbox_bounds.origin = Point::new(pos.x, pos.y + (self.bounds.size.height - 20.0) / 2.0);
    }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "Checkbox" }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(f) = style.get("font-family").and_then(|v| v.as_string().map(|s| s.to_string())) {
            self.mss.font_family = Some(f);
        }
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        _active: Option<&ComputedStyle>,
        _focus: Option<&ComputedStyle>,
        _selected: Option<&ComputedStyle>,
        checked: Option<&ComputedStyle>,
    ) {
        self.mss.apply_transitions(base, hover, None, None, None);
        self.style_checked = checked
            .or(hover)
            .map(ResolvedProps::from_style)
            .or_else(|| {
                self.mss.accent_color.map(|accent| AnimatedPropertyMap::new()
                    .with_color("background-color", accent)
                    .with_color("border-color", accent))
            });
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::CheckBox,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                focused: self.focused,
                checked: Some(self.checked),
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: self.label.clone(),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for CheckboxElement {
    fn apply_style(&mut self, _style: &ComputedStyle) { self.mark_dirty(DirtyFlags::RENDER); }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
}
