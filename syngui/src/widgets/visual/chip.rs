use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields, TextAlign, TextDecoration};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;

const DEFAULT_HEIGHT: f32 = 32.0;
const DEFAULT_FONT_SIZE: f32 = 13.0;
const DEFAULT_ICON_SIZE: f32 = 18.0;
const DEFAULT_PADDING_H: f32 = 10.0;
const DEFAULT_ICON_GAP: f32 = 8.0;
const DEFAULT_DELETE_ZONE: f32 = 22.0;

pub struct Chip {
    pub label: String,
    pub icon: Option<String>,
    pub deletable: bool,
    pub selected: bool,
    pub disabled: bool,
    pub on_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    pub on_delete: Option<Arc<Mutex<dyn FnMut() + Send>>>,
}

impl Chip {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            deletable: false,
            selected: false,
            disabled: false,
            on_click: None,
            on_delete: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn deletable(mut self) -> Self {
        self.deletable = true;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(mut self, cb: impl FnMut() + Send + 'static) -> Self {
        self.on_click = Some(Arc::new(Mutex::new(cb)));
        self
    }

    pub fn on_delete(mut self, cb: impl FnMut() + Send + 'static) -> Self {
        self.on_delete = Some(Arc::new(Mutex::new(cb)));
        self
    }
}

impl Widget for Chip {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ChipElement {
            id: ElementId::new(),
            label: self.label.clone(),
            icon: self.icon.clone(),
            deletable: self.deletable,
            selected: self.selected,
            disabled: self.disabled,
            on_click: self.on_click.clone(),
            on_delete: self.on_delete.clone(),
            hovered: false,
            pressed: false,
            delete_hovered: false,
            bounds: Rect::zero(),
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

pub struct ChipElement {
    id: ElementId,
    label: String,
    icon: Option<String>,
    deletable: bool,
    selected: bool,
    disabled: bool,
    on_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    on_delete: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    hovered: bool,
    pressed: bool,
    delete_hovered: bool,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
}

impl ChipElement {
    fn height(&self) -> f32 {
        self.mss
            .height
            .map(|d| d.resolve(DEFAULT_HEIGHT))
            .unwrap_or(DEFAULT_HEIGHT)
    }

    fn padding_h(&self) -> f32 {
        self.mss.padding_left.unwrap_or(DEFAULT_PADDING_H)
    }

    fn icon_size(&self) -> f32 {
        self.mss.icon_size.unwrap_or(DEFAULT_ICON_SIZE)
    }

    fn background_color(&self) -> Color {
        let base_bg = self.mss.background_color.unwrap_or(Color::TRANSPARENT);
        let accent = self
            .mss
            .accent_color
            .unwrap_or_else(|| Color::from_hex("#3B82F6"));

        if self.disabled {
            return base_bg.with_alpha(0.5);
        }
        if self.selected {
            if self.pressed {
                accent.darken(0.15)
            } else if self.hovered {
                accent.darken(0.08)
            } else {
                accent
            }
        } else if self.pressed {
            base_bg.lighten(0.15)
        } else if self.hovered {
            base_bg.lighten(0.08)
        } else {
            base_bg
        }
    }

    fn text_color(&self) -> Color {
        let base_fg = self.mss.color.unwrap_or_else(|| Color::from_hex("#374151"));
        if self.disabled {
            base_fg.with_alpha(0.5)
        } else if self.selected {
            Color::WHITE
        } else {
            base_fg
        }
    }

    fn border_color(&self) -> Color {
        let base = self.mss.border_color.unwrap_or(Color::TRANSPARENT);
        if self.selected {
            Color::TRANSPARENT
        } else {
            base
        }
    }
}

impl Element for ChipElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(chip) = widget.as_any().downcast_ref::<Chip>() {
            self.label = chip.label.clone();
            self.icon = chip.icon.clone();
            self.deletable = chip.deletable;
            self.selected = chip.selected;
            self.disabled = chip.disabled;
            self.on_click = chip.on_click.clone();
            self.on_delete = chip.on_delete.clone();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let font_size = self.mss.font_size_or(DEFAULT_FONT_SIZE);
        let height = self.height();
        let pad_h = self.padding_h();
        let icon_size = self.icon_size();
        let mut width = pad_h * 2.0;

        if self.icon.is_some() {
            width += icon_size + DEFAULT_ICON_GAP;
        }

        let bold = self.selected;
        let text_width = self
            .text_measure
            .as_ref()
            .map(|tm| {
                tm.measure_text_width_styled(
                    &self.label,
                    font_size,
                    self.label.chars().count(),
                    bold,
                    self.mss.font_family.as_deref(),
                )
            })
            .unwrap_or_else(|| self.label.chars().count() as f32 * font_size * 0.65);
        width += text_width;

        if self.deletable {
            width += 4.0 + DEFAULT_DELETE_ZONE;
        }

