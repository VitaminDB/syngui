use crate::animation::transition::mss_color_to_core;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension, MssFields, TextAlign, TextDecoration};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

#[derive(Clone, Debug)]
pub struct DropdownItem {
    pub value: String,
    pub label: String,
    pub icon: Option<String>,
    pub disabled: bool,
}

impl DropdownItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            icon: None,
            disabled: false,
        }
    }

    pub fn simple(text: impl Into<String>) -> Self {
        let text = text.into();
        Self::new(text.clone(), text)
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

pub type DropdownState = Arc<Mutex<Option<String>>>;

pub struct Dropdown {
    pub items: Vec<DropdownItem>,
    pub selected: DropdownState,
    pub placeholder: String,
    pub disabled: bool,
    pub width: Option<Dimension>,
    pub max_height: f32,
    pub on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    pub leading_icon: Option<String>,
}

impl Dropdown {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: Arc::new(Mutex::new(None)),
            placeholder: String::new(),
            disabled: false,
            width: None,
            max_height: 200.0,
            on_change: None,
            leading_icon: None,
        }
    }

    pub fn with_items(items: Vec<DropdownItem>) -> Self {
        Self::new().items(items)
    }

    pub fn item(mut self, item: DropdownItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: Vec<DropdownItem>) -> Self {
        self.items = items;
        self
    }

    pub fn selected(self, value: impl Into<String>) -> Self {
        *self.selected.lock().unwrap() = Some(value.into());
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
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

    pub fn max_height(mut self, max_height: f32) -> Self {
        self.max_height = max_height;
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn leading_icon(mut self, icon: impl Into<String>) -> Self {
        self.leading_icon = Some(icon.into());
        self
    }

}

impl Default for Dropdown {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Dropdown {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(DropdownElement {
            id: ElementId::new(),
            items: self.items.clone(),
            selected: self.selected.clone(),
            placeholder: self.placeholder.clone(),
            disabled: self.disabled,
            width: self.width,
            max_height: self.max_height,
            on_change: self.on_change.clone(),
            leading_icon: self.leading_icon.clone(),
            bounds: Rect::zero(),
            dropdown_bounds: Rect::zero(),
            item_bounds: Vec::new(),
            is_open: false,
            focused: false,
            hover_button: false,
            hover_item: None,
            scroll_offset: 0.0,
            opens_upward: false,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::default(),
            mss_popup_bg: None,
            mss_popup_fg: None,
            mss_popup_accent: None,
            mss_popup_border: None,
            mss_popup_hover_bg: None,
            mss_popup_hover_fg: None,
            mss_popup_selected_bg: None,
            mss_popup_selected_fg: None,
            mss_popup_max_height: None,
            mss_popup_min_height: None,
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

pub struct DropdownElement {
    id: ElementId,
    items: Vec<DropdownItem>,
    selected: DropdownState,
    placeholder: String,
    disabled: bool,
    width: Option<Dimension>,
    max_height: f32,
    on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    leading_icon: Option<String>,
    bounds: Rect,
    dropdown_bounds: Rect,
    item_bounds: Vec<(Rect, usize)>,
    is_open: bool,
    focused: bool,
    hover_button: bool,
    hover_item: Option<usize>,
    scroll_offset: f32,
    opens_upward: bool,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    mss_popup_bg: Option<Color>,
    mss_popup_fg: Option<Color>,
    mss_popup_accent: Option<Color>,
    mss_popup_border: Option<Color>,
    mss_popup_hover_bg: Option<Color>,
    mss_popup_hover_fg: Option<Color>,
    mss_popup_selected_bg: Option<Color>,
    mss_popup_selected_fg: Option<Color>,
    mss_popup_max_height: Option<f32>,
    mss_popup_min_height: Option<f32>,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
}

impl DropdownElement {
    fn get_selected_label(&self) -> Option<String> {
        let selected_value = self.selected.lock().unwrap();
        if let Some(ref value) = *selected_value {
            for item in &self.items {
                if &item.value == value {
                    return Some(item.label.clone());
                }
            }
        }
        None
    }

    fn effective_popup_height(&self) -> f32 {
        let content_height = self.items.len() as f32 * 36.0;
        let max_h = self.mss_popup_max_height.unwrap_or(self.max_height);
        let min_h = self.mss_popup_min_height.unwrap_or(0.0);
        content_height.min(max_h).max(min_h)
    }

    fn max_scroll(&self) -> f32 {
        let total_height = self.items.len() as f32 * 36.0;
        (total_height - self.effective_popup_height()).max(0.0)
    }

    fn compute_overlay_bounds(&self) -> Rect {
        let dd_h = self.dropdown_bounds.size.height;
        let popup_gap = 4.0;
        if self.opens_upward {
            Rect::new(
                Point::new(self.bounds.x(), self.bounds.y() - dd_h - popup_gap),
                Size::new(self.bounds.size.width, dd_h + popup_gap + self.bounds.size.height),
            )
        } else {
            Rect::new(
                Point::new(self.bounds.x(), self.bounds.y()),
                Size::new(self.bounds.size.width, self.bounds.size.height + popup_gap + dd_h),
            )
        }
    }

    fn determine_direction(&mut self, viewport_height: f32) {
        let dd_h = self.dropdown_bounds.size.height;
        let popup_gap = 4.0;
        self.opens_upward = self.bounds.y() + self.bounds.size.height + dd_h + popup_gap > viewport_height
            && self.bounds.y() >= dd_h + popup_gap;
    }

    fn recompute_dropdown_position(&mut self) {
        let width = self.bounds.size.width;
        let height = self.bounds.size.height;
        let dropdown_h = self.dropdown_bounds.size.height;
        let popup_gap = 4.0;

        if self.opens_upward {
            self.dropdown_bounds.origin = Point::new(self.bounds.x(), self.bounds.y() - dropdown_h - popup_gap);
        } else {
            self.dropdown_bounds.origin = Point::new(self.bounds.x(), self.bounds.y() + height + popup_gap);
        }

        let base_y = if self.opens_upward { -dropdown_h - popup_gap } else { height + popup_gap };
        self.item_bounds.clear();
        let mut y = base_y;
        for (i, _) in self.items.iter().enumerate() {
            let item_rect = Rect::new(
                Point::new(0.0, y),
                Size::new(width, 36.0),
            );
            self.item_bounds.push((item_rect, i));
            y += 36.0;
        }
    }
}

impl DropdownElement {
    fn placeholder_text(&self) -> String {
        if self.placeholder.is_empty() {
            crate::i18n::builtin("dropdown.placeholder", "Select...")
        } else {
            self.placeholder.clone()
        }
    }
}

impl Element for DropdownElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(dropdown) = widget.as_any().downcast_ref::<Dropdown>() {
            self.items = dropdown.items.clone();
            self.selected = dropdown.selected.clone();
            self.placeholder = dropdown.placeholder.clone();
            self.disabled = dropdown.disabled;
            self.width = dropdown.width;
            self.max_height = dropdown.max_height;
            self.on_change = dropdown.on_change.clone();
            self.leading_icon = dropdown.leading_icon.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let max_w = self.mss.max_width.map(|d| d.resolve(constraints.max_width)).unwrap_or(constraints.max_width);
        let mss_width = self.mss.width.or(self.width);
        let intrinsic = if mss_width.is_none() {
            let font_size = self.mss.font_size_or(14.0);
            let max_text_w = if let Some(ref tm) = self.text_measure {
                let placeholder = self.placeholder_text();
                let mut max_w = tm.measure_text_width(&placeholder, font_size, placeholder.chars().count());
                for item in &self.items {
                    let w = tm.measure_text_width(&item.label, font_size, item.label.chars().count());
                    if w > max_w { max_w = w; }
                }
                max_w
            } else {
                100.0
            };
            let icon_w = if self.leading_icon.is_some() { font_size + 8.0 } else { 0.0 };
            max_text_w + 12.0 + icon_w + 44.0
        } else {
            constraints.max_width
        };
        let width = mss_width.map(|d| d.resolve(constraints.max_width)).unwrap_or(intrinsic).min(max_w).min(constraints.max_width);
        let height = self.mss.height.map(|d| d.resolve(constraints.max_height)).unwrap_or(40.0);

        self.bounds = Rect::new(Point::zero(), Size::new(width, height));

        let dropdown_height = self.effective_popup_height();
        let popup_gap = 4.0;
        let base_y = if self.opens_upward { -dropdown_height - popup_gap } else { height + popup_gap };
        self.dropdown_bounds = Rect::new(
            Point::new(0.0, base_y),
            Size::new(width, dropdown_height),
        );

        self.item_bounds.clear();
        let mut y = base_y;
        for (i, _item) in self.items.iter().enumerate() {
            let item_rect = Rect::new(
                Point::new(0.0, y),
                Size::new(width, 36.0),
            );
            self.item_bounds.push((item_rect, i));
            y += 36.0;
        }

        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {

        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg = self.mss.color.unwrap_or(Color::from_hex("#111827"));
        let border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let muted = fg.with_alpha(0.5);
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));
        let disabled_bg = bg.darken(0.05);
        let font_size = self.mss.font_size_or(14.0);
        let font_weight = self.mss.font_weight_or(400);
        let resolve_base = self.bounds.size.width.min(self.bounds.size.height);
        let radius = self.mss.border_radius_uniform(resolve_base, 6.0);
        let bw_normal = self.mss.border_width_or(1.0);
        let bw_active = bw_normal.max(2.0);

        let (bg_color, border_color) = if self.disabled {
            (disabled_bg, border)
        } else if self.is_open || self.hover_button {
            (bg, primary)
        } else {
            (bg, border)
        };

        list.push_rect_bordered(
            self.bounds,
            bg_color,
            [radius; 4],
            Border { width: if self.is_open || self.hover_button { bw_active } else { bw_normal }, color: border_color },
        );

        let mut text_left = self.bounds.x() + 12.0;
        if let Some(ref icon) = self.leading_icon {
            let icon_size = font_size + 2.0;
            let icon_rect = Rect::new(
                Point::new(text_left, self.bounds.y()),
                Size::new(icon_size + 4.0, self.bounds.size.height),
            );
            let icon_color = if self.disabled { muted } else { fg.with_alpha(0.6) };
            list.push_text_centered(icon, icon_rect, icon_color, icon_size);
            text_left += icon_size + 6.0;
        }

        let text = self.get_selected_label().unwrap_or_else(|| self.placeholder_text());
        let text_color = if self.disabled {
            muted
        } else if self.get_selected_label().is_none() {
            muted
        } else {
            fg
        };

        let text_rect = Rect::new(
            Point::new(text_left, self.bounds.y()),
            Size::new(self.bounds.x() + self.bounds.size.width - 32.0 - text_left, self.bounds.size.height),
        );
        // Одной строкой: узкий Dropdown не должен ломать подпись на две строки —
        // лишнее обрезается краем контрола.
        list.push_text_styled_singleline(&text, text_rect, text_color, font_size,
            TextAlign::DEFAULT, TextDecoration::None, font_weight, self.mss.font_family.clone());

        let arrow = if self.is_open { "\u{E5CE}" } else { "\u{E5CF}" };
        let arrow_rect = Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - 32.0, self.bounds.y()),
            Size::new(24.0, self.bounds.size.height),
        );
        list.push_text_centered(arrow, arrow_rect, muted, 18.0);

        if self.is_open {
            let popup_bg = self.mss_popup_bg.unwrap_or(bg);
            let popup_fg = self.mss_popup_fg.unwrap_or(fg);
            let popup_accent = self.mss_popup_accent.unwrap_or(primary);
            let popup_border = self.mss_popup_border.unwrap_or(border);
            let popup_muted = popup_fg.with_alpha(0.5);
            let popup_hover_bg = self.mss_popup_hover_bg.unwrap_or_else(|| popup_bg.darken(0.06));
            let popup_selected_bg = self.mss_popup_selected_bg
                .unwrap_or_else(|| popup_accent.with_alpha(0.1));
            let popup_disabled_bg = popup_bg.darken(0.05);

            list.begin_overlay();

            let menu_radius = radius;
            list.push_shadow(
                self.dropdown_bounds,
                Color::new(0.0, 0.0, 0.0, 0.15),
                12.0,
                (0.0, 4.0),
                [menu_radius; 4],
            );

            let menu_bw = bw_normal;
            list.push_rect_bordered(
                self.dropdown_bounds,
                popup_bg,
                [menu_radius; 4],
                Border { width: menu_bw, color: popup_border },
            );

            let inset = Rect::new(
                Point::new(self.dropdown_bounds.x() + menu_bw, self.dropdown_bounds.y() + menu_bw),
                Size::new(
                    (self.dropdown_bounds.size.width - menu_bw * 2.0).max(0.0),
                    (self.dropdown_bounds.size.height - menu_bw * 2.0).max(0.0),
                ),
            );
            list.push_clip(inset);

            let inner_radius = (menu_radius - menu_bw).max(0.0);
            let dd_top = self.dropdown_bounds.y();
            let dd_bottom = dd_top + self.dropdown_bounds.size.height;

            let mut first_visible: Option<usize> = None;
            let mut last_visible: Option<usize> = None;
            for (item_rect, idx) in &self.item_bounds {
                let iy = self.bounds.y() + item_rect.y() - self.scroll_offset;
                if iy + item_rect.size.height <= dd_top { continue; }
                if iy >= dd_bottom { break; }
                if first_visible.is_none() { first_visible = Some(*idx); }
                last_visible = Some(*idx);
            }

            for (item_rect, idx) in &self.item_bounds {
                let adjusted_rect = Rect::new(
                    Point::new(self.dropdown_bounds.x() + menu_bw, self.bounds.y() + item_rect.y() - self.scroll_offset),
                    Size::new(inset.size.width, item_rect.size.height),
                );

                if adjusted_rect.y() + adjusted_rect.size.height <= dd_top {
                    continue;
                }
                if adjusted_rect.y() >= dd_bottom {
                    break;
                }

                let item = &self.items[*idx];
                let is_selected = *self.selected.lock().unwrap() == Some(item.value.clone());
                let is_hover = self.hover_item == Some(*idx);

                let item_bg = if item.disabled {
                    popup_disabled_bg
                } else if is_selected {
                    popup_selected_bg
                } else if is_hover {
                    popup_hover_bg
                } else {
                    Color::TRANSPARENT
                };

                let clamped_top = adjusted_rect.y().max(dd_top + menu_bw);
                let clamped_bottom = (adjusted_rect.y() + adjusted_rect.size.height).min(dd_bottom - menu_bw);
                let clamped_rect = Rect::new(
                    Point::new(adjusted_rect.x(), clamped_top),
                    Size::new(adjusted_rect.size.width, (clamped_bottom - clamped_top).max(0.0)),
                );

                let is_first = first_visible == Some(*idx);
                let is_last = last_visible == Some(*idx);
                let item_radius = match (is_first, is_last) {
                    (true, true) => [inner_radius; 4],
                    (true, false) => [inner_radius, inner_radius, 0.0, 0.0],
                    (false, true) => [0.0, 0.0, inner_radius, inner_radius],
                    _ => [0.0; 4],
                };
                list.push_rect(clamped_rect, item_bg, item_radius);

                let item_h = adjusted_rect.size.height;
                let mut text_x = adjusted_rect.x() + 12.0;
                if let Some(ref icon) = item.icon {
                    let icon_rect = Rect::new(
                        Point::new(text_x, adjusted_rect.y()),
                        Size::new(20.0, item_h),
                    );
                    let icon_color = if item.disabled { popup_muted } else { popup_fg.with_alpha(0.6) };
                    list.push_text_centered(icon, icon_rect, icon_color, font_size);
                    text_x += 24.0;
                }

                let text_color = if item.disabled {
                    popup_muted
                } else if is_selected {
                    self.mss_popup_selected_fg.unwrap_or(popup_accent)
                } else if is_hover {
                    self.mss_popup_hover_fg.unwrap_or(popup_fg)
                } else {
                    popup_fg
                };

                let label_rect = Rect::new(
                    Point::new(text_x, adjusted_rect.y()),
                    Size::new(adjusted_rect.size.width - (text_x - adjusted_rect.x()) - 32.0, item_h),
                );
                list.push_text_styled_singleline(&item.label, label_rect, text_color, font_size,
                    TextAlign::DEFAULT, TextDecoration::None, font_weight, self.mss.font_family.clone());

                if is_selected {
                    let check_rect = Rect::new(
                        Point::new(adjusted_rect.x() + adjusted_rect.size.width - 28.0, adjusted_rect.y()),
                        Size::new(20.0, item_h),
                    );
                    list.push_text_centered("\u{E5CA}", check_rect, popup_accent, font_size);
                }
            }

            list.pop_clip();

            list.end_overlay();
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled {
            return EventResult::Ignored;
        }

        match event {
            Event::MouseMove(pos) => {
                let was_hover_button = self.hover_button;
                self.hover_button = self.bounds.contains(*pos);
                if self.hover_button { ctx.set_cursor(CursorIcon::Pointer); }

                if self.is_open {
                    let mut new_hover_item = None;
                    for (item_rect, idx) in &self.item_bounds {
                        let adjusted_rect = Rect::new(
                            Point::new(self.bounds.x() + item_rect.x(), self.bounds.y() + item_rect.y() - self.scroll_offset),
                            item_rect.size,
                        );
                        if adjusted_rect.contains(*pos) {
                            new_hover_item = Some(*idx);
                            break;
                        }
                    }

                    if new_hover_item != self.hover_item {
                        self.hover_item = new_hover_item;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                if self.hover_button != was_hover_button {
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left {
                    if self.bounds.contains(*position) {
                        self.is_open = !self.is_open;
                        if self.is_open {
                            self.determine_direction(ctx.viewport_size().height);
                            self.recompute_dropdown_position();
                            ctx.register_overlay(self.compute_overlay_bounds(), false);
                        } else {
                            ctx.unregister_overlay();
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    } else if self.is_open {
                        for (item_rect, idx) in &self.item_bounds {
                            let adjusted_rect = Rect::new(
                                Point::new(self.bounds.x() + item_rect.x(), self.bounds.y() + item_rect.y() - self.scroll_offset),
                                item_rect.size,
                            );
                            if adjusted_rect.contains(*position) {
                                let item = &self.items[*idx];
                                if !item.disabled {
                                    *self.selected.lock().unwrap() = Some(item.value.clone());
                                    if let Some(ref callback) = self.on_change {
                                        if let Ok(mut cb) = callback.lock() {
                                            cb(&item.value);
                                        }
                                    }
                                    self.is_open = false;
                                    ctx.unregister_overlay();
                                    ctx.request_paint();
                                    return EventResult::Handled;
                                }
                            }
                        }

                        self.is_open = false;
                        ctx.unregister_overlay();
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::BackPressed => {
                if self.is_open {
                    self.is_open = false;
                    ctx.unregister_overlay();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(key) => {
                if self.is_open {
                    match key {
                        Key::Escape => {
                            self.is_open = false;
                            ctx.unregister_overlay();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::Up => {
                            if let Some(hover) = self.hover_item {
                                if hover > 0 {
                                    self.hover_item = Some(hover - 1);
                                }
                            } else if !self.items.is_empty() {
                                self.hover_item = Some(0);
                            }
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::Down => {
                            if let Some(hover) = self.hover_item {
                                if hover < self.items.len() - 1 {
                                    self.hover_item = Some(hover + 1);
                                }
                            } else if !self.items.is_empty() {
                                self.hover_item = Some(0);
                            }
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::Enter => {
                            if let Some(hover) = self.hover_item {
                                let item = &self.items[hover];
                                if !item.disabled {
                                    *self.selected.lock().unwrap() = Some(item.value.clone());
                                    if let Some(ref callback) = self.on_change {
                                        if let Ok(mut cb) = callback.lock() {
                                            cb(&item.value);
                                        }
                                    }
                                    self.is_open = false;
                                    ctx.unregister_overlay();
                                    ctx.request_paint();
                                    return EventResult::Handled;
                                }
                            }
                        }
                        _ => {}
                    }
                } else if self.focused {
                    match key {
                        Key::Enter | Key::Space => {
                            self.is_open = true;
                            self.determine_direction(ctx.viewport_size().height);
                            self.recompute_dropdown_position();
                            ctx.register_overlay(self.compute_overlay_bounds(), false);
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::Down => {
                            self.is_open = true;
                            self.hover_item = Some(0);
                            self.determine_direction(ctx.viewport_size().height);
                            self.recompute_dropdown_position();
                            ctx.register_overlay(self.compute_overlay_bounds(), false);
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::Up => {
                            self.is_open = true;
                            if !self.items.is_empty() {
                                self.hover_item = Some(self.items.len() - 1);
                            }
                            self.determine_direction(ctx.viewport_size().height);
                            self.recompute_dropdown_position();
                            ctx.register_overlay(self.compute_overlay_bounds(), false);
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        _ => {}
                    }
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
                if self.is_open {
                    self.is_open = false;
                    ctx.unregister_overlay();
                }
                ctx.request_paint();
                EventResult::Handled
            }
            Event::MouseWheel { delta, .. } => {
                if self.is_open {
                    let new_offset = (self.scroll_offset - delta).clamp(0.0, self.max_scroll());
                    if new_offset != self.scroll_offset {
                        self.scroll_offset = new_offset;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
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

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        let w = self.mss.width.or(self.width).map(|d| d.resolve(1000.0));
        let h = self.mss.height.map(|d| d.resolve(1000.0));
        (w, h)
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds.origin = pos;
        let popup_gap = 4.0;
        if self.opens_upward {
            self.dropdown_bounds.origin = Point::new(pos.x, pos.y - self.dropdown_bounds.size.height - popup_gap);
        } else {
            self.dropdown_bounds.origin = Point::new(pos.x, pos.y + self.bounds.size.height + popup_gap);
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
        "Dropdown"
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(c) = style.get("--popup-background").and_then(|v| v.as_color()) { self.mss_popup_bg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-color").and_then(|v| v.as_color()) { self.mss_popup_fg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-accent").and_then(|v| v.as_color()) { self.mss_popup_accent = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-border").and_then(|v| v.as_color()) { self.mss_popup_border = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-hover-background").and_then(|v| v.as_color()) { self.mss_popup_hover_bg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-hover-color").and_then(|v| v.as_color()) { self.mss_popup_hover_fg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-selected-background").and_then(|v| v.as_color()) { self.mss_popup_selected_bg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-selected-color").and_then(|v| v.as_color()) { self.mss_popup_selected_fg = Some(mss_color_to_core(c)); }
        if let Some(d) = style.get("--popup-max-height").and_then(|v| v.as_dimension()) { self.mss_popup_max_height = Some(d.resolve(1000.0)); }
        if let Some(d) = style.get("--popup-min-height").and_then(|v| v.as_dimension()) { self.mss_popup_min_height = Some(d.resolve(1000.0)); }
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
        let selected_label = self.get_selected_label();
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::ComboBox,
            state: crate::a11y::NodeState {
                disabled: self.disabled,
                focused: self.focused,
                expanded: Some(self.is_open),
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                value: selected_label,
                placeholder: Some(self.placeholder_text()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for DropdownElement {
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
