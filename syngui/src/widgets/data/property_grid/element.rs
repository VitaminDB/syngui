use super::{Property, PropertyGrid, PropertyValue};
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension};
use crate::mss::MssFields;
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widgets::input::color_picker::ColorValue;
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

impl Widget for PropertyGrid {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(PropertyGridElement {
            id: ElementId::new(),
            properties: self.properties.clone(),
            on_change: self.on_change.clone(),
            label_width: self.label_width,
            row_height: self.row_height,
            fixed_width: self.width,
            fixed_height: self.height,
            scroll_offset: 0.0,
            hover_row: None,
            editing_row: None,
            edit_buffer: String::new(),
            edit_cursor: 0,
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
            editable: self.editable,
            suggestions: self.suggestions.clone(),
            on_add: self.on_add.clone(),
            on_remove: self.on_remove.clone(),
            add_mode: false,
            add_buffer: String::new(),
            add_cursor: 0,
            add_filtered: Vec::new(),
            add_hover: None,
            add_dropdown_open: false,
            color_picker_row: None,
            cp_hue: 0.0,
            cp_sat: 0.0,
            cp_val: 0.0,
            cp_drag: CpDragTarget::None,
        })
    }

    fn can_update(&self, other: &dyn Any) -> bool { other.is::<Self>() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn mount(&self, _tree: &mut ElementTree, _parent_id: ElementId) {}
}

pub struct PropertyGridElement {
    id: ElementId,
    properties: Vec<Property>,
    on_change: Option<Arc<Mutex<dyn FnMut(usize, PropertyValue) + Send>>>,
    label_width: Option<f32>,
    row_height: f32,
    fixed_width: Option<Dimension>,
    fixed_height: Option<Dimension>,
    scroll_offset: f32,
    hover_row: Option<usize>,
    editing_row: Option<usize>,
    edit_buffer: String,
    edit_cursor: usize,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<Arc<dyn crate::widget::context::TextMeasure>>,
    editable: bool,
    suggestions: Vec<String>,
    on_add: Option<Arc<Mutex<dyn FnMut(&str, PropertyValue) + Send>>>,
    on_remove: Option<Arc<Mutex<dyn FnMut(usize, &str) + Send>>>,
    add_mode: bool,
    add_buffer: String,
    add_cursor: usize,
    add_filtered: Vec<usize>,
    add_hover: Option<usize>,
    add_dropdown_open: bool,
    color_picker_row: Option<usize>,
    cp_hue: f32,
    cp_sat: f32,
    cp_val: f32,
    cp_drag: CpDragTarget,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CpDragTarget {
    None,
    SvField,
    HueBar,
}

impl PropertyGridElement {
    fn content_height(&self) -> f32 {
        self.total_rows() as f32 * self.row_height
    }

    fn natural_width(&self) -> f32 {
        let font_size = self.mss.font_size_or(12.0);
        let label_w = self.label_width.unwrap_or(120.0);
        let padding = 40.0;

        let max_value_w = self.properties.iter().map(|p| {
            let text = p.value.display();
            let char_count = text.chars().count();
            if let Some(ref tm) = self.text_measure {
                tm.measure_text_width(&text, font_size, char_count)
            } else {
                char_count as f32 * font_size * 0.6
            }
        }).fold(0.0f32, f32::max);

        label_w + max_value_w + padding
    }

    fn max_scroll(&self) -> f32 {
        (self.content_height() - self.bounds.size.height).max(0.0)
    }

    fn label_w(&self) -> f32 {
        self.label_width.unwrap_or(self.bounds.size.width * 0.4)
    }

    fn row_at_y(&self, y: f32) -> Option<usize> {
        let local_y = y - self.bounds.y() + self.scroll_offset;
        if local_y < 0.0 { return None; }
        let idx = (local_y / self.row_height) as usize;
        if idx < self.properties.len() { Some(idx) } else { None }
    }

    fn fire_change(&self, idx: usize, value: PropertyValue) {
        if let Some(ref cb) = self.on_change {
            if let Ok(mut f) = cb.lock() { f(idx, value); }
        }
    }

