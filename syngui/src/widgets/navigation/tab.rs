use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::{IconState, MssFields};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;
use std::time::Duration;
use crate::signal::RwSignal;

pub type TabState = RwSignal<usize>;

pub struct Tab {
    pub title: String,
    pub icon: Option<String>,
    pub index: usize,
    pub selected_state: TabState,
    pub disabled: bool,
    pub closable: bool,
    pub on_close: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    pub badge: Option<String>,
    pub badge_color: Option<crate::core::Color>,
}

impl Tab {
    pub fn new(title: impl Into<String>, index: usize, state: &TabState) -> Self {
        Self {
            title: title.into(), icon: None, index, selected_state: *state,
            disabled: false, closable: false, on_close: None,
            badge: None, badge_color: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self { self.icon = Some(icon.into()); self }
    pub fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub fn closable(mut self) -> Self { self.closable = true; self }
    pub fn on_close(mut self, callback: impl FnMut(usize) + Send + 'static) -> Self {
        self.on_close = Some(Arc::new(Mutex::new(callback))); self
    }

    pub fn badge(mut self, text: impl Into<String>) -> Self {
        let t = text.into();
        self.badge = if t.is_empty() || t == "0" { None } else { Some(t) };
        self
    }

    pub fn badge_color(mut self, color: crate::core::Color) -> Self {
        self.badge_color = Some(color); self
    }

    fn is_selected(&self) -> bool {
        self.selected_state.get_untracked() == self.index
    }
}

impl Widget for Tab {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(TabElement {
            id: ElementId::new(),
            title: self.title.clone(), icon: self.icon.clone(),
            index: self.index, selected_state: self.selected_state,
            disabled: self.disabled, closable: self.closable, on_close: self.on_close.clone(),
            is_selected: self.is_selected(),
            bounds: Rect::zero(), close_button_bounds: None,
            hover: false, hover_close: false, focused: false,
            classes: Vec::new(), dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            text_measure: None,
            mss: MssFields::new(),
            badge: self.badge.clone(),
            badge_color: self.badge_color,
            mss_indicator_height: None,
            mss_indicator_inset: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct TabElement {
    id: ElementId,
    title: String, icon: Option<String>,
    index: usize, selected_state: TabState,
    disabled: bool, closable: bool,
    on_close: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    is_selected: bool,
    bounds: Rect, close_button_bounds: Option<Rect>,
    hover: bool, hover_close: bool, focused: bool,
    classes: Vec<String>, dirty_flags: DirtyFlags,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
    mss: MssFields,
    badge: Option<String>,
    badge_color: Option<crate::core::Color>,
    mss_indicator_height: Option<f32>,
    mss_indicator_inset: Option<f32>,
}

impl TabElement {
    fn start_transition_to_current_state(&mut self) {
        self.mss.start_transition_to(self.hover, false, false, self.is_selected);
    }
}

impl Element for TabElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(tab) = widget.as_any().downcast_ref::<Tab>() {
            self.title = tab.title.clone(); self.icon = tab.icon.clone();
            self.index = tab.index; self.selected_state = tab.selected_state;
            self.disabled = tab.disabled; self.closable = tab.closable;
            self.on_close = tab.on_close.clone();
            let was_selected = self.is_selected;
            self.is_selected = tab.is_selected();
            if self.is_selected != was_selected {
                self.start_transition_to_current_state();
            }
            let badge_changed = self.badge != tab.badge;
            self.badge = tab.badge.clone();
            self.badge_color = tab.badge_color;
            if badge_changed {
                self.mark_dirty(DirtyFlags::LAYOUT);
            }
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let padding = 16.0;
        let font_size = self.mss.font_size_or(14.0);
        let bold = self.mss.font_weight_or(400) >= 700;
        let icon_width = if self.icon.is_some() { 24.0 } else { 0.0 };
        let close_width = if self.closable { 24.0 } else { 0.0 };
        let text_width = self.text_measure.as_ref()
            .map(|tm| tm.measure_text_width_styled(&self.title, font_size, self.title.chars().count(), bold, self.mss.font_family.as_deref()))
            .unwrap_or(self.title.chars().count() as f32 * 8.0);
        let badge_width = self.badge.as_ref().map(|t| {
            let tw = self.text_measure.as_ref()
                .map(|tm| tm.measure_text_width(t, 11.0, t.chars().count()))
                .unwrap_or(t.chars().count() as f32 * 7.0);
            8.0 + (tw + 14.0).max(18.0)
        }).unwrap_or(0.0);
        let intrinsic = (padding * 2.0 + icon_width + text_width + badge_width + close_width).max(80.0);
        let width = intrinsic
            .max(constraints.min_width)
            .min(constraints.max_width);
        let height = self.mss.height.map(|d| d.resolve(constraints.max_height)).unwrap_or(44.0);
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        if self.closable {
            self.close_button_bounds = Some(Rect::new(
                Point::new(self.bounds.x() + width - 28.0, self.bounds.y() + 10.0),
                Size::new(20.0, 20.0),
            ));
        }
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let gray_100 = self.mss.background_color.unwrap_or(Color::from_hex("#F3F4F6"));
        let gray_200 = self.mss.border_color.map(|c| c.lighten(0.1)).unwrap_or(Color::from_hex("#E5E7EB"));
        let gray_300 = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let gray_500 = self.mss.color.map(|c| c.with_alpha(0.6)).unwrap_or(Color::from_hex("#6B7280"));
        let gray_700 = self.mss.color.unwrap_or(Color::from_hex("#374151"));
        let gray_900 = self.mss.color.map(|c| c.darken(0.3)).unwrap_or(Color::from_hex("#111827"));
        let target = self.mss.target_props(self.hover, false, false, self.is_selected);
        let target_accent = match target.get("accent-color") {
            crate::animation::transition::AnimatedValue::Color(c) => Some(c),
            _ => None,
        };
        let primary = target_accent
            .or(self.mss.accent_color)
            .unwrap_or(Color::from_hex("#3B82F6"));
        let white = Color::WHITE;
        let bg_color = if self.mss.has_mss_styles {
            self.mss.effective_bg(&target, Color::TRANSPARENT)
        } else if self.is_selected {
            white
        } else if self.hover {
            gray_100
        } else {
            Color::TRANSPARENT
        };

        let ref_size = self.bounds.size.width.min(self.bounds.size.height);
        let r = self.mss.border_radius_uniform(ref_size, 8.0);
        list.push_rect(self.bounds, bg_color, [r, r, 0.0, 0.0]);

        let border_width = self.mss.border_width_or(1.0);
        let font_size = self.mss.font_size_or(14.0);
        let font_weight = self.mss.font_weight_or(400);

        if !self.is_selected {
            let bottom_line = Rect::new(
                Point::new(self.bounds.x(), self.bounds.y() + self.bounds.size.height - border_width),
                Size::new(self.bounds.size.width, border_width),
            );
            list.push_rect(bottom_line, gray_300, [0.0; 4]);
        }

        if self.is_selected {
            let h = self.mss_indicator_height.unwrap_or(3.0);
            let inset = self.mss_indicator_inset.unwrap_or(0.0);
            let indicator = Rect::new(
                Point::new(self.bounds.x() + inset, self.bounds.y() + self.bounds.size.height - h),
                Size::new((self.bounds.size.width - inset * 2.0).max(0.0), h),
            );
            list.push_rect(indicator, primary, [0.0; 4]);
        }

        let mut text_x = self.bounds.x() + 12.0;
        if let Some(ref icon) = self.icon {
            let icon_rect = Rect::new(
                Point::new(text_x, self.bounds.y() + 10.0),
                Size::new(20.0, 20.0),
            );
            let icon_state = if self.disabled {
                IconState::Disabled
            } else if self.is_selected {
                IconState::Selected
            } else if self.hover {
                IconState::Hover
            } else {
                IconState::Normal
            };
            let icon_color = self.mss.icon_color(icon_state, gray_700);
            list.push_text_styled(icon, icon_rect, icon_color, font_size,
                crate::mss::TextAlign::DEFAULT, crate::mss::TextDecoration::None,
                font_weight, self.mss.font_family.clone());
            text_x += 24.0;
        }

        let text_color = if self.mss.has_mss_styles {
            self.mss.effective_fg(&target, gray_500)
        } else if self.disabled {
            gray_300
        } else if self.is_selected {
            gray_900
        } else {
            gray_500
        };

        let right_padding = if self.closable { 36.0 } else { 8.0 };
        let badge_bg_width = self.badge.as_ref().map(|t| {
            let tw = self.text_measure.as_ref()
                .map(|tm| tm.measure_text_width(t, 11.0, t.chars().count()))
                .unwrap_or(t.chars().count() as f32 * 7.0);
            (tw + 14.0).max(18.0)
        }).unwrap_or(0.0);
        let badge_reserve = if badge_bg_width > 0.0 { badge_bg_width + 8.0 } else { 0.0 };

        let text_rect = Rect::new(
            Point::new(text_x, self.bounds.y() + 11.0),
            Size::new((self.bounds.size.width - (text_x - self.bounds.x()) - right_padding - badge_reserve).max(0.0), 18.0),
        );
        list.push_text_styled(&self.title, text_rect, text_color, font_size,
            crate::mss::TextAlign::DEFAULT, crate::mss::TextDecoration::None,
            font_weight, self.mss.font_family.clone());

        if let Some(ref badge_text) = self.badge {
            let badge_h = 18.0;
            let badge_y = self.bounds.y() + (self.bounds.size.height - badge_h) / 2.0;
            let badge_x = text_rect.x() + text_rect.size.width + 6.0;
            let badge_rect = Rect::new(
                Point::new(badge_x, badge_y),
                Size::new(badge_bg_width, badge_h),
            );
            let r = badge_h / 2.0;
            let badge_bg = self.badge_color
                .or(self.mss.accent_color)
                .unwrap_or(Color::from_hex("#EF4444"));
            list.push_rect(badge_rect, badge_bg, [r, r, r, r]);
            list.push_text_centered(badge_text, badge_rect, Color::WHITE, 11.0);
        }

        if self.closable {
            if let Some(close_rect) = self.close_button_bounds {
                let close_bg = if self.hover_close { gray_200 } else { Color::TRANSPARENT };
                list.push_rect(close_rect, close_bg, [4.0; 4]);
                let close_color = if self.hover_close { gray_700 } else { gray_500 };
                list.push_text("×", Rect::new(
                    Point::new(close_rect.x() + 5.0, close_rect.y() + 1.0),
                    Size::new(10.0, 16.0),
                ), close_color, 14.0);
            }
        }

    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled { return EventResult::Ignored; }
        match event {
            Event::MouseMove(pos) => {
                let was_hover = self.hover;
                self.hover = self.bounds.contains(*pos);
                if let Some(close_rect) = self.close_button_bounds {
                    self.hover_close = close_rect.contains(*pos);
                }
                if self.hover != was_hover {
                    self.start_transition_to_current_state();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    if let Some(close_rect) = self.close_button_bounds {
                        if close_rect.contains(*position) {
                            if let Some(ref callback) = self.on_close {
                                if let Ok(mut cb) = callback.lock() { cb(self.index); }
                            }
                            return EventResult::Handled;
                        }
                    }
                    let was_selected = self.is_selected;
                    self.selected_state.set(self.index);
                    self.is_selected = true;
                    if !was_selected { self.start_transition_to_current_state(); }
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
                let was_selected = self.is_selected;
                self.selected_state.set(self.index);
                self.is_selected = true;
                if !was_selected { self.start_transition_to_current_state(); }
                ctx.request_paint();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        let new_selected = self.selected_state.get_untracked() == self.index;
        let state_changed = new_selected != self.is_selected;
        if state_changed {
            self.is_selected = new_selected;
            self.start_transition_to_current_state();
            self.mark_dirty(DirtyFlags::RENDER);
        }
        let trans = self.mss.transition.tick(dt.as_secs_f32());
        trans || state_changed
    }

    fn needs_repaint(&self) -> bool {
        self.mss.transition.is_animating()
            || (self.selected_state.get_untracked() == self.index) != self.is_selected
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
        if let Some(ref mut close_rect) = self.close_button_bounds {
            close_rect.origin = Point::new(pos.x + self.bounds.size.width - 28.0, pos.y + 10.0);
        }
    }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, tree: &mut ElementTree) {
        self.text_measure = tree.text_measure.clone();
        self.selected_state.subscribe_element(self.id);
    }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
    fn get_classes(&self) -> &[String] { &self.classes }
    fn element_type_name(&self) -> &str { "Tab" }
    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(d) = style.get("--tab-indicator-height").and_then(|v| v.as_dimension()) {
            self.mss_indicator_height = Some(d.resolve(self.bounds.size.height.max(1.0)));
        }
        if let Some(d) = style.get("--tab-indicator-inset").and_then(|v| v.as_dimension()) {
            self.mss_indicator_inset = Some(d.resolve(self.bounds.size.width.max(1.0)));
        }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn apply_transition_styles(
        &mut self,
        base: &ComputedStyle,
        hover: Option<&ComputedStyle>,
        _active: Option<&ComputedStyle>,
        focus: Option<&ComputedStyle>,
        selected: Option<&ComputedStyle>,
        _checked: Option<&ComputedStyle>,
    ) {
        self.mss.apply_transitions(base, hover, None, None, None);
        self.mss.style_selected = selected.map(crate::animation::transition::ResolvedProps::from_style)
            .or_else(|| focus.map(crate::animation::transition::ResolvedProps::from_style));
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Button,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                selected: self.is_selected,
                focused: self.focused,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                label: Some(self.title.clone()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for TabElement {
    fn apply_style(&mut self, _style: &ComputedStyle) { self.mark_dirty(DirtyFlags::RENDER); }
    fn classes(&self) -> &[String] { &self.classes }
    fn set_classes(&mut self, classes: Vec<String>) { self.classes = classes; self.mark_dirty(DirtyFlags::RENDER); }
}
