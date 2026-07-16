use crate::animation::transition::ResolvedProps;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;
use std::time::Duration;

pub struct Toggle {
    pub is_on: bool,
    pub disabled: bool,
    pub on_change: Option<Arc<Mutex<dyn FnMut(bool) + Send>>>,
}

impl Toggle {
    pub fn new() -> Self { Self { is_on: false, disabled: false, on_change: None } }
    pub fn with_state(is_on: bool) -> Self { Self::new().on(is_on) }
    pub fn on(mut self, is_on: bool) -> Self { self.is_on = is_on; self }
    pub fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub fn on_change(mut self, callback: impl FnMut(bool) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(callback))); self
    }
}

impl Default for Toggle {
    fn default() -> Self { Self::new() }
}

impl Widget for Toggle {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ToggleElement {
            id: ElementId::new(), is_on: self.is_on, disabled: self.disabled,
            bounds: Rect::zero(), hover: false, focused: false,
            on_change: self.on_change.clone(),
            classes: Vec::new(), dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            style_checked: None,
        })
    }
    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct ToggleElement {
    id: ElementId,
    is_on: bool, disabled: bool,
    bounds: Rect, hover: bool, focused: bool,
    on_change: Option<Arc<Mutex<dyn FnMut(bool) + Send>>>,
    classes: Vec<String>, dirty_flags: DirtyFlags,
    mss: MssFields,
    style_checked: Option<ResolvedProps>,
}

impl ToggleElement {
    fn target_props(&self) -> ResolvedProps {
        if self.is_on {
            if let Some(ref p) = self.style_checked { return p.clone(); }
        }
        self.mss.style_normal.clone().unwrap_or_else(|| ResolvedProps::new())
    }

    fn start_transition_to_current_state(&mut self) {
        if !self.mss.has_mss_styles || !self.mss.transition.has_specs() { return; }
        let Some(ref normal) = self.mss.style_normal else { return };
        let mut from = ResolvedProps::new();
        if let Some(c) = self.mss.transition.background_color().or(normal.background_color()) {
            from.set_color("background-color", c);
        }
        if let Some(c) = self.mss.transition.color().or(normal.color()) {
            from.set_color("color", c);
        }
        if let Some(c) = self.mss.transition.border_color().or(normal.border_color()) {
            from.set_color("border-color", c);
        }
        if let Some(c) = self.mss.transition.outline_color().or(normal.outline_color()) {
            from.set_color("outline-color", c);
        }
        if let Some(v) = self.mss.transition.opacity().or(normal.opacity()) {
            from.set_float("opacity", v);
        }
        if let Some(v) = self.mss.transition.outline_width().or(normal.outline_width()) {
            from.set_float("outline-width", v);
        }
        if let Some(v) = self.mss.transition.border_width().or(normal.border_width()) {
            from.set_float("border-width", v);
        }
        let to = self.target_props();
        self.mss.transition.start_transition(&from, &to);
    }
}

impl Element for ToggleElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(toggle) = widget.as_any().downcast_ref::<Toggle>() {
            let was_on = self.is_on;
            self.is_on = toggle.is_on;
            self.disabled = toggle.disabled;
            self.on_change = toggle.on_change.clone();
            if self.is_on != was_on { self.start_transition_to_current_state(); }
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width: f32 = 44.0;
        let height: f32 = 24.0;
        let width = width.min(constraints.max_width);
        let height = height.min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let gray_200 = self.mss.background_color.unwrap_or(Color::from_hex("#E5E7EB"));
        let gray_300 = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let gray_400 = self.mss.color.unwrap_or(Color::from_hex("#9CA3AF")).with_alpha(0.6);
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let primary_dark = primary.darken(0.15);
        let _white = Color::WHITE;
        let track_radius = self.mss.border_radius_uniform(24.0, 12.0);
        let thumb_radius = (track_radius - 2.0).max(0.0);

        let track_color = if self.disabled {
            gray_200
        } else if self.is_on {
            let checked_bg = self.style_checked
                .as_ref()
                .and_then(|s| s.background_color());
            let base_on = checked_bg.unwrap_or(primary);
            if self.hover { base_on.darken(0.15) } else { base_on }
        } else if self.mss.has_mss_styles {
            let target = self.target_props();
            self.mss.transition.background_color()
                .or(target.background_color())
                .unwrap_or(gray_200)
        } else {
            if self.hover { gray_300 } else { gray_200 }
        };
        let _ = primary_dark;

        list.push_rect(self.bounds, track_color, [track_radius; 4]);

        let thumb_size: f32 = self.bounds.size.height - 4.0;
        let thumb_x = if self.is_on {
            self.bounds.x() + self.bounds.size.width - thumb_size - 2.0
        } else {
            self.bounds.x() + 2.0
        };
        let thumb_y = self.bounds.y() + 2.0;
        let thumb_rect = Rect::new(Point::new(thumb_x, thumb_y), Size::new(thumb_size, thumb_size));
        let thumb_base = self.mss.color.unwrap_or(Color::WHITE);
        let thumb_color = if self.disabled { gray_400 } else { thumb_base };
        list.push_shadow(thumb_rect, Color::new(0.0, 0.0, 0.0, 0.15), 2.0, (0.0, 1.0), [thumb_radius; 4]);
        list.push_rect(thumb_rect, thumb_color, [thumb_radius; 4]);

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
                    self.is_on = !self.is_on;
                    self.start_transition_to_current_state();
                    if let Some(ref callback) = self.on_change {
                        if let Ok(mut cb) = callback.lock() { cb(self.is_on); }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::Enter) | Event::KeyDown(Key::Space) => {
                if self.focused {
                    self.is_on = !self.is_on;
                    self.start_transition_to_current_state();
                    if let Some(ref callback) = self.on_change {
                        if let Ok(mut cb) = callback.lock() { cb(self.is_on); }
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
        self.mss.transition.tick(dt.as_secs_f32())
    }

    fn needs_repaint(&self) -> bool {
        self.mss.transition.is_animating()
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "Toggle" }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
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
            .map(crate::animation::transition::ResolvedProps::from_style);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::CheckBox,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                focused: self.focused,
                checked: Some(self.is_on),
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties::default(),
        })
    }
}

impl StyledElement for ToggleElement {
    fn apply_style(&mut self, _style: &ComputedStyle) { self.mark_dirty(DirtyFlags::RENDER); }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
}