    fn commit_edit(&mut self) {
        if let Some(idx) = self.editing_row {
            if idx < self.properties.len() {
                let new_value = match &self.properties[idx].value {
                    PropertyValue::Text(_) => PropertyValue::Text(self.edit_buffer.clone()),
                    PropertyValue::Number(_) => {
                        if let Ok(n) = self.edit_buffer.parse::<f64>() {
                            PropertyValue::Number(n)
                        } else {
                            return;
                        }
                    }
                    PropertyValue::Bool(_) => {
                        PropertyValue::Bool(
                            self.edit_buffer == "true" || self.edit_buffer == "1" || self.edit_buffer == "yes"
                        )
                    }
                    PropertyValue::Color(_) => {
                        let hex = self.edit_buffer.trim_start_matches('#');
                        if hex.len() == 6 {
                            if let (Ok(r), Ok(g), Ok(b)) = (
                                u8::from_str_radix(&hex[0..2], 16),
                                u8::from_str_radix(&hex[2..4], 16),
                                u8::from_str_radix(&hex[4..6], 16),
                            ) {
                                PropertyValue::Color(Color::new(
                                    r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0
                                ))
                            } else {
                                return;
                            }
                        } else {
                            return;
                        }
                    }
                    PropertyValue::Choice(items, _) => {
                        let selected = items.iter().position(|s| s == &self.edit_buffer).unwrap_or(0);
                        PropertyValue::Choice(items.clone(), selected)
                    }
                };
                self.fire_change(idx, new_value.clone());
                self.properties[idx].value = new_value;
            }
        }
        self.editing_row = None;
    }

    fn start_edit(&mut self, idx: usize) {
        if idx >= self.properties.len() { return; }
        self.editing_row = Some(idx);
        self.edit_buffer = self.properties[idx].value.display();
        self.edit_cursor = self.edit_buffer.len();
    }

    const CP_SV_SIZE: f32 = 140.0;
    const CP_HUE_W: f32 = 16.0;
    const CP_GAP: f32 = 8.0;
    const CP_PAD: f32 = 12.0;

    fn cp_popup_w(&self) -> f32 { Self::CP_PAD * 2.0 + Self::CP_SV_SIZE + Self::CP_GAP + Self::CP_HUE_W }
    fn cp_popup_h(&self) -> f32 { Self::CP_PAD * 2.0 + Self::CP_SV_SIZE }

    fn cp_popup_rect(&self, row_idx: usize) -> Rect {
        let row_y = self.bounds.y() + (row_idx as f32 * self.row_height) - self.scroll_offset;
        let label_w = self.label_w();
        let x = self.bounds.x() + label_w;
        let y = row_y + self.row_height + 2.0;
        Rect::new(Point::new(x, y), Size::new(self.cp_popup_w(), self.cp_popup_h()))
    }

    fn cp_sv_rect(&self, popup: Rect) -> Rect {
        Rect::new(
            Point::new(popup.x() + Self::CP_PAD, popup.y() + Self::CP_PAD),
            Size::new(Self::CP_SV_SIZE, Self::CP_SV_SIZE),
        )
    }

    fn cp_hue_rect(&self, popup: Rect) -> Rect {
        Rect::new(
            Point::new(popup.x() + Self::CP_PAD + Self::CP_SV_SIZE + Self::CP_GAP, popup.y() + Self::CP_PAD),
            Size::new(Self::CP_HUE_W, Self::CP_SV_SIZE),
        )
    }

    fn cp_open(&mut self, row_idx: usize) {
        if let PropertyValue::Color(c) = &self.properties[row_idx].value {
            let cv = ColorValue::from_color(*c);
            let (h, s, v) = cv.to_hsv();
            self.color_picker_row = Some(row_idx);
            self.cp_hue = h;
            self.cp_sat = s;
            self.cp_val = v;
        }
    }

    fn cp_close(&mut self) {
        self.color_picker_row = None;
        self.cp_drag = CpDragTarget::None;
    }

    fn cp_update_color(&mut self) {
        if let Some(idx) = self.color_picker_row {
            let cv = ColorValue::from_hsv(self.cp_hue, self.cp_sat, self.cp_val);
            let color = cv.to_color();
            let pv = PropertyValue::Color(color);
            self.fire_change(idx, pv.clone());
            self.properties[idx].value = pv;
        }
    }

    fn total_rows(&self) -> usize {
        self.properties.len() + if self.editable { 1 } else { 0 }
    }

    fn add_row_y(&self) -> f32 {
        let base_y = self.bounds.y().round();
        (base_y + (self.properties.len() as f32 * self.row_height) - self.scroll_offset).round()
    }

    fn filter_suggestions(&mut self) {
        let query = self.add_buffer.to_lowercase();
        let existing: Vec<String> = self.properties.iter().map(|p| p.name.to_lowercase()).collect();
        self.add_filtered = self.suggestions.iter().enumerate()
            .filter(|(_, s)| {
                let sl = s.to_lowercase();
                sl.contains(&query) && !existing.contains(&sl)
            })
            .map(|(i, _)| i)
            .collect();
        self.add_dropdown_open = !self.add_filtered.is_empty() && !self.add_buffer.is_empty();
        if let Some(h) = self.add_hover {
            if h >= self.add_filtered.len() {
                self.add_hover = if self.add_filtered.is_empty() { None } else { Some(self.add_filtered.len() - 1) };
            }
        }
    }

