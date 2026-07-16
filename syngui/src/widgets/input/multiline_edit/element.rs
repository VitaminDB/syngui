use super::MultilineTextEdit;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::mss::{MssFields, TextAlign, TextDecoration};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt, TextMeasure};
use crate::widget::selection::TextSelectionState;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

impl Widget for MultilineTextEdit {
    fn create_element(&self) -> Box<dyn Element> {
        let lines: Vec<&str> = self.text.split('\n').collect();
        let cursor_col = lines.last().map(|l| l.chars().count()).unwrap_or(0);
        let cursor_line = if lines.is_empty() { 0 } else { lines.len() - 1 };

        Box::new(MultilineTextEditElement {
            id: ElementId::new(),
            text: self.text.clone(),
            placeholder: self.placeholder.clone(),
            rows: self.rows,
            read_only: self.read_only,
            show_line_numbers: self.show_line_numbers,
            soft_wrap: self.soft_wrap,
            auto_height: self.auto_height,
            max_rows: self.max_rows,
            bounds: Rect::zero(),
            focused: false,
            cursor_line,
            cursor_col,
            scroll_offset: 0,
            selection: TextSelectionState::new(),
            on_change: self.on_change.clone(),
            submit_on_enter: self.submit_on_enter,
            on_submit: self.on_submit.clone(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            hover: false,
            text_measure: None,
            wrap_cache: Vec::new(),
            text_area_width: 0.0,
            total_visual_lines: 1,
            hover_scrollbar: false,
            dragging_scrollbar: false,
            drag_start_y: 0.0,
            drag_start_offset: 0,
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

struct MultilineTextEditElement {
    id: ElementId,
    text: String,
    placeholder: String,
    rows: usize,
    read_only: bool,
    show_line_numbers: bool,
    soft_wrap: bool,
    auto_height: bool,
    max_rows: Option<usize>,
    bounds: Rect,
    focused: bool,
    cursor_line: usize,
    cursor_col: usize,
    scroll_offset: usize,
    selection: TextSelectionState,
    on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    submit_on_enter: bool,
    on_submit: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    hover: bool,
    text_measure: Option<Arc<dyn TextMeasure>>,
    wrap_cache: Vec<Vec<usize>>,
    text_area_width: f32,
    total_visual_lines: usize,
    hover_scrollbar: bool,
    dragging_scrollbar: bool,
    drag_start_y: f32,
    drag_start_offset: usize,
}

impl MultilineTextEditElement {
    fn start_transition_to_current_state(&mut self) {
        self.mss.start_transition_to(self.hover, false, self.focused, false);
    }

    fn effective_border_color(&self) -> Option<Color> {
        self.mss.transition.border_color()
            .or_else(|| {
                self.mss.target_props(self.hover, false, self.focused, false).border_color()
            })
            .or(self.mss.border_color)
    }

    const LINE_HEIGHT: f32 = 20.0;
    const GUTTER_WIDTH: f32 = 40.0;
    const PADDING: f32 = 8.0;
    const FONT_SIZE: f32 = 14.0;
    const SCROLLBAR_WIDTH: f32 = 6.0;
    const SCROLLBAR_MIN_THUMB: f32 = 20.0;

    fn lines(&self) -> Vec<&str> {
        self.text.split('\n').collect()
    }

    fn line_count(&self) -> usize {
        self.text.split('\n').count().max(1)
    }

    fn cursor_byte_offset(&self) -> usize {
        self.line_col_to_byte(self.cursor_line, self.cursor_col)
    }

    fn line_col_to_byte(&self, line: usize, col: usize) -> usize {
        let mut offset = 0;
        for (i, l) in self.text.split('\n').enumerate() {
            if i == line {
                let c = col.min(l.chars().count());
                offset += l.char_indices()
                    .nth(c)
                    .map(|(idx, _)| idx)
                    .unwrap_or(l.len());
                return offset;
            }
            offset += l.len() + 1;
        }
        self.text.len()
    }

    fn byte_to_line_col(&self, byte_offset: usize) -> (usize, usize) {
        let byte_offset = byte_offset.min(self.text.len());
        let mut offset = 0;
        for (i, line) in self.text.split('\n').enumerate() {
            let line_end = offset + line.len();
            if byte_offset <= line_end {
                let local = byte_offset - offset;
                let safe_local = if line.is_char_boundary(local) {
                    local
                } else {
                    (0..local).rev().find(|&b| line.is_char_boundary(b)).unwrap_or(0)
                };
                let col = line[..safe_local].chars().count();
                return (i, col);
            }
            offset = line_end + 1;
        }
        let lc = self.line_count();
        (lc.saturating_sub(1), self.line_char_count(lc.saturating_sub(1)))
    }

    fn line_char_count(&self, line_idx: usize) -> usize {
        self.text.split('\n')
            .nth(line_idx)
            .map(|l| l.chars().count())
            .unwrap_or(0)
    }

    fn trigger_change(&mut self) {
        if let Some(ref callback) = self.on_change {
            if let Ok(mut cb) = callback.lock() {
                cb(&self.text);
            }
        }
    }

    fn visible_rows(&self) -> usize {
        if self.auto_height {
            let base = self.total_visual_lines.max(self.rows);
            match self.max_rows {
                Some(cap) => base.min(cap),
                None => base,
            }
        } else {
            let bounds_rows = ((self.bounds.height() - Self::PADDING * 2.0) / Self::LINE_HEIGHT)
                .floor()
                .max(0.0) as usize;
            bounds_rows.max(self.rows)
        }
    }

    fn ensure_cursor_visible(&mut self) {
        let vis_line = self.logical_to_visual_line(self.cursor_line, self.cursor_col);
        let rows = self.visible_rows();
        if vis_line < self.scroll_offset {
            self.scroll_offset = vis_line;
        } else if vis_line >= self.scroll_offset + rows {
            self.scroll_offset = vis_line - rows + 1;
        }
    }

    fn clamp_cursor_col(&mut self) {
        let max_col = self.line_char_count(self.cursor_line);
        self.cursor_col = self.cursor_col.min(max_col);
    }

    fn text_x_offset(&self) -> f32 {
        if self.show_line_numbers {
            Self::GUTTER_WIDTH + Self::PADDING
        } else {
            Self::PADDING
        }
    }

    fn needs_scrollbar(&self) -> bool {
        self.total_visual_lines > self.visible_rows()
    }

    fn right_padding(&self) -> f32 {
        if self.needs_scrollbar() {
            Self::SCROLLBAR_WIDTH + Self::PADDING
        } else {
            Self::PADDING
        }
    }

    fn scrollbar_track_rect(&self) -> Rect {
        let bw = self.mss.border_width.unwrap_or(1.0);
        Rect::new(
            Point::new(
                self.bounds.x() + self.bounds.width() - Self::SCROLLBAR_WIDTH - bw - 2.0,
                self.bounds.y() + Self::PADDING,
            ),
            Size::new(
                Self::SCROLLBAR_WIDTH,
                self.bounds.height() - Self::PADDING * 2.0,
            ),
        )
    }

    fn scrollbar_thumb_rect(&self) -> Rect {
        let track = self.scrollbar_track_rect();
        let total = self.total_visual_lines as f32;
        let visible = self.visible_rows() as f32;
        let max_offset = (self.total_visual_lines - self.visible_rows()) as f32;

        let thumb_h = (visible / total * track.height())
            .max(Self::SCROLLBAR_MIN_THUMB)
            .min(track.height());
        let available = track.height() - thumb_h;
        let thumb_y = if max_offset > 0.0 {
            track.y() + (self.scroll_offset as f32 / max_offset) * available
        } else {
            track.y()
        };

        Rect::new(
            Point::new(track.x(), thumb_y),
            Size::new(Self::SCROLLBAR_WIDTH, thumb_h),
        )
    }

    fn sync_cursor_from_byte(&mut self, byte_offset: usize) {
        let (line, col) = self.byte_to_line_col(byte_offset);
        self.cursor_line = line;
        self.cursor_col = col;
    }

    fn line_start_byte(&self, line_idx: usize) -> usize {
        let mut offset = 0;
        for (i, l) in self.text.split('\n').enumerate() {
            if i == line_idx {
                return offset;
            }
            offset += l.len() + 1;
        }
        self.text.len()
    }

    fn recompute_wraps(&mut self) {
        let old_visual_lines = self.total_visual_lines;
        self.wrap_cache.clear();
        let avail = self.text_area_width;
        if avail <= 0.0 || !self.soft_wrap || self.text_measure.is_none() {
            let lc = self.line_count();
            self.wrap_cache.resize(lc, vec![]);
            self.total_visual_lines = lc;
        } else {
            let tm = self.text_measure.as_ref().unwrap().clone();
            let mut total = 0usize;
            for line in self.text.split('\n') {
                let breaks = Self::word_wrap_breaks(line, avail, &*tm);
                total += breaks.len() + 1;
                self.wrap_cache.push(breaks);
            }
            self.total_visual_lines = total.max(1);
        }
        if self.auto_height && self.total_visual_lines != old_visual_lines {
            self.mark_dirty(DirtyFlags::LAYOUT);
        }
    }

    fn word_wrap_breaks(line: &str, avail_width: f32, tm: &dyn TextMeasure) -> Vec<usize> {
        if line.is_empty() {
            return vec![];
        }
        let chars: Vec<char> = line.chars().collect();
        let full_width = tm.measure_text_width(line, Self::FONT_SIZE, chars.len());
        if full_width <= avail_width {
            return vec![];
        }

        let mut breaks = Vec::new();
        let mut seg_start: usize = 0;
        let mut last_space: Option<usize> = None;

        for i in 0..chars.len() {
            if chars[i] == ' ' {
                last_space = Some(i);
            }

            let seg: String = chars[seg_start..=i].iter().collect();
            let seg_width = tm.measure_text_width(&seg, Self::FONT_SIZE, i + 1 - seg_start);

            if seg_width > avail_width && i > seg_start {
                if let Some(space_idx) = last_space {
                    if space_idx >= seg_start {
                        let break_at = space_idx + 1;
                        breaks.push(break_at);
                        seg_start = break_at;
                        last_space = None;
                        continue;
                    }
                }
                breaks.push(i);
                seg_start = i;
                last_space = None;
            }
        }
        breaks
    }

    fn visual_lines_for(&self, logical_idx: usize) -> usize {
        self.wrap_cache.get(logical_idx).map(|b| b.len() + 1).unwrap_or(1)
    }

    fn logical_to_visual_line(&self, logical_line: usize, col: usize) -> usize {
        let mut visual = 0;
        for i in 0..logical_line.min(self.wrap_cache.len()) {
            visual += self.visual_lines_for(i);
        }
        if let Some(breaks) = self.wrap_cache.get(logical_line) {
            for (sub, &break_char) in breaks.iter().enumerate() {
                if col < break_char {
                    return visual + sub;
                }
            }
            visual + breaks.len()
        } else {
            visual
        }
    }

    fn visual_to_logical(&self, visual_line: usize) -> (usize, usize, usize) {
        let mut remaining = visual_line;
        for (logical, breaks) in self.wrap_cache.iter().enumerate() {
            let vlines = breaks.len() + 1;
            if remaining < vlines {
                let start = if remaining == 0 { 0 } else { breaks[remaining - 1] };
                let end = breaks.get(remaining).copied()
                    .unwrap_or_else(|| self.line_char_count(logical));
                return (logical, start, end);
            }
            remaining -= vlines;
        }
        let last = self.line_count().saturating_sub(1);
        (last, 0, self.line_char_count(last))
    }

    fn segment_text<'a>(&self, line: &'a str, logical_idx: usize, seg_start: usize, seg_end: usize) -> String {
        let _ = logical_idx;
        line.chars().skip(seg_start).take(seg_end - seg_start).collect()
    }

    fn click_to_cursor(&self, pos: Point, ctx: &EventContext) -> (usize, usize) {
        let text_x = self.bounds.x() + self.text_x_offset();
        let rel_y = pos.y - self.bounds.y() - Self::PADDING;
        let vis_row = self.scroll_offset + (rel_y / Self::LINE_HEIGHT).max(0.0) as usize;
        let vis_row = vis_row.min(self.total_visual_lines.saturating_sub(1));
        let (logical_line, seg_start, seg_end) = self.visual_to_logical(vis_row);

        let line = self.text.split('\n').nth(logical_line).unwrap_or("");
        let segment = self.segment_text(line, logical_line, seg_start, seg_end);
        let rel_x = (pos.x - text_x).max(0.0);

        let local_col = ctx.hit_test_char(&segment, Self::FONT_SIZE, rel_x)
            .unwrap_or_else(|| {
                let avg_advance = Self::FONT_SIZE * 0.52;
                ((rel_x / avg_advance).round() as usize).min(segment.chars().count())
            });
        (logical_line, seg_start + local_col)
    }
}

impl Element for MultilineTextEditElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(mte) = widget.as_any().downcast_ref::<MultilineTextEdit>() {
            self.placeholder = mte.placeholder.clone();
            self.rows = mte.rows;
            self.read_only = mte.read_only;
            self.show_line_numbers = mte.show_line_numbers;
            self.soft_wrap = mte.soft_wrap;
            self.auto_height = mte.auto_height;
            self.max_rows = mte.max_rows;
            if mte.text != self.text {
                self.text = mte.text.clone();
                self.recompute_wraps();
                let line_count = self.line_count();
                self.cursor_line = self.cursor_line.min(line_count.saturating_sub(1));
                self.clamp_cursor_col();
                self.selection.clear();
                if self.read_only {
                    let rows = self.visible_rows();
                    self.scroll_offset = self.total_visual_lines.saturating_sub(rows);
                }
            }
            self.on_change = mte.on_change.clone();
            self.submit_on_enter = mte.submit_on_enter;
            self.on_submit = mte.on_submit.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        const FALLBACK_WIDTH: f32 = 200.0;
        let width = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            self.mss
                .min_width
                .and_then(|d| d.resolve_opt(constraints.max_width))
                .unwrap_or(FALLBACK_WIDTH)
        };
        let new_text_area_width = width - self.text_x_offset() - self.right_padding();
        if (new_text_area_width - self.text_area_width).abs() > 0.5 {
            self.text_area_width = new_text_area_width;
            self.recompute_wraps();
        }
        let rows_height = self.visible_rows() as f32 * Self::LINE_HEIGHT + Self::PADDING * 2.0;
        let height = if constraints.max_height.is_finite() {
            constraints.max_height.max(rows_height.min(constraints.max_height))
        } else {
            rows_height
        };
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        let bg = self.mss.background_color.unwrap_or(Color::WHITE);
        let fg = self.mss.color.unwrap_or(Color::from_hex("#374151"));
        let effective_bc = self.effective_border_color();
        let border_base = effective_bc.unwrap_or(Color::from_hex("#D1D5DB"));
        let radius = self.mss.border_radius_uniform(self.bounds.size.width.min(self.bounds.size.height), 6.0);

        let border_color = if self.focused {
            effective_bc.map(|c| c.lighten(0.3)).unwrap_or(primary)
        } else {
            border_base
        };
        let border_width = if self.focused { 2.0 } else { 1.0 };

        list.push_rect_bordered(
            self.bounds,
            bg,
            [radius; 4],
            Border { width: border_width, color: border_color },
        );

        if self.show_line_numbers {
            let bw = border_width;
            let gutter_rect = Rect::new(
                Point::new(self.bounds.x() + bw, self.bounds.y() + bw),
                Size::new(Self::GUTTER_WIDTH, self.bounds.size.height - bw * 2.0),
            );
            let gutter_bg = self.mss.gutter_color.unwrap_or_else(|| bg.darken(0.08));
            let inner_radius = (radius - bw).max(0.0);
            list.push_rect(gutter_rect, gutter_bg, [inner_radius, 0.0, 0.0, inner_radius]);
        }

        let lines = self.lines();
        let text_x = self.bounds.x() + self.text_x_offset();
        let text_width = self.bounds.size.width - self.text_x_offset() - self.right_padding();
        let placeholder_color = fg.with_alpha(0.5);
        let line_num_color = fg.with_alpha(0.4);

        let content_clip = Rect::new(
            Point::new(self.bounds.x(), self.bounds.y() + Self::PADDING),
            Size::new(self.bounds.size.width, self.bounds.size.height - Self::PADDING * 2.0),
        );
        list.push_clip(content_clip);

        if self.text.is_empty() && !self.focused {
            let placeholder_rect = Rect::new(
                Point::new(text_x, self.bounds.y() + Self::PADDING),
                Size::new(text_width, Self::LINE_HEIGHT),
            );
            list.push_text(&self.placeholder, placeholder_rect, placeholder_color, Self::FONT_SIZE);
            list.pop_clip();
            return;
        }

        let sel_range = self.selection.range(self.cursor_byte_offset());
        let sel_color = self.mss.selection_color_or_default();
        let cursor_color = self.mss.caret_color
            .unwrap_or_else(|| self.effective_border_color()
                .map(|c| c.lighten(0.3))
                .unwrap_or(primary));

        let visible_end = (self.scroll_offset + self.visible_rows()).min(self.total_visual_lines);
        for vis_row in self.scroll_offset..visible_end {
            let (logical_line, seg_start, seg_end) = self.visual_to_logical(vis_row);
            let y = self.bounds.y() + Self::PADDING
                + (vis_row - self.scroll_offset) as f32 * Self::LINE_HEIGHT;

            let line_text = lines.get(logical_line).copied().unwrap_or("");

            if self.show_line_numbers && seg_start == 0 {
                let num_str = format!("{}", logical_line + 1);
                let num_rect = Rect::new(
                    Point::new(self.bounds.x() + 4.0, y),
                    Size::new(Self::GUTTER_WIDTH - 12.0, Self::LINE_HEIGHT),
                );
                list.push_text_aligned(&num_str, num_rect, line_num_color, 12.0, TextAlign::RIGHT | TextAlign::VCENTER, TextDecoration::None, 400);
            }

            let segment = self.segment_text(line_text, logical_line, seg_start, seg_end);

            if let Some((sel_s, sel_e)) = sel_range {
                let line_start_byte = self.line_start_byte(logical_line);
                let seg_start_byte = line_text.char_indices()
                    .nth(seg_start).map(|(i, _)| i).unwrap_or(line_text.len());
                let seg_end_byte = line_text.char_indices()
                    .nth(seg_end).map(|(i, _)| i).unwrap_or(line_text.len());
                let abs_seg_start = line_start_byte + seg_start_byte;
                let abs_seg_end = line_start_byte + seg_end_byte;

                if sel_s < abs_seg_end && sel_e > abs_seg_start {
                    let local_start = if sel_s > abs_seg_start { sel_s - abs_seg_start } else { 0 };
                    let local_end = if sel_e < abs_seg_end { sel_e - abs_seg_start } else { segment.len() };
                    list.push_text_selection_styled(
                        &segment, local_start, local_end,
                        text_x, y, Self::LINE_HEIGHT, Self::FONT_SIZE, sel_color,
                        self.mss.font_family.clone(),
                    );
                }
            }

            let seg_rect = Rect::new(
                Point::new(text_x, y),
                Size::new(10000.0, Self::LINE_HEIGHT),
            );
            list.push_text(&segment, seg_rect, fg, Self::FONT_SIZE);

            if self.focused && self.cursor_line == logical_line
                && self.cursor_col >= seg_start && self.cursor_col <= seg_end
            {
                let local_col = self.cursor_col - seg_start;
                let text_before: String = segment.chars().take(local_col).collect();
                list.push_text_cursor_styled(
                    &text_before, text_before.len(),
                    text_x, y, Self::LINE_HEIGHT, Self::FONT_SIZE,
                    self.mss.font_weight_or(400), cursor_color,
                    self.mss.font_family.clone(),
                );
            }
        }

        list.pop_clip();

        if self.needs_scrollbar() {
            let radius = [Self::SCROLLBAR_WIDTH / 2.0; 4];
            let thumb_base = self.mss.color.unwrap_or(Color::from_hex("#9CA3AF"));

            if self.hover_scrollbar || self.dragging_scrollbar {
                let track_color = self.mss.border_color
                    .unwrap_or(Color::from_hex("#808080"))
                    .with_alpha(0.2);
                list.push_rect(self.scrollbar_track_rect(), track_color, radius);
            }

            let thumb_color = if self.dragging_scrollbar {
                thumb_base.darken(0.3).with_alpha(0.9)
            } else if self.hover_scrollbar {
                thumb_base.with_alpha(0.7)
            } else {
                thumb_base.with_alpha(0.4)
            };
            list.push_rect(self.scrollbar_thumb_rect(), thumb_color, radius);
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::FocusGained => {
                self.focused = true;
                self.start_transition_to_current_state();
                ctx.set_virtual_keyboard_visible(true);
                ctx.set_focused_text(self.text.clone());
                ctx.request_paint();
                return EventResult::Handled;
            }
            Event::FocusLost => {
                self.focused = false;
                self.selection.clear();
                self.start_transition_to_current_state();
                ctx.set_virtual_keyboard_visible(false);
                ctx.request_paint();
                return EventResult::Handled;
            }
            Event::MouseMove(pos) => {
                if self.mss.has_mss_styles {
                    let was_hover = self.hover;
                    self.hover = self.bounds.contains(*pos);
                    if self.hover != was_hover {
                        self.start_transition_to_current_state();
                        ctx.request_paint();
                    }
                }

                if self.dragging_scrollbar {
                    let track = self.scrollbar_track_rect();
                    let thumb_h = self.scrollbar_thumb_rect().height();
                    let available = track.height() - thumb_h;
                    if available > 0.0 {
                        let max_offset = self.total_visual_lines.saturating_sub(self.visible_rows());
                        let dy = pos.y - self.drag_start_y;
                        let new_offset = self.drag_start_offset as f32 + (dy / available) * max_offset as f32;
                        self.scroll_offset = (new_offset.round() as isize)
                            .max(0)
                            .min(max_offset as isize) as usize;
                    }
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.needs_scrollbar() && self.bounds.contains(*pos) {
                    let track = self.scrollbar_track_rect();
                    let hit_area = Rect::new(
                        Point::new(track.x() - 4.0, track.y()),
                        Size::new(track.width() + 8.0, track.height()),
                    );
                    let was_hover = self.hover_scrollbar;
                    self.hover_scrollbar = hit_area.contains(*pos);
                    if self.hover_scrollbar != was_hover {
                        ctx.request_paint();
                    }
                    if self.hover_scrollbar {
                        ctx.set_cursor(CursorIcon::Default);
                        return EventResult::Handled;
                    }
                }
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Text);
                }
                if self.selection.mouse_selecting && self.focused {
                    let (line, col) = self.click_to_cursor(*pos, ctx);
                    self.cursor_line = line;
                    self.cursor_col = col;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.bounds.contains(*pos) {
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.needs_scrollbar() {
                    let track = self.scrollbar_track_rect();
                    let hit_area = Rect::new(
                        Point::new(track.x() - 4.0, track.y()),
                        Size::new(track.width() + 8.0, track.height()),
                    );
                    if hit_area.contains(*position) {
                        let thumb = self.scrollbar_thumb_rect();
                        if thumb.contains(*position) {
                            self.dragging_scrollbar = true;
                            self.drag_start_y = position.y;
                            self.drag_start_offset = self.scroll_offset;
                        } else {
                            let max_offset = self.total_visual_lines.saturating_sub(self.visible_rows());
                            let ratio = (position.y - track.y()) / track.height();
                            self.scroll_offset = ((ratio * max_offset as f32).round() as usize)
                                .min(max_offset);
                        }
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    self.focused = true;
                    let (line, col) = self.click_to_cursor(*position, ctx);
                    self.cursor_line = line;
                    self.cursor_col = col;

                    let byte_pos = self.cursor_byte_offset();
                    if ctx.modifiers.shift {
                        self.selection.extend_or_start(byte_pos);
                    } else {
                        self.selection.clear();
                        self.selection.start(byte_pos);
                        self.selection.mouse_selecting = true;
                    }

                    ctx.set_cursor(CursorIcon::Text);
                    ctx.request_paint();
                    return EventResult::Handled;
                } else if self.focused {
                    self.focused = false;
                    self.selection.clear();
                    ctx.request_paint();
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } => {
                if *button == MouseButton::Left && self.dragging_scrollbar {
                    self.dragging_scrollbar = false;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if *button == MouseButton::Left && self.selection.mouse_selecting {
                    self.selection.mouse_selecting = false;
                    if !self.selection.has_selection(self.cursor_byte_offset()) {
                        self.selection.anchor = None;
                    }
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::DoubleClick { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    self.focused = true;
                    let (line, col) = self.click_to_cursor(*position, ctx);
                    self.cursor_line = line;
                    self.cursor_col = col;

                    let byte_offset = self.cursor_byte_offset();
                    let word_end = self.selection.select_word(&self.text, byte_offset);
                    self.sync_cursor_from_byte(word_end);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseWheel { delta, position, .. } => {
                if self.bounds.contains(*position) {
                    if *delta < 0.0 && self.scroll_offset + self.visible_rows() < self.total_visual_lines {
                        self.scroll_offset += 1;
                        ctx.request_paint();
                        return EventResult::Handled;
                    } else if *delta > 0.0 && self.scroll_offset > 0 {
                        self.scroll_offset -= 1;
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::KeyDown(key) => {
                if !self.focused {
                    return EventResult::Ignored;
                }

                let shift = ctx.modifiers.shift;
                let ctrl = ctx.modifiers.ctrl;

                if ctrl && matches!(key, Key::A) {
                    self.selection.select_all();
                    self.cursor_line = self.line_count().saturating_sub(1);
                    self.cursor_col = self.line_char_count(self.cursor_line);
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if ctrl && matches!(key, Key::C) {
                    let cursor_byte = self.cursor_byte_offset();
                    if let Some(selected) = self.selection.selected_text(&self.text, cursor_byte) {
                        ctx.copy_to_clipboard(selected);
                    }
                    return EventResult::Handled;
                }

                if ctrl && matches!(key, Key::X) && !self.read_only {
                    let mut cursor_byte = self.cursor_byte_offset();
                    if let Some(selected) = self.selection.selected_text(&self.text, cursor_byte) {
                        ctx.copy_to_clipboard(selected);
                        self.selection.delete_selection(&mut self.text, &mut cursor_byte);
                        self.sync_cursor_from_byte(cursor_byte);
                        self.recompute_wraps();
                        self.trigger_change();
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }

                if ctrl && matches!(key, Key::V) && !self.read_only {
                    if let Some(paste_text) = ctx.paste_from_clipboard() {
                        let mut cursor_byte = self.cursor_byte_offset();
                        self.selection.replace_selection(&mut self.text, &mut cursor_byte, &paste_text);
                        self.sync_cursor_from_byte(cursor_byte);
                        self.recompute_wraps();
                        self.ensure_cursor_visible();
                        self.trigger_change();
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }

                if self.read_only {
                    match key {
                        Key::Left | Key::Right | Key::Up | Key::Down | Key::Home | Key::End => {}
                        _ => return EventResult::Ignored,
                    }
                }

                match key {
                    Key::Enter => {
                        if self.read_only {
                            return EventResult::Ignored;
                        }
                        if self.submit_on_enter && !shift {
                            if let Some(cb) = self.on_submit.clone() {
                                if let Ok(mut f) = cb.lock() {
                                    f(&self.text);
                                }
                            }
                            return EventResult::Handled;
                        }
                        let cursor_byte = self.cursor_byte_offset();
                        if self.selection.has_selection(cursor_byte) {
                            self.selection.delete_selection(&mut self.text, &mut { cursor_byte });
                            self.sync_cursor_from_byte(cursor_byte);
                        }
                        let offset = self.cursor_byte_offset();
                        self.text.insert(offset, '\n');
                        self.cursor_line += 1;
                        self.cursor_col = 0;
                        self.selection.clear();
                        self.recompute_wraps();
                        self.ensure_cursor_visible();
                        self.trigger_change();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Backspace => {
                        if self.read_only {
                            return EventResult::Ignored;
                        }
                        let mut cursor_byte = self.cursor_byte_offset();
                        if self.selection.delete_selection(&mut self.text, &mut cursor_byte) {
                            self.sync_cursor_from_byte(cursor_byte);
                            self.recompute_wraps();
                            self.trigger_change();
                        } else if self.cursor_col > 0 {
                            let offset = self.cursor_byte_offset();
                            let prev = self.text[..offset]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            self.text.remove(prev);
                            self.cursor_col -= 1;
                            self.recompute_wraps();
                            self.trigger_change();
                        } else if self.cursor_line > 0 {
                            let prev_line_chars = self.line_char_count(self.cursor_line - 1);
                            let offset = self.cursor_byte_offset();
                            self.text.remove(offset - 1);
                            self.cursor_line -= 1;
                            self.cursor_col = prev_line_chars;
                            self.recompute_wraps();
                            self.ensure_cursor_visible();
                            self.trigger_change();
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Delete => {
                        if self.read_only {
                            return EventResult::Ignored;
                        }
                        let mut cursor_byte = self.cursor_byte_offset();
                        if self.selection.delete_selection(&mut self.text, &mut cursor_byte) {
                            self.sync_cursor_from_byte(cursor_byte);
                            self.recompute_wraps();
                            self.trigger_change();
                        } else {
                            let offset = self.cursor_byte_offset();
                            if offset < self.text.len() {
                                if self.text.is_char_boundary(offset) {
                                    self.text.remove(offset);
                                    self.recompute_wraps();
                                    self.trigger_change();
                                }
                            }
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Left => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_byte_offset());
                        } else if self.selection.has_selection(self.cursor_byte_offset()) {
                            if let Some((start, _)) = self.selection.range(self.cursor_byte_offset()) {
                                self.sync_cursor_from_byte(start);
                            }
                            self.selection.clear();
                            self.ensure_cursor_visible();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        if self.cursor_col > 0 {
                            self.cursor_col -= 1;
                        } else if self.cursor_line > 0 {
                            self.cursor_line -= 1;
                            self.cursor_col = self.line_char_count(self.cursor_line);
                        }
                        if !shift { self.selection.clear(); }
                        self.ensure_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Right => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_byte_offset());
                        } else if self.selection.has_selection(self.cursor_byte_offset()) {
                            if let Some((_, end)) = self.selection.range(self.cursor_byte_offset()) {
                                self.sync_cursor_from_byte(end);
                            }
                            self.selection.clear();
                            self.ensure_cursor_visible();
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                        let line_len = self.line_char_count(self.cursor_line);
                        if self.cursor_col < line_len {
                            self.cursor_col += 1;
                        } else if self.cursor_line < self.line_count().saturating_sub(1) {
                            self.cursor_line += 1;
                            self.cursor_col = 0;
                        }
                        if !shift { self.selection.clear(); }
                        self.ensure_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Up => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_byte_offset());
                        } else {
                            self.selection.clear();
                        }
                        let vis = self.logical_to_visual_line(self.cursor_line, self.cursor_col);
                        if vis > 0 {
                            let (_, cur_seg_start, _) = self.visual_to_logical(vis);
                            let local_col = self.cursor_col.saturating_sub(cur_seg_start);

                            let (new_logical, seg_start, seg_end) = self.visual_to_logical(vis - 1);
                            self.cursor_line = new_logical;
                            self.cursor_col = (seg_start + local_col).min(seg_end);
                            self.ensure_cursor_visible();
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Down => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_byte_offset());
                        } else {
                            self.selection.clear();
                        }
                        let vis = self.logical_to_visual_line(self.cursor_line, self.cursor_col);
                        if vis + 1 < self.total_visual_lines {
                            let (_, cur_seg_start, _) = self.visual_to_logical(vis);
                            let local_col = self.cursor_col.saturating_sub(cur_seg_start);

                            let (new_logical, seg_start, seg_end) = self.visual_to_logical(vis + 1);
                            self.cursor_line = new_logical;
                            self.cursor_col = (seg_start + local_col).min(seg_end);
                            self.ensure_cursor_visible();
                        }
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Home => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_byte_offset());
                        } else {
                            self.selection.clear();
                        }
                        let vis = self.logical_to_visual_line(self.cursor_line, self.cursor_col);
                        let (_, seg_start, _) = self.visual_to_logical(vis);
                        self.cursor_col = seg_start;
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::End => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_byte_offset());
                        } else {
                            self.selection.clear();
                        }
                        let vis = self.logical_to_visual_line(self.cursor_line, self.cursor_col);
                        let (_, _, seg_end) = self.visual_to_logical(vis);
                        self.cursor_col = seg_end;
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Escape => {
                        self.focused = false;
                        self.selection.clear();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }
            Event::CharInput(ch) => {
                if !self.focused || self.read_only || ch.is_control() || ctx.modifiers.ctrl {
                    return EventResult::Ignored;
                }
                let mut cursor_byte = self.cursor_byte_offset();
                let mut ch_buf = [0u8; 4];
                let ch_str = ch.encode_utf8(&mut ch_buf);
                self.selection.replace_selection(&mut self.text, &mut cursor_byte, ch_str);
                self.sync_cursor_from_byte(cursor_byte);
                self.recompute_wraps();
                self.trigger_change();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImeCommit(text) => {
                if !self.focused || self.read_only {
                    return EventResult::Ignored;
                }
                let mut cursor_byte = self.cursor_byte_offset();
                self.selection.replace_selection(&mut self.text, &mut cursor_byte, text);
                self.sync_cursor_from_byte(cursor_byte);
                self.recompute_wraps();
                self.trigger_change();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImePreedit { .. } => {
                if self.focused { EventResult::Handled } else { EventResult::Ignored }
            }
            Event::ImeEnabled | Event::ImeDisabled => {
                if self.focused { EventResult::Handled } else { EventResult::Ignored }
            }
            _ => EventResult::Ignored,
        }
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        self.mss.transition.tick(dt.as_secs_f32())
    }

    fn needs_repaint(&self) -> bool {
        self.mss.transition.is_animating()
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

    fn element_type_name(&self) -> &str { "MultilineTextEdit" }

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
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::TextField,
            state: crate::a11y::NodeState {
                focused: self.focused,
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

impl StyledElement for MultilineTextEditElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
        self.mark_dirty(DirtyFlags::RENDER);
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

    fn make_element(rows: usize, auto_height: bool, max_rows: Option<usize>, total_visual_lines: usize) -> MultilineTextEditElement {
        MultilineTextEditElement {
            id: ElementId::new(),
            text: String::new(),
            placeholder: String::new(),
            rows,
            read_only: false,
            show_line_numbers: false,
            soft_wrap: true,
            auto_height,
            max_rows,
            bounds: Rect::zero(),
            focused: false,
            cursor_line: 0,
            cursor_col: 0,
            scroll_offset: 0,
            selection: TextSelectionState::new(),
            on_change: None,
            submit_on_enter: false,
            on_submit: None,
            classes: Vec::new(),
            dirty_flags: DirtyFlags::empty(),
            mss: MssFields::new(),
            hover: false,
            text_measure: None,
            wrap_cache: Vec::new(),
            text_area_width: 0.0,
            total_visual_lines,
            hover_scrollbar: false,
            dragging_scrollbar: false,
            drag_start_y: 0.0,
            drag_start_offset: 0,
        }
    }

    #[test]
    fn auto_height_caps_visible_rows_at_max_rows() {
        let el = make_element( 1,  true,  Some(15),  30);
        assert_eq!(el.visible_rows(), 15);
        assert!(el.needs_scrollbar(), "scrollbar must engage when total > cap");
    }

    #[test]
    fn auto_height_without_cap_grows_with_content() {
        let el = make_element(1, true, None, 30);
        assert_eq!(el.visible_rows(), 30);
        assert!(!el.needs_scrollbar(), "no scrollbar when content fits");
    }

    #[test]
    fn auto_height_below_min_rows_uses_min() {
        let el = make_element(5, true, Some(15), 2);
        assert_eq!(el.visible_rows(), 5);
    }

    #[test]
    fn auto_height_below_cap_returns_total() {
        let el = make_element(1, true, Some(15), 7);
        assert_eq!(el.visible_rows(), 7);
        assert!(!el.needs_scrollbar());
    }

    #[test]
    fn fixed_height_ignores_max_rows() {
        let el = make_element(3, false, Some(15), 100);
        assert_eq!(el.visible_rows(), 3);
    }
}
