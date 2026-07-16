use crate::animation::transition::mss_color_to_core;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;
use crate::core::sync::Mutex;
use crate::widget::context::TextMeasure;

use super::dropdown::DropdownItem;

pub struct Multiselect {
    items: Vec<DropdownItem>,
    selected: Vec<usize>,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(&[usize]) + Send>>>,
    width: Option<Dimension>,
    autocomplete: bool,
    max_visible_chips: Option<usize>,
}

impl Multiselect {
    pub fn new(items: Vec<DropdownItem>) -> Self {
        Self {
            items,
            selected: Vec::new(),
            placeholder: "Select...".to_string(),
            on_change: None,
            width: None,
            autocomplete: false,
            max_visible_chips: None,
        }
    }

    pub fn selected(mut self, indices: Vec<usize>) -> Self {
        self.selected = indices;
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn on_change(mut self, f: impl FnMut(&[usize]) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }

    pub fn with_autocomplete(mut self, enabled: bool) -> Self {
        self.autocomplete = enabled;
        self
    }

    pub fn max_visible(mut self, n: usize) -> Self {
        self.max_visible_chips = Some(n);
        self
    }
}

impl Widget for Multiselect {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(MultiselectElement {
            id: ElementId::new(),
            items: self.items.clone(),
            selected: self.selected.clone(),
            placeholder: self.placeholder.clone(),
            on_change: self.on_change.clone(),
            width: self.width,
            autocomplete: self.autocomplete,
            max_visible_chips: self.max_visible_chips,
            is_open: false,
            hover_index: None,
            scroll_offset: 0.0,
            opens_upward: false,
            bounds: Rect::zero(),
            filter_text: String::new(),
            filtered_indices: Vec::new(),
            cursor_blink: 0.0,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            mss_popup_bg: None,
            mss_popup_fg: None,
            mss_popup_accent: None,
            mss_popup_border: None,
            mss_popup_hover_bg: None,
            mss_popup_hover_fg: None,
            mss_popup_max_height: None,
            mss_popup_min_height: None,
            text_measure: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

const INPUT_HEIGHT: f32 = 40.0;
const CHIP_HEIGHT: f32 = 26.0;
const CHIP_GAP: f32 = 4.0;
const CHIP_FONT_SIZE: f32 = 11.0;
const CHIP_PADDING: f32 = 28.0;
const ITEM_HEIGHT: f32 = 36.0;
const MAX_DROPDOWN_HEIGHT: f32 = 200.0;
const FILTER_HEIGHT: f32 = 36.0;
const FILTER_GAP: f32 = 4.0;

const CURSOR_BLINK_RATE: f32 = 1.0;

pub struct MultiselectElement {
    id: ElementId,
    items: Vec<DropdownItem>,
    selected: Vec<usize>,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(&[usize]) + Send>>>,
    width: Option<Dimension>,
    autocomplete: bool,
    max_visible_chips: Option<usize>,
    is_open: bool,
    hover_index: Option<usize>,
    scroll_offset: f32,
    opens_upward: bool,
    bounds: Rect,
    filter_text: String,
    filtered_indices: Vec<usize>,
    cursor_blink: f32,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    mss_popup_bg: Option<Color>,
    mss_popup_fg: Option<Color>,
    mss_popup_accent: Option<Color>,
    mss_popup_border: Option<Color>,
    mss_popup_hover_bg: Option<Color>,
    mss_popup_hover_fg: Option<Color>,
    mss_popup_max_height: Option<f32>,
    mss_popup_min_height: Option<f32>,
    text_measure: Option<Arc<dyn TextMeasure>>,
}

impl MultiselectElement {
    fn fire_change(&self) {
        if let Some(ref cb) = self.on_change {
            if let Ok(mut f) = cb.lock() { f(&self.selected); }
        }
    }

    fn effective_popup_height(&self, item_count: usize) -> f32 {
        let content_h = item_count as f32 * ITEM_HEIGHT;
        let filter_h = if self.autocomplete { FILTER_HEIGHT + FILTER_GAP } else { 0.0 };
        let max_h = self.mss_popup_max_height.unwrap_or(MAX_DROPDOWN_HEIGHT);
        let min_h = self.mss_popup_min_height.unwrap_or(0.0);
        (content_h + filter_h).min(max_h).max(min_h)
    }

    fn chip_width(&self, label: &str) -> f32 {
        let text_w = self.text_measure.as_ref()
            .map(|tm| tm.measure_text_width(label, CHIP_FONT_SIZE, label.chars().count()))
            .unwrap_or_else(|| label.chars().count() as f32 * CHIP_FONT_SIZE * 0.65);
        text_w + CHIP_PADDING
    }

    fn toggle_selection(&mut self, idx: usize) {
        if let Some(pos) = self.selected.iter().position(|&i| i == idx) {
            self.selected.remove(pos);
        } else {
            self.selected.push(idx);
        }
        self.fire_change();
    }

    fn visible_item_count(&self) -> usize {
        if self.autocomplete && !self.filter_text.is_empty() {
            self.filtered_indices.len()
        } else {
            self.items.len()
        }
    }

    fn dropdown_rect(&self) -> Rect {
        let count = self.visible_item_count();
        let h = self.effective_popup_height(count);
        let popup_gap = 4.0;
        let y = if self.opens_upward {
            self.bounds.y() - h - popup_gap
        } else {
            self.bounds.y() + self.bounds.size.height + popup_gap
        };
        Rect::new(
            Point::new(self.bounds.x(), y),
            Size::new(self.bounds.size.width, h),
        )
    }

    fn display_indices(&self) -> Vec<usize> {
        if self.autocomplete && !self.filter_text.is_empty() {
            self.filtered_indices.clone()
        } else {
            (0..self.items.len()).collect()
        }
    }

    fn update_filter(&mut self) {
        if self.filter_text.is_empty() {
            self.filtered_indices.clear();
        } else {
            let lower = self.filter_text.to_lowercase();
            self.filtered_indices = self.items.iter().enumerate()
                .filter(|(_, item)| item.label.to_lowercase().contains(&lower))
                .map(|(i, _)| i)
                .collect();
        }
        self.scroll_offset = 0.0;
        self.hover_index = None;
    }

    fn visible_chips(&self) -> (&[usize], usize) {
        if let Some(max) = self.max_visible_chips {
            if self.selected.len() > max {
                return (&self.selected[..max], self.selected.len() - max);
            }
        }
        (&self.selected, 0)
    }

    fn input_height(&self) -> f32 {
        if self.selected.is_empty() {
            INPUT_HEIGHT
        } else {
            let (visible, extra) = self.visible_chips();
            let available_w = self.bounds.size.width - 40.0;
            let mut row_w: f32 = 0.0;
            let mut rows = 1;
            for &idx in visible {
                if idx < self.items.len() {
                    let chip_w = self.chip_width(&self.items[idx].label);
                    if row_w + chip_w + CHIP_GAP > available_w && row_w > 0.0 {
                        rows += 1;
                        row_w = chip_w + CHIP_GAP;
                    } else {
                        row_w += chip_w + CHIP_GAP;
                    }
                }
            }
            if extra > 0 {
                let badge_w = 50.0;
                if row_w + badge_w + CHIP_GAP > available_w && row_w > 0.0 {
                    rows += 1;
                }
            }
            (8.0 + rows as f32 * (CHIP_HEIGHT + CHIP_GAP) + 4.0).max(INPUT_HEIGHT)
        }
    }

    fn items_y_offset(&self) -> f32 {
        if self.autocomplete { FILTER_HEIGHT + FILTER_GAP } else { 0.0 }
    }
}

impl Element for MultiselectElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(ms) = widget.as_any().downcast_ref::<Multiselect>() {
            self.items = ms.items.clone();
            self.selected = ms.selected.clone();
            self.placeholder = ms.placeholder.clone();
            self.on_change = ms.on_change.clone();
            self.width = ms.width;
            self.autocomplete = ms.autocomplete;
            self.max_visible_chips = ms.max_visible_chips;
            if self.autocomplete && !self.filter_text.is_empty() {
                self.update_filter();
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self.width.map(|d| d.resolve(constraints.max_width)).unwrap_or(constraints.max_width).min(constraints.max_width);
        let h = self.input_height();
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
        let border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let muted = fg.with_alpha(0.5);
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        let border_color = if self.is_open { primary } else { border };

        list.push_rect_bordered(
            self.bounds, bg, [8.0; 4],
            Border::new(if self.is_open { 2.0 } else { 1.0 }, border_color),
        );

        if self.selected.is_empty() {
            let text_rect = Rect::new(
                Point::new(self.bounds.x() + 12.0, self.bounds.y() + (self.bounds.size.height - 14.0) / 2.0),
                Size::new(self.bounds.size.width - 40.0, 16.0),
            );
            list.push_text(&self.placeholder, text_rect, muted, 14.0);
        } else {
            let (visible, extra) = self.visible_chips();
            let available_w = self.bounds.size.width - 40.0;
            let mut x = self.bounds.x() + 8.0;
            let mut y = self.bounds.y() + 6.0;

            let chip_area = Rect::new(
                Point::new(self.bounds.x(), self.bounds.y()),
                Size::new(self.bounds.size.width - 32.0, self.bounds.size.height),
            );
            list.push_clip(chip_area);

            for &idx in visible {
                if idx >= self.items.len() { continue; }
                let label = &self.items[idx].label;
                let chip_w = self.chip_width(label);

                if x - self.bounds.x() - 8.0 + chip_w > available_w && x > self.bounds.x() + 9.0 {
                    x = self.bounds.x() + 8.0;
                    y += CHIP_HEIGHT + CHIP_GAP;
                }

                let chip_rect = Rect::new(Point::new(x, y), Size::new(chip_w, CHIP_HEIGHT));
                list.push_rect_bordered(
                    chip_rect,
                    primary.with_alpha(0.15),
                    [13.0; 4],
                    Border::new(1.0, primary.with_alpha(0.3)),
                );

                let text_rect = Rect::new(
                    Point::new(x + 8.0, y + (CHIP_HEIGHT - 11.0) / 2.0),
                    Size::new(chip_w - 24.0, 12.0),
                );
                list.push_text(label, text_rect, fg, CHIP_FONT_SIZE);

                let close_rect = Rect::new(
                    Point::new(x + chip_w - 16.0, y + (CHIP_HEIGHT - 10.0) / 2.0),
                    Size::new(10.0, 10.0),
                );
                list.push_text("✕", close_rect, muted, 9.0);

                x += chip_w + CHIP_GAP;
            }

            if extra > 0 {
                let badge_text = format!("+{}", extra);
                let badge_w = 50.0;
                if x - self.bounds.x() - 8.0 + badge_w > available_w && x > self.bounds.x() + 9.0 {
                    x = self.bounds.x() + 8.0;
                    y += CHIP_HEIGHT + CHIP_GAP;
                }
                let badge_rect = Rect::new(Point::new(x, y), Size::new(badge_w, CHIP_HEIGHT));
                list.push_rect(badge_rect, muted.with_alpha(0.15), [13.0; 4]);
                list.push_text_centered(&badge_text, badge_rect, muted, CHIP_FONT_SIZE);
            }

            list.pop_clip();
        }

        let arrow_rect = Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - 28.0, self.bounds.y() + (self.bounds.size.height - 10.0) / 2.0),
            Size::new(16.0, 12.0),
        );
        let arrow = if self.is_open { "\u{E5CE}" } else { "\u{E5CF}" };
        list.push_text(arrow, arrow_rect, muted, 14.0);

        if self.is_open {
            let dd = self.dropdown_rect();
            let popup_bg = self.mss_popup_bg.unwrap_or(bg);
            let popup_fg = self.mss_popup_fg.unwrap_or(fg);
            let popup_accent = self.mss_popup_accent.unwrap_or(primary);
            let popup_border_color = self.mss_popup_border.unwrap_or(border);
            let popup_hover_bg = self.mss_popup_hover_bg.unwrap_or_else(|| popup_bg.darken(0.05));
            let popup_muted = popup_fg.with_alpha(0.5);

            list.begin_overlay();
            let menu_radius: f32 = 8.0;
            let menu_bw: f32 = 1.0;
            list.push_shadow(dd, Color::BLACK.with_alpha(0.12), 12.0, (0.0, 4.0), [menu_radius; 4]);
            list.push_rect_bordered(dd, popup_bg, [menu_radius; 4], Border::new(menu_bw, popup_border_color));

            let inset = Rect::new(
                Point::new(dd.x() + menu_bw, dd.y() + menu_bw),
                Size::new((dd.size.width - menu_bw * 2.0).max(0.0), (dd.size.height - menu_bw * 2.0).max(0.0)),
            );
            list.push_clip(inset);
            let inner_radius = (menu_radius - menu_bw).max(0.0);

            let items_y_start = dd.y() + self.items_y_offset();
            if self.autocomplete {
                let filter_rect = Rect::new(
                    Point::new(dd.x() + 8.0, dd.y() + 4.0),
                    Size::new(dd.size.width - 16.0, FILTER_HEIGHT - 4.0),
                );
                list.push_rect_bordered(filter_rect, popup_bg, [6.0; 4], Border::new(1.5, popup_accent));

                let icon_rect = Rect::new(
                    Point::new(filter_rect.x() + 8.0, filter_rect.y() + (filter_rect.size.height - 14.0) / 2.0),
                    Size::new(14.0, 14.0),
                );
                list.push_text("\u{E8B6}", icon_rect, popup_muted, 14.0);

                let text_rect = Rect::new(
                    Point::new(filter_rect.x() + 28.0, filter_rect.y() + (filter_rect.size.height - 14.0) / 2.0),
                    Size::new(filter_rect.size.width - 36.0, 16.0),
                );
                if self.filter_text.is_empty() {
                    list.push_text("\u{041F}\u{043E}\u{0438}\u{0441}\u{043A}...", text_rect, popup_muted, 13.0);
                } else {
                    list.push_text(&self.filter_text, text_rect, popup_fg, 13.0);
                }

                let blink_phase = (self.cursor_blink * CURSOR_BLINK_RATE * 2.0) % 2.0;
                if blink_phase < 1.0 {
                    let char_w = 7.5;
                    let cursor_x = text_rect.x() + self.filter_text.chars().count() as f32 * char_w;
                    let cursor_rect = Rect::new(
                        Point::new(cursor_x, text_rect.y()),
                        Size::new(1.5, 14.0),
                    );
                    list.push_rect(cursor_rect, popup_accent, [0.0; 4]);
                }

                let div_rect = Rect::new(
                    Point::new(dd.x() + 8.0, dd.y() + FILTER_HEIGHT),
                    Size::new(dd.size.width - 16.0, 1.0),
                );
                list.push_rect(div_rect, popup_border_color.with_alpha(0.3), [0.0; 4]);
            }

            let display_items = self.display_indices();
            let dd_top = items_y_start;
            let dd_bottom = dd.y() + dd.size.height;

            let mut first_visible: Option<usize> = None;
            let mut last_visible: Option<usize> = None;
            for (vi, _) in display_items.iter().enumerate() {
                let y = items_y_start + vi as f32 * ITEM_HEIGHT - self.scroll_offset;
                if y + ITEM_HEIGHT <= dd_top { continue; }
                if y >= dd_bottom { break; }
                if first_visible.is_none() { first_visible = Some(vi); }
                last_visible = Some(vi);
            }

            for (vi, &actual_idx) in display_items.iter().enumerate() {
                let y = items_y_start + vi as f32 * ITEM_HEIGHT - self.scroll_offset;
                if y + ITEM_HEIGHT < dd_top || y > dd_bottom { continue; }

                let adjusted_rect = Rect::new(Point::new(dd.x() + menu_bw, y), Size::new(inset.size.width, ITEM_HEIGHT));

                let clamped_top = adjusted_rect.y().max(dd_top);
                let clamped_bottom = (adjusted_rect.y() + adjusted_rect.size.height).min(dd_bottom - menu_bw);
                let clamped_rect = Rect::new(
                    Point::new(adjusted_rect.x(), clamped_top),
                    Size::new(adjusted_rect.size.width, (clamped_bottom - clamped_top).max(0.0)),
                );

                let is_first = first_visible == Some(vi);
                let is_last = last_visible == Some(vi);
                let item_radius = match (is_first, is_last) {
                    (true, true) if !self.autocomplete => [inner_radius; 4],
                    (true, false) if !self.autocomplete => [inner_radius, inner_radius, 0.0, 0.0],
                    (false, true) => [0.0, 0.0, inner_radius, inner_radius],
                    _ => [0.0; 4],
                };
                if self.hover_index == Some(vi) {
                    list.push_rect(clamped_rect, popup_hover_bg, item_radius);
                }

                let cb_rect = Rect::new(
                    Point::new(dd.x() + 12.0, y + (ITEM_HEIGHT - 16.0) / 2.0),
                    Size::new(16.0, 16.0),
                );
                let is_selected = self.selected.contains(&actual_idx);
                if is_selected {
                    list.push_rect(cb_rect, popup_accent, [3.0; 4]);
                    let check_rect = Rect::new(
                        Point::new(cb_rect.x() + 2.0, cb_rect.y() + 1.0),
                        Size::new(12.0, 14.0),
                    );
                    list.push_text("\u{E5CA}", check_rect, Color::WHITE, 11.0);
                } else {
                    list.push_rect_bordered(cb_rect, popup_bg, [3.0; 4], Border::new(1.0, popup_border_color));
                }

                let item = &self.items[actual_idx];
                let ir = Rect::new(
                    Point::new(dd.x() + 36.0, y + (ITEM_HEIGHT - 14.0) / 2.0),
                    Size::new(dd.size.width - 48.0, 16.0),
                );
                let is_hover = self.hover_index == Some(vi);
                let color = if item.disabled {
                    popup_muted
                } else if is_hover {
                    self.mss_popup_hover_fg.unwrap_or(popup_fg)
                } else {
                    popup_fg
                };
                list.push_text(&item.label, ir, color, 14.0);
            }

            if display_items.is_empty() && self.autocomplete && !self.filter_text.is_empty() {
                let empty_rect = Rect::new(
                    Point::new(dd.x() + 12.0, items_y_start + 8.0),
                    Size::new(dd.size.width - 24.0, 20.0),
                );
                list.push_text("\u{041D}\u{0438}\u{0447}\u{0435}\u{0433}\u{043E} \u{043D}\u{0435} \u{043D}\u{0430}\u{0439}\u{0434}\u{0435}\u{043D}\u{043E}", empty_rect, popup_muted, 13.0);
            }

            list.pop_clip();
            list.end_overlay();
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Pointer);
                    return EventResult::Handled;
                }
                if self.is_open {
                    let dd = self.dropdown_rect();
                    if dd.contains(*pos) {
                        let items_y = dd.y() + self.items_y_offset();
                        if pos.y >= items_y {
                            let vi = ((pos.y - items_y + self.scroll_offset) / ITEM_HEIGHT) as usize;
                            if self.hover_index != Some(vi) {
                                self.hover_index = Some(vi);
                                ctx.request_paint();
                            }
                        }
                        ctx.set_cursor(CursorIcon::Pointer);
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.bounds.contains(*position) {
                    let (visible, _extra) = self.visible_chips();
                    let available_w = self.bounds.size.width - 40.0;
                    let mut x = self.bounds.x() + 8.0;
                    let mut y = self.bounds.y() + 6.0;
                    let mut removed = false;

                    let visible_vec: Vec<usize> = visible.to_vec();
                    for &idx in visible_vec.iter() {
                        if idx >= self.items.len() { continue; }
                        let chip_w = self.chip_width(&self.items[idx].label);
                        if x - self.bounds.x() - 8.0 + chip_w > available_w && x > self.bounds.x() + 9.0 {
                            x = self.bounds.x() + 8.0;
                            y += CHIP_HEIGHT + CHIP_GAP;
                        }
                        let close_x = x + chip_w - 16.0;
                        let close_rect = Rect::new(Point::new(close_x, y), Size::new(16.0, CHIP_HEIGHT));
                        if close_rect.contains(*position) {
                            if let Some(pos) = self.selected.iter().position(|&i| i == idx) {
                                self.selected.remove(pos);
                            }
                            self.fire_change();
                            ctx.request_paint();
                            removed = true;
                            break;
                        }
                        x += chip_w + CHIP_GAP;
                    }

                    if !removed {
                        self.is_open = !self.is_open;
                        if self.is_open {
                            self.filter_text.clear();
                            self.filtered_indices.clear();
                            self.cursor_blink = 0.0;
                            let popup_gap = 4.0;
                            let dd_h = self.effective_popup_height(self.items.len());
                            self.opens_upward = self.bounds.y() + self.bounds.size.height + dd_h + popup_gap > ctx.viewport_size().height
                                && self.bounds.y() >= dd_h + popup_gap;
                            let dd = self.dropdown_rect();
                            let overlay_bounds = if self.opens_upward {
                                Rect::new(
                                    Point::new(self.bounds.x(), dd.y()),
                                    Size::new(self.bounds.size.width, dd.size.height + popup_gap + self.bounds.size.height),
                                )
                            } else {
                                Rect::new(
                                    self.bounds.origin,
                                    Size::new(self.bounds.size.width, self.bounds.size.height + popup_gap + dd.size.height),
                                )
                            };
                            ctx.register_overlay(overlay_bounds, false);
                        } else {
                            ctx.unregister_overlay();
                        }
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.is_open {
                    let dd = self.dropdown_rect();
                    if dd.contains(*position) {
                        let items_y = dd.y() + self.items_y_offset();

                        if self.autocomplete && position.y < items_y {
                            return EventResult::Handled;
                        }

                        if position.y >= items_y {
                            let vi = ((position.y - items_y + self.scroll_offset) / ITEM_HEIGHT) as usize;
                            let display = self.display_indices();
                            if vi < display.len() {
                                let actual_idx = display[vi];
                                if !self.items[actual_idx].disabled {
                                    self.toggle_selection(actual_idx);
                                }
                            }
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                    self.is_open = false;
                    self.filter_text.clear();
                    ctx.unregister_overlay();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::CharInput(ch) if self.is_open && self.autocomplete => {
                if !ch.is_control() {
                    self.filter_text.push(*ch);
                    self.update_filter();
                    self.cursor_blink = 0.0;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::Backspace) if self.is_open && self.autocomplete => {
                self.filter_text.pop();
                self.update_filter();
                self.cursor_blink = 0.0;
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(Key::Escape) if self.is_open => {
                self.is_open = false;
                self.filter_text.clear();
                ctx.unregister_overlay();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::MouseWheel { delta, position, .. } if self.is_open => {
                let dd = self.dropdown_rect();
                if dd.contains(*position) {
                    let display_count = self.visible_item_count();
                    let total_height = display_count as f32 * ITEM_HEIGHT;
                    let visible_h = dd.size.height - self.items_y_offset();
                    let max_scroll = (total_height - visible_h).max(0.0);
                    self.scroll_offset = (self.scroll_offset - delta).clamp(0.0, max_scroll);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if self.is_open && self.autocomplete {
            self.cursor_blink += dt.as_secs_f32();
            self.mark_dirty(DirtyFlags::RENDER);
            return true;
        }
        false
    }

    fn needs_repaint(&self) -> bool {
        self.is_open && self.autocomplete
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        let w = self.width.map(|d| d.resolve(1000.0));
        (w, None)
    }

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

    fn element_type_name(&self) -> &str { "Multiselect" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = self.mss.width { self.width = Some(w); }
        if let Some(c) = style.get("--popup-background").and_then(|v| v.as_color()) { self.mss_popup_bg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-color").and_then(|v| v.as_color()) { self.mss_popup_fg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-accent").and_then(|v| v.as_color()) { self.mss_popup_accent = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-border").and_then(|v| v.as_color()) { self.mss_popup_border = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-hover-background").and_then(|v| v.as_color()) { self.mss_popup_hover_bg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-hover-color").and_then(|v| v.as_color()) { self.mss_popup_hover_fg = Some(mss_color_to_core(c)); }
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
}

impl StyledElement for MultiselectElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