    fn fire_add(&self, name: &str, value: PropertyValue) {
        if let Some(ref cb) = self.on_add {
            if let Ok(mut f) = cb.lock() { f(name, value); }
        }
    }

    fn fire_remove(&self, idx: usize, name: &str) {
        if let Some(ref cb) = self.on_remove {
            if let Ok(mut f) = cb.lock() { f(idx, name); }
        }
    }

    fn commit_add(&mut self) {
        let name = self.add_buffer.trim().to_string();
        if name.is_empty() {
            self.add_mode = false;
            return;
        }
        let value = PropertyValue::Text(String::new());
        self.fire_add(&name, value.clone());
        self.properties.push(Property { name: name.clone(), value });
        self.add_buffer.clear();
        self.add_cursor = 0;
        self.add_mode = false;
        self.add_dropdown_open = false;
    }

    fn select_suggestion(&mut self, filtered_idx: usize) {
        if filtered_idx < self.add_filtered.len() {
            let suggestion_idx = self.add_filtered[filtered_idx];
            if let Some(name) = self.suggestions.get(suggestion_idx) {
                let name = name.clone();
                let value = PropertyValue::Text(String::new());
                self.fire_add(&name, value.clone());
                self.properties.push(Property { name, value });
            }
        }
        self.add_buffer.clear();
        self.add_cursor = 0;
        self.add_mode = false;
        self.add_dropdown_open = false;
    }

    const DELETE_BTN_SIZE: f32 = 16.0;
    const DROPDOWN_ITEM_H: f32 = 28.0;
    const DROPDOWN_MAX_H: f32 = 168.0;

    fn delete_btn_rect(&self, row_idx: usize) -> Rect {
        let base_y = self.bounds.y().round();
        let y = (base_y + (row_idx as f32 * self.row_height) - self.scroll_offset).round();
        let x = self.bounds.x() + self.bounds.size.width - Self::DELETE_BTN_SIZE - 8.0;
        let cy = y + (self.row_height - Self::DELETE_BTN_SIZE) / 2.0;
        Rect::new(Point::new(x, cy), Size::new(Self::DELETE_BTN_SIZE, Self::DELETE_BTN_SIZE))
    }

    fn dropdown_rect(&self) -> Rect {
        let add_y = self.add_row_y() + self.row_height;
        let count = self.add_filtered.len().min(6);
        let h = (count as f32 * Self::DROPDOWN_ITEM_H).min(Self::DROPDOWN_MAX_H);
        Rect::new(
            Point::new(self.bounds.x(), add_y),
            Size::new(self.bounds.size.width, h),
        )
    }

    fn draw_cp_sv_field(&self, list: &mut DisplayList, rect: Rect) {
        let steps = 14;
        let cell_w = rect.size.width / steps as f32;
        let cell_h = rect.size.height / steps as f32;
        for yi in 0..steps {
            for xi in 0..steps {
                let s = (xi as f32 + 0.5) / steps as f32;
                let v = 1.0 - (yi as f32 + 0.5) / steps as f32;
                let cv = ColorValue::from_hsv(self.cp_hue, s, v);
                let cell_rect = Rect::new(
                    Point::new(rect.x() + xi as f32 * cell_w, rect.y() + yi as f32 * cell_h),
                    Size::new(cell_w + 0.5, cell_h + 0.5),
                );
                list.push_rect(cell_rect, cv.to_color(), [0.0; 4]);
            }
        }
        let cx = rect.x() + self.cp_sat * rect.size.width;
        let cy = rect.y() + (1.0 - self.cp_val) * rect.size.height;
        let r = 5.0;
        let cur = Rect::new(Point::new(cx - r, cy - r), Size::new(r * 2.0, r * 2.0));
        list.push_rect_bordered(cur, Color::TRANSPARENT, [r; 4], Border::new(2.0, Color::WHITE));
        list.push_rect_bordered(cur, Color::TRANSPARENT, [r; 4], Border::new(1.0, Color::BLACK.with_alpha(0.3)));
    }

