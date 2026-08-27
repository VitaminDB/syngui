use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;
use std::time::Duration;

pub struct ToolButton {
    pub icon: String, pub tooltip: Option<String>, pub text: Option<String>,
    pub disabled: bool, pub active: bool,
    /// Кнопка не заявляет нажатия: MouseDown/MouseUp проходят к предку
    /// (например, `Draggable::on_click`), hover и tooltip остаются свои.
    pub press_passthrough: bool,
    pub on_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    pub on_click_at: Option<Arc<Mutex<dyn FnMut(Point) + Send>>>,
    pub on_click_with_bounds: Option<Arc<Mutex<dyn FnMut(Point, Rect) + Send>>>,
}

const DEFAULT_ICON_SIZE: f32 = 18.0;

impl ToolButton {
    pub fn new(icon: impl Into<String>) -> Self {
        Self {
            icon: icon.into(), tooltip: None, text: None,
            disabled: false, active: false, press_passthrough: false,
            on_click: None, on_click_at: None, on_click_with_bounds: None,
        }
    }
    pub fn tooltip(mut self, t: impl Into<String>) -> Self { self.tooltip = Some(t.into()); self }
    /// Визуальная кнопка без собственного нажатия.
    ///
    /// События идут от самого глубокого элемента под курсором к корню, и
    /// обычная кнопка «заклеймивает» MouseDown — предок вроде `Draggable`
    /// его не видит, а с ним теряет и клик, и старт перетаскивания. В этом
    /// режиме кнопка отдаёт нажатия наверх, сохраняя hover-подсветку и
    /// tooltip; свои `on_click*` при этом не вызываются.
    pub fn press_passthrough(mut self) -> Self { self.press_passthrough = true; self }
    pub fn text(mut self, t: impl Into<String>) -> Self { self.text = Some(t.into()); self }
    pub fn disabled(mut self, d: bool) -> Self { self.disabled = d; self }
    pub fn active(mut self, a: bool) -> Self { self.active = a; self }
    pub fn on_click(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_click = Some(Arc::new(Mutex::new(callback)));
        self
    }
    pub fn on_click_at(mut self, callback: impl FnMut(Point) + Send + 'static) -> Self {
        self.on_click_at = Some(Arc::new(Mutex::new(callback)));
        self
    }
    pub fn on_click_with_bounds(mut self, callback: impl FnMut(Point, Rect) + Send + 'static) -> Self {
        self.on_click_with_bounds = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Widget for ToolButton {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ToolButtonElement {
            id: ElementId::new(), icon: self.icon.clone(), tooltip: self.tooltip.clone(),
            text: self.text.clone(), disabled: self.disabled, active: self.active,
            press_passthrough: self.press_passthrough,
            on_click: self.on_click.clone(),
            on_click_at: self.on_click_at.clone(),
            on_click_with_bounds: self.on_click_with_bounds.clone(),
            bounds: Rect::zero(), hover: false, pressed: false, focused: false, classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
            tip_elapsed: Duration::ZERO,
            tip_visible: false,
            tip_suppressed: false,
        })
    }
    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct ToolButtonElement {
    id: ElementId, icon: String, tooltip: Option<String>, text: Option<String>,
    disabled: bool, active: bool, press_passthrough: bool,
    on_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    on_click_at: Option<Arc<Mutex<dyn FnMut(Point) + Send>>>,
    on_click_with_bounds: Option<Arc<Mutex<dyn FnMut(Point, Rect) + Send>>>,
    bounds: Rect, hover: bool, pressed: bool, focused: bool,
    classes: Vec<String>, dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
    /// Сколько курсор уже стоит над кнопкой (для задержки подсказки).
    tip_elapsed: Duration,
    /// Подсказка показана.
    tip_visible: bool,
    /// Подсказку погасило нажатие — до ухода курсора не показывать снова.
    tip_suppressed: bool,
}

/// Задержка появления подсказки — как у виджета `Tooltip`.
const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
/// Потолок ширины подсказки, px.
const TOOLTIP_MAX_WIDTH: f32 = 320.0;

impl ToolButtonElement {
    fn start_transition_to_current_state(&mut self) {
        self.mss.start_transition_to(self.hover, self.pressed, false, false);
    }

