use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{Border, DisplayList};
use crate::signal::RwSignal;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum IconPosition {
    #[default]
    Leading,
    Trailing,
}

const DEFAULT_ICON_SIZE: f32 = 18.0;
const DEFAULT_ICON_GAP: f32 = 8.0;

pub struct Button {
    pub text: String,
    pub disabled: bool,
    pub on_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    pub on_click_at: Option<Arc<Mutex<dyn FnMut(Point) + Send>>>,
    /// Клик с прямоугольником самой кнопки — чтобы выпадающие панели можно
    /// было привязать к ней, а не к позиции курсора.
    pub on_click_bounds: Option<Arc<Mutex<dyn FnMut(Rect) + Send>>>,
    pub active_index: Option<(RwSignal<usize>, usize)>,
    pub icon: Option<String>,
    pub icon_position: IconPosition,
    classes: Vec<String>,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            disabled: false,
            on_click: None,
            on_click_at: None,
            on_click_bounds: None,
            active_index: None,
            icon: None,
            icon_position: IconPosition::Leading,
            classes: Vec::new(),
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn leading_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self.icon_position = IconPosition::Leading;
        self
    }

    pub fn trailing_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self.icon_position = IconPosition::Trailing;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[deprecated(note = "Use MSS width property via .class() instead")]
    pub fn width(self, _width: f32) -> Self {
        self
    }
    #[deprecated(note = "Use MSS height property via .class() instead")]
    pub fn height(self, _height: f32) -> Self {
        self
    }

    pub fn active_index(mut self, signal: RwSignal<usize>, index: usize) -> Self {
        self.active_index = Some((signal, index));
        self
    }

    pub fn on_click(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_click = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn on_click_at(mut self, callback: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_click_at = Some(Arc::new(Mutex::new(callback)));
        self
    }

    /// Обработчик клика, получающий прямоугольник кнопки в координатах окна.
    pub fn on_click_with_bounds(mut self, callback: impl FnMut(Rect) + Send + 'static) -> Self {
        self.on_click_bounds = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn class(mut self, name: &str) -> Self {
        for c in name.split_whitespace() {
            let s = c.to_string();
            if !self.classes.contains(&s) {
                self.classes.push(s);
            }
        }
        self
    }
}

impl Widget for Button {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ButtonElement {
            id: ElementId::new(),
            text: self.text.clone(),
            disabled: self.disabled,
            on_click: self.on_click.clone(),
            on_click_at: self.on_click_at.clone(),
            on_click_bounds: self.on_click_bounds.clone(),
            active_index: self.active_index,
            icon: self.icon.clone(),
            icon_position: self.icon_position,
            bounds: Rect::zero(),
            hover: false,
            pressed: false,
            focused: false,
            selected: false,
            classes: self.classes.clone(),
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

    fn widget_classes(&self) -> &[String] {
        &self.classes
    }
}

pub struct ButtonElement {
    id: ElementId,
    text: String,
    disabled: bool,
    on_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    on_click_at: Option<Arc<Mutex<dyn FnMut(Point) + Send>>>,
    on_click_bounds: Option<Arc<Mutex<dyn FnMut(Rect) + Send>>>,
    active_index: Option<(RwSignal<usize>, usize)>,
    icon: Option<String>,
    icon_position: IconPosition,
    bounds: Rect,
    hover: bool,
    pressed: bool,
    focused: bool,
    selected: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
}

impl ButtonElement {
    fn update_selected_state(&mut self) -> bool {
        if let Some((signal, index)) = self.active_index {
            let was = self.selected;
            self.selected = signal.get_untracked() == index;
            if self.selected != was {
                self.start_transition_to_current_state();
                return true;
            }
        }
        false
    }

    fn get_colors(&self) -> (Color, Color, Option<Border>) {
        let target = self
            .mss
            .target_props(self.hover, self.pressed, self.focused, self.selected);
        let bg = self.mss.effective_bg(&target, Color::TRANSPARENT);
        let fg = self.mss.effective_fg(&target, Color::WHITE);
        let bc = self
            .mss
            .transition
            .border_color()
            .or(target.border_color())
            .or(self
                .mss
                .style_normal
                .as_ref()
                .and_then(|n| n.border_color()));
        let bw = self.mss.border_width_or(0.0);
        let border = if bw > 0.0 {
            bc.map(|c| Border {
                width: bw,
                color: c,
            })
        } else {
            None
        };
        (bg, fg, border)
    }

    fn start_transition_to_current_state(&mut self) {
        self.mss
            .start_transition_to(self.hover, self.pressed, self.focused, self.selected);
    }
}

impl Element for ButtonElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(btn) = widget.as_any().downcast_ref::<Button>() {
            self.text = btn.text.clone();
            self.disabled = btn.disabled;
            self.on_click = btn.on_click.clone();
            self.on_click_at = btn.on_click_at.clone();
            self.on_click_bounds = btn.on_click_bounds.clone();
            self.active_index = btn.active_index;
            self.icon = btn.icon.clone();
            self.icon_position = btn.icon_position;
            self.update_selected_state();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        crate::perf::incr(crate::perf::Counter::ButtonLayout);
        let font_size = self.mss.font_size.unwrap_or(14.0);
        let pad_l = self.mss.padding_left.unwrap_or(16.0);
        let pad_r = self.mss.padding_right.unwrap_or(16.0);
        let pad_t = self.mss.padding_top.unwrap_or(8.0);
        let pad_b = self.mss.padding_bottom.unwrap_or(8.0);
        let bold = self.mss.font_weight.unwrap_or(400) >= 700;
        let icon_size = self.mss.icon_size.unwrap_or(DEFAULT_ICON_SIZE);
        let icon_space = if self.icon.is_some() {
            if self.text.is_empty() {
                icon_size
            } else {
                icon_size + DEFAULT_ICON_GAP
            }
        } else {
            0.0
        };
        let text_width = if self.text.is_empty() {
            0.0
        } else {
            self
                .text_measure
                .as_ref()
                .map(|tm| {
                    crate::perf::incr(crate::perf::Counter::ButtonTextMeasure);
                    let t = web_time::Instant::now();
                    let w = tm.measure_text_width_styled(
                        &self.text,
                        font_size,
                        self.text.chars().count(),
                        bold,
                        self.mss.font_family.as_deref(),
                    );
                    crate::perf::add_time(crate::perf::TimeKind::ButtonTextMeasure, t.elapsed());
                    w
                })
                .unwrap_or(self.text.chars().count() as f32 * font_size * 0.6)
        };
        let natural_width = text_width + icon_space + pad_l + pad_r;
        let mut width = self
            .mss
            .width
            .map(|d| {
                let resolved = d.resolve(constraints.max_width);
                if resolved <= 0.0 {
                    natural_width
                } else {
                    resolved
                }
            })
            .unwrap_or_else(|| {
                natural_width
                    .max(constraints.min_width)
                    .min(constraints.max_width)
            });
        if let Some(max_w) = self.mss.max_width {
            width = width.min(max_w.resolve(constraints.max_width));
        }
        if let Some(min_w) = self.mss.min_width {
            width = width.max(min_w.resolve(constraints.max_width));
        }
        let available_text_w = (width - pad_l - pad_r).max(1.0);
        let line_count = if text_width > available_text_w + 0.1 {
            (text_width / available_text_w).ceil() as u32
        } else {
            1
        };
        let line_height = font_size * 1.2;
        let natural_height = (line_count as f32 * line_height) + pad_t + pad_b;
        let mut height = self
            .mss
            .height
            .map(|d| d.resolve(constraints.max_height))
            .unwrap_or_else(|| {
                natural_height
                    .max(constraints.min_height)
                    .min(constraints.max_height)
            });
        if let Some(min_h) = self.mss.min_height {
            height = height.max(min_h.resolve(constraints.max_height));
        }
        if let Some(max_h) = self.mss.max_height {
            height = height.min(max_h.resolve(constraints.max_height));
        }
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, clip: Rect) {
        let (mut bg_color, mut text_color, mut border) = self.get_colors();
        if self.disabled {
            bg_color = bg_color.with_alpha(bg_color.a * 0.5);
            text_color = text_color.with_alpha(text_color.a * 0.5);
            if let Some(ref mut b) = border {
                b.color = b.color.with_alpha(b.color.a * 0.5);
            }
        }
        let radius = self
            .mss
            .border_radius_uniform(self.bounds.size.width.min(self.bounds.size.height), 0.0);
        let cr = [radius; 4];

        if let Some(ref shadows) = self.mss.box_shadow {
            for shadow in &shadows.0 {
                if !shadow.inset {
                    list.push_shadow(
                        self.bounds,
                        shadow.color,
                        shadow.blur_radius,
                        (shadow.offset_x, shadow.offset_y),
                        cr,
                    );
                }
            }
        }

        // Градиентный фон (как DecoratedBox). Состояние-специфичные solid-цвета
        // (:hover/:pressed/:selected и transition) перекрывают градиент; при disabled
        // рисуем приглушённый solid.
        let use_gradient = self.mss.background_gradient.is_some()
            && !self.disabled
            && self.mss.transition.background_color().is_none()
            && self
                .mss
                .target_props(self.hover, self.pressed, self.focused, self.selected)
                .background_color()
                .is_none();

        if use_gradient {
            let grad = self.mss.background_gradient.as_ref().unwrap().clone();
            if let Some(border) = border {
                list.push_gradient_rect_bordered(self.bounds, grad, cr, border);
            } else {
                list.push_gradient_rect(self.bounds, grad, cr);
            }
        } else if let Some(border) = border {
            list.push_rect_bordered(self.bounds, bg_color, cr, border);
        } else if bg_color.a > 0.0 {
            list.push_rect(self.bounds, bg_color, cr);
        }
        let pad_l = self.mss.padding_left.unwrap_or(12.0);
        let pad_r = self.mss.padding_right.unwrap_or(12.0);
        let font_size = self.mss.font_size.unwrap_or(14.0);
        let font_weight = self.mss.font_weight.unwrap_or(400);
        let icon_size = self.mss.icon_size.unwrap_or(DEFAULT_ICON_SIZE);
        let has_icon = self.icon.is_some();
        let has_text = !self.text.is_empty();
        let icon_gap = if has_icon && has_text { DEFAULT_ICON_GAP } else { 0.0 };

        let content_x = self.bounds.x() + pad_l;
        let content_w = (self.bounds.size.width - pad_l - pad_r).max(0.0);

        if has_icon && has_text {
            let icon_str = self.icon.as_deref().unwrap();
            let icon_y = self.bounds.y() + (self.bounds.size.height - icon_size) / 2.0;
            let text_y = self.bounds.y() + (self.bounds.size.height - font_size) / 2.0;

            match self.icon_position {
                IconPosition::Leading => {
                    let icon_rect = Rect::new(
                        Point::new(content_x, icon_y),
                        Size::new(icon_size, icon_size),
                    );
                    list.push_text_centered(icon_str, icon_rect, text_color, icon_size);
                    let text_x = content_x + icon_size + icon_gap;
                    let text_w = (content_w - icon_size - icon_gap).max(0.0);
                    let text_rect = Rect::new(
                        Point::new(text_x, text_y),
                        Size::new(text_w, font_size),
                    );
                    list.push_text_styled(
                        &self.text, text_rect, text_color, font_size,
                        crate::mss::TextAlign::CENTER, crate::mss::TextDecoration::None,
                        font_weight, self.mss.font_family.clone(),
                    );
                }
                IconPosition::Trailing => {
                    let text_w = (content_w - icon_size - icon_gap).max(0.0);
                    let text_rect = Rect::new(
                        Point::new(content_x, text_y),
                        Size::new(text_w, font_size),
                    );
                    list.push_text_styled(
                        &self.text, text_rect, text_color, font_size,
                        crate::mss::TextAlign::CENTER, crate::mss::TextDecoration::None,
                        font_weight, self.mss.font_family.clone(),
                    );
                    let icon_x = content_x + text_w + icon_gap;
                    let icon_rect = Rect::new(
                        Point::new(icon_x, icon_y),
                        Size::new(icon_size, icon_size),
                    );
                    list.push_text_centered(icon_str, icon_rect, text_color, icon_size);
                }
            }
        } else if has_icon {
            let icon_str = self.icon.as_deref().unwrap();
            let icon_y = self.bounds.y() + (self.bounds.size.height - icon_size) / 2.0;
            let icon_rect = Rect::new(
                Point::new(content_x, icon_y),
                Size::new(content_w, icon_size),
            );
            list.push_text_centered(icon_str, icon_rect, text_color, icon_size);
        } else {
            let text_y = self.bounds.y() + (self.bounds.size.height - font_size) / 2.0;
            let text_rect = Rect::new(
                Point::new(content_x, text_y),
                Size::new(content_w, font_size),
            );
            list.push_text_styled(
                &self.text, text_rect, text_color, font_size,
                crate::mss::TextAlign::CENTER, crate::mss::TextDecoration::None,
                font_weight, self.mss.font_family.clone(),
            );
        }
        let _ = clip;
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled {
            return EventResult::Ignored;
        }
        match event {
            Event::MouseMove(pos) => {
                let was_hover = self.hover;
                self.hover = self.bounds.contains(*pos);
                if self.hover {
                    ctx.set_cursor(CursorIcon::Pointer);
                }
                if self.hover != was_hover {
                    self.start_transition_to_current_state();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hover {
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position }
                if *button == MouseButton::Left && self.bounds.contains(*position) =>
            {
                self.pressed = true;
                self.start_transition_to_current_state();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::MouseUp { button, position } if *button == MouseButton::Left && self.pressed => {
                self.pressed = false;
                self.start_transition_to_current_state();
                if self.bounds.contains(*position) {
                    if let Some(ref cb) = self.on_click {
                        if let Ok(mut f) = cb.lock() {
                            f();
                        }
                    }
                    if let Some(ref cb) = self.on_click_at {
                        if let Ok(mut f) = cb.lock() {
                            f(*position);
                        }
                    }
                    if let Some(ref cb) = self.on_click_bounds {
                        if let Ok(mut f) = cb.lock() {
                            f(self.bounds);
                        }
                    }
                }
                ctx.request_paint();
                EventResult::Handled
            }
            Event::FocusGained => {
                self.focused = true;
                self.start_transition_to_current_state();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::FocusLost => {
                self.focused = false;
                self.start_transition_to_current_state();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(Key::Enter) | Event::KeyDown(Key::Space) if self.focused => {
                if let Some(ref cb) = self.on_click {
                    if let Ok(mut f) = cb.lock() {
                        f();
                    }
                }
                if let Some(ref cb) = self.on_click_bounds {
                    if let Ok(mut f) = cb.lock() {
                        f(self.bounds);
                    }
                }
                if let Some(ref cb) = self.on_click_at {
                    let center = Point::new(
                        self.bounds.x() + self.bounds.size.width / 2.0,
                        self.bounds.y() + self.bounds.size.height / 2.0,
                    );
                    if let Ok(mut f) = cb.lock() {
                        f(center);
                    }
                }
                ctx.request_paint();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        self.update_selected_state();
        self.mss.transition.tick(dt.as_secs_f32())
    }

    fn needs_repaint(&self) -> bool {
        self.mss.transition.is_animating()
    }

    fn needs_rebuild(&self) -> bool {
        self.active_index.is_some() && crate::signal::is_element_dirty(self.id)
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        Vec::new()
    }

    fn clear_rebuild(&mut self) {
        crate::signal::clear_element_dirty(self.id);
        if self.update_selected_state() {
            self.mark_dirty(DirtyFlags::RENDER);
        }
        if let Some((signal, _)) = self.active_index {
            crate::signal::begin_tracking(self.id);
            let _ = signal.get();
            crate::signal::end_tracking();
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
        if let Some((signal, _)) = self.active_index {
            crate::signal::begin_tracking(self.id);
            let _ = signal.get();
            crate::signal::end_tracking();
            self.update_selected_state();
        }
    }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
    fn get_classes(&self) -> &[String] {
        &self.classes
    }
    fn element_type_name(&self) -> &str {
        "Button"
    }
    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(f) = style
            .get("font-family")
            .and_then(|v| v.as_string().map(|s| s.to_string()))
        {
            self.mss.font_family = Some(f);
        }
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
        self.update_selected_state();
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

impl StyledElement for ButtonElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER | DirtyFlags::LAYOUT);
    }
    fn classes(&self) -> &[String] {
        &self.classes
    }
    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