    fn draw_cp_hue_bar(&self, list: &mut DisplayList, rect: Rect) {
        let steps = 12;
        let cell_h = rect.size.height / steps as f32;
        let hues = [0.0, 30.0, 60.0, 90.0, 120.0, 150.0, 180.0, 210.0, 240.0, 270.0, 300.0, 330.0];
        for (i, &h) in hues.iter().enumerate() {
            let cv = ColorValue::from_hsv(h, 1.0, 1.0);
            let cell = Rect::new(
                Point::new(rect.x(), rect.y() + i as f32 * cell_h),
                Size::new(rect.size.width, cell_h + 0.5),
            );
            list.push_rect(cell, cv.to_color(), [0.0; 4]);
        }
        let cy = rect.y() + (self.cp_hue / 360.0) * rect.size.height;
        let cur = Rect::new(Point::new(rect.x() - 1.0, cy - 2.0), Size::new(rect.size.width + 2.0, 4.0));
        list.push_rect_bordered(cur, Color::TRANSPARENT, [2.0; 4], Border::new(2.0, Color::WHITE));
    }
}

impl Element for PropertyGridElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(pg) = widget.as_any().downcast_ref::<PropertyGrid>() {
            self.properties = pg.properties.clone();
            self.on_change = pg.on_change.clone();
            self.on_add = pg.on_add.clone();
            self.on_remove = pg.on_remove.clone();
            self.label_width = pg.label_width;
            self.row_height = pg.row_height;
            self.fixed_width = pg.width;
            self.fixed_height = pg.height;
            self.editable = pg.editable;
            self.suggestions = pg.suggestions.clone();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let min_w = self.mss.min_width.map(|d| d.resolve(constraints.max_width)).unwrap_or(300.0);
        let max_w = self.mss.max_width.map(|d| d.resolve(constraints.max_width)).unwrap_or(f32::INFINITY);
        let w = self.mss.width.or(self.fixed_width)
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or_else(|| self.natural_width().max(min_w))
            .clamp(min_w, max_w)
            .min(constraints.max_width);
        let natural_h = self.content_height();
        let h = self.mss.height.or(self.fixed_height).map(|d| d.resolve(constraints.max_height)).unwrap_or(natural_h.min(400.0)).min(constraints.max_height);
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let border_color = self.mss.border_color.unwrap_or(Color::from_hex("#E5E7EB"));
        let fg = self.mss.color.unwrap_or(Color::from_hex("#374151"));
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        list.push_rect_bordered(self.bounds, bg, [8.0; 4], Border::new(1.0, border_color));

        list.push_clip(self.bounds);

        let viewport_top = self.scroll_offset;
        let viewport_bottom = viewport_top + self.bounds.size.height;
        let first = (viewport_top / self.row_height) as usize;
        let last = ((viewport_bottom / self.row_height) as usize + 1).min(self.properties.len());

        let label_w = self.label_w();
        let value_x = self.bounds.x() + label_w;
        let value_w = self.bounds.size.width - label_w;

        let base_y = self.bounds.y().round();
        let base_x = self.bounds.x().round();

        for row_idx in first..last {
            let y = (base_y + (row_idx as f32 * self.row_height) - self.scroll_offset).round();
            let row_rect = Rect::new(
                Point::new(base_x, y),
                Size::new(self.bounds.size.width, self.row_height),
            );
            let row_bg = if self.hover_row == Some(row_idx) {
                bg.darken(0.03)
            } else if row_idx % 2 == 1 {
                bg.darken(0.015)
            } else {
                bg
            };
            list.push_rect(row_rect, row_bg, [0.0; 4]);
        }

        let div_x = (base_x + label_w).round();