        let width = self
            .mss
            .width
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(width.max(height))
            .min(constraints.max_width);

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let font_size = self.mss.font_size_or(DEFAULT_FONT_SIZE);
        let font_weight = self
            .mss
            .font_weight_or(if self.selected { 600 } else { 400 });
        let height = self.height();
        let pad_h = self.padding_h();
        let icon_size = self.icon_size();
        let actual_h = self.bounds.size.height;
        let radius = self.mss.border_radius_uniform(actual_h, actual_h / 2.0);
        let bg = self.background_color();
        let fg = self.text_color();
        let border_c = self.border_color();
        let border_w = self.mss.border_width.unwrap_or(0.0);

        if border_w > 0.0 && border_c != Color::TRANSPARENT {
            list.push_rect_bordered(
                self.bounds,
                bg,
                [radius; 4],
                Border {
                    color: border_c,
                    width: border_w,
                },
            );
        } else {
            list.push_rect(self.bounds, bg, [radius; 4]);
        }

        let mut x_offset = self.bounds.x() + pad_h;

        if let Some(ref icon) = self.icon {
            let icon_rect = Rect::new(
                Point::new(x_offset, self.bounds.y() + (height - icon_size) / 2.0),
                Size::new(icon_size, icon_size),
            );
            list.push_text_centered(icon, icon_rect, fg, icon_size);
            x_offset += icon_size + DEFAULT_ICON_GAP;
        }

        let measured_text_w = self
            .text_measure
            .as_ref()
            .map(|tm| {
                tm.measure_text_width_styled(
                    &self.label,
                    font_size,
                    self.label.chars().count(),
                    self.selected,
                    self.mss.font_family.as_deref(),
                )
            })
            .unwrap_or_else(|| self.label.chars().count() as f32 * font_size * 0.65);

        let avail_text = self.bounds.size.width
            - (x_offset - self.bounds.x())
            - pad_h
            - if self.deletable { DEFAULT_DELETE_ZONE } else { 0.0 };
        let text_width = measured_text_w.min(avail_text);

        let text_rect = Rect::new(
            Point::new(x_offset, self.bounds.y() + (height - font_size) / 2.0),
            Size::new(text_width, font_size + 2.0),
        );
        list.push_text_styled(
            &self.label,
            text_rect,
            fg,
            font_size,
            TextAlign::LEFT,
            TextDecoration::None,
            font_weight,
            self.mss.font_family.clone(),
        );

        if self.deletable {
            let delete_x = x_offset + text_width + 4.0;
            let delete_color = if self.delete_hovered && !self.disabled {
                fg.with_alpha(0.9)
            } else {
                fg.with_alpha(0.5)
            };
            let icon_sz = 16.0;
            let delete_rect = Rect::new(
                Point::new(delete_x, self.bounds.y() + (height - icon_sz) / 2.0),
                Size::new(DEFAULT_DELETE_ZONE, icon_sz),
            );
            list.push_text_styled(
                "\u{E5CD}",
                delete_rect,
                delete_color,
                icon_sz,
                TextAlign::CENTER,
                TextDecoration::None,
                400,
                Some("Material Icons".to_string()),
            );
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        let pad_h = self.padding_h();
        match event {
            Event::MouseMove(pos) => {
                let hovering = self.bounds.contains(*pos);
                let was_hover = self.hovered;
                self.hovered = hovering;

                if self.deletable && hovering {
                    let delete_x = self.bounds.x() + self.bounds.size.width - DEFAULT_DELETE_ZONE - pad_h;
                    self.delete_hovered = pos.x >= delete_x;
                } else {
                    self.delete_hovered = false;
                }

                if hovering && !self.disabled {
                    ctx.set_cursor(CursorIcon::Pointer);
                }

                if hovering != was_hover {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if hovering {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.disabled {
                    return EventResult::Ignored;
                }
                if self.bounds.contains(*position) {
                    self.pressed = true;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, position } if *button == MouseButton::Left && self.pressed => {
                self.pressed = false;
                if self.bounds.contains(*position) {
                    if self.deletable {
                        let delete_x =
                            self.bounds.x() + self.bounds.size.width - DEFAULT_DELETE_ZONE - pad_h;
                        if position.x >= delete_x {
                            if let Some(ref cb) = self.on_delete {
                                if let Ok(mut f) = cb.lock() {
                                    f();
                                }
                            }
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                    if let Some(ref cb) = self.on_click {
                        if let Ok(mut f) = cb.lock() {
                            f();
                        }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
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
    fn element_type_name(&self) -> &str {
        "Chip"
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.apply_style(style);
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
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        self.mss.transition.tick(dt.as_secs_f32())
    }

    fn needs_repaint(&self) -> bool {
        self.mss.transition.is_animating()
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Button,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                pressed: self.pressed,
                selected: self.selected,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: Some(self.label.clone()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for ChipElement {
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