    fn hide_tip(&mut self) {
        self.tip_visible = false;
        self.tip_elapsed = Duration::ZERO;
    }

    /// Ждём ли задержку показа подсказки (нужны кадры анимации).
    fn tip_pending(&self) -> bool {
        self.tooltip.is_some() && self.hover && !self.tip_visible && !self.tip_suppressed
    }

    /// Геометрия подсказки: под кнопкой по центру; если не влезает в `clip`
    /// снизу — над кнопкой; по горизонтали прижимается к краям `clip`.
    fn tooltip_rect(&self, text: &str, clip: Rect) -> Rect {
        let font_size = 12.0_f32;
        let line_height = font_size + 4.0;
        let (pad_h, pad_v) = (16.0_f32, 12.0_f32);
        let lines: Vec<&str> = text.lines().collect();
        let max_line = lines
            .iter()
            .map(|line| {
                self.text_measure
                    .as_ref()
                    .map(|tm| tm.measure_text_width(line, font_size, line.chars().count()))
                    .unwrap_or_else(|| line.chars().count() as f32 * font_size * 0.6)
            })
            .fold(0.0f32, f32::max)
            .min(TOOLTIP_MAX_WIDTH);
        let w = max_line + pad_h;
        let h = lines.len().max(1) as f32 * line_height + pad_v;
        let gap = 6.0;
        let mut x = self.bounds.x() + (self.bounds.size.width - w) / 2.0;
        let mut y = self.bounds.y() + self.bounds.size.height + gap;
        if clip.size.height.is_finite() && y + h > clip.y() + clip.size.height {
            y = self.bounds.y() - h - gap;
        }
        if clip.size.width.is_finite() && clip.size.width > w {
            x = x.clamp(clip.x(), clip.x() + clip.size.width - w);
        }
        Rect::new(Point::new(x, y), Size::new(w, h))
    }
}

impl Element for ToolButtonElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(btn) = widget.as_any().downcast_ref::<ToolButton>() {
            self.icon = btn.icon.clone(); self.tooltip = btn.tooltip.clone(); self.text = btn.text.clone();
            self.disabled = btn.disabled; self.active = btn.active;
            self.press_passthrough = btn.press_passthrough;
            self.on_click = btn.on_click.clone();
            self.on_click_at = btn.on_click_at.clone();
            self.on_click_with_bounds = btn.on_click_with_bounds.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let gap = 6.0_f32;
        let [pad_l, pad_t, pad_r, pad_b] = self.mss.padding_ltrb([4.0; 4]);
        let font_size = self.mss.font_size_or(12.0);
        let bold = self.mss.font_weight_or(400) >= 700;
        let icon_size = self.mss.icon_size.unwrap_or(DEFAULT_ICON_SIZE);

        let content_w = if let Some(ref text) = self.text {
            let text_width = self.text_measure.as_ref()
                .map(|tm| tm.measure_text_width_styled(text, font_size, text.chars().count(), bold, self.mss.font_family.as_deref()))
                .unwrap_or(text.chars().count() as f32 * font_size * 0.6);
            icon_size + gap + text_width
        } else {
            icon_size
        };
        let intrinsic_w = content_w + pad_l + pad_r;
        let intrinsic_h = icon_size + pad_t + pad_b;

        let max_w = constraints.max_width;
        let max_h = constraints.max_height;
        let mss_w = self.mss.width.map(|d| d.resolve(max_w));
        let mss_h = self.mss.height.map(|d| d.resolve(max_h));
        let min_w = self.mss.min_width.map(|d| d.resolve(max_w)).unwrap_or(0.0);
        let max_w_mss = self.mss.max_width.map(|d| d.resolve(max_w)).unwrap_or(max_w);
        let min_h = self.mss.min_height.map(|d| d.resolve(max_h)).unwrap_or(0.0);
        let max_h_mss = self.mss.max_height.map(|d| d.resolve(max_h)).unwrap_or(max_h);