        for row_idx in first..last {
            let y = (base_y + (row_idx as f32 * self.row_height) - self.scroll_offset).round();

            if row_idx + 1 < self.properties.len() {
                let next_y = (base_y + ((row_idx + 1) as f32 * self.row_height) - self.scroll_offset).round();
                list.push_rect(
                    Rect::new(Point::new(base_x, next_y), Size::new(self.bounds.size.width, 1.0)),
                    border_color,
                    [0.0; 4],
                );
            }

            list.push_rect(
                Rect::new(Point::new(div_x, y), Size::new(1.0, self.row_height)),
                border_color,
                [0.0; 4],
            );

            let prop = &self.properties[row_idx];

            let label_rect = Rect::new(
                Point::new(self.bounds.x() + 12.0, y + (self.row_height - 13.0) / 2.0),
                Size::new(label_w - 20.0, 14.0),
            );
            list.push_text(&prop.name, label_rect, fg.with_alpha(0.5), 12.0);

            let is_editing = self.editing_row == Some(row_idx);
            let value_rect = Rect::new(
                Point::new(value_x + 8.0, y + (self.row_height - 13.0) / 2.0),
                Size::new(value_w - 16.0, 14.0),
            );

            if is_editing {
                let edit_bg = Rect::new(
                    Point::new(value_x + 2.0, y + 2.0),
                    Size::new(value_w - 4.0, self.row_height - 4.0),
                );
                list.push_rect_bordered(edit_bg, bg, [4.0; 4], Border::new(2.0, primary));
                let edit_text_rect = Rect::new(
                    Point::new(value_x + 8.0, y + (self.row_height - 13.0) / 2.0),
                    Size::new(value_w - 16.0, 14.0),
                );
                list.push_text(&self.edit_buffer, edit_text_rect, fg, 12.0);

                list.push_text_cursor_styled(
                    &self.edit_buffer,
                    self.edit_cursor,
                    value_x + 8.0,
                    y + 6.0,
                    self.row_height - 12.0,
                    12.0,
                    self.mss.font_weight_or(400),
                    primary,
                    self.mss.font_family.clone(),
                );
            } else {
                match &prop.value {
                    PropertyValue::Color(c) => {
                        let swatch_rect = Rect::new(
                            Point::new(value_x + 8.0, y + (self.row_height - 16.0) / 2.0),
                            Size::new(16.0, 16.0),
                        );
                        list.push_rect(swatch_rect, *c, [3.0; 4]);
                        let text_rect = Rect::new(
                            Point::new(value_x + 30.0, y + (self.row_height - 13.0) / 2.0),
                            Size::new(value_w - 38.0, 14.0),
                        );
                        list.push_text(&prop.value.display(), text_rect, fg, 12.0);
                    }
                    PropertyValue::Bool(b) => {
                        let check_rect = Rect::new(
                            Point::new(value_x + 8.0, y + (self.row_height - 16.0) / 2.0),
                            Size::new(16.0, 16.0),
                        );
                        let check_bg = if *b { primary } else { bg };
                        list.push_rect_bordered(check_rect, check_bg, [3.0; 4], Border::new(1.0, border_color));
                        if *b {
                            list.push_text_centered("\u{2713}", check_rect, bg, 11.0);
                        }
                        let text_rect = Rect::new(
                            Point::new(value_x + 30.0, y + (self.row_height - 13.0) / 2.0),
                            Size::new(value_w - 38.0, 14.0),
                        );
                        list.push_text(&prop.value.display(), text_rect, fg, 12.0);
                    }
                    _ => {
                        list.push_text(&prop.value.display(), value_rect, fg, 12.0);
                    }
                }
            }
        }

        if self.editable {
            if let Some(hover) = self.hover_row {
                if hover < self.properties.len() {
                    let btn = self.delete_btn_rect(hover);
                    let btn_bg = fg.with_alpha(0.08);
                    list.push_rect(btn, btn_bg, [3.0; 4]);
                    let x_rect = Rect::new(
                        Point::new(btn.x() + 2.0, btn.y() + 1.0),
                        Size::new(Self::DELETE_BTN_SIZE - 4.0, Self::DELETE_BTN_SIZE - 2.0),
                    );
                    list.push_text_centered("\u{E5CD}", x_rect, fg.with_alpha(0.4), 10.0);
                }
            }
        }

        if self.editable {
            let add_y = self.add_row_y();
            list.push_rect(
                Rect::new(Point::new(base_x, add_y), Size::new(self.bounds.size.width, 1.0)),
                border_color,
                [0.0; 4],
            );

            if self.add_mode {
                let edit_bg = Rect::new(
                    Point::new(base_x + 2.0, add_y + 2.0),
                    Size::new(self.bounds.size.width - 4.0, self.row_height - 4.0),
                );
                list.push_rect_bordered(edit_bg, bg, [4.0; 4], Border::new(2.0, primary));
                let text_rect = Rect::new(
                    Point::new(base_x + 12.0, add_y + (self.row_height - 13.0) / 2.0),
                    Size::new(self.bounds.size.width - 24.0, 14.0),
                );
                list.push_text(&self.add_buffer, text_rect, fg, 12.0);
                list.push_text_cursor_styled(
                    &self.add_buffer,
                    self.add_cursor,
                    base_x + 12.0,
                    add_y + 6.0,
                    self.row_height - 12.0,
                    12.0,
                    self.mss.font_weight_or(400),
                    primary,
                    self.mss.font_family.clone(),
                );
            } else {
                let plus_rect = Rect::new(
                    Point::new(base_x + 12.0, add_y + (self.row_height - 13.0) / 2.0),
                    Size::new(self.bounds.size.width - 24.0, 14.0),
                );
                list.push_text("+", plus_rect, fg.with_alpha(0.3), 12.0);
            }
        }

