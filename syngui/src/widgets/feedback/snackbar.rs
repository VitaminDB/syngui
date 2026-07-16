use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::MssFields;
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widget::context::TextMeasure;
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;
use std::time::Duration;
use crate::signal::RwSignal;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SnackbarPosition {
    #[default]
    BottomCenter,
    BottomLeft,
    BottomRight,
    TopCenter,
}

pub struct Snackbar {
    message: String,
    action_text: Option<String>,
    on_action: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    duration_ms: u32,
    position: SnackbarPosition,
    show: RwSignal<bool>,
}

impl Snackbar {
    pub fn new(message: impl Into<String>, show: RwSignal<bool>) -> Self {
        Self {
            message: message.into(),
            action_text: None,
            on_action: None,
            duration_ms: 4000,
            position: SnackbarPosition::default(),
            show,
        }
    }

    pub fn action(mut self, text: impl Into<String>, f: impl FnMut() + Send + 'static) -> Self {
        self.action_text = Some(text.into());
        self.on_action = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn duration_ms(mut self, ms: u32) -> Self {
        self.duration_ms = ms;
        self
    }

    pub fn position(mut self, pos: SnackbarPosition) -> Self {
        self.position = pos;
        self
    }
}

impl Widget for Snackbar {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(SnackbarElement {
            id: ElementId::new(),
            message: self.message.clone(),
            action_text: self.action_text.clone(),
            on_action: self.on_action.clone(),
            duration_ms: self.duration_ms,
            position: self.position,
            show: self.show,
            elapsed: Duration::ZERO,
            opacity: 0.0,
            action_hovered: false,
            viewport_size: Size::zero(),
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

const SNACKBAR_HEIGHT: f32 = 48.0;
const SNACKBAR_MAX_WIDTH: f32 = 560.0;
const FADE_DURATION: f32 = 0.2;

pub struct SnackbarElement {
    id: ElementId,
    message: String,
    action_text: Option<String>,
    on_action: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    duration_ms: u32,
    position: SnackbarPosition,
    show: RwSignal<bool>,
    elapsed: Duration,
    opacity: f32,
    action_hovered: bool,
    viewport_size: Size,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<Arc<dyn TextMeasure>>,
}

impl SnackbarElement {
    fn is_visible(&self) -> bool {
        self.show.get_untracked()
    }

    fn measure_text(&self, text: &str, font_size: f32) -> f32 {
        self.text_measure.as_ref()
            .map(|tm| tm.measure_text_width(text, font_size, text.chars().count()))
            .unwrap_or_else(|| text.chars().count() as f32 * font_size * 0.6)
    }

    fn snackbar_rect(&self) -> Rect {
        let font_size = self.mss.font_size_or(14.0);
        let text_w = self.measure_text(&self.message, font_size);
        let action_w = self.action_text.as_ref().map(|t| self.measure_text(t, font_size) + 32.0).unwrap_or(0.0);
        let total_w = (text_w + action_w + 48.0).min(SNACKBAR_MAX_WIDTH);

        let vw = self.viewport_size.width;
        let vh = self.viewport_size.height;
        let margin = 24.0;

        let (x, y) = match self.position {
            SnackbarPosition::BottomCenter => ((vw - total_w) / 2.0, vh - SNACKBAR_HEIGHT - margin),
            SnackbarPosition::BottomLeft => (margin, vh - SNACKBAR_HEIGHT - margin),
            SnackbarPosition::BottomRight => (vw - total_w - margin, vh - SNACKBAR_HEIGHT - margin),
            SnackbarPosition::TopCenter => ((vw - total_w) / 2.0, margin),
        };

        Rect::new(Point::new(x, y), Size::new(total_w, SNACKBAR_HEIGHT))
    }
}

impl Element for SnackbarElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(sb) = widget.as_any().downcast_ref::<Snackbar>() {
            self.message = sb.message.clone();
            self.action_text = sb.action_text.clone();
            self.on_action = sb.on_action.clone();
            self.duration_ms = sb.duration_ms;
            self.position = sb.position;
            self.show = sb.show;
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        self.viewport_size = Size::new(
            if constraints.max_width.is_finite() { constraints.max_width } else { 800.0 },
            if constraints.max_height.is_finite() { constraints.max_height } else { 600.0 },
        );
        self.bounds = Rect::new(Point::zero(), Size::zero());
        Size::zero()
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        if !self.is_visible() || self.opacity <= 0.01 { return; }

        let rect = self.snackbar_rect();
        let bg = self.mss.background_color.unwrap_or(Color::from_hex("#323232")).with_alpha(self.opacity);
        let text_color = self.mss.color.unwrap_or(Color::WHITE).with_alpha(self.opacity);
        let accent = self.mss.accent_color.unwrap_or(Color::from_hex("#64B5F6"));
        let font_size = self.mss.font_size_or(14.0);
        let font_weight = self.mss.font_weight_or(400);
        let radius = self.mss.border_radius_uniform(rect.size.width.min(rect.size.height), 8.0);

        list.begin_overlay();

        list.push_shadow(rect, Color::BLACK.with_alpha(0.2 * self.opacity), 12.0, (0.0, 4.0), [radius; 4]);
        list.push_rect(rect, bg, [radius; 4]);

        let text_rect = Rect::new(
            Point::new(rect.x() + 16.0, rect.y() + (SNACKBAR_HEIGHT - font_size) / 2.0),
            Size::new(rect.size.width - 32.0, font_size + 2.0),
        );
        list.push_text_styled(
            &self.message, text_rect, text_color, font_size,
            crate::mss::TextAlign::DEFAULT, crate::mss::TextDecoration::None,
            font_weight, self.mss.font_family.clone(),
        );

        if let Some(ref action) = self.action_text {
            let action_w = self.measure_text(action, font_size) + 16.0;
            let action_rect = Rect::new(
                Point::new(rect.x() + rect.size.width - action_w - 8.0, rect.y() + (SNACKBAR_HEIGHT - font_size) / 2.0),
                Size::new(action_w, font_size + 2.0),
            );
            let action_color = if self.action_hovered {
                accent.lighten(0.15).with_alpha(self.opacity)
            } else {
                accent.with_alpha(self.opacity)
            };
            list.push_text_styled(
                action, action_rect, action_color, font_size,
                crate::mss::TextAlign::DEFAULT, crate::mss::TextDecoration::None,
                font_weight, self.mss.font_family.clone(),
            );
        }

        list.end_overlay();
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let is_showing = self.is_visible();

        if is_showing {
            self.elapsed += dt;
            if self.opacity < 1.0 {
                self.opacity = (self.opacity + dt.as_secs_f32() / FADE_DURATION).min(1.0);
            }
            if self.duration_ms > 0 && self.elapsed >= Duration::from_millis(self.duration_ms as u64) {
                self.show.set(false);
            }
            return true;
        } else {
            if self.opacity > 0.0 {
                self.opacity = (self.opacity - dt.as_secs_f32() / FADE_DURATION).max(0.0);
                if self.opacity <= 0.01 {
                    self.elapsed = Duration::ZERO;
                }
                return true;
            }
        }
        false
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if !self.is_visible() { return EventResult::Ignored; }

        let rect = self.snackbar_rect();
        match event {
            Event::MouseMove(pos) => {
                if rect.contains(*pos) {
                    if self.action_text.is_some() {
                        let font_size = self.mss.font_size_or(14.0);
                        let action_w = self.action_text.as_ref().map(|t| self.measure_text(t, font_size) + 16.0).unwrap_or(0.0);
                        let action_x = rect.x() + rect.size.width - action_w - 8.0;
                        let hovering_action = pos.x >= action_x;
                        if hovering_action != self.action_hovered {
                            self.action_hovered = hovering_action;
                            if hovering_action { ctx.set_cursor(CursorIcon::Pointer); }
                            ctx.request_paint();
                        }
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if rect.contains(*position) {
                    if let Some(ref action) = self.action_text {
                        let font_size = self.mss.font_size_or(14.0);
                        let action_w = self.measure_text(action, font_size) + 16.0;
                        let action_x = rect.x() + rect.size.width - action_w - 8.0;
                        if position.x >= action_x {
                            if let Some(ref cb) = self.on_action {
                                if let Ok(mut f) = cb.lock() { f(); }
                            }
                            self.show.set(false);
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
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

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "Snackbar" }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::StaticText,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(self.message.clone()),
                live_region: Some(crate::a11y::LiveRegion::Polite),
                ..Default::default()
            },
        })
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }
}

impl StyledElement for SnackbarElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
