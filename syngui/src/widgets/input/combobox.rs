use crate::animation::transition::mss_color_to_core;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension, MssFields};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

use super::dropdown::DropdownItem;

pub struct Combobox {
    items: Vec<DropdownItem>,
    text: String,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    width: Option<Dimension>,
    popup_min_width: Option<f32>,
}

impl Combobox {
    pub fn new(items: Vec<DropdownItem>) -> Self {
        Self {
            items,
            text: String::new(),
            placeholder: String::new(),
            on_change: None,
            width: None,
            popup_min_width: None,
        }
    }

    /// Минимальная ширина выпадающего списка. Полезно для узких полей с
    /// длинными подписями пунктов: попап становится шире поля (прижимается к
    /// правому краю окна, если не помещается), а пункты не переносятся.
    /// То же задаётся из MSS переменной `--popup-min-width`.
    pub fn popup_min_width(mut self, w: f32) -> Self {
        self.popup_min_width = Some(w);
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn on_change(mut self, f: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }
}

impl Widget for Combobox {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(ComboboxElement {
            id: ElementId::new(),
            items: self.items.clone(),
            text: self.text.clone(),
            placeholder: self.placeholder.clone(),
            on_change: self.on_change.clone(),
            width: self.width,
            is_open: false,
            filtered_indices: Vec::new(),
            hover_index: None,
            focused: false,
            cursor_pos: self.text.len(),
            scroll_offset: 0.0,
            opens_upward: false,
            bounds: Rect::zero(),
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
            popup_min_width: self.popup_min_width,
            mss_popup_min_width: None,
            viewport_width: 0.0,
            mss_popup_min_height: None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

const INPUT_HEIGHT: f32 = 40.0;
const ITEM_HEIGHT: f32 = 36.0;
const MAX_DROPDOWN_HEIGHT: f32 = 200.0;

pub struct ComboboxElement {
    id: ElementId,
    items: Vec<DropdownItem>,
    text: String,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    width: Option<Dimension>,
    is_open: bool,
    filtered_indices: Vec<usize>,
    hover_index: Option<usize>,
    focused: bool,
    cursor_pos: usize,
    scroll_offset: f32,
    opens_upward: bool,
    bounds: Rect,
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
    popup_min_width: Option<f32>,
    mss_popup_min_width: Option<f32>,
    /// Ширина окна на момент открытия — чтобы широкий попап не уезжал за край.
    viewport_width: f32,
}

impl ComboboxElement {
    fn char_boundary_pos(&self) -> usize {
        if self.text.is_char_boundary(self.cursor_pos) {
            self.cursor_pos
        } else {
            self.text.char_indices()
                .map(|(i, _)| i)
                .filter(|&i| i <= self.cursor_pos)
                .last()
                .unwrap_or(0)
        }
    }

    fn effective_popup_height(&self, item_count: usize) -> f32 {
        let content_h = item_count as f32 * ITEM_HEIGHT;
        let max_h = self.mss_popup_max_height.unwrap_or(MAX_DROPDOWN_HEIGHT);
        let min_h = self.mss_popup_min_height.unwrap_or(0.0);
        content_h.min(max_h).max(min_h)
    }

    fn update_filter(&mut self) {
        let query = self.text.to_lowercase();
        self.filtered_indices = if query.is_empty() {
            (0..self.items.len()).collect()
        } else {
            self.items.iter().enumerate()
                .filter(|(_, item)| item.label.to_lowercase().contains(&query))
                .map(|(i, _)| i)
                .collect()
        };
        // Список стал короче — смещение прокрутки не должно уводить за конец.
        let h = self.effective_popup_height(self.filtered_indices.len());
        let max_scroll = (self.filtered_indices.len() as f32 * ITEM_HEIGHT - h).max(0.0);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
    }

    fn dropdown_rect(&self) -> Rect {
        let count = self.filtered_indices.len();
        let h = self.effective_popup_height(count);
        let popup_gap = 4.0;
        let y = if self.opens_upward {
            self.bounds.y() - h - popup_gap
        } else {
            self.bounds.y() + INPUT_HEIGHT + popup_gap
        };
        let min_w = self.popup_min_width.or(self.mss_popup_min_width).unwrap_or(0.0);
        let w = self.bounds.size.width.max(min_w);
        let mut x = self.bounds.x();
        if self.viewport_width > 0.0 && x + w > self.viewport_width - 8.0 {
            x = (self.viewport_width - 8.0 - w).max(0.0);
        }
        Rect::new(Point::new(x, y), Size::new(w, h))
    }

    fn fire_change(&self) {
        if let Some(ref cb) = self.on_change {
            if let Ok(mut f) = cb.lock() { f(&self.text); }
        }
    }
}

impl Element for ComboboxElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(cb) = widget.as_any().downcast_ref::<Combobox>() {
            self.items = cb.items.clone();
            self.text = cb.text.clone();
            self.placeholder = cb.placeholder.clone();
            self.on_change = cb.on_change.clone();
            self.width = cb.width;
            self.popup_min_width = cb.popup_min_width;
            self.update_filter();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self.width.map(|d| d.resolve(constraints.max_width)).unwrap_or(constraints.max_width).min(constraints.max_width);
        self.bounds = Rect::new(Point::zero(), Size::new(w, INPUT_HEIGHT));
        Size::new(w, INPUT_HEIGHT)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg = self.mss.color.unwrap_or(Color::from_hex("#1F2937"));
        let border = self.mss.border_color.unwrap_or(Color::from_hex("#D1D5DB"));
        let muted = fg.with_alpha(0.5);

        let border_color = if self.focused { self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6")) } else { border };

        list.push_rect_bordered(
            self.bounds, bg, [8.0; 4],
            Border::new(if self.focused { 2.0 } else { 1.0 }, border_color),
        );

        let text_rect = Rect::new(
            Point::new(self.bounds.x() + 12.0, self.bounds.y() + (INPUT_HEIGHT - 14.0) / 2.0),
            Size::new(self.bounds.size.width - 40.0, 16.0),
        );
        if self.text.is_empty() {
            list.push_text_singleline(&self.placeholder, text_rect, muted, 14.0, crate::mss::TextAlign::LEFT | crate::mss::TextAlign::VCENTER, 400);
        } else {
            list.push_text_singleline(&self.text, text_rect, fg, 14.0, crate::mss::TextAlign::LEFT | crate::mss::TextAlign::VCENTER, 400);
        }

        if self.focused {
            let caret = self.mss.caret_color_or(
                self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6")),
            );
            list.push_text_cursor_styled(
                &self.text,
                self.cursor_pos,
                self.bounds.x() + 12.0,
                self.bounds.y() + 10.0,
                INPUT_HEIGHT - 20.0,
                14.0,
                self.mss.font_weight_or(400),
                caret,
                self.mss.font_family.clone(),
            );
        }

        let arrow_rect = Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - 28.0, self.bounds.y() + (INPUT_HEIGHT - 10.0) / 2.0),
            Size::new(16.0, 12.0),
        );
        let arrow = if self.is_open { "\u{E5CE}" } else { "\u{E5CF}" };
        list.push_text(arrow, arrow_rect, muted, 14.0);

        if self.is_open && !self.filtered_indices.is_empty() {
            let dd = self.dropdown_rect();
            let popup_bg = self.mss_popup_bg.unwrap_or(bg);
            let popup_fg = self.mss_popup_fg.unwrap_or(fg);
            let _popup_accent = self.mss_popup_accent.unwrap_or(self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6")));
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
            let dd_top = dd.y();
            let dd_bottom = dd_top + dd.size.height;

            let mut first_visible: Option<usize> = None;
            let mut last_visible: Option<usize> = None;
            for (vi, _) in self.filtered_indices.iter().enumerate() {
                let y = dd.y() + vi as f32 * ITEM_HEIGHT - self.scroll_offset;
                if y + ITEM_HEIGHT <= dd_top { continue; }
                if y >= dd_bottom { break; }
                if first_visible.is_none() { first_visible = Some(vi); }
                last_visible = Some(vi);
            }

            for (vi, &item_idx) in self.filtered_indices.iter().enumerate() {
                let y = dd.y() + vi as f32 * ITEM_HEIGHT - self.scroll_offset;
                if y + ITEM_HEIGHT < dd_top || y > dd_bottom { continue; }

                let item = &self.items[item_idx];
                let adjusted_rect = Rect::new(Point::new(dd.x() + menu_bw, y), Size::new(inset.size.width, ITEM_HEIGHT));

                let clamped_top = adjusted_rect.y().max(dd_top + menu_bw);
                let clamped_bottom = (adjusted_rect.y() + adjusted_rect.size.height).min(dd_bottom - menu_bw);
                let clamped_rect = Rect::new(
                    Point::new(adjusted_rect.x(), clamped_top),
                    Size::new(adjusted_rect.size.width, (clamped_bottom - clamped_top).max(0.0)),
                );

                let is_first = first_visible == Some(vi);
                let is_last = last_visible == Some(vi);
                let item_radius = match (is_first, is_last) {
                    (true, true) => [inner_radius; 4],
                    (true, false) => [inner_radius, inner_radius, 0.0, 0.0],
                    (false, true) => [0.0, 0.0, inner_radius, inner_radius],
                    _ => [0.0; 4],
                };
                if self.hover_index == Some(vi) {
                    list.push_rect(clamped_rect, popup_hover_bg, item_radius);
                }

                let ir = Rect::new(
                    Point::new(dd.x() + 12.0, y + (ITEM_HEIGHT - 14.0) / 2.0),
                    Size::new(dd.size.width - 24.0, 16.0),
                );
                let is_hover = self.hover_index == Some(vi);
                let color = if item.disabled {
                    popup_muted
                } else if is_hover {
                    self.mss_popup_hover_fg.unwrap_or(popup_fg)
                } else {
                    popup_fg
                };
                // Одна строка: длинная подпись обрезается клипом попапа, а не
                // переносится поверх соседних пунктов.
                list.push_text_singleline(&item.label, ir, color, 14.0, crate::mss::TextAlign::LEFT | crate::mss::TextAlign::VCENTER, 400);
            }

            list.pop_clip();
            list.end_overlay();
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::FocusGained => {
                self.focused = true;
                ctx.request_paint();
                return EventResult::Handled;
            }
            Event::FocusLost => {
                self.focused = false;
                if self.is_open {
                    self.is_open = false;
                    ctx.unregister_overlay();
                }
                self.hover_index = None;
                ctx.request_paint();
                return EventResult::Handled;
            }
            Event::MouseMove(pos) => {
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Text);
                    return EventResult::Handled;
                }
                if self.is_open {
                    let dd = self.dropdown_rect();
                    if dd.contains(*pos) {
                        let vi = ((pos.y - dd.y() + self.scroll_offset) / ITEM_HEIGHT) as usize;
                        if self.hover_index != Some(vi) {
                            self.hover_index = Some(vi);
                            ctx.request_paint();
                        }
                        ctx.set_cursor(CursorIcon::Pointer);
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.bounds.contains(*position) {
                    self.focused = true;
                    let text_x = self.bounds.x() + 12.0;
                    let rel_x = (position.x - text_x).max(0.0);
                    self.cursor_pos = ctx.hit_test_char(&self.text, 14.0, rel_x)
                        .map(|char_idx| {
                            self.text.char_indices()
                                .nth(char_idx)
                                .map(|(i, _)| i)
                                .unwrap_or(self.text.len())
                        })
                        .unwrap_or(self.text.len());
                    self.is_open = !self.is_open;
                    if self.is_open {
                        self.update_filter();
                        // Список открывается с начала; если текущее значение
                        // есть среди пунктов — прокручиваем к нему.
                        self.scroll_offset = 0.0;
                        let popup_gap = 4.0;
                        let dd_h = self.effective_popup_height(self.filtered_indices.len());
                        self.viewport_width = ctx.viewport_size().width;
                        self.opens_upward = self.bounds.y() + INPUT_HEIGHT + dd_h + popup_gap > ctx.viewport_size().height
                            && self.bounds.y() >= dd_h + popup_gap;
                        let dd = self.dropdown_rect();
                        if let Some(vi) = self.filtered_indices.iter().position(|&i| {
                            self.items[i].value == self.text || self.items[i].label == self.text
                        }) {
                            let max_scroll = (self.filtered_indices.len() as f32 * ITEM_HEIGHT
                                - dd.size.height)
                                .max(0.0);
                            self.scroll_offset = (vi as f32 * ITEM_HEIGHT).min(max_scroll);
                        }
                        // Область оверлея — объединение поля и попапа (попап
                        // может быть шире поля и сдвинут влево).
                        let left = self.bounds.x().min(dd.x());
                        let right = (self.bounds.x() + self.bounds.size.width).max(dd.x() + dd.size.width);
                        let overlay_bounds = if self.opens_upward {
                            Rect::new(
                                Point::new(left, dd.y()),
                                Size::new(right - left, dd.size.height + popup_gap + INPUT_HEIGHT),
                            )
                        } else {
                            Rect::new(
                                Point::new(left, self.bounds.y()),
                                Size::new(right - left, INPUT_HEIGHT + popup_gap + dd.size.height),
                            )
                        };
                        ctx.register_overlay(overlay_bounds, false);
                    } else {
                        ctx.unregister_overlay();
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.is_open {
                    let dd = self.dropdown_rect();
                    if dd.contains(*position) {
                        let vi = ((position.y - dd.y() + self.scroll_offset) / ITEM_HEIGHT) as usize;
                        if let Some(&item_idx) = self.filtered_indices.get(vi) {
                            let item = &self.items[item_idx];
                            if !item.disabled {
                                self.text = item.value.clone();
                                self.cursor_pos = self.text.len();
                                self.is_open = false;
                                ctx.unregister_overlay();
                                self.fire_change();
                            }
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    self.is_open = false;
                    self.focused = false;
                    ctx.unregister_overlay();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.focused {
                    self.focused = false;
                    ctx.request_paint();
                }
                EventResult::Ignored
            }
            Event::CharInput(ch) if self.focused => {
                if ch.is_control() || ctx.modifiers.ctrl { return EventResult::Ignored; }
                let pos = self.char_boundary_pos();
                self.text.insert(pos, *ch);
                self.cursor_pos = pos + ch.len_utf8();
                self.update_filter();
                self.is_open = true;
                self.hover_index = None;
                self.fire_change();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(key) if self.focused => {
                match key {
                    Key::Backspace => {
                        if self.cursor_pos > 0 {
                            let safe_pos = self.char_boundary_pos();
                            let prev = self.text[..safe_pos]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            self.text.remove(prev);
                            self.cursor_pos = prev;
                            self.update_filter();
                            self.is_open = true;
                            self.fire_change();
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    Key::Left => {
                        if self.cursor_pos > 0 {
                            let safe_pos = self.char_boundary_pos();
                            self.cursor_pos = self.text[..safe_pos]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    Key::Right => {
                        let safe_pos = self.char_boundary_pos();
                        if safe_pos < self.text.len() {
                            let ch = self.text[safe_pos..].chars().next().unwrap();
                            self.cursor_pos = safe_pos + ch.len_utf8();
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    Key::Escape => {
                        self.is_open = false;
                        self.focused = false;
                        ctx.unregister_overlay();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Enter => {
                        if self.is_open {
                            if let Some(vi) = self.hover_index {
                                if let Some(&item_idx) = self.filtered_indices.get(vi) {
                                    let item = &self.items[item_idx];
                                    if !item.disabled {
                                        self.text = item.value.clone();
                                        self.cursor_pos = self.text.len();
                                        self.fire_change();
                                    }
                                }
                            }
                            self.is_open = false;
                            ctx.unregister_overlay();
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    Key::Down if self.is_open => {
                        let max = self.filtered_indices.len();
                        self.hover_index = Some(self.hover_index.map(|i| (i + 1).min(max - 1)).unwrap_or(0));
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Up if self.is_open => {
                        self.hover_index = self.hover_index.map(|i| if i > 0 { i - 1 } else { 0 });
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }
            Event::MouseWheel { delta, position, .. } if self.is_open => {
                let dd = self.dropdown_rect();
                if dd.contains(*position) {
                    let total_height = self.filtered_indices.len() as f32 * ITEM_HEIGHT;
                    let max_scroll = (total_height - self.effective_popup_height(self.filtered_indices.len())).max(0.0);
                    self.scroll_offset = (self.scroll_offset - delta).clamp(0.0, max_scroll);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn children(&self) -> &[ElementId] { &[] }
    fn bounds(&self) -> Rect { self.bounds }

    fn explicit_dimensions(&self, _parent_width: f32, _parent_height: f32) -> (Option<f32>, Option<f32>) {
        let w = self.width.map(|d| d.resolve(1000.0));
        (w, Some(INPUT_HEIGHT))
    }

    fn set_position(&mut self, pos: Point) { self.bounds.origin = pos; }
    fn mark_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags |= flags; }
    fn clear_dirty(&mut self, flags: DirtyFlags) { self.dirty_flags.remove(flags); }
    fn is_dirty(&self, flags: DirtyFlags) -> bool { self.dirty_flags.contains(flags) }
    fn id(&self) -> ElementId { self.id }
    fn set_id(&mut self, id: ElementId) { self.id = id; }
    fn mount(&mut self, _tree: &mut ElementTree) {}

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] { &self.classes }

    fn element_type_name(&self) -> &str { "Combobox" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = style.width() { self.width = Some(w); }
        if let Some(c) = style.get("--popup-background").and_then(|v| v.as_color()) { self.mss_popup_bg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-color").and_then(|v| v.as_color()) { self.mss_popup_fg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-accent").and_then(|v| v.as_color()) { self.mss_popup_accent = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-border").and_then(|v| v.as_color()) { self.mss_popup_border = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-hover-background").and_then(|v| v.as_color()) { self.mss_popup_hover_bg = Some(mss_color_to_core(c)); }
        if let Some(c) = style.get("--popup-hover-color").and_then(|v| v.as_color()) { self.mss_popup_hover_fg = Some(mss_color_to_core(c)); }
        if let Some(d) = style.get("--popup-max-height").and_then(|v| v.as_dimension()) { self.mss_popup_max_height = Some(d.resolve(1000.0)); }
        if let Some(d) = style.get("--popup-min-height").and_then(|v| v.as_dimension()) { self.mss_popup_min_height = Some(d.resolve(1000.0)); }
        if let Some(d) = style.get("--popup-min-width").and_then(|v| v.as_dimension()) { self.mss_popup_min_width = Some(d.resolve(1000.0)); }
        self.apply_style(style);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::ComboBox,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                value: Some(self.text.clone()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for ComboboxElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        if let Some(w) = style.width() { self.width = Some(w); }
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