        if self.content_height() > self.bounds.size.height {
            let track_h = self.bounds.size.height;
            let thumb_h = (track_h / self.content_height() * track_h).max(20.0);
            let max_s = self.max_scroll();
            let thumb_y = self.bounds.y() + if max_s > 0.0 {
                (self.scroll_offset / max_s) * (track_h - thumb_h)
            } else {
                0.0
            };
            let scrollbar_x = self.bounds.x() + self.bounds.size.width - 6.0;
            let thumb_rect = Rect::new(
                Point::new(scrollbar_x, thumb_y),
                Size::new(4.0, thumb_h),
            );
            list.push_rect(thumb_rect, fg.with_alpha(0.2), [2.0; 4]);
        }

        list.pop_clip();

        list.push_rect_bordered(self.bounds, Color::TRANSPARENT, [8.0; 4], Border::new(1.0, border_color));

        if self.add_dropdown_open && !self.add_filtered.is_empty() {
            let dd = self.dropdown_rect();
            list.begin_overlay();
            list.push_shadow(dd, Color::BLACK.with_alpha(0.1), 8.0, (0.0, 2.0), [6.0; 4]);
            list.push_rect_bordered(dd, bg, [6.0; 4], Border::new(1.0, border_color));

            for (fi, &si) in self.add_filtered.iter().enumerate() {
                if fi >= 6 { break; }
                let item_y = dd.y() + fi as f32 * Self::DROPDOWN_ITEM_H;
                let item_rect = Rect::new(
                    Point::new(dd.x(), item_y),
                    Size::new(dd.size.width, Self::DROPDOWN_ITEM_H),
                );
                if self.add_hover == Some(fi) {
                    list.push_rect(item_rect, primary.with_alpha(0.1), [0.0; 4]);
                }
                if let Some(name) = self.suggestions.get(si) {
                    let text_rect = Rect::new(
                        Point::new(dd.x() + 12.0, item_y + (Self::DROPDOWN_ITEM_H - 13.0) / 2.0),
                        Size::new(dd.size.width - 24.0, 14.0),
                    );
                    let text_color = if self.add_hover == Some(fi) { primary } else { fg };
                    list.push_text(name, text_rect, text_color, 12.0);
                }
            }

            list.end_overlay();
        }