        let width = mss_w.unwrap_or(intrinsic_w).clamp(min_w, max_w_mss).min(max_w);
        let height = mss_h.unwrap_or(intrinsic_h).clamp(min_h, max_h_mss).min(max_h);

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let default_bg = self.mss.background_color.unwrap_or(Color::from_hex("#F3F4F6"));
        let fg = self.mss.color.unwrap_or(Color::from_hex("#374151"));
        let accent = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        let (bg, icon_color) = if self.mss.has_mss_styles {
            let target = self.mss.target_props(self.hover, self.pressed, false, false);
            let bg = self.mss.effective_bg(&target, Color::TRANSPARENT);
            let ic = self.mss.effective_fg(&target, fg);
            (bg, ic)
        } else {
            let bg = if self.disabled { Color::TRANSPARENT }
                else if self.pressed { default_bg.darken(0.05) } else if self.hover || self.active { default_bg } else { Color::TRANSPARENT };
            let ic = if self.disabled { fg.with_alpha(0.4) } else if self.active { accent } else { fg };
            (bg, ic)
        };

        let cr = self.mss.border_radius_resolved(
            self.bounds.size.width.min(self.bounds.size.height),
            6.0,
        );
        let border_width = self.mss.border_width.unwrap_or(0.0);
        let border = if border_width > 0.0 {
            self.mss.border_color.map(|c| crate::Border {
                width: border_width,
                color: c,
            })
        } else {
            None
        };

        if let Some(ref shadows) = self.mss.box_shadow {
            for shadow in &shadows.0 {
                if !shadow.inset {
                    list.push_shadow(
                        self.bounds, shadow.color, shadow.blur_radius,
                        (shadow.offset_x, shadow.offset_y), cr,
                    );
                }
            }
        }

        match border {
            Some(b) => list.push_rect_bordered(self.bounds, bg, cr, b),
            None if bg.a > 0.0 => list.push_rect(self.bounds, bg, cr),
            None => {}
        }

        let gap = 6.0_f32;
        let [pad_l, pad_t, pad_r, pad_b] = self.mss.padding_ltrb([4.0; 4]);
        let icon_size = self.mss.icon_size.unwrap_or(DEFAULT_ICON_SIZE);
        let inner_h = self.bounds.size.height - pad_t - pad_b;
        let icon_y = self.bounds.y() + pad_t + (inner_h - icon_size) / 2.0;

