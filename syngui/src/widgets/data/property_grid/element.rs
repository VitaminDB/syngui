use super::{Property, PropertyGrid, PropertyValue};
use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::MssFields;
use crate::mss::{ComputedStyle, Dimension, TextAlign, TextDecoration};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use crate::widgets::input::color_picker::ColorValue;
use std::any::Any;
use std::sync::Arc;

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
            auto_height: self.auto_height,
            scroll_offset: 0.0,
            hover_row: None,
            editing_row: None,
            edit_buffer: String::new(),
            edit_original: String::new(),
            edit_cursor: 0,
            edit_sel_anchor: None,
            edit_selecting: false,
            edit_scroll_x: 0.0,
            edit_scroll_y: 0.0,
            edit_preferred_x: None,
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            text_measure: None,
            editable: self.editable,
            suggestions: self.suggestions.clone(),
            on_add: self.on_add.clone(),
            on_remove: self.on_remove.clone(),
            on_mark: self.on_mark.clone(),
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
            range_drag: None,
            choice_row: None,
            choice_hover: None,
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

pub struct PropertyGridElement {
    id: ElementId,
    properties: Vec<Property>,
    on_change: Option<Arc<Mutex<dyn FnMut(usize, PropertyValue) + Send>>>,
    label_width: Option<f32>,
    row_height: f32,
    fixed_width: Option<Dimension>,
    fixed_height: Option<Dimension>,
    auto_height: bool,
    scroll_offset: f32,
    hover_row: Option<usize>,
    editing_row: Option<usize>,
    edit_buffer: String,
    edit_original: String,
    edit_cursor: usize,
    edit_sel_anchor: Option<usize>,
    edit_selecting: bool,
    edit_scroll_x: f32,
    edit_scroll_y: f32,
    edit_preferred_x: Option<f32>,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<Arc<dyn crate::widget::context::TextMeasure>>,
    editable: bool,
    suggestions: Vec<String>,
    on_add: Option<Arc<Mutex<dyn FnMut(&str, PropertyValue) + Send>>>,
    on_remove: Option<Arc<Mutex<dyn FnMut(usize, &str) + Send>>>,
    on_mark: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
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
    /// Протяжка числа: строка, x нажатия, значение при нажатии, был ли сдвиг.
    range_drag: Option<(usize, f32, f64, bool)>,
    /// Открытый список выбора: строка и пункт под курсором.
    choice_row: Option<usize>,
    choice_hover: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CpDragTarget {
    None,
    SvField,
    HueBar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VisualLine {
    start: usize,
    end: usize,
}

impl PropertyGridElement {
    const MULTILINE_FONT_SIZE: f32 = 12.0;
    const MULTILINE_LINE_H: f32 = 17.0;
    const MULTILINE_PAD_Y: f32 = 6.0;
    const MULTILINE_MIN_LINES: usize = 2;
    const MULTILINE_MAX_LINES: usize = 8;

    fn is_row_multiline(&self, row: usize) -> bool {
        self.properties
            .get(row)
            .is_some_and(|p| matches!(p.value, PropertyValue::Multiline(_)))
    }

    fn is_editing_multiline(&self) -> bool {
        self.editing_row
            .is_some_and(|r| self.is_row_multiline(r))
    }

    fn measure_width(&self, text: &str, font_size: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let weight = self.mss.font_weight_or(400);
        let ff = self.mss.font_family.as_deref();
        let chars = text.chars().count();
        if let Some(ref tm) = self.text_measure {
            tm.measure_text_width_weight(text, font_size, chars, weight, ff)
        } else {
            chars as f32 * font_size * 0.58
        }
    }

    fn hit_test_line_byte(&self, line: &str, x_offset: f32, font_size: f32) -> usize {
        if x_offset <= 0.0 || line.is_empty() {
            return 0;
        }
        let weight = self.mss.font_weight_or(400);
        let ff = self.mss.font_family.as_deref();
        let char_idx = if let Some(ref tm) = self.text_measure {
            tm.hit_test_char_weight(line, font_size, x_offset, weight, ff)
        } else {
            ((x_offset / (font_size * 0.58)).round() as usize).min(line.chars().count())
        };
        line.char_indices()
            .nth(char_idx)
            .map(|(b, _)| b)
            .unwrap_or(line.len())
    }

    fn value_inner_w(&self) -> f32 {
        let total_w = if self.bounds.size.width > 1.0 {
            self.bounds.size.width
        } else {
            300.0
        };
        let label_w = self.label_width.unwrap_or(total_w * 0.4);
        (total_w - label_w - 16.0).max(80.0)
    }

    fn wrap_lines(&self, text: &str, max_w: f32) -> Vec<VisualLine> {
        let mut out = Vec::new();
        let max_w = max_w.max(24.0);
        let mut para_start = 0usize;
        for para in text.split('\n') {
            let para_end = para_start + para.len();
            if para.is_empty() {
                out.push(VisualLine {
                    start: para_start,
                    end: para_end,
                });
            } else {
                let mut line_start = 0usize;
                let mut last_space: Option<usize> = None;
                for (ci, ch) in para.char_indices() {
                    let char_end = ci + ch.len_utf8();
                    let w = self.measure_width(&para[line_start..char_end], Self::MULTILINE_FONT_SIZE);
                    if w > max_w && ci > line_start {
                        let break_at = if let Some(sp) = last_space.filter(|&s| s > line_start) {
                            sp
                        } else {
                            ci
                        };
                        out.push(VisualLine {
                            start: para_start + line_start,
                            end: para_start + break_at,
                        });
                        line_start = break_at;
                        last_space = None;
                        if ch.is_whitespace() {
                            last_space = Some(char_end);
                        }
                    } else if ch.is_whitespace() {
                        last_space = Some(char_end);
                    }
                }
                if line_start < para.len() {
                    out.push(VisualLine {
                        start: para_start + line_start,
                        end: para_end,
                    });
                }
            }
            para_start = para_end + 1;
        }
        if out.is_empty() {
            out.push(VisualLine { start: 0, end: 0 });
        }
        out
    }

    fn cursor_visual_line(lines: &[VisualLine], cursor: usize) -> usize {
        for (i, vl) in lines.iter().enumerate() {
            if cursor >= vl.start && cursor < vl.end {
                return i;
            }
            if cursor == vl.end {
                // на границе мягкого переноса (следующая строка начинается с того же байта) —
                // курсор стоит в начале следующей строки, если только эта строка не последняя
                if i + 1 < lines.len() && lines[i + 1].start == vl.end {
                    return i + 1;
                }
                return i;
            }
        }
        lines.len().saturating_sub(1)
    }

    fn row_height_for(&self, row: usize) -> f32 {
        match self.properties.get(row).map(|p| &p.value) {
            Some(PropertyValue::Multiline(s)) => {
                let text = if self.editing_row == Some(row) {
                    self.edit_buffer.as_str()
                } else {
                    s.as_str()
                };
                let n = self
                    .wrap_lines(text, self.value_inner_w())
                    .len()
                    .clamp(Self::MULTILINE_MIN_LINES, Self::MULTILINE_MAX_LINES);
                (n as f32 * Self::MULTILINE_LINE_H + Self::MULTILINE_PAD_Y * 2.0)
                    .max(self.row_height)
            }
            _ => self.row_height,
        }
    }

    fn row_top(&self, row: usize) -> f32 {
        (0..row.min(self.properties.len()))
            .map(|i| self.row_height_for(i))
            .sum()
    }

    fn content_height(&self) -> f32 {
        let rows_h: f32 = (0..self.properties.len())
            .map(|i| self.row_height_for(i))
            .sum();
        rows_h + if self.editable { self.row_height } else { 0.0 }
    }

    fn natural_width(&self) -> f32 {
        let font_size = self.mss.font_size_or(12.0);
        let label_w = self.label_width.unwrap_or(120.0);
        let padding = 40.0;

        let max_value_w = self
            .properties
            .iter()
            .map(|p| {
                if matches!(p.value, PropertyValue::Multiline(_)) {
                    return 180.0;
                }
                let text = p.value.display();
                let char_count = text.chars().count();
                if let Some(ref tm) = self.text_measure {
                    tm.measure_text_width(&text, font_size, char_count)
                } else {
                    char_count as f32 * font_size * 0.6
                }
            })
            .fold(0.0f32, f32::max);

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
        if local_y < 0.0 {
            return None;
        }
        let mut acc = 0.0;
        for i in 0..self.properties.len() {
            let h = self.row_height_for(i);
            if local_y < acc + h {
                return Some(i);
            }
            acc += h;
        }
        None
    }

    /// Квадрат отметки строки — у правого края подписи.
    fn mark_rect(&self, row: usize) -> Rect {
        let s = (self.row_height - 8.0).min(18.0);
        Rect::new(
            Point::new(
                self.bounds.x() + self.label_w() - s - 4.0,
                self.row_y(row) + (self.row_height - s) / 2.0,
            ),
            Size::new(s, s),
        )
    }

    fn row_y(&self, row: usize) -> f32 {
        self.bounds.y() + self.row_top(row) - self.scroll_offset
    }

    /// Список выбора под строкой `row` (в ширину значения).
    fn choice_rect(&self, row: usize) -> Rect {
        let n = match &self.properties.get(row).map(|p| &p.value) {
            Some(PropertyValue::Choice(items, _)) => items.len(),
            _ => 0,
        };
        let label_w = self.label_w();
        let rh = self.row_height_for(row);
        Rect::new(
            Point::new(self.bounds.x() + label_w, self.row_y(row) + rh),
            Size::new(self.bounds.size.width - label_w, n as f32 * self.row_height),
        )
    }

    /// Значение диапазона после сдвига мыши на `dx` от нажатия (по шагу).
    fn range_at(&self, row: usize, dx: f32, v0: f64) -> Option<PropertyValue> {
        let PropertyValue::Range { min, max, step, .. } = self.properties.get(row)?.value else { return None };
        let w = (self.bounds.size.width - self.label_w()).max(1.0) as f64;
        let raw = v0 + dx as f64 / w * (max - min);
        let v = if step > 0.0 { ((raw - min) / step).round() * step + min } else { raw };
        Some(PropertyValue::Range { value: v.clamp(min, max), min, max, step })
    }

    fn fire_change(&self, idx: usize, value: PropertyValue) {
        if let Some(ref cb) = self.on_change {
            if let Ok(mut f) = cb.lock() {
                f(idx, value);
            }
        }
    }

    fn edit_selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.edit_sel_anchor?;
        if anchor == self.edit_cursor {
            return None;
        }
        Some((anchor.min(self.edit_cursor), anchor.max(self.edit_cursor)))
    }

    fn delete_edit_selection(&mut self) -> bool {
        let Some((a, b)) = self.edit_selection_range() else {
            return false;
        };
        let a = a.min(self.edit_buffer.len());
        let b = b.min(self.edit_buffer.len());
        self.edit_buffer.drain(a..b);
        self.edit_cursor = a;
        self.edit_sel_anchor = None;
        true
    }

    fn insert_edit_str(&mut self, s: &str) {
        self.delete_edit_selection();
        let pos = self.edit_cursor.min(self.edit_buffer.len());
        self.edit_buffer.insert_str(pos, s);
        self.edit_cursor = pos + s.len();
        self.edit_sel_anchor = None;
        self.edit_preferred_x = None;
    }

    fn ensure_edit_cursor_visible(&mut self) {
        let Some(row) = self.editing_row else { return };
        if self.is_row_multiline(row) {
            let lines = self.wrap_lines(&self.edit_buffer, self.value_inner_w());
            let vi = Self::cursor_visual_line(&lines, self.edit_cursor);
            let visible_h = (self.row_height_for(row) - Self::MULTILINE_PAD_Y * 2.0).max(Self::MULTILINE_LINE_H);
            let total_h = lines.len() as f32 * Self::MULTILINE_LINE_H;
            let max_y = (total_h - visible_h).max(0.0);
            let cy0 = vi as f32 * Self::MULTILINE_LINE_H;
            let cy1 = cy0 + Self::MULTILINE_LINE_H;
            if cy0 < self.edit_scroll_y {
                self.edit_scroll_y = cy0;
            } else if cy1 > self.edit_scroll_y + visible_h {
                self.edit_scroll_y = cy1 - visible_h;
            }
            self.edit_scroll_y = self.edit_scroll_y.clamp(0.0, max_y);
        } else {
            let inner_w = self.value_inner_w();
            let cur = self.edit_cursor.min(self.edit_buffer.len());
            let cx = self.measure_width(&self.edit_buffer[..cur], 12.0);
            let total_w = self.measure_width(&self.edit_buffer, 12.0);
            let max_x = (total_w - inner_w + 8.0).max(0.0);
            if cx < self.edit_scroll_x {
                self.edit_scroll_x = (cx - 8.0).max(0.0);
            } else if cx > self.edit_scroll_x + inner_w - 6.0 {
                self.edit_scroll_x = cx - inner_w + 10.0;
            }
            self.edit_scroll_x = self.edit_scroll_x.clamp(0.0, max_x);
        }
    }

    fn sync_live_multiline(&mut self, ctx: &mut EventContext) {
        if let Some(idx) = self.editing_row {
            if self.is_row_multiline(idx) {
                let pv = PropertyValue::Multiline(self.edit_buffer.clone());
                self.properties[idx].value = pv.clone();
                self.fire_change(idx, pv);
                ctx.request_layout();
            }
        }
    }

    fn hit_test_edit_pos(&self, row: usize, pos: Point) -> usize {
        let value_x = self.bounds.x() + self.label_w();
        let text_x = value_x + 8.0;
        if self.is_row_multiline(row) {
            let lines = self.wrap_lines(&self.edit_buffer, self.value_inner_w());
            let text_y = self.row_y(row) + Self::MULTILINE_PAD_Y;
            let rel_y = (pos.y - text_y + self.edit_scroll_y).max(0.0);
            let vi = ((rel_y / Self::MULTILINE_LINE_H) as usize).min(lines.len().saturating_sub(1));
            let vl = lines[vi];
            let local = self.hit_test_line_byte(
                &self.edit_buffer[vl.start..vl.end],
                pos.x - text_x,
                Self::MULTILINE_FONT_SIZE,
            );
            vl.start + local
        } else {
            self.hit_test_line_byte(&self.edit_buffer, pos.x - text_x + self.edit_scroll_x, 12.0)
        }
    }

    fn move_cursor_vertical(&mut self, delta: isize, extend_sel: bool) {
        let Some(row) = self.editing_row else { return };
        if extend_sel && self.edit_sel_anchor.is_none() {
            self.edit_sel_anchor = Some(self.edit_cursor);
        } else if !extend_sel {
            self.edit_sel_anchor = None;
        }
        if !self.is_row_multiline(row) {
            self.edit_cursor = if delta < 0 { 0 } else { self.edit_buffer.len() };
            self.edit_preferred_x = None;
            return;
        }
        let lines = self.wrap_lines(&self.edit_buffer, self.value_inner_w());
        let cur_vi = Self::cursor_visual_line(&lines, self.edit_cursor);
        let cur_vl = lines[cur_vi];
        let pref_x = self.edit_preferred_x.unwrap_or_else(|| {
            let local_end = self.edit_cursor.clamp(cur_vl.start, cur_vl.end);
            self.measure_width(&self.edit_buffer[cur_vl.start..local_end], Self::MULTILINE_FONT_SIZE)
        });
        let target_vi = (cur_vi as isize + delta).clamp(0, lines.len().saturating_sub(1) as isize) as usize;
        if target_vi == cur_vi {
            self.edit_cursor = if delta < 0 { 0 } else { self.edit_buffer.len() };
            self.edit_preferred_x = None;
        } else {
            let tvl = lines[target_vi];
            let local = self.hit_test_line_byte(
                &self.edit_buffer[tvl.start..tvl.end],
                pref_x,
                Self::MULTILINE_FONT_SIZE,
            );
            self.edit_cursor = tvl.start + local;
            self.edit_preferred_x = Some(pref_x);
        }
    }

    fn prev_word_boundary(s: &str, pos: usize) -> usize {
        let mut p = pos.min(s.len());
        while p > 0 {
            let (i, ch) = s[..p].char_indices().next_back().unwrap();
            if !ch.is_whitespace() {
                break;
            }
            p = i;
        }
        while p > 0 {
            let (i, ch) = s[..p].char_indices().next_back().unwrap();
            if ch.is_whitespace() {
                break;
            }
            p = i;
        }
        p
    }

    fn next_word_boundary(s: &str, pos: usize) -> usize {
        let mut p = pos.min(s.len());
        while p < s.len() {
            let ch = s[p..].chars().next().unwrap();
            if ch.is_whitespace() {
                break;
            }
            p += ch.len_utf8();
        }
        while p < s.len() {
            let ch = s[p..].chars().next().unwrap();
            if !ch.is_whitespace() {
                break;
            }
            p += ch.len_utf8();
        }
        p
    }

    fn commit_edit(&mut self) {
        if let Some(idx) = self.editing_row {
            if idx < self.properties.len() {
                let new_value = match &self.properties[idx].value {
                    PropertyValue::Text(_) => PropertyValue::Text(self.edit_buffer.clone()),
                    PropertyValue::Multiline(_) => {
                        let pv = PropertyValue::Multiline(self.edit_buffer.clone());
                        if self.edit_buffer != self.edit_original {
                            self.properties[idx].value = pv.clone();
                        }
                        self.editing_row = None;
                        self.edit_sel_anchor = None;
                        self.edit_selecting = false;
                        self.edit_preferred_x = None;
                        return;
                    }
                    PropertyValue::Number(_) => {
                        if let Ok(n) = self.edit_buffer.parse::<f64>() {
                            PropertyValue::Number(n)
                        } else {
                            self.editing_row = None;
                            self.edit_sel_anchor = None;
                            self.edit_selecting = false;
                            return;
                        }
                    }
                    PropertyValue::Bool(_) => PropertyValue::Bool(
                        self.edit_buffer == "true"
                            || self.edit_buffer == "1"
                            || self.edit_buffer == "yes",
                    ),
                    PropertyValue::Color(_) => {
                        let hex = self.edit_buffer.trim_start_matches('#');
                        if hex.len() == 6 {
                            if let (Ok(r), Ok(g), Ok(b)) = (
                                u8::from_str_radix(&hex[0..2], 16),
                                u8::from_str_radix(&hex[2..4], 16),
                                u8::from_str_radix(&hex[4..6], 16),
                            ) {
                                PropertyValue::Color(Color::new(
                                    r as f32 / 255.0,
                                    g as f32 / 255.0,
                                    b as f32 / 255.0,
                                    1.0,
                                ))
                            } else {
                                self.editing_row = None;
                                self.edit_sel_anchor = None;
                                self.edit_selecting = false;
                                return;
                            }
                        } else {
                            self.editing_row = None;
                            self.edit_sel_anchor = None;
                            self.edit_selecting = false;
                            return;
                        }
                    }
                    PropertyValue::Range { min, max, step, .. } => match self.edit_buffer.trim().replace(',', ".").parse::<f64>() {
                        Ok(v) => PropertyValue::Range { value: v.clamp(*min, *max), min: *min, max: *max, step: *step },
                        Err(_) => {
                            self.editing_row = None;
                            self.edit_sel_anchor = None;
                            self.edit_selecting = false;
                            return;
                        }
                    },
                    PropertyValue::Header | PropertyValue::Action(_) => {
                        self.editing_row = None;
                        self.edit_sel_anchor = None;
                        self.edit_selecting = false;
                        return;
                    }
                    PropertyValue::Choice(items, _) => {
                        let selected = items
                            .iter()
                            .position(|s| s == &self.edit_buffer)
                            .unwrap_or(0);
                        PropertyValue::Choice(items.clone(), selected)
                    }
                };
                self.fire_change(idx, new_value.clone());
                self.properties[idx].value = new_value;
            }
        }
        self.editing_row = None;
        self.edit_sel_anchor = None;
        self.edit_selecting = false;
        self.edit_preferred_x = None;
    }

    fn cancel_edit(&mut self, ctx: &mut EventContext) {
        if let Some(idx) = self.editing_row.take() {
            if self.is_row_multiline(idx) && self.edit_buffer != self.edit_original {
                let pv = PropertyValue::Multiline(self.edit_original.clone());
                self.properties[idx].value = pv.clone();
                self.fire_change(idx, pv);
                ctx.request_layout();
            }
        }
        self.edit_sel_anchor = None;
        self.edit_selecting = false;
        self.edit_preferred_x = None;
    }

    fn start_edit(&mut self, idx: usize) {
        if idx >= self.properties.len() {
            return;
        }
        self.editing_row = Some(idx);
        self.edit_buffer = self.properties[idx].value.display();
        self.edit_original = self.edit_buffer.clone();
        self.edit_cursor = self.edit_buffer.len();
        self.edit_sel_anchor = None;
        self.edit_selecting = false;
        self.edit_scroll_x = 0.0;
        self.edit_scroll_y = 0.0;
        self.edit_preferred_x = None;
        self.ensure_edit_cursor_visible();
    }

    const CP_SV_SIZE: f32 = 140.0;
    const CP_HUE_W: f32 = 16.0;
    const CP_GAP: f32 = 8.0;
    const CP_PAD: f32 = 12.0;

    fn cp_popup_w(&self) -> f32 {
        Self::CP_PAD * 2.0 + Self::CP_SV_SIZE + Self::CP_GAP + Self::CP_HUE_W
    }
    fn cp_popup_h(&self) -> f32 {
        Self::CP_PAD * 2.0 + Self::CP_SV_SIZE
    }

    fn cp_popup_rect(&self, row_idx: usize) -> Rect {
        let row_y = self.row_y(row_idx);
        let rh = self.row_height_for(row_idx);
        let label_w = self.label_w();
        let x = self.bounds.x() + label_w;
        let y = row_y + rh + 2.0;
        Rect::new(
            Point::new(x, y),
            Size::new(self.cp_popup_w(), self.cp_popup_h()),
        )
    }

    fn cp_sv_rect(&self, popup: Rect) -> Rect {
        Rect::new(
            Point::new(popup.x() + Self::CP_PAD, popup.y() + Self::CP_PAD),
            Size::new(Self::CP_SV_SIZE, Self::CP_SV_SIZE),
        )
    }

    fn cp_hue_rect(&self, popup: Rect) -> Rect {
        Rect::new(
            Point::new(
                popup.x() + Self::CP_PAD + Self::CP_SV_SIZE + Self::CP_GAP,
                popup.y() + Self::CP_PAD,
            ),
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

    fn add_row_y(&self) -> f32 {
        (self.bounds.y() + self.row_top(self.properties.len()) - self.scroll_offset).round()
    }

    fn filter_suggestions(&mut self) {
        let query = self.add_buffer.to_lowercase();
        let existing: Vec<String> = self
            .properties
            .iter()
            .map(|p| p.name.to_lowercase())
            .collect();
        self.add_filtered = self
            .suggestions
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                let sl = s.to_lowercase();
                sl.contains(&query) && !existing.contains(&sl)
            })
            .map(|(i, _)| i)
            .collect();
        self.add_dropdown_open = !self.add_filtered.is_empty() && !self.add_buffer.is_empty();
        if let Some(h) = self.add_hover {
            if h >= self.add_filtered.len() {
                self.add_hover = if self.add_filtered.is_empty() {
                    None
                } else {
                    Some(self.add_filtered.len() - 1)
                };
            }
        }
    }

    fn fire_add(&self, name: &str, value: PropertyValue) {
        if let Some(ref cb) = self.on_add {
            if let Ok(mut f) = cb.lock() {
                f(name, value);
            }
        }
    }

    fn fire_remove(&self, idx: usize, name: &str) {
        if let Some(ref cb) = self.on_remove {
            if let Ok(mut f) = cb.lock() {
                f(idx, name);
            }
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
        self.properties.push(Property {
            name: name.clone(),
            value,
            mark: None,
        });
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
                self.properties.push(Property { name, value, mark: None });
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
        let y = self.row_y(row_idx).round();
        let x = self.bounds.x() + self.bounds.size.width - Self::DELETE_BTN_SIZE - 8.0;
        let cy = y + (self.row_height - Self::DELETE_BTN_SIZE) / 2.0;
        Rect::new(
            Point::new(x, cy),
            Size::new(Self::DELETE_BTN_SIZE, Self::DELETE_BTN_SIZE),
        )
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
        list.push_rect_bordered(
            cur,
            Color::TRANSPARENT,
            [r; 4],
            Border::new(2.0, Color::WHITE),
        );
        list.push_rect_bordered(
            cur,
            Color::TRANSPARENT,
            [r; 4],
            Border::new(1.0, Color::BLACK.with_alpha(0.3)),
        );
    }

    fn draw_cp_hue_bar(&self, list: &mut DisplayList, rect: Rect) {
        let steps = 12;
        let cell_h = rect.size.height / steps as f32;
        let hues = [
            0.0, 30.0, 60.0, 90.0, 120.0, 150.0, 180.0, 210.0, 240.0, 270.0, 300.0, 330.0,
        ];
        for (i, &h) in hues.iter().enumerate() {
            let cv = ColorValue::from_hsv(h, 1.0, 1.0);
            let cell = Rect::new(
                Point::new(rect.x(), rect.y() + i as f32 * cell_h),
                Size::new(rect.size.width, cell_h + 0.5),
            );
            list.push_rect(cell, cv.to_color(), [0.0; 4]);
        }
        let cy = rect.y() + (self.cp_hue / 360.0) * rect.size.height;
        let cur = Rect::new(
            Point::new(rect.x() - 1.0, cy - 2.0),
            Size::new(rect.size.width + 2.0, 4.0),
        );
        list.push_rect_bordered(
            cur,
            Color::TRANSPARENT,
            [2.0; 4],
            Border::new(2.0, Color::WHITE),
        );
    }
}

impl Element for PropertyGridElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(pg) = widget.as_any().downcast_ref::<PropertyGrid>() {
            let prev_edit = self.editing_row.and_then(|idx| {
                self.properties
                    .get(idx)
                    .map(|p| (idx, std::mem::discriminant(&p.value)))
            });
            self.properties = pg.properties.clone();
            self.on_change = pg.on_change.clone();
            self.on_add = pg.on_add.clone();
            self.on_remove = pg.on_remove.clone();
            self.on_mark = pg.on_mark.clone();
            self.label_width = pg.label_width;
            self.row_height = pg.row_height;
            self.fixed_width = pg.width;
            self.fixed_height = pg.height;
            self.auto_height = pg.auto_height;
            self.editable = pg.editable;
            self.suggestions = pg.suggestions.clone();
            if let Some((idx, disc)) = prev_edit {
                if self
                    .properties
                    .get(idx)
                    .is_some_and(|p| std::mem::discriminant(&p.value) == disc)
                {
                    if matches!(self.properties[idx].value, PropertyValue::Multiline(_)) {
                        self.properties[idx].value =
                            PropertyValue::Multiline(self.edit_buffer.clone());
                    }
                } else {
                    self.editing_row = None;
                    self.edit_sel_anchor = None;
                    self.edit_selecting = false;
                }
            }
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let min_w = self
            .mss
            .min_width
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(300.0);
        let max_w = self
            .mss
            .max_width
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(f32::INFINITY);
        let w = self
            .mss
            .width
            .or(self.fixed_width)
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or_else(|| self.natural_width().max(min_w))
            .clamp(min_w, max_w)
            .min(constraints.max_width);
        // ширина нужна до расчёта content_height — от неё зависит перенос многострочных строк
        self.bounds.size.width = w;
        let natural_h = self.content_height();
        let h = if self.auto_height {
            natural_h.min(constraints.max_height)
        } else {
            self.mss
                .height
                .or(self.fixed_height)
                .map(|d| d.resolve(constraints.max_height))
                .unwrap_or(natural_h.min(400.0))
                .min(constraints.max_height)
        };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let bg = self
            .mss
            .background_color
            .unwrap_or_else(crate::theme_fallback::fallback_surface);
        let border_color = self
            .mss
            .border_color
            .unwrap_or_else(crate::theme_fallback::fallback_divider);
        let fg = self
            .mss
            .color
            .unwrap_or_else(crate::theme_fallback::fallback_fg);
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        list.push_rect_bordered(self.bounds, bg, [8.0; 4], Border::new(1.0, border_color));

        list.push_clip(self.bounds);

        let viewport_top = self.scroll_offset;
        let viewport_bottom = viewport_top + self.bounds.size.height;

        let label_w = self.label_w();
        let value_x = self.bounds.x() + label_w;
        let value_w = self.bounds.size.width - label_w;

        let base_y = self.bounds.y().round();
        let base_x = self.bounds.x().round();

        // вычисляем y и высоту каждой видимой строки (многострочные строки выше обычных)
        let mut visible_rows: Vec<(usize, f32, f32)> = Vec::new();
        let mut acc_y = 0.0f32;
        for row_idx in 0..self.properties.len() {
            let rh = self.row_height_for(row_idx);
            if acc_y + rh >= viewport_top && acc_y <= viewport_bottom {
                visible_rows.push((row_idx, (base_y + acc_y - self.scroll_offset).round(), rh));
            }
            acc_y += rh;
        }

        for &(row_idx, y, rh) in &visible_rows {
            let row_rect = Rect::new(
                Point::new(base_x, y),
                Size::new(self.bounds.size.width, rh),
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
        let weight = self.mss.font_weight_or(400);
        let font_family = self.mss.font_family.clone();

        for &(row_idx, y, rh) in &visible_rows {
            if row_idx + 1 < self.properties.len() {
                let next_y = (y + rh).round();
                list.push_rect(
                    Rect::new(
                        Point::new(base_x, next_y),
                        Size::new(self.bounds.size.width, 1.0),
                    ),
                    border_color,
                    [0.0; 4],
                );
            }

            let prop = &self.properties[row_idx];
            // заголовок раздела: строка во всю ширину, без делителя и значения
            if matches!(prop.value, PropertyValue::Header) {
                let hr = Rect::new(Point::new(base_x, y), Size::new(self.bounds.size.width, rh));
                list.push_rect(hr, bg.darken(0.06), [0.0; 4]);
                let tr = Rect::new(
                    Point::new(self.bounds.x() + 10.0, y + (rh - 12.0) / 2.0),
                    Size::new(self.bounds.size.width - 20.0, 13.0),
                );
                list.push_text_singleline(&prop.name.to_uppercase(), tr, fg.with_alpha(0.62), 11.0, TextAlign::LEFT, weight);
                continue;
            }
            list.push_rect(
                Rect::new(Point::new(div_x, y), Size::new(1.0, rh)),
                border_color,
                [0.0; 4],
            );

            // подпись слева: для многострочной ячейки — по уровню первой строки
            let label_clip = Rect::new(
                Point::new(self.bounds.x() + 8.0, y),
                Size::new((label_w - 12.0 - if prop.mark.is_some() { 22.0 } else { 0.0 }).max(1.0), rh),
            );
            list.push_clip(label_clip);
            let label_rect = Rect::new(
                Point::new(self.bounds.x() + 12.0, y + (self.row_height - 13.0) / 2.0),
                Size::new((label_w - 20.0 - if prop.mark.is_some() { 22.0 } else { 0.0 }).max(1.0), 14.0),
            );
            list.push_text_singleline(&prop.name, label_rect, fg.with_alpha(0.5), 12.0, TextAlign::LEFT, weight);
            list.pop_clip();

            if let Some(m) = prop.mark {
                let r = self.mark_rect(row_idx);
                let (glyph, col) = match m {
                    crate::widgets::data::PropertyMark::Empty => ("\u{25C7}", fg.with_alpha(0.45)),
                    crate::widgets::data::PropertyMark::Partial => ("\u{25C8}", primary),
                    crate::widgets::data::PropertyMark::Full => ("\u{25C6}", primary),
                };
                list.push_text_centered(glyph, r, col, 13.0);
            }
            let is_editing = self.editing_row == Some(row_idx);
            let value_clip = Rect::new(Point::new(value_x + 2.0, y + 1.0), Size::new((value_w - 4.0).max(1.0), (rh - 2.0).max(1.0)));
            let value_rect = Rect::new(
                Point::new(value_x + 8.0, y + (rh - 13.0) / 2.0),
                Size::new((value_w - 16.0).max(1.0), 14.0),
            );

            if is_editing && matches!(prop.value, PropertyValue::Multiline(_)) {
                let edit_bg = Rect::new(
                    Point::new(value_x + 2.0, y + 2.0),
                    Size::new((value_w - 4.0).max(1.0), (rh - 4.0).max(1.0)),
                );
                list.push_rect_bordered(edit_bg, bg, [4.0; 4], Border::new(2.0, primary));
                let inner_clip = Rect::new(
                    Point::new(value_x + 6.0, y + Self::MULTILINE_PAD_Y - 1.0),
                    Size::new((value_w - 12.0).max(1.0), (rh - Self::MULTILINE_PAD_Y * 2.0 + 2.0).max(1.0)),
                );
                list.push_clip(inner_clip);
                let lines = self.wrap_lines(&self.edit_buffer, self.value_inner_w());
                let cur_vi = Self::cursor_visual_line(&lines, self.edit_cursor);
                let sel = self.edit_selection_range();
                let text_x = value_x + 8.0;
                let text_y0 = y + Self::MULTILINE_PAD_Y - self.edit_scroll_y;
                for (i, vl) in lines.iter().enumerate() {
                    let ly = text_y0 + i as f32 * Self::MULTILINE_LINE_H;
                    if ly + Self::MULTILINE_LINE_H < inner_clip.y() || ly > inner_clip.y() + inner_clip.size.height {
                        continue;
                    }
                    let slice = &self.edit_buffer[vl.start..vl.end];
                    if let Some((sa, sb)) = sel {
                        if sb > vl.start && sa < vl.end {
                            let ls = sa.saturating_sub(vl.start).min(slice.len());
                            let le = sb.saturating_sub(vl.start).min(slice.len());
                            list.push_text_selection_weighted(
                                slice,
                                ls,
                                le,
                                text_x,
                                ly,
                                Self::MULTILINE_LINE_H,
                                Self::MULTILINE_FONT_SIZE,
                                weight,
                                primary.with_alpha(0.32),
                                font_family.clone(),
                            );
                        } else if slice.is_empty() && sa <= vl.start && sb > vl.start {
                            list.push_rect(
                                Rect::new(Point::new(text_x, ly), Size::new(5.0, Self::MULTILINE_LINE_H)),
                                primary.with_alpha(0.32),
                                [1.0; 4],
                            );
                        }
                    }
                    let lr = Rect::new(Point::new(text_x, ly + 1.0), Size::new((value_w - 16.0).max(1.0), Self::MULTILINE_LINE_H));
                    list.push_text_styled_singleline(
                        slice,
                        lr,
                        fg,
                        Self::MULTILINE_FONT_SIZE,
                        TextAlign::LEFT,
                        TextDecoration::None,
                        weight,
                        font_family.clone(),
                    );
                    if i == cur_vi {
                        let local_cur = self.edit_cursor.clamp(vl.start, vl.end) - vl.start;
                        list.push_text_cursor_styled(
                            slice,
                            local_cur,
                            text_x,
                            ly + 1.0,
                            Self::MULTILINE_LINE_H - 2.0,
                            Self::MULTILINE_FONT_SIZE,
                            weight,
                            primary,
                            font_family.clone(),
                        );
                    }
                }
                list.pop_clip();
                // ползунок прокрутки, если строк больше видимого предела
                let total_h = lines.len() as f32 * Self::MULTILINE_LINE_H;
                let vis_h = (rh - Self::MULTILINE_PAD_Y * 2.0).max(Self::MULTILINE_LINE_H);
                if total_h > vis_h + 1.0 {
                    let max_sy = (total_h - vis_h).max(1.0);
                    let th = (vis_h / total_h * vis_h).max(14.0);
                    let ty = y + Self::MULTILINE_PAD_Y + (self.edit_scroll_y / max_sy) * (vis_h - th);
                    list.push_rect(
                        Rect::new(Point::new(value_x + value_w - 7.0, ty), Size::new(3.0, th)),
                        fg.with_alpha(0.28),
                        [1.5; 4],
                    );
                }
            } else if is_editing {
                let edit_bg = Rect::new(
                    Point::new(value_x + 2.0, y + 2.0),
                    Size::new((value_w - 4.0).max(1.0), (rh - 4.0).max(1.0)),
                );
                list.push_rect_bordered(edit_bg, bg, [4.0; 4], Border::new(2.0, primary));
                let inner_clip = Rect::new(
                    Point::new(value_x + 6.0, y + 3.0),
                    Size::new((value_w - 12.0).max(1.0), (rh - 6.0).max(1.0)),
                );
                list.push_clip(inner_clip);
                let text_x = value_x + 8.0 - self.edit_scroll_x;
                if let Some((sa, sb)) = self.edit_selection_range() {
                    list.push_text_selection_weighted(
                        &self.edit_buffer,
                        sa,
                        sb,
                        text_x,
                        y + 5.0,
                        rh - 10.0,
                        12.0,
                        weight,
                        primary.with_alpha(0.32),
                        font_family.clone(),
                    );
                }
                let edit_text_rect = Rect::new(
                    Point::new(text_x, y + (rh - 13.0) / 2.0),
                    Size::new((value_w - 16.0).max(1.0), 14.0),
                );
                list.push_text_styled_singleline(
                    &self.edit_buffer,
                    edit_text_rect,
                    fg,
                    12.0,
                    TextAlign::LEFT,
                    TextDecoration::None,
                    weight,
                    font_family.clone(),
                );
                list.push_text_cursor_styled(
                    &self.edit_buffer,
                    self.edit_cursor,
                    text_x,
                    y + 6.0,
                    rh - 12.0,
                    12.0,
                    weight,
                    primary,
                    font_family.clone(),
                );
                list.pop_clip();
            } else {
                match &prop.value {
                    PropertyValue::Multiline(s) => {
                        let box_r = Rect::new(
                            Point::new(value_x + 2.0, y + 2.0),
                            Size::new((value_w - 4.0).max(1.0), (rh - 4.0).max(1.0)),
                        );
                        let box_bg = if self.hover_row == Some(row_idx) {
                            fg.with_alpha(0.05)
                        } else {
                            fg.with_alpha(0.025)
                        };
                        list.push_rect_bordered(box_r, box_bg, [4.0; 4], Border::new(1.0, border_color.with_alpha(0.6)));
                        let inner_clip = Rect::new(
                            Point::new(value_x + 6.0, y + Self::MULTILINE_PAD_Y - 1.0),
                            Size::new((value_w - 12.0).max(1.0), (rh - Self::MULTILINE_PAD_Y * 2.0 + 2.0).max(1.0)),
                        );
                        list.push_clip(inner_clip);
                        let lines = self.wrap_lines(s, self.value_inner_w());
                        let text_x = value_x + 8.0;
                        let text_y0 = y + Self::MULTILINE_PAD_Y;
                        for (i, vl) in lines.iter().take(Self::MULTILINE_MAX_LINES).enumerate() {
                            let ly = text_y0 + i as f32 * Self::MULTILINE_LINE_H;
                            let lr = Rect::new(Point::new(text_x, ly + 1.0), Size::new((value_w - 16.0).max(1.0), Self::MULTILINE_LINE_H));
                            list.push_text_styled_singleline(
                                &s[vl.start..vl.end],
                                lr,
                                fg,
                                Self::MULTILINE_FONT_SIZE,
                                TextAlign::LEFT,
                                TextDecoration::None,
                                weight,
                                font_family.clone(),
                            );
                        }
                        list.pop_clip();
                    }
                    PropertyValue::Color(c) => {
                        list.push_clip(value_clip);
                        let swatch_rect = Rect::new(
                            Point::new(value_x + 8.0, y + (rh - 16.0) / 2.0),
                            Size::new(16.0, 16.0),
                        );
                        list.push_rect(swatch_rect, *c, [3.0; 4]);
                        let text_rect = Rect::new(
                            Point::new(value_x + 30.0, y + (rh - 13.0) / 2.0),
                            Size::new((value_w - 38.0).max(1.0), 14.0),
                        );
                        list.push_text_singleline(&prop.value.display(), text_rect, fg, 12.0, TextAlign::LEFT, weight);
                        list.pop_clip();
                    }
                    PropertyValue::Bool(b) => {
                        list.push_clip(value_clip);
                        let check_rect = Rect::new(
                            Point::new(value_x + 8.0, y + (rh - 16.0) / 2.0),
                            Size::new(16.0, 16.0),
                        );
                        let check_bg = if *b { primary } else { bg };
                        list.push_rect_bordered(
                            check_rect,
                            check_bg,
                            [3.0; 4],
                            Border::new(1.0, border_color),
                        );
                        if *b {
                            list.push_text_centered("\u{2713}", check_rect, bg, 11.0);
                        }
                        let text_rect = Rect::new(
                            Point::new(value_x + 30.0, y + (rh - 13.0) / 2.0),
                            Size::new((value_w - 38.0).max(1.0), 14.0),
                        );
                        list.push_text_singleline(&prop.value.display(), text_rect, fg, 12.0, TextAlign::LEFT, weight);
                        list.pop_clip();
                    }
                    PropertyValue::Range { value, min, max, .. } => {
                        list.push_clip(value_clip);
                        // полоса заполнения: доля значения в пределах
                        let frac = if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) as f32 } else { 0.0 };
                        let bar = Rect::new(Point::new(value_x + 2.0, y + 3.0), Size::new((value_w - 4.0) * frac, rh - 6.0));
                        list.push_rect(bar, primary.with_alpha(if self.range_drag.map(|d| d.0) == Some(row_idx) { 0.35 } else { 0.2 }), [3.0; 4]);
                        list.push_text_singleline(&prop.value.display(), value_rect, fg, 12.0, TextAlign::LEFT, weight);
                        list.pop_clip();
                    }
                    PropertyValue::Action(label) => {
                        let br = Rect::new(Point::new(value_x + 4.0, y + 3.0), Size::new((value_w - 8.0).max(1.0), rh - 6.0));
                        let hot = self.hover_row == Some(row_idx);
                        list.push_rect_bordered(br, if hot { primary.with_alpha(0.18) } else { fg.with_alpha(0.06) }, [5.0; 4], Border::new(1.0, border_color));
                        list.push_text_centered(label, br, fg, 12.0);
                    }
                    PropertyValue::Choice(..) => {
                        list.push_clip(value_clip);
                        let tr = Rect::new(value_rect.origin, Size::new((value_w - 30.0).max(1.0), 14.0));
                        list.push_text_singleline(&prop.value.display(), tr, fg, 12.0, TextAlign::LEFT, weight);
                        let ar = Rect::new(Point::new(value_x + value_w - 22.0, y), Size::new(16.0, rh));
                        list.push_text_centered("\u{25BE}", ar, fg.with_alpha(0.5), 11.0);
                        list.pop_clip();
                    }
                    _ => {
                        list.push_clip(value_clip);
                        list.push_text_singleline(&prop.value.display(), value_rect, fg, 12.0, TextAlign::LEFT, weight);
                        list.pop_clip();
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
                Rect::new(
                    Point::new(base_x, add_y),
                    Size::new(self.bounds.size.width, 1.0),
                ),
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
            let thumb_y = self.bounds.y()
                + if max_s > 0.0 {
                    (self.scroll_offset / max_s) * (track_h - thumb_h)
                } else {
                    0.0
                };
            let scrollbar_x = self.bounds.x() + self.bounds.size.width - 6.0;
            let thumb_rect = Rect::new(Point::new(scrollbar_x, thumb_y), Size::new(4.0, thumb_h));
            list.push_rect(thumb_rect, fg.with_alpha(0.2), [2.0; 4]);
        }

        list.pop_clip();

        list.push_rect_bordered(
            self.bounds,
            Color::TRANSPARENT,
            [8.0; 4],
            Border::new(1.0, border_color),
        );

        if self.add_dropdown_open && !self.add_filtered.is_empty() {
            let dd = self.dropdown_rect();
            list.begin_overlay();
            list.push_shadow(dd, Color::BLACK.with_alpha(0.1), 8.0, (0.0, 2.0), [6.0; 4]);
            list.push_rect_bordered(dd, bg, [6.0; 4], Border::new(1.0, border_color));

            for (fi, &si) in self.add_filtered.iter().enumerate() {
                if fi >= 6 {
                    break;
                }
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
                    let text_color = if self.add_hover == Some(fi) {
                        primary
                    } else {
                        fg
                    };
                    list.push_text(name, text_rect, text_color, 12.0);
                }
            }

            list.end_overlay();
        }

        if let Some(row_idx) = self.choice_row {
            if let PropertyValue::Choice(items, sel) = &self.properties[row_idx].value {
                let r = self.choice_rect(row_idx);
                list.begin_overlay();
                list.push_shadow(r, Color::BLACK.with_alpha(0.15), 10.0, (0.0, 3.0), [6.0; 4]);
                list.push_rect_bordered(r, bg, [6.0; 4], Border::new(1.0, border_color));
                for (i, it) in items.iter().enumerate() {
                    let ir = Rect::new(Point::new(r.x(), r.y() + i as f32 * self.row_height), Size::new(r.size.width, self.row_height));
                    if self.choice_hover == Some(i) {
                        list.push_rect(ir, primary.with_alpha(0.14), [0.0; 4]);
                    }
                    let tr = Rect::new(Point::new(ir.x() + 10.0, ir.y() + (self.row_height - 13.0) / 2.0), Size::new(ir.size.width - 20.0, 14.0));
                    list.push_text(it, tr, if i == *sel { primary } else { fg }, 12.0);
                }
                list.end_overlay();
            }
        }

        if let Some(row_idx) = self.color_picker_row {
            let popup = self.cp_popup_rect(row_idx);
            list.begin_overlay();
            list.push_shadow(
                popup,
                Color::BLACK.with_alpha(0.15),
                12.0,
                (0.0, 4.0),
                [8.0; 4],
            );
            list.push_rect_bordered(popup, bg, [8.0; 4], Border::new(1.0, border_color));
            let sv = self.cp_sv_rect(popup);
            list.push_rect_bordered(
                sv,
                Color::TRANSPARENT,
                [4.0; 4],
                Border::new(1.0, border_color),
            );
            self.draw_cp_sv_field(list, sv);
            let hue = self.cp_hue_rect(popup);
            list.push_rect_bordered(
                hue,
                Color::TRANSPARENT,
                [4.0; 4],
                Border::new(1.0, border_color),
            );
            self.draw_cp_hue_bar(list, hue);
            list.end_overlay();
        }
    }

    /// Поле насыщенности и полоса тона открытой палитры строки-цвета.
    fn drags_with_touch(&self, pos: Point) -> bool {
        let Some(row_idx) = self.color_picker_row else {
            return false;
        };
        let popup = self.cp_popup_rect(row_idx);
        self.cp_sv_rect(popup).contains(pos) || self.cp_hue_rect(popup).contains(pos)
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::MouseMove(pos) => {
                // выделение текста мышью в редактируемой ячейке
                if self.edit_selecting {
                    if let Some(row) = self.editing_row {
                        self.edit_cursor = self.hit_test_edit_pos(row, *pos);
                        self.edit_preferred_x = None;
                        self.ensure_edit_cursor_visible();
                        ctx.set_cursor(CursorIcon::Text);
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }
                // протяжка числа
                if let Some((row, x0, v0, _)) = self.range_drag {
                    let dx = pos.x - x0;
                    if dx.abs() > 2.0 {
                        self.range_drag = Some((row, x0, v0, true));
                        if let Some(pv) = self.range_at(row, dx, v0) {
                            let changed = !matches!((&pv, &self.properties[row].value), (PropertyValue::Range { value: a, .. }, PropertyValue::Range { value: b, .. }) if (a - b).abs() < 1e-12);
                            if changed {
                                self.properties[row].value = pv.clone();
                                self.fire_change(row, pv);
                            }
                        }
                    }
                    ctx.set_cursor(CursorIcon::EResize);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if let Some(row) = self.choice_row {
                    let r = self.choice_rect(row);
                    if r.contains(*pos) {
                        let h = Some(((pos.y - r.y()) / self.row_height) as usize);
                        if h != self.choice_hover {
                            self.choice_hover = h;
                            ctx.request_paint();
                        }
                        ctx.set_cursor(CursorIcon::Pointer);
                        return EventResult::Handled;
                    }
                }
                if self.cp_drag != CpDragTarget::None {
                    if let Some(row_idx) = self.color_picker_row {
                        let popup = self.cp_popup_rect(row_idx);
                        match self.cp_drag {
                            CpDragTarget::SvField => {
                                let sv = self.cp_sv_rect(popup);
                                self.cp_sat = ((pos.x - sv.x()) / sv.size.width).clamp(0.0, 1.0);
                                self.cp_val =
                                    1.0 - ((pos.y - sv.y()) / sv.size.height).clamp(0.0, 1.0);
                                self.cp_update_color();
                            }
                            CpDragTarget::HueBar => {
                                let hb = self.cp_hue_rect(popup);
                                self.cp_hue =
                                    ((pos.y - hb.y()) / hb.size.height).clamp(0.0, 1.0) * 360.0;
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
                        let new_hover = if fi < self.add_filtered.len() {
                            Some(fi)
                        } else {
                            None
                        };
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
                let text_cursor = new_hover.is_some_and(|r| {
                    pos.x >= self.bounds.x() + self.label_w()
                        && matches!(
                            self.properties.get(r).map(|p| &p.value),
                            Some(PropertyValue::Text(_) | PropertyValue::Multiline(_))
                        )
                });
                ctx.set_cursor(if text_cursor { CursorIcon::Text } else { CursorIcon::Pointer });
                EventResult::Handled
            }
            Event::MouseUp { button, .. } if *button == MouseButton::Left => {
                if self.edit_selecting {
                    self.edit_selecting = false;
                    return EventResult::Handled;
                }
                if let Some((row, _, _, moved)) = self.range_drag.take() {
                    // щелчок без протяжки — ввод числа
                    if !moved {
                        self.start_edit(row);
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.cp_drag != CpDragTarget::None {
                    self.cp_drag = CpDragTarget::None;
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } if *button == MouseButton::Left => {
                if let Some(row) = self.choice_row.take() {
                    let r = self.choice_rect(row);
                    ctx.unregister_overlay();
                    if r.contains(*position) {
                        let i = ((position.y - r.y()) / self.row_height) as usize;
                        if let PropertyValue::Choice(items, _) = &self.properties[row].value {
                            if i < items.len() {
                                let pv = PropertyValue::Choice(items.clone(), i);
                                self.properties[row].value = pv.clone();
                                self.fire_change(row, pv);
                            }
                        }
                        self.choice_hover = None;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    self.choice_hover = None;
                    ctx.request_paint();
                }
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
                        self.cp_val =
                            1.0 - ((position.y - sv.y()) / sv.size.height).clamp(0.0, 1.0);
                        self.cp_update_color();
                        ctx.request_paint();
                        return EventResult::Handled;
                    }

                    let hb = self.cp_hue_rect(popup);
                    if hb.contains(*position) {
                        self.cp_drag = CpDragTarget::HueBar;
                        self.cp_hue =
                            ((position.y - hb.y()) / hb.size.height).clamp(0.0, 1.0) * 360.0;
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
                    if self.properties[row_idx].mark.is_some() && self.mark_rect(row_idx).contains(*position) {
                        if let Some(cb) = &self.on_mark {
                            if let Ok(mut f) = cb.lock() {
                                f(row_idx);
                            }
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    let value_x = self.bounds.x() + label_w;

                    if position.x >= value_x {
                        match self.properties[row_idx].value.clone() {
                            PropertyValue::Header => return EventResult::Handled,
                            PropertyValue::Action(label) => {
                                self.fire_change(row_idx, PropertyValue::Action(label));
                                ctx.request_paint();
                                return EventResult::Handled;
                            }
                            PropertyValue::Range { value, .. } => {
                                if self.editing_row.is_some() {
                                    self.commit_edit();
                                }
                                self.range_drag = Some((row_idx, position.x, value, false));
                                ctx.request_paint();
                                return EventResult::Handled;
                            }
                            PropertyValue::Choice(..) => {
                                if self.editing_row.is_some() {
                                    self.commit_edit();
                                }
                                self.choice_row = Some(row_idx);
                                self.choice_hover = None;
                                ctx.register_overlay(self.choice_rect(row_idx), false);
                                ctx.request_paint();
                                return EventResult::Handled;
                            }
                            _ => {}
                        }
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
                                    Size::new(
                                        popup.size.width,
                                        popup.y() + popup.size.height - self.bounds.y(),
                                    ),
                                );
                                ctx.register_overlay(overlay, false);
                            }
                            ctx.request_paint();
                            return EventResult::Handled;
                        }

                        if self.editing_row == Some(row_idx) {
                            // клик внутри уже редактируемого поля: ставим каретку в точку клика
                            let hit = self.hit_test_edit_pos(row_idx, *position);
                            if ctx.modifiers.shift {
                                if self.edit_sel_anchor.is_none() {
                                    self.edit_sel_anchor = Some(self.edit_cursor);
                                }
                            } else {
                                self.edit_sel_anchor = Some(hit);
                            }
                            self.edit_cursor = hit;
                            self.edit_selecting = true;
                            self.edit_preferred_x = None;
                            self.ensure_edit_cursor_visible();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        if self.editing_row.is_some() {
                            self.commit_edit();
                        }
                        self.start_edit(row_idx);
                        let hit = self.hit_test_edit_pos(row_idx, *position);
                        self.edit_cursor = hit;
                        self.edit_sel_anchor = Some(hit);
                        self.edit_selecting = true;
                        self.ensure_edit_cursor_visible();
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
            Event::MouseWheel {
                delta, position, ..
            } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                // внутренняя прокрутка длинного многострочного поля под курсором
                if let Some(row) = self.editing_row {
                    if self.is_row_multiline(row) && self.row_at_y(position.y) == Some(row) {
                        let lines = self.wrap_lines(&self.edit_buffer, self.value_inner_w());
                        let total_h = lines.len() as f32 * Self::MULTILINE_LINE_H;
                        let vis_h = (self.row_height_for(row) - Self::MULTILINE_PAD_Y * 2.0).max(Self::MULTILINE_LINE_H);
                        let max_sy = (total_h - vis_h).max(0.0);
                        if max_sy > 0.5 {
                            let next = (self.edit_scroll_y - *delta).clamp(0.0, max_sy);
                            if (next - self.edit_scroll_y).abs() > 0.01 {
                                self.edit_scroll_y = next;
                                ctx.request_paint();
                                return EventResult::Handled;
                            }
                        }
                    }
                }
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
            Event::KeyDown(key) if self.add_mode => match key {
                crate::input::Key::Backspace => {
                    if self.add_cursor > 0 {
                        let prev = self.add_buffer[..self.add_cursor]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
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
                        self.add_hover =
                            Some(self.add_hover.map(|h| (h + 1).min(max)).unwrap_or(0));
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
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
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
            },
            Event::CharInput(ch) if self.editing_row.is_some() => {
                if !ch.is_control() && !ctx.modifiers.ctrl && !ctx.modifiers.alt {
                    let mut buf = [0u8; 4];
                    let s = ch.encode_utf8(&mut buf);
                    self.insert_edit_str(s);
                    self.ensure_edit_cursor_visible();
                    self.sync_live_multiline(ctx);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(key) if self.editing_row.is_some() => {
                use crate::input::Key;
                let shift = ctx.modifiers.shift;
                let ctrl = ctx.modifiers.ctrl;
                if ctrl && !ctx.modifiers.alt {
                    match key {
                        Key::A => {
                            self.edit_sel_anchor = Some(0);
                            self.edit_cursor = self.edit_buffer.len();
                            self.edit_preferred_x = None;
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::C => {
                            if let Some((a, b)) = self.edit_selection_range() {
                                ctx.copy_to_clipboard(&self.edit_buffer[a..b]);
                            }
                            return EventResult::Handled;
                        }
                        Key::X => {
                            if let Some((a, b)) = self.edit_selection_range() {
                                ctx.copy_to_clipboard(&self.edit_buffer[a..b]);
                                self.delete_edit_selection();
                                self.ensure_edit_cursor_visible();
                                self.sync_live_multiline(ctx);
                                ctx.request_paint();
                            }
                            return EventResult::Handled;
                        }
                        Key::V => {
                            if let Some(raw) = ctx.paste_from_clipboard() {
                                let clean = if self.is_editing_multiline() {
                                    raw.replace("\r\n", "\n").replace('\r', "\n")
                                } else {
                                    raw.replace("\r\n", " ").replace(['\r', '\n'], " ")
                                };
                                self.insert_edit_str(&clean);
                                self.ensure_edit_cursor_visible();
                                self.sync_live_multiline(ctx);
                                ctx.request_paint();
                            }
                            return EventResult::Handled;
                        }
                        Key::Enter => {
                            self.commit_edit();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::Left => {
                            if shift && self.edit_sel_anchor.is_none() {
                                self.edit_sel_anchor = Some(self.edit_cursor);
                            } else if !shift {
                                self.edit_sel_anchor = None;
                            }
                            self.edit_cursor = Self::prev_word_boundary(&self.edit_buffer, self.edit_cursor);
                            self.edit_preferred_x = None;
                            self.ensure_edit_cursor_visible();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::Right => {
                            if shift && self.edit_sel_anchor.is_none() {
                                self.edit_sel_anchor = Some(self.edit_cursor);
                            } else if !shift {
                                self.edit_sel_anchor = None;
                            }
                            self.edit_cursor = Self::next_word_boundary(&self.edit_buffer, self.edit_cursor);
                            self.edit_preferred_x = None;
                            self.ensure_edit_cursor_visible();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::Backspace => {
                            if !self.delete_edit_selection() && self.edit_cursor > 0 {
                                let prev = Self::prev_word_boundary(&self.edit_buffer, self.edit_cursor);
                                self.edit_buffer.drain(prev..self.edit_cursor);
                                self.edit_cursor = prev;
                            }
                            self.edit_preferred_x = None;
                            self.ensure_edit_cursor_visible();
                            self.sync_live_multiline(ctx);
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::Delete => {
                            if !self.delete_edit_selection() && self.edit_cursor < self.edit_buffer.len() {
                                let next = Self::next_word_boundary(&self.edit_buffer, self.edit_cursor);
                                self.edit_buffer.drain(self.edit_cursor..next);
                            }
                            self.edit_preferred_x = None;
                            self.ensure_edit_cursor_visible();
                            self.sync_live_multiline(ctx);
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::Home => {
                            if shift && self.edit_sel_anchor.is_none() {
                                self.edit_sel_anchor = Some(self.edit_cursor);
                            } else if !shift {
                                self.edit_sel_anchor = None;
                            }
                            self.edit_cursor = 0;
                            self.edit_preferred_x = None;
                            self.ensure_edit_cursor_visible();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        Key::End => {
                            if shift && self.edit_sel_anchor.is_none() {
                                self.edit_sel_anchor = Some(self.edit_cursor);
                            } else if !shift {
                                self.edit_sel_anchor = None;
                            }
                            self.edit_cursor = self.edit_buffer.len();
                            self.edit_preferred_x = None;
                            self.ensure_edit_cursor_visible();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        _ => {}
                    }
                }
                match key {
                    Key::Backspace => {
                        if !self.delete_edit_selection() && self.edit_cursor > 0 {
                            let prev = self.edit_buffer[..self.edit_cursor]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            self.edit_buffer.remove(prev);
                            self.edit_cursor = prev;
                        }
                        self.edit_preferred_x = None;
                        self.ensure_edit_cursor_visible();
                        self.sync_live_multiline(ctx);
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Delete => {
                        if !self.delete_edit_selection() && self.edit_cursor < self.edit_buffer.len() {
                            self.edit_buffer.remove(self.edit_cursor);
                        }
                        self.edit_preferred_x = None;
                        self.ensure_edit_cursor_visible();
                        self.sync_live_multiline(ctx);
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Home => {
                        if shift && self.edit_sel_anchor.is_none() {
                            self.edit_sel_anchor = Some(self.edit_cursor);
                        } else if !shift {
                            self.edit_sel_anchor = None;
                        }
                        if self.is_editing_multiline() {
                            let lines = self.wrap_lines(&self.edit_buffer, self.value_inner_w());
                            let vi = Self::cursor_visual_line(&lines, self.edit_cursor);
                            self.edit_cursor = lines[vi].start;
                        } else {
                            self.edit_cursor = 0;
                        }
                        self.edit_preferred_x = None;
                        self.ensure_edit_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::End => {
                        if shift && self.edit_sel_anchor.is_none() {
                            self.edit_sel_anchor = Some(self.edit_cursor);
                        } else if !shift {
                            self.edit_sel_anchor = None;
                        }
                        if self.is_editing_multiline() {
                            let lines = self.wrap_lines(&self.edit_buffer, self.value_inner_w());
                            let vi = Self::cursor_visual_line(&lines, self.edit_cursor);
                            self.edit_cursor = lines[vi].end;
                        } else {
                            self.edit_cursor = self.edit_buffer.len();
                        }
                        self.edit_preferred_x = None;
                        self.ensure_edit_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Left => {
                        if !shift {
                            if let Some((a, _)) = self.edit_selection_range() {
                                self.edit_cursor = a;
                                self.edit_sel_anchor = None;
                                self.edit_preferred_x = None;
                                self.ensure_edit_cursor_visible();
                                ctx.request_paint();
                                return EventResult::Handled;
                            }
                            self.edit_sel_anchor = None;
                        } else if self.edit_sel_anchor.is_none() {
                            self.edit_sel_anchor = Some(self.edit_cursor);
                        }
                        if self.edit_cursor > 0 {
                            self.edit_cursor = self.edit_buffer[..self.edit_cursor]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                        }
                        self.edit_preferred_x = None;
                        self.ensure_edit_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Right => {
                        if !shift {
                            if let Some((_, b)) = self.edit_selection_range() {
                                self.edit_cursor = b;
                                self.edit_sel_anchor = None;
                                self.edit_preferred_x = None;
                                self.ensure_edit_cursor_visible();
                                ctx.request_paint();
                                return EventResult::Handled;
                            }
                            self.edit_sel_anchor = None;
                        } else if self.edit_sel_anchor.is_none() {
                            self.edit_sel_anchor = Some(self.edit_cursor);
                        }
                        if self.edit_cursor < self.edit_buffer.len() {
                            let ch = self.edit_buffer[self.edit_cursor..].chars().next().unwrap();
                            self.edit_cursor += ch.len_utf8();
                        }
                        self.edit_preferred_x = None;
                        self.ensure_edit_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Up => {
                        self.move_cursor_vertical(-1, shift);
                        self.ensure_edit_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Down => {
                        self.move_cursor_vertical(1, shift);
                        self.ensure_edit_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Escape => {
                        self.cancel_edit(ctx);
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Enter => {
                        if self.is_editing_multiline() {
                            self.insert_edit_str("\n");
                            self.ensure_edit_cursor_visible();
                            self.sync_live_multiline(ctx);
                        } else {
                            self.commit_edit();
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    k if k.types_text() && !ctrl && !ctx.modifiers.alt => EventResult::Handled,
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

    fn accessibility_children(&self) -> Vec<(crate::a11y::AccessibilityInfo, Rect)> {
        use crate::a11y::{AccessibilityInfo, NodeProperties, NodeState, Role};
        let label_w = self.label_w();
        let mut out = Vec::new();
        for (i, p) in self.properties.iter().enumerate() {
            let y = self.row_y(i);
            let rh = self.row_height_for(i);
            if y + rh < self.bounds.y() || y > self.bounds.y() + self.bounds.size.height {
                continue;
            }
            let full = Rect::new(Point::new(self.bounds.x(), y), Size::new(self.bounds.size.width, rh));
            let val = Rect::new(Point::new(self.bounds.x() + label_w, y), Size::new(self.bounds.size.width - label_w, rh));
            let (role, label, value, rect, checked) = match &p.value {
                PropertyValue::Header => (Role::Heading(3), p.name.clone(), None, full, None),
                PropertyValue::Action(l) => (Role::Button, if p.name.is_empty() { l.clone() } else { format!("{}: {l}", p.name) }, None, val, None),
                PropertyValue::Range { .. } => (Role::Slider, p.name.clone(), Some(p.value.display()), val, None),
                PropertyValue::Choice(..) => (Role::ComboBox, p.name.clone(), Some(p.value.display()), val, None),
                PropertyValue::Bool(b) => (Role::CheckBox, p.name.clone(), None, val, Some(*b)),
                _ => (Role::TextField, p.name.clone(), Some(p.value.display()), val, None),
            };
            let state = NodeState { checked, expanded: matches!(p.value, PropertyValue::Choice(..)).then_some(self.choice_row == Some(i)), ..Default::default() };
            out.push((AccessibilityInfo { role, state, properties: NodeProperties { label: Some(label), value, ..Default::default() } }, rect));
            if let Some(m) = p.mark {
                let st = NodeState { pressed: m == crate::widgets::data::PropertyMark::Full, ..Default::default() };
                out.push((AccessibilityInfo { role: Role::Button, state: st, properties: NodeProperties { label: Some(format!("Ключ: {}", p.name)), ..Default::default() } }, self.mark_rect(i)));
            }
        }
        // пункты открытого списка
        if let Some(row) = self.choice_row {
            if let PropertyValue::Choice(items, sel) = &self.properties[row].value {
                let r = self.choice_rect(row);
                for (j, it) in items.iter().enumerate() {
                    let ir = Rect::new(Point::new(r.x(), r.y() + j as f32 * self.row_height), Size::new(r.size.width, self.row_height));
                    out.push((AccessibilityInfo { role: Role::Button, state: NodeState { selected: j == *sel, ..Default::default() }, properties: NodeProperties { label: Some(it.clone()), ..Default::default() } }, ir));
                }
            }
        }
        out
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
        "PropertyGrid"
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }
    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }
    fn mss_mut(&mut self) -> Option<&mut crate::mss::MssFields> {
        Some(&mut self.mss)
    }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        if let Some(w) = style.width() {
            self.fixed_width = Some(w);
        }
        if let Some(h) = style.height() {
            self.fixed_height = Some(h);
        }
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
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }
}

impl StyledElement for PropertyGridElement {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Key, Modifiers};
    use std::sync::Arc;

    #[test]
    fn multiline_wraps_grows_and_edits_cyrillic_and_emoji() {
        let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log2 = log.clone();
        let long = "Энергообмен должен быть честным: я взращиваю тебе тело к жизни, а ты — меня к сознанию 🥰🤗😂";
        let grid = PropertyGrid::new()
            .properties(vec![
                Property::header("Титр"),
                Property::multiline("Текст", long),
                Property::range("Размер", 108.0, 12.0, 400.0, 1.0),
            ])
            .label_width(120.0)
            .row_height(28.0)
            .auto_height(true)
            .on_change(move |_row, val| {
                if let PropertyValue::Multiline(s) = val {
                    log2.lock().unwrap().push(s);
                }
            });
        let mut boxed = grid.create_element();
        let sz0 = boxed.layout(Constraints::new(0.0, 320.0, 0.0, 1000.0));
        let el = boxed.as_any_mut().unwrap().downcast_mut::<PropertyGridElement>().unwrap();
        let rh_multi = el.row_height_for(1);
        assert!(rh_multi > 28.0 * 2.0, "многострочная строка должна расти по переносу: {rh_multi}");
        assert!((sz0.height - (28.0 + rh_multi + 28.0)).abs() < 0.5, "auto_height={}", sz0.height);

        // клик по ячейке значения включает редактирование
        let mut ctx = EventContext::new(el.id());
        let click_pos = Point::new(150.0, el.row_y(1) + 10.0);
        assert_eq!(
            el.handle_event(&Event::MouseDown { button: MouseButton::Left, position: click_pos }, &mut ctx),
            EventResult::Handled
        );
        assert_eq!(el.editing_row, Some(1));
        el.handle_event(&Event::MouseUp { button: MouseButton::Left, position: click_pos }, &mut ctx);

        // End → Enter → ввод новой строки
        assert_eq!(el.handle_event(&Event::KeyDown(Key::End), &mut ctx), EventResult::Handled);
        // Ctrl+End переносит в самый конец текста
        ctx.modifiers = Modifiers { ctrl: true, ..Modifiers::empty() };
        assert_eq!(el.handle_event(&Event::KeyDown(Key::End), &mut ctx), EventResult::Handled);
        ctx.modifiers = Modifiers::empty();
        assert_eq!(el.edit_cursor, long.len());
        assert_eq!(el.handle_event(&Event::KeyDown(Key::Enter), &mut ctx), EventResult::Handled);
        assert_eq!(el.handle_event(&Event::CharInput('!'), &mut ctx), EventResult::Handled);
        assert!(el.edit_buffer.ends_with("\n!"));
        assert_eq!(log.lock().unwrap().last().map(String::as_str), Some(el.edit_buffer.as_str()));

        // высота после добавления новой строки выросла
        let sz1 = el.layout(Constraints::new(0.0, 320.0, 0.0, 1000.0));
        assert!(sz1.height > sz0.height, "после Enter высота должна вырасти: {sz0:?} -> {sz1:?}");

        // Up/Down обрабатываются внутри и двигают каретку по визуальным строкам
        let cur_bottom = el.edit_cursor;
        assert_eq!(el.handle_event(&Event::KeyDown(Key::Up), &mut ctx), EventResult::Handled);
        assert!(el.edit_cursor < cur_bottom);

        // Ctrl+A + Backspace очищает весь буфер
        ctx.modifiers = Modifiers { ctrl: true, ..Modifiers::empty() };
        assert_eq!(el.handle_event(&Event::KeyDown(Key::A), &mut ctx), EventResult::Handled);
        ctx.modifiers = Modifiers::empty();
        assert_eq!(el.handle_event(&Event::KeyDown(Key::Backspace), &mut ctx), EventResult::Handled);
        assert_eq!(el.edit_buffer, "");

        // Escape откатывает к исходному тексту
        assert_eq!(el.handle_event(&Event::KeyDown(Key::Escape), &mut ctx), EventResult::Handled);
        assert_eq!(el.editing_row, None);
        assert_eq!(el.properties[1].value.display(), long);

        // повторный вход + ввод + Ctrl+Enter фиксирует правку
        el.start_edit(1);
        assert_eq!(el.handle_event(&Event::KeyDown(Key::Enter), &mut ctx), EventResult::Handled);
        assert_eq!(el.handle_event(&Event::CharInput('2'), &mut ctx), EventResult::Handled);
        ctx.modifiers = Modifiers { ctrl: true, ..Modifiers::empty() };
        assert_eq!(el.handle_event(&Event::KeyDown(Key::Enter), &mut ctx), EventResult::Handled);
        assert_eq!(el.editing_row, None);
        assert!(el.properties[1].value.display().ends_with("\n2"));
    }
}
