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

pub type RadioGroupState = Arc<Mutex<String>>;

pub struct RadioGroup {
    pub group_id: String,
    pub selected: RadioGroupState,
}

impl RadioGroup {
    pub fn new(group_id: impl Into<String>) -> Self {
        Self {
            group_id: group_id.into(),
            selected: Arc::new(Mutex::new(String::new())),
        }
    }

    pub fn selected(self, value: impl Into<String>) -> Self {
        *self.selected.lock().unwrap() = value.into();
        self
    }
}

pub struct RadioButton {
    pub value: String,
    pub group_id: String,
    pub group_state: RadioGroupState,
    pub label: Option<String>,
    pub disabled: bool,
}

impl RadioButton {
    pub fn new(value: impl Into<String>, group: &RadioGroup) -> Self {
        Self {
            value: value.into(),
            group_id: group.group_id.clone(),
            group_state: group.selected.clone(),
            label: None,
            disabled: false,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Widget for RadioButton {
    fn create_element(&self) -> Box<dyn Element> {
        let selected = self.group_state.lock().unwrap().clone();
        Box::new(RadioButtonElement {
            id: ElementId::new(),
            value: self.value.clone(),
            group_id: self.group_id.clone(),
            group_state: self.group_state.clone(),
            label: self.label.clone(),
            disabled: self.disabled,
            is_selected: selected == self.value,
            bounds: Rect::zero(),
            radio_bounds: Rect::zero(),
            hover: false,
            focused: false,
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

pub struct RadioButtonElement {
    id: ElementId,
    value: String,
    group_id: String,
    group_state: RadioGroupState,
    label: Option<String>,
    disabled: bool,
    is_selected: bool,
    bounds: Rect,
    radio_bounds: Rect,
    hover: bool,
    focused: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
}

impl Element for RadioButtonElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(rb) = widget.as_any().downcast_ref::<RadioButton>() {
            self.value = rb.value.clone();
            self.group_id = rb.group_id.clone();
            self.group_state = rb.group_state.clone();
            self.label = rb.label.clone();
            self.disabled = rb.disabled;

            let selected = self.group_state.lock().unwrap().clone();
            self.is_selected = selected == self.value;

            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let radio_size: f32 = 20.0;
        let gap = 8.0;

        let font_size = self.mss.font_size.unwrap_or(14.0);
        let bold = self.mss.font_weight.unwrap_or(400) >= 700;
        let label_width = self.label.as_ref().map(|l| {
            self.text_measure.as_ref()
                .map(|tm| tm.measure_text_width_styled(l, font_size, l.chars().count(), bold, self.mss.font_family.as_deref()))
                .unwrap_or(l.chars().count() as f32 * font_size * 0.65)
        }).unwrap_or(0.0);
        let width = radio_size + if self.label.is_some() { gap + label_width } else { 0.0 };
        let height = radio_size.max(20.0);

        self.radio_bounds = Rect::new(
            Point::new(0.0, (height - radio_size) / 2.0),
            Size::new(radio_size, radio_size),
        );

        let width = width.min(constraints.max_width);
        let height = height.min(constraints.max_height);

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let fg = self.mss.color.unwrap_or(Color::from_hex("#374151"));
        let default_border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let disabled_fg = fg.with_alpha(0.5);
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let white = Color::WHITE;
        let disabled_bg = default_border.lighten(0.4);
        let border_width = self.mss.border_width_or(2.0);

        let (bg_color, border_color) = if self.disabled {
            (disabled_bg, default_border)
        } else if self.is_selected {
            (white, primary)
        } else if self.hover {
            (white, primary)
        } else {
            (white, default_border)
        };

        list.push_rect_bordered(
            self.radio_bounds,
            bg_color,
            [10.0; 4],
            Border { width: border_width, color: border_color },
        );

        if self.is_selected {
            let inner_size: f32 = 10.0;
            let inner_rect = Rect::new(
                Point::new(
                    self.radio_bounds.x() + (self.radio_bounds.size.width - inner_size) / 2.0,
                    self.radio_bounds.y() + (self.radio_bounds.size.height - inner_size) / 2.0,
                ),
                Size::new(inner_size, inner_size),
            );
            let dot_color = if self.disabled { disabled_fg } else { primary };
            list.push_rect(inner_rect, dot_color, [5.0; 4]);
        }

        if let Some(ref label) = self.label {
            let font_size = self.mss.font_size.unwrap_or(14.0);
            let font_weight = self.mss.font_weight.unwrap_or(400);
            let text_x = self.radio_bounds.x() + self.radio_bounds.size.width + 8.0;
            let bold = font_weight >= 700;
            let label_w = self.text_measure.as_ref()
                .map(|tm| tm.measure_text_width_styled(label, font_size, label.chars().count(), bold, self.mss.font_family.as_deref()))
                .unwrap_or(label.chars().count() as f32 * font_size * 0.65) + 4.0;
            let label_rect = Rect::new(
                Point::new(text_x, self.radio_bounds.y() + 2.0),
                Size::new(label_w, font_size + 2.0),
            );
            let label_color = if self.disabled { disabled_fg } else { fg };
            list.push_text_styled(label, label_rect, label_color, font_size,
                TextAlign::DEFAULT, TextDecoration::None, font_weight, self.mss.font_family.clone());
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
                if self.hover != was_hover {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hover { return EventResult::Handled; }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    *self.group_state.lock().unwrap() = self.value.clone();
                    self.is_selected = true;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::Enter) | Event::KeyDown(Key::Space) => {
                if self.focused {
                    *self.group_state.lock().unwrap() = self.value.clone();
                    self.is_selected = true;
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
        self.radio_bounds.origin = Point::new(
            pos.x,
            pos.y + (self.bounds.size.height - 20.0) / 2.0,
        );
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

    fn element_type_name(&self) -> &str { "RadioButton" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(f) = style.get("font-family").and_then(|v| v.as_string().map(|s| s.to_string())) {
            self.mss.font_family = Some(f);
        }
        self.apply_style(style);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::RadioButton,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                focused: self.focused,
                selected: self.is_selected,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: self.label.clone(),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for RadioButtonElement {
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