        if self.text.is_none() {
            let inner_w = self.bounds.size.width - pad_l - pad_r;
            let icon_rect = Rect::new(
                Point::new(
                    self.bounds.x() + pad_l + (inner_w - icon_size) / 2.0,
                    icon_y,
                ),
                Size::new(icon_size, icon_size),
            );
            list.push_text_centered(&self.icon, icon_rect, icon_color, icon_size);
        } else {
            let icon_rect = Rect::new(
                Point::new(self.bounds.x() + pad_l, icon_y),
                Size::new(icon_size, icon_size),
            );
            list.push_text_centered(&self.icon, icon_rect, icon_color, icon_size);

            if let Some(ref text) = self.text {
                let font_size = self.mss.font_size_or(12.0);
                let font_weight = self.mss.font_weight_or(400);
                let text_x = icon_rect.x() + icon_size + gap;
                let text_w = (self.bounds.x() + self.bounds.size.width - pad_r - text_x).max(0.0);
                let text_rect = Rect::new(
                    Point::new(text_x, self.bounds.y()),
                    Size::new(text_w, self.bounds.size.height),
                );
                list.push_text_styled(
                    text, text_rect, icon_color, font_size,
                    crate::mss::TextAlign::DEFAULT, crate::mss::TextDecoration::None,
                    font_weight, self.mss.font_family.clone(),
                );
            }
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled { return EventResult::Ignored; }
        match event {
            Event::MouseMove(pos) => {
                let was = self.hover; self.hover = self.bounds.contains(*pos);
                if self.hover { ctx.set_cursor(CursorIcon::Pointer); }
                if self.hover != was {
                    // Подсказка: таймер стартует при входе, гаснет при уходе.
                    self.hide_tip();
                    if !self.hover { self.tip_suppressed = false; }
                    self.start_transition_to_current_state();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hover { return EventResult::Handled; }
                EventResult::Ignored
            }
            Event::MouseDown { button, position }
                if *button == MouseButton::Left
                    && !self.press_passthrough
                    && self.bounds.contains(*position) =>
            {
                self.pressed = true;
                // Нажали — подсказка больше не нужна до следующего наведения.
                self.hide_tip();
                self.tip_suppressed = true;
                self.start_transition_to_current_state();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::MouseUp { button, position } if *button == MouseButton::Left && self.pressed => {
                self.pressed = false;
                self.start_transition_to_current_state();
                if self.bounds.contains(*position) {
                    if let Some(ref cb) = self.on_click {
                        if let Ok(mut f) = cb.lock() { f(); }
                    }
                    if let Some(ref cb) = self.on_click_at {
                        if let Ok(mut f) = cb.lock() { f(*position); }
                    }
                    if let Some(ref cb) = self.on_click_with_bounds {
                        if let Ok(mut f) = cb.lock() { f(*position, self.bounds); }
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
            Event::KeyDown(Key::Enter) | Event::KeyDown(Key::Space) if self.focused => {
                self.active = !self.active;
                if let Some(ref cb) = self.on_click {
                    if let Ok(mut f) = cb.lock() { f(); }
                }
                let center = Point::new(
                    self.bounds.x() + self.bounds.size.width / 2.0,
                    self.bounds.y() + self.bounds.size.height / 2.0,
                );
                if let Some(ref cb) = self.on_click_at {
                    if let Ok(mut f) = cb.lock() { f(center); }
                }
                if let Some(ref cb) = self.on_click_with_bounds {
                    if let Ok(mut f) = cb.lock() { f(center, self.bounds); }
                }
                self.start_transition_to_current_state();
                ctx.request_paint();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let mut animating = self.mss.transition.tick(dt.as_secs_f32());
        if self.tip_pending() {
            self.tip_elapsed += dt;
            if self.tip_elapsed >= TOOLTIP_DELAY {
                self.tip_visible = true;
                self.mark_dirty(DirtyFlags::RENDER);
            }
            animating = true;
        }
        animating
    }

    fn needs_repaint(&self) -> bool {
        self.mss.transition.is_animating() || self.tip_pending()
    }

    /// Подсказка рисуется overlay-слоем поверх всего, как у виджета
    /// `Tooltip` в текстовом режиме: раньше `tooltip` шёл только в
    /// accessibility-label и визуально не показывался.
    fn post_build_display_list(&self, list: &mut DisplayList, clip: Rect) {
        if !self.tip_visible { return; }
        let Some(text) = self.tooltip.as_deref() else { return; };
        if text.trim().is_empty() { return; }
        let tip = self.tooltip_rect(text, clip);
        let bg = Color::from_hex("#1E1F22");
        let border = Color::from_hex("#3F4147");
        let fg = Color::WHITE;
        let font_size = 12.0_f32;
        let line_height = font_size + 4.0;
        let radius = tip.size.height.min(tip.size.width).min(20.0) / 2.0;
        let radius = radius.min(8.0);
        list.begin_overlay();
        list.push_shadow(tip, Color::from_hex("#000000").with_alpha(0.35), 12.0, (0.0, 4.0), [radius; 4]);
        list.push_rect_bordered(tip, bg, [radius; 4], crate::Border::new(1.0, border));
        for (i, line) in text.lines().enumerate() {
            let text_rect = Rect::new(
                Point::new(tip.x() + 8.0, tip.y() + 6.0 + i as f32 * line_height),
                Size::new(tip.size.width - 16.0, line_height),
            );
            list.push_text_styled(
                line, text_rect, fg, font_size,
                crate::mss::TextAlign::DEFAULT, crate::mss::TextDecoration::None,
                400, self.mss.font_family.clone(),
            );
        }
        list.end_overlay();
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
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
    fn element_type_name(&self) -> &str { "ToolButton" }
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
        let label = self.tooltip.clone()
            .or_else(|| self.text.clone())
            .unwrap_or_else(|| self.icon.clone());
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Button,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                pressed: self.pressed,
                focused: self.focused,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: Some(label),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for ToolButtonElement {
    fn apply_style(&mut self, _style: &ComputedStyle) { self.mark_dirty(DirtyFlags::RENDER); }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
}