        if let Some(row_idx) = self.color_picker_row {
            let popup = self.cp_popup_rect(row_idx);
            list.begin_overlay();
            list.push_shadow(popup, Color::BLACK.with_alpha(0.15), 12.0, (0.0, 4.0), [8.0; 4]);
            list.push_rect_bordered(popup, bg, [8.0; 4], Border::new(1.0, border_color));
            let sv = self.cp_sv_rect(popup);
            list.push_rect_bordered(sv, Color::TRANSPARENT, [4.0; 4], Border::new(1.0, border_color));
            self.draw_cp_sv_field(list, sv);
            let hue = self.cp_hue_rect(popup);
            list.push_rect_bordered(hue, Color::TRANSPARENT, [4.0; 4], Border::new(1.0, border_color));
            self.draw_cp_hue_bar(list, hue);
            list.end_overlay();
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                if self.cp_drag != CpDragTarget::None {
                    if let Some(row_idx) = self.color_picker_row {
                        let popup = self.cp_popup_rect(row_idx);
                        match self.cp_drag {
                            CpDragTarget::SvField => {
                                let sv = self.cp_sv_rect(popup);
                                self.cp_sat = ((pos.x - sv.x()) / sv.size.width).clamp(0.0, 1.0);
                                self.cp_val = 1.0 - ((pos.y - sv.y()) / sv.size.height).clamp(0.0, 1.0);
                                self.cp_update_color();
                            }
                            CpDragTarget::HueBar => {
                                let hb = self.cp_hue_rect(popup);
                                self.cp_hue = ((pos.y - hb.y()) / hb.size.height).clamp(0.0, 1.0) * 360.0;
                                self.cp_update_color();
                            }
                            CpDragTarget::None => {}
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                if let Some(row_idx) = self.color_picker_row {
                    let popup = self.cp_popup_rect(row_idx);
                    if popup.contains(*pos) {
                        ctx.set_cursor(CursorIcon::Crosshair);
                        return EventResult::Handled;
                    }
                }

                if self.add_dropdown_open {
                    let dd = self.dropdown_rect();
                    if dd.contains(*pos) {
                        let fi = ((pos.y - dd.y()) / Self::DROPDOWN_ITEM_H) as usize;
                        let new_hover = if fi < self.add_filtered.len() { Some(fi) } else { None };
                        if new_hover != self.add_hover {
                            self.add_hover = new_hover;
                            ctx.request_paint();
                        }
                        ctx.set_cursor(CursorIcon::Pointer);
                        return EventResult::Handled;
                    }
                }

                if !self.bounds.contains(*pos) {
                    if self.hover_row.is_some() {
                        self.hover_row = None;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    return EventResult::Ignored;
                }

                let new_hover = self.row_at_y(pos.y);
                if new_hover != self.hover_row {
                    self.hover_row = new_hover;
                    ctx.request_paint();
                }
                ctx.set_cursor(CursorIcon::Pointer);
                EventResult::Handled
            }
            Event::MouseUp { button, .. } if *button == MouseButton::Left => {
                if self.cp_drag != CpDragTarget::None {
                    self.cp_drag = CpDragTarget::None;
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if self.add_dropdown_open {
                    let dd = self.dropdown_rect();
                    if dd.contains(*position) {
                        let fi = ((position.y - dd.y()) / Self::DROPDOWN_ITEM_H) as usize;
                        if fi < self.add_filtered.len() {
                            self.select_suggestion(fi);
                            ctx.request_paint();
                        }
                        return EventResult::Handled;
                    }
                    self.add_dropdown_open = false;
                    ctx.request_paint();
                }

                if self.editable {
                    if let Some(hover) = self.hover_row {
                        if hover < self.properties.len() {
                            let btn = self.delete_btn_rect(hover);
                            if btn.contains(*position) {
                                let name = self.properties[hover].name.clone();
                                self.fire_remove(hover, &name);
                                self.properties.remove(hover);
                                self.hover_row = None;
                                ctx.request_paint();
                                return EventResult::Handled;
                            }
                        }
                    }
                }

                if self.editable {
                    let add_y = self.add_row_y();
                    let add_rect = Rect::new(
                        Point::new(self.bounds.x(), add_y),
                        Size::new(self.bounds.size.width, self.row_height),
                    );
                    if add_rect.contains(*position) {
                        if !self.add_mode {
                            self.add_mode = true;
                            self.add_buffer.clear();
                            self.add_cursor = 0;
                            self.add_filtered.clear();
                            self.add_hover = None;
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                if let Some(row_idx) = self.color_picker_row {
                    let popup = self.cp_popup_rect(row_idx);

                    let sv = self.cp_sv_rect(popup);
                    if sv.contains(*position) {
                        self.cp_drag = CpDragTarget::SvField;
                        self.cp_sat = ((position.x - sv.x()) / sv.size.width).clamp(0.0, 1.0);
                        self.cp_val = 1.0 - ((position.y - sv.y()) / sv.size.height).clamp(0.0, 1.0);
                        self.cp_update_color();
                        ctx.request_paint();
                        return EventResult::Handled;
                    }

                    let hb = self.cp_hue_rect(popup);
                    if hb.contains(*position) {
                        self.cp_drag = CpDragTarget::HueBar;
                        self.cp_hue = ((position.y - hb.y()) / hb.size.height).clamp(0.0, 1.0) * 360.0;
                        self.cp_update_color();
                        ctx.request_paint();
                        return EventResult::Handled;
                    }

                    if popup.contains(*position) {
                        return EventResult::Handled;
                    }

                    self.cp_close();
                    ctx.unregister_overlay();
                    ctx.request_paint();
                }

                if !self.bounds.contains(*position) {
                    if self.editing_row.is_some() {
                        self.commit_edit();
                        ctx.request_paint();
                    }
                    return EventResult::Ignored;
                }

                if let Some(row_idx) = self.row_at_y(position.y) {
                    let label_w = self.label_w();
                    let value_x = self.bounds.x() + label_w;

                    if position.x >= value_x {
                        if let PropertyValue::Bool(b) = &self.properties[row_idx].value {
                            let new_val = !b;
                            let pv = PropertyValue::Bool(new_val);
                            self.fire_change(row_idx, pv.clone());
                            self.properties[row_idx].value = pv;
                            ctx.request_paint();
                            return EventResult::Handled;
                        }

                        if let PropertyValue::Choice(items, idx) = &self.properties[row_idx].value {
                            let new_idx = (idx + 1) % items.len();
                            let pv = PropertyValue::Choice(items.clone(), new_idx);
                            self.fire_change(row_idx, pv.clone());
                            self.properties[row_idx].value = pv;
                            ctx.request_paint();
                            return EventResult::Handled;
                        }

                        if matches!(&self.properties[row_idx].value, PropertyValue::Color(_)) {
                            if self.color_picker_row == Some(row_idx) {
                                self.cp_close();
                                ctx.unregister_overlay();
                            } else {
                                self.cp_open(row_idx);
                                let popup = self.cp_popup_rect(row_idx);
                                let overlay = Rect::new(
                                    Point::new(popup.x(), self.bounds.y()),
                                    Size::new(popup.size.width, popup.y() + popup.size.height - self.bounds.y()),
                                );
                                ctx.register_overlay(overlay, false);
                            }
                            ctx.request_paint();
                            return EventResult::Handled;
                        }

                        if self.editing_row.is_some() {
                            self.commit_edit();
                        }
                        self.start_edit(row_idx);
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }

                if self.editing_row.is_some() {
                    self.commit_edit();
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::MouseWheel { delta, position, .. } => {
                if !self.bounds.contains(*position) { return EventResult::Ignored; }
                let scroll_amount = *delta;
                let new_offset = (self.scroll_offset - scroll_amount).clamp(0.0, self.max_scroll());
                if (new_offset - self.scroll_offset).abs() > 0.01 {
                    self.scroll_offset = new_offset;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::CharInput(ch) if self.add_mode => {
                if !ch.is_control() && !ctx.modifiers.ctrl {
                    self.add_buffer.insert(self.add_cursor, *ch);
                    self.add_cursor += ch.len_utf8();
                    self.filter_suggestions();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(key) if self.add_mode => {
                match key {
                    crate::input::Key::Backspace => {
                        if self.add_cursor > 0 {
                            let prev = self.add_buffer[..self.add_cursor]
                                .char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
                            self.add_buffer.remove(prev);
                            self.add_cursor = prev;
                            self.filter_suggestions();
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    crate::input::Key::Enter => {
                        if let Some(hi) = self.add_hover {
                            self.select_suggestion(hi);
                        } else {
                            self.commit_add();
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    crate::input::Key::Escape => {
                        self.add_mode = false;
                        self.add_dropdown_open = false;
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    crate::input::Key::Down => {
                        if self.add_dropdown_open {
                            let max = self.add_filtered.len().saturating_sub(1);
                            self.add_hover = Some(self.add_hover.map(|h| (h + 1).min(max)).unwrap_or(0));
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    crate::input::Key::Up => {
                        if self.add_dropdown_open {
                            self.add_hover = self.add_hover.map(|h| h.saturating_sub(1));
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    crate::input::Key::Left => {
                        if self.add_cursor > 0 {
                            self.add_cursor = self.add_buffer[..self.add_cursor]
                                .char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    crate::input::Key::Right => {
                        if self.add_cursor < self.add_buffer.len() {
                            let ch = self.add_buffer[self.add_cursor..].chars().next().unwrap();
                            self.add_cursor += ch.len_utf8();
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }
            Event::CharInput(ch) if self.editing_row.is_some() => {
                if !ch.is_control() && !ctx.modifiers.ctrl {
                    self.edit_buffer.insert(self.edit_cursor, *ch);
                    self.edit_cursor += ch.len_utf8();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(key) if self.editing_row.is_some() => {
                match key {
                    crate::input::Key::Backspace => {
                        if self.edit_cursor > 0 {
                            let prev = self.edit_buffer[..self.edit_cursor]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            self.edit_buffer.remove(prev);
                            self.edit_cursor = prev;
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    crate::input::Key::Delete => {
                        if self.edit_cursor < self.edit_buffer.len() {
                            self.edit_buffer.remove(self.edit_cursor);
                            ctx.request_paint();
                        }
                        EventResult::Handled
                    }
                    crate::input::Key::Home => {
                        self.edit_cursor = 0;
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    crate::input::Key::End => {
                        self.edit_cursor = self.edit_buffer.len();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    crate::input::Key::Left => {
                        if self.edit_cursor > 0 {
                            self.edit_cursor = self.edit_buffer[..self.edit_cursor]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    crate::input::Key::Right => {
                        if self.edit_cursor < self.edit_buffer.len() {
                            let ch = self.edit_buffer[self.edit_cursor..].chars().next().unwrap();
                            self.edit_cursor += ch.len_utf8();
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    crate::input::Key::Escape => {
                        self.editing_row = None;
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    crate::input::Key::Enter => {
                        self.commit_edit();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }
            Event::FocusLost => {
                if self.editing_row.is_some() {
                    self.commit_edit();
                    ctx.request_paint();
                }
                if self.color_picker_row.is_some() {
                    self.cp_close();
                    ctx.unregister_overlay();
                    ctx.request_paint();
                }
                EventResult::Handled
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

    fn element_type_name(&self) -> &str { "PropertyGrid" }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = style.width() { self.fixed_width = Some(w); }
        if let Some(h) = style.height() { self.fixed_height = Some(h); }
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

impl StyledElement for PropertyGridElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn classes(&self) -> &[String] { &self.classes }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }
}
