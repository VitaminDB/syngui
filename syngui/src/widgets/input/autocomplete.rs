use crate::animation::transition::mss_color_to_core;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::selection::TextSelectionState;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

pub struct Autocomplete {
    suggestions: Vec<String>,
    text: String,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    on_select: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    width: Option<Dimension>,
    min_chars: usize,
}

impl Autocomplete {
    pub fn new(suggestions: Vec<String>) -> Self {
        Self {
            suggestions,
            text: String::new(),
            placeholder: String::new(),
            on_change: None,
            on_select: None,
            width: None,
            min_chars: 1,
        }
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

    pub fn on_select(mut self, f: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_select = Some(Arc::new(Mutex::new(f)));
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(Dimension::Px(w));
        self
    }

    pub fn min_chars(mut self, n: usize) -> Self {
        self.min_chars = n;
        self
    }
}

impl Widget for Autocomplete {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(AutocompleteElement {
            id: ElementId::new(),
            suggestions: self.suggestions.clone(),
            text: self.text.clone(),
            placeholder: self.placeholder.clone(),
            on_change: self.on_change.clone(),
            on_select: self.on_select.clone(),
            width: self.width,
            min_chars: self.min_chars,
            filtered: Vec::new(),
            is_open: false,
            hover_index: None,
            focused: false,
            cursor_pos: self.text.len(),
            selection: TextSelectionState::new(),
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

pub struct AutocompleteElement {
    id: ElementId,
    suggestions: Vec<String>,
    text: String,
    placeholder: String,
    on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    on_select: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    width: Option<Dimension>,
    min_chars: usize,
    filtered: Vec<usize>,
    is_open: bool,
    hover_index: Option<usize>,
    focused: bool,
    cursor_pos: usize,
    selection: TextSelectionState,
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
}

impl AutocompleteElement {
    fn char_idx_to_byte(&self, char_idx: usize) -> usize {
        self.text.char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    fn hit_test_cursor(&self, rel_x: f32, ctx: &EventContext) -> usize {
        ctx.hit_test_char(&self.text, 14.0, rel_x)
            .map(|char_idx| self.char_idx_to_byte(char_idx))
            .unwrap_or(self.text.len())
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos = self.text[..self.cursor_pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.text.len() {
            let ch = self.text[self.cursor_pos..].chars().next().unwrap();
            self.cursor_pos += ch.len_utf8();
        }
    }

    fn effective_popup_height(&self, item_count: usize) -> f32 {
        let content_h = item_count as f32 * ITEM_HEIGHT;
        let max_h = self.mss_popup_max_height.unwrap_or(MAX_DROPDOWN_HEIGHT);
        let min_h = self.mss_popup_min_height.unwrap_or(0.0);
        content_h.min(max_h).max(min_h)
    }

    fn update_filter(&mut self) {
        if self.text.len() < self.min_chars {
            self.filtered.clear();
            self.is_open = false;
            return;
        }
        let query = self.text.to_lowercase();
        self.filtered = self.suggestions.iter().enumerate()
            .filter(|(_, s)| s.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();
        self.is_open = !self.filtered.is_empty();
    }

    fn dropdown_rect(&self) -> Rect {
        let count = self.filtered.len();
        let h = self.effective_popup_height(count);
        let popup_gap = 4.0;
        let y = if self.opens_upward {
            self.bounds.y() - h - popup_gap
        } else {
            self.bounds.y() + INPUT_HEIGHT + popup_gap
        };
        Rect::new(
            Point::new(self.bounds.x(), y),
            Size::new(self.bounds.size.width, h),
        )
    }

    fn fire_change(&self) {
        if let Some(ref cb) = self.on_change {
            if let Ok(mut f) = cb.lock() { f(&self.text); }
        }
    }

    fn fire_select(&self, value: &str) {
        if let Some(ref cb) = self.on_select {
            if let Ok(mut f) = cb.lock() { f(value); }
        }
    }
}

impl Element for AutocompleteElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(ac) = widget.as_any().downcast_ref::<Autocomplete>() {
            self.suggestions = ac.suggestions.clone();
            self.text = ac.text.clone();
            self.placeholder = ac.placeholder.clone();
            self.on_change = ac.on_change.clone();
            self.on_select = ac.on_select.clone();
            self.width = ac.width;
            self.min_chars = ac.min_chars;
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
        let fg = self.mss.color.unwrap_or_else(|| Color::from_hex("#1F2937"));
        let border_base = self.mss.border_color.unwrap_or_else(|| Color::from_hex("#D1D5DB"));
        let accent = self.mss.border_color.map(|c| c.lighten(0.3)).unwrap_or_else(|| self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6")));
        let placeholder_color = if self.mss.color.is_some() { fg.with_alpha(0.5) } else { Color::from_hex("#9CA3AF") };

        let border_color = if self.focused { accent } else { border_base };

        list.push_rect_bordered(
            self.bounds, bg, [8.0; 4],
            Border::new(if self.focused { 2.0 } else { 1.0 }, border_color),
        );

        let text_rect = Rect::new(
            Point::new(self.bounds.x() + 12.0, self.bounds.y() + (INPUT_HEIGHT - 14.0) / 2.0),
            Size::new(self.bounds.size.width - 24.0, 16.0),
        );
        if self.text.is_empty() {
            list.push_text(&self.placeholder, text_rect, placeholder_color, 14.0);
        } else {
            if let Some((sel_start, sel_end)) = self.selection.range(self.cursor_pos) {
                let sel_color = self.mss.selection_color_or_default();
                list.push_text_selection_styled(
                    &self.text, sel_start, sel_end,
                    self.bounds.x() + 12.0, self.bounds.y() + 10.0,
                    INPUT_HEIGHT - 20.0, 14.0, sel_color,
                    self.mss.font_family.clone(),
                );
            }
            list.push_text(&self.text, text_rect, fg, 14.0);
        }

        if self.focused {
            let caret = self.mss.caret_color_or(accent);
            list.push_text_cursor_styled(
                &self.text, self.cursor_pos,
                self.bounds.x() + 12.0, self.bounds.y() + 10.0,
                INPUT_HEIGHT - 20.0, 14.0, self.mss.font_weight_or(400), caret,
                self.mss.font_family.clone(),
            );
        }

        let icon_rect = Rect::new(
            Point::new(self.bounds.x() + self.bounds.size.width - 28.0, self.bounds.y() + (INPUT_HEIGHT - 14.0) / 2.0),
            Size::new(16.0, 14.0),
        );
        list.push_text("🔍", icon_rect, placeholder_color, 12.0);

        if self.is_open && !self.filtered.is_empty() {
            let dd = self.dropdown_rect();
            let popup_bg = self.mss_popup_bg.unwrap_or(bg);
            let popup_fg = self.mss_popup_fg.unwrap_or(fg);
            let _popup_accent = self.mss_popup_accent.unwrap_or(accent);
            let popup_border_color = self.mss_popup_border.unwrap_or(
                if self.mss.border_color.is_some() { border_base } else { Color::from_hex("#E5E7EB") }
            );
            let popup_hover_bg = self.mss_popup_hover_bg.unwrap_or_else(|| popup_bg.darken(0.05));

            let menu_radius: f32 = 8.0;
            let menu_bw: f32 = 1.0;
            list.begin_overlay();
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
            for (vi, _) in self.filtered.iter().enumerate() {
                let y = dd.y() + vi as f32 * ITEM_HEIGHT - self.scroll_offset;
                if y + ITEM_HEIGHT <= dd_top { continue; }
                if y >= dd_bottom { break; }
                if first_visible.is_none() { first_visible = Some(vi); }
                last_visible = Some(vi);
            }

            for (vi, &suggestion_idx) in self.filtered.iter().enumerate() {
                let y = dd.y() + vi as f32 * ITEM_HEIGHT - self.scroll_offset;
                if y + ITEM_HEIGHT < dd_top || y > dd_bottom { continue; }

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

                let text = &self.suggestions[suggestion_idx];
                let ir = Rect::new(
                    Point::new(dd.x() + 12.0, y + (ITEM_HEIGHT - 14.0) / 2.0),
                    Size::new(dd.size.width - 24.0, 16.0),
                );
                let is_hover = self.hover_index == Some(vi);
                let text_color = if is_hover {
                    self.mss_popup_hover_fg.unwrap_or(popup_fg)
                } else {
                    popup_fg
                };
                list.push_text(text, ir, text_color, 14.0);
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
                self.is_open = false;
                self.selection.clear();
                ctx.unregister_overlay();
                ctx.request_paint();
                return EventResult::Handled;
            }
            Event::MouseMove(pos) => {
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Text);
                }
                if self.selection.mouse_selecting && self.focused {
                    let text_x = self.bounds.x() + 12.0;
                    let rel_x = (pos.x - text_x).max(0.0);
                    self.cursor_pos = self.hit_test_cursor(rel_x, ctx);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.bounds.contains(*pos) {
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
                    let new_pos = self.hit_test_cursor(rel_x, ctx);

                    if ctx.modifiers.shift {
                        self.selection.extend_or_start(self.cursor_pos);
                        self.cursor_pos = new_pos;
                    } else {
                        self.selection.clear();
                        self.cursor_pos = new_pos;
                        self.selection.start(new_pos);
                        self.selection.mouse_selecting = true;
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.is_open {
                    let dd = self.dropdown_rect();
                    if dd.contains(*position) {
                        let vi = ((position.y - dd.y() + self.scroll_offset) / ITEM_HEIGHT) as usize;
                        if let Some(&idx) = self.filtered.get(vi) {
                            let selected = self.suggestions[idx].clone();
                            self.text = selected.clone();
                            self.cursor_pos = self.text.len();
                            self.selection.clear();
                            self.is_open = false;
                            ctx.unregister_overlay();
                            self.fire_select(&selected);
                            self.fire_change();
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    self.is_open = false;
                    self.focused = false;
                    self.selection.clear();
                    ctx.unregister_overlay();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.focused {
                    self.focused = false;
                    self.is_open = false;
                    self.selection.clear();
                    ctx.unregister_overlay();
                    ctx.request_paint();
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } => {
                if *button == MouseButton::Left && self.selection.mouse_selecting {
                    self.selection.mouse_selecting = false;
                    if !self.selection.has_selection(self.cursor_pos) {
                        self.selection.anchor = None;
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::CharInput(ch) if self.focused => {
                if ch.is_control() || ctx.modifiers.ctrl { return EventResult::Ignored; }
                let mut ch_buf = [0u8; 4];
                let ch_str = ch.encode_utf8(&mut ch_buf);
                self.selection.replace_selection(&mut self.text, &mut self.cursor_pos, ch_str);
                self.update_filter();
                if self.is_open {
                    let popup_gap = 4.0;
                    let dd_h = self.effective_popup_height(self.filtered.len());
                    self.opens_upward = self.bounds.y() + INPUT_HEIGHT + dd_h + popup_gap > ctx.viewport_size().height
                        && self.bounds.y() >= dd_h + popup_gap;
                    let dd = self.dropdown_rect();
                    let overlay_bounds = if self.opens_upward {
                        Rect::new(
                            Point::new(self.bounds.x(), dd.y()),
                            Size::new(self.bounds.size.width, dd.size.height + popup_gap + INPUT_HEIGHT),
                        )
                    } else {
                        Rect::new(
                            self.bounds.origin,
                            Size::new(self.bounds.size.width, INPUT_HEIGHT + popup_gap + dd.size.height),
                        )
                    };
                    ctx.register_overlay(overlay_bounds, false);
                }
                self.hover_index = None;
                self.fire_change();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyDown(key) if self.focused => {
                let shift = ctx.modifiers.shift;
                let ctrl = ctx.modifiers.ctrl;

                if ctrl && matches!(key, Key::A) {
                    self.selection.select_all();
                    self.cursor_pos = self.text.len();
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                match key {
                    Key::Backspace => {
                        if self.selection.delete_selection(&mut self.text, &mut self.cursor_pos) {
                            self.update_filter();
                            self.fire_change();
                            ctx.request_paint();
                        } else if self.cursor_pos > 0 {
                            let prev = self.text[..self.cursor_pos]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            self.text.remove(prev);
                            self.cursor_pos = prev;
                            self.update_filter();
                            self.fire_change();
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    Key::Delete => {
                        if self.selection.delete_selection(&mut self.text, &mut self.cursor_pos) {
                            self.update_filter();
                            self.fire_change();
                        } else if self.cursor_pos < self.text.len() && self.text.is_char_boundary(self.cursor_pos) {
                            self.text.remove(self.cursor_pos);
                            self.update_filter();
                            self.fire_change();
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Left => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_pos);
                            self.move_cursor_left();
                        } else if self.selection.has_selection(self.cursor_pos) {
                            if let Some((start, _)) = self.selection.range(self.cursor_pos) {
                                self.cursor_pos = start;
                            }
                            self.selection.clear();
                        } else {
                            self.move_cursor_left();
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Right => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_pos);
                            self.move_cursor_right();
                        } else if self.selection.has_selection(self.cursor_pos) {
                            if let Some((_, end)) = self.selection.range(self.cursor_pos) {
                                self.cursor_pos = end;
                            }
                            self.selection.clear();
                        } else {
                            self.move_cursor_right();
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Home => {
                        if shift { self.selection.extend_or_start(self.cursor_pos); } else { self.selection.clear(); }
                        self.cursor_pos = 0;
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::End => {
                        if shift { self.selection.extend_or_start(self.cursor_pos); } else { self.selection.clear(); }
                        self.cursor_pos = self.text.len();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Escape => {
                        self.is_open = false;
                        self.focused = false;
                        self.selection.clear();
                        ctx.unregister_overlay();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Enter => {
                        if self.is_open {
                            if let Some(vi) = self.hover_index {
                                if let Some(&idx) = self.filtered.get(vi) {
                                    let selected = self.suggestions[idx].clone();
                                    self.text = selected.clone();
                                    self.cursor_pos = self.text.len();
                                    self.selection.clear();
                                    self.fire_select(&selected);
                                    self.fire_change();
                                }
                            }
                            self.is_open = false;
                            ctx.unregister_overlay();
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    Key::Down if self.is_open => {
                        let max = self.filtered.len();
                        if max > 0 {
                            self.hover_index = Some(self.hover_index.map(|i| (i + 1).min(max - 1)).unwrap_or(0));
                            ctx.request_paint();
                        }
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
                    let total_height = self.filtered.len() as f32 * ITEM_HEIGHT;
                    let max_scroll = (total_height - self.effective_popup_height(self.filtered.len())).max(0.0);
                    self.scroll_offset = (self.scroll_offset - delta).clamp(0.0, max_scroll);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::ImeCommit(text) if self.focused => {
                self.selection.replace_selection(&mut self.text, &mut self.cursor_pos, text);
                self.update_filter();
                self.hover_index = None;
                self.fire_change();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImePreedit { .. } if self.focused => EventResult::Handled,
            Event::ImeEnabled | Event::ImeDisabled if self.focused => EventResult::Handled,
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

    fn element_type_name(&self) -> &str { "Autocomplete" }

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

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::ComboBox,
            state: crate::a11y::NodeState {
                focused: self.focused,
                expanded: Some(self.is_open),
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                value: if self.text.is_empty() { None } else { Some(self.text.clone()) },
                placeholder: if self.placeholder.is_empty() { None } else { Some(self.placeholder.clone()) },
                ..Default::default()
            },
        })
    }
}

impl StyledElement for AutocompleteElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
