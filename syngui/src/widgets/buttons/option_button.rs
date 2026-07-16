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

pub struct OptionButton {
    pub text: String,
    pub pressed: Arc<Mutex<bool>>,
    pub icon: Option<String>,
    pub disabled: bool,
    pub on_toggle: Option<Arc<Mutex<dyn FnMut(bool) + Send>>>,
}

impl OptionButton {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            pressed: Arc::new(Mutex::new(false)),
            icon: None,
            disabled: false,
            on_toggle: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn pressed_state(mut self, pressed: Arc<Mutex<bool>>) -> Self {
        self.pressed = pressed;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_toggle(mut self, callback: impl FnMut(bool) + Send + 'static) -> Self {
        self.on_toggle = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Widget for OptionButton {
    fn create_element(&self) -> Box<dyn Element> {
        let pressed = self.pressed.lock().map(|p| *p).unwrap_or(false);
        Box::new(OptionButtonElement {
            id: ElementId::new(),
            text: self.text.clone(),
            icon: self.icon.clone(),
            disabled: self.disabled,
            pressed,
            hover: false,
            focused: false,
            bounds: Rect::zero(),
            shared_pressed: self.pressed.clone(),
            on_toggle: self.on_toggle.clone(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
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
}

pub struct OptionButtonElement {
    id: ElementId,
    text: String,
    icon: Option<String>,
    disabled: bool,
    pressed: bool,
    hover: bool,
    focused: bool,
    bounds: Rect,
    shared_pressed: Arc<Mutex<bool>>,
    on_toggle: Option<Arc<Mutex<dyn FnMut(bool) + Send>>>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
}

impl Element for OptionButtonElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(btn) = widget.as_any().downcast_ref::<OptionButton>() {
            self.text = btn.text.clone();
            self.icon = btn.icon.clone();
            self.disabled = btn.disabled;
            self.shared_pressed = btn.pressed.clone();
            self.pressed = btn.pressed.lock().map(|p| *p).unwrap_or(false);
            self.on_toggle = btn.on_toggle.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let padding_h: f32 = 12.0;
        let font_size = self.mss.font_size_or(14.0);
        let bold = self.mss.font_weight_or(400) >= 700;
        let base_height = self.mss.height.map(|d| d.resolve(constraints.max_height)).unwrap_or(36.0);

        let text_width = self.text_measure.as_ref()
            .map(|tm| tm.measure_text_width_styled(&self.text, font_size, self.text.chars().count(), bold, self.mss.font_family.as_deref()))
            .unwrap_or(self.text.chars().count() as f32 * font_size * 0.6);
        let icon_width = if self.icon.is_some() { 16.0 + 6.0 } else { 0.0 };
        let width = (text_width + icon_width + padding_h * 2.0).max(36.0).min(constraints.max_width);
        let height = base_height.min(constraints.max_height);

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let base_bg = self.mss.background_color.unwrap_or(Color::TRANSPARENT);
        let base_fg = self.mss.color.unwrap_or(Color::from_hex("#374151"));
        let base_border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let accent = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let font_size = self.mss.font_size_or(14.0);
        let font_weight = self.mss.font_weight_or(400);
        let border_radius = self.mss.border_radius_uniform(self.bounds.size.width.min(self.bounds.size.height), 6.0);
        let border_width = self.mss.border_width_or(1.0);

        let (bg_color, text_color, border) = if self.disabled {
            (base_border.with_alpha(0.3), base_fg.with_alpha(0.4), Some(Border { width: border_width, color: base_border.with_alpha(0.5) }))
        } else if self.pressed {
            if self.hover {
                (accent.darken(0.1), Color::WHITE, None)
            } else {
                (accent, Color::WHITE, None)
            }
        } else if self.hover {
            (base_bg.darken(0.05), base_fg, Some(Border { width: border_width, color: base_border }))
        } else {
            (base_bg, base_fg, Some(Border { width: border_width, color: base_border }))
        };

        if let Some(border) = border {
            list.push_rect_bordered(self.bounds, bg_color, [border_radius; 4], border);
        } else if bg_color.a > 0.0 {
            list.push_rect(self.bounds, bg_color, [border_radius; 4]);
        }

        let mut text_x = self.bounds.x() + 12.0;

        if let Some(ref icon) = self.icon {
            let icon_rect = Rect::new(
                Point::new(text_x, self.bounds.y() + (self.bounds.size.height - 16.0) / 2.0),
                Size::new(16.0, 16.0),
            );
            list.push_text(icon, icon_rect, text_color, font_size);
            text_x += 16.0 + 6.0;
        }

        let text_rect = Rect::new(
            Point::new(text_x, self.bounds.y() + (self.bounds.size.height - 18.0) / 2.0),
            Size::new(self.bounds.size.width - (text_x - self.bounds.x()) - 12.0, 18.0),
        );
        list.push_text_styled(
            &self.text, text_rect, text_color, font_size,
            crate::mss::TextAlign::DEFAULT, crate::mss::TextDecoration::None,
            font_weight, self.mss.font_family.clone(),
        );
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
                if self.hover != was_hover {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hover { return EventResult::Handled; }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    self.pressed = !self.pressed;
                    if let Ok(mut p) = self.shared_pressed.lock() {
                        *p = self.pressed;
                    }
                    if let Some(ref callback) = self.on_toggle {
                        if let Ok(mut cb) = callback.lock() {
                            cb(self.pressed);
                        }
                    }
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
            Event::KeyDown(Key::Enter) | Event::KeyDown(Key::Space) if self.focused => {
                self.pressed = !self.pressed;
                if let Ok(mut p) = self.shared_pressed.lock() {
                    *p = self.pressed;
                }
                if let Some(ref callback) = self.on_toggle {
                    if let Ok(mut cb) = callback.lock() {
                        cb(self.pressed);
                    }
                }
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

    fn element_type_name(&self) -> &str { "OptionButton" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
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
            role: crate::a11y::Role::Button,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                pressed: self.pressed,
                focused: self.focused,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: Some(self.text.clone()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for OptionButtonElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] {
        &self.classes
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
