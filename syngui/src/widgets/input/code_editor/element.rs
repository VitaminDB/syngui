use super::buffer::{Edit, RopeBuffer, UndoStack};
use super::find::FindState;
use super::input::keymap::map_key;
use super::input::{Cursor, Cursors, KeyAction, MotionGranularity};
use super::render::{bracket, find_toolbar, gutter, line as line_render, overlay};
use super::syntax::{Highlighter, Language};
use super::theme::Theme;
use super::widget::{CodeEditor, CodeEditorChange, CursorInfo, EditorPersistedState};
use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::code_editor::CodeEditorPalette;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::DisplayList;
use crate::widget::context::{EventContext, EventContextExt, TextMeasure};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget,
};
use std::sync::Arc;
use std::time::Instant;

pub struct CodeEditorElement {
    id: ElementId,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    palette: CodeEditorPalette,
    text_measure: Option<Arc<dyn TextMeasure>>,

    focused: bool,
    hover: bool,
    scroll_offset_lines: usize,
    scroll_offset_x: f32,
    mouse_selecting: bool,
    scrollbar_fader: crate::widgets::scroll::ScrollbarFader,
    scrollbar_interaction: crate::widgets::scroll::ScrollbarInteraction,
    scrollbar_fader_h: crate::widgets::scroll::ScrollbarFader,
    #[allow(dead_code)]
    scrollbar_interaction_h: crate::widgets::scroll::ScrollbarInteraction,
    wrap_cache: Vec<Vec<usize>>,
    text_area_width: f32,
    total_visual_lines: usize,
    command_signal: Option<crate::signal::RwSignal<Option<super::widget::EditorCommand>>>,
    state_signal: Option<crate::signal::RwSignal<EditorPersistedState>>,
    last_state_snapshot: EditorPersistedState,

    buffer: RopeBuffer,
    text_snapshot: String,
    cursors: Cursors,
    undo: UndoStack,
    highlighter: Option<Highlighter>,
    over_size_limit: bool,
    find: FindState,
    preedit: Option<String>,
    goto_buffer: Option<String>,
    bracket_match: Option<(usize, usize)>,

    read_only: bool,
    show_line_numbers: bool,
    soft_wrap: bool,
    size_limit_mb: usize,
    tab_width: u8,
    insert_spaces: bool,
    language: Option<Language>,

    on_change: Option<Arc<Mutex<dyn FnMut(CodeEditorChange) + Send>>>,
    on_save: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    on_cursor: Option<Arc<Mutex<dyn FnMut(CursorInfo) + Send>>>,
}

impl CodeEditorElement {
    pub fn new(widget: &CodeEditor) -> Self {
        let buffer = RopeBuffer::from_str(&widget.initial_text);
        let bytes_len = buffer.len_bytes();
        let limit_bytes = widget.size_limit_mb.saturating_mul(1024 * 1024);
        let over_limit = bytes_len > limit_bytes;

        let highlighter = if over_limit {
            None
        } else {
            widget.language.map(|lang| {
                let mut h = Highlighter::new(lang, widget.tab_width as usize);
                h.reparse(&widget.initial_text);
                h
            })
        };

        let (initial_cursors, initial_scroll_lines, initial_scroll_x, initial_state) =
            if let Some(sig) = widget.state_signal {
                let st = sig.get_untracked();
                let mut cursor_byte = st.cursor_offset.min(widget.initial_text.len());
                while cursor_byte > 0 && !widget.initial_text.is_char_boundary(cursor_byte) {
                    cursor_byte -= 1;
                }
                (
                    Cursors::single(super::input::Cursor::new(cursor_byte)),
                    st.scroll_lines,
                    st.scroll_x.max(0.0),
                    EditorPersistedState {
                        cursor_offset: cursor_byte,
                        scroll_lines: st.scroll_lines,
                        scroll_x: st.scroll_x.max(0.0),
                    },
                )
            } else {
                (Cursors::at_origin(), 0usize, 0.0f32, EditorPersistedState::default())
            };

        let element = Self {
            id: ElementId::new(),
            bounds: Rect::zero(),
            classes: widget.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            palette: CodeEditorPalette::default(),
            text_measure: None,

            focused: false,
            hover: false,
            scroll_offset_lines: initial_scroll_lines,
            scroll_offset_x: initial_scroll_x,
            mouse_selecting: false,
            scrollbar_fader: crate::widgets::scroll::ScrollbarFader::default(),
            scrollbar_interaction: crate::widgets::scroll::ScrollbarInteraction::default(),
            scrollbar_fader_h: crate::widgets::scroll::ScrollbarFader::default(),
            scrollbar_interaction_h: crate::widgets::scroll::ScrollbarInteraction::default(),
            wrap_cache: Vec::new(),
            text_area_width: 0.0,
            total_visual_lines: 0,
            command_signal: widget.command_signal,
            state_signal: widget.state_signal,
            last_state_snapshot: initial_state,

            text_snapshot: widget.initial_text.clone(),
            buffer,
            cursors: initial_cursors,
            undo: UndoStack::new(),
            highlighter,
            over_size_limit: over_limit,
            find: FindState::new(),
            preedit: None,
            goto_buffer: None,
            bracket_match: None,

            read_only: widget.read_only || over_limit,
            show_line_numbers: widget.show_line_numbers,
            soft_wrap: widget.soft_wrap,
            size_limit_mb: widget.size_limit_mb,
            tab_width: widget.tab_width,
            insert_spaces: widget.insert_spaces,
            language: widget.language,

            on_change: widget.on_change.clone(),
            on_save: widget.on_save.clone(),
            on_cursor: widget.on_cursor.clone(),
        };
        if over_limit {
            eprintln!(
                "[CodeEditor] файл превышает size_limit_mb={} → read-only без подсветки",
                widget.size_limit_mb
            );
        }
        element
    }

    fn push_state_to_signal(&mut self) {
        let Some(sig) = self.state_signal else { return };
        let snap = EditorPersistedState {
            cursor_offset: self.cursors.primary().pos,
            scroll_lines: self.scroll_offset_lines,
            scroll_x: self.scroll_offset_x,
        };
        if snap != self.last_state_snapshot {
            self.last_state_snapshot = snap;
            sig.set(snap);
        }
    }

    fn font_size(&self) -> f32 {
        self.mss.font_size.unwrap_or(13.0)
    }

    fn line_height(&self) -> f32 {
        (self.font_size() * 1.5).round()
    }

    fn font_family_str(&self) -> Option<&str> {
        self.mss.font_family.as_deref()
    }

    fn text_origin_x(&self) -> f32 {
        let gutter_w = if self.show_line_numbers {
            gutter::GUTTER_WIDTH
        } else {
            0.0
        };
        self.bounds.x() + gutter_w + 8.0 - self.scroll_offset_x
    }

    fn visible_text_width(&self) -> f32 {
        let gutter_w = if self.show_line_numbers {
            gutter::GUTTER_WIDTH
        } else {
            0.0
        };
        let scrollbar_w = self.mss.scrollbar_style(Color::default()).width;
        let scrollbar_reserve = scrollbar_w + 4.0;
        (self.bounds.size.width - gutter_w - 16.0 - scrollbar_reserve).max(0.0)
    }

    fn visible_lines_count(&self) -> usize {
        let lh = self.line_height().max(1.0);
        ((self.bounds.size.height / lh).floor() as usize).max(1)
    }

    fn scrollbar_geom(&self) -> crate::widgets::scroll::ScrollbarGeom {
        let line_h = self.line_height();
        let content_h = self.total_visual_lines.max(1) as f32 * line_h;
        let scroll_y = self.scroll_offset_lines as f32 * line_h;
        crate::widgets::scroll::ScrollbarGeom {
            viewport: self.bounds,
            content_w: 0.0,
            content_h,
            scroll_x: 0.0,
            scroll_y,
        }
    }

    fn scrollbar_style_now(&self) -> crate::widgets::scroll::ScrollbarStyle {
        let fg = self.mss.color.unwrap_or(Color::from_hex("#9CA3AF"));
        self.mss.scrollbar_style(fg)
    }

    fn theme(&self) -> Theme {
        Theme::from_palette(self.palette.clone())
    }

    fn primary_byte(&self) -> usize {
        self.cursors.primary().pos
    }

    fn snapshot_text(&self) -> &str {
        &self.text_snapshot
    }

    fn refresh_snapshot(&mut self) {
        self.text_snapshot = self.buffer.to_string();
        self.recompute_wraps();
    }

    fn update_bracket_match(&mut self) {
        self.bracket_match = if self.cursors.is_single() && !self.cursors.primary().has_selection()
        {
            bracket::find_match(&self.text_snapshot, self.primary_byte())
        } else {
            None
        };
    }

    fn apply_per_cursor<F>(&mut self, edit_fn: F)
    where
        F: Fn(&Cursor) -> Option<Edit>,
    {
        if self.read_only {
            return;
        }
        let cursors_before = self.cursors.clone();

        let mut edits: Vec<(usize, Edit)> = Vec::with_capacity(self.cursors.len());
        for idx in 0..self.cursors.len() {
            let cursor = self.cursors.get(idx).unwrap();
            if let Some(edit) = edit_fn(cursor) {
                edits.push((idx, edit));
            }
        }
        if edits.is_empty() {
            return;
        }

        edits.sort_by(|a, b| b.1.range.start.cmp(&a.1.range.start));

        let mut inverses = Vec::with_capacity(edits.len());
        for (cursor_idx, edit) in &edits {
            let inverse = self.buffer.apply_edit(edit);
            let removed_len = inverse.replacement.len();
            let added_len = edit.replacement.len();
            let edit_start = edit.range.start;
            let edit_end_old = edit_start + removed_len;
            let delta = added_len as isize - removed_len as isize;

            self.cursors.shift_after(edit_start, edit_end_old, delta);
            if let Some(c) = self.cursors.get_mut(*cursor_idx) {
                c.pos = edit_start + added_len;
                c.anchor = None;
                c.sticky_col = None;
            }

            inverses.push(inverse);
        }

        self.cursors.merge_overlapping();
        let cursors_after = self.cursors.clone();

        let now = Instant::now();
        if inverses.len() == 1 {
            self.undo.push(
                inverses.into_iter().next().unwrap(),
                cursors_before,
                cursors_after,
                now,
            );
        } else {
            self.undo.push_group(inverses, cursors_before, cursors_after);
        }

        self.refresh_snapshot();
        if let Some(h) = self.highlighter.as_mut() {
            h.reparse(&self.text_snapshot);
        }
        if self.find.visible {
            self.find.search(&self.text_snapshot);
        }
        self.fire_on_change();
        self.fire_on_cursor();
    }

    fn fire_on_change(&self) {
        if let Some(cb) = &self.on_change {
            if let Ok(mut f) = cb.lock() {
                f(CodeEditorChange {
                    full_text: &self.text_snapshot,
                });
            }
        }
    }

    fn fire_on_cursor(&mut self) {
        self.update_bracket_match();
        if let Some(cb) = &self.on_cursor {
            if let Ok(mut f) = cb.lock() {
                let pri = self.cursors.primary();
                let (line, col) = self.buffer.byte_to_line_col(pri.pos);
                let total = self.buffer.len_lines();
                let sel_len = pri.selection_range().map(|r| r.end - r.start);
                f(CursorInfo {
                    line,
                    col,
                    total_lines: total,
                    selection_len: sel_len,
                });
            }
        }
    }

    fn replace_selection_or_insert(&mut self, text: &str) {
        if self.read_only || text.is_empty() {
            return;
        }
        let owned = text.to_string();
        self.apply_per_cursor(move |c| {
            let edit = if let Some(range) = c.selection_range() {
                Edit::replace(range, owned.clone())
            } else {
                Edit::insert(c.pos, owned.clone())
            };
            Some(edit)
        });
        self.ensure_cursor_visible();
    }

    fn delete_char(&mut self, forward: bool, word: bool) {
        if self.read_only {
            return;
        }
        let snap = self.text_snapshot.clone();
        let buf_len = self.buffer.len_bytes();
        self.apply_per_cursor(move |c| {
            if let Some(range) = c.selection_range() {
                return Some(Edit::replace(range, ""));
            }
            let pos = c.pos;
            if forward {
                if pos >= buf_len {
                    return None;
                }
                let end = if word {
                    next_word_boundary(&snap, pos, true)
                } else {
                    next_char_boundary(&snap, pos)
                };
                if end <= pos {
                    return None;
                }
                Some(Edit::delete(pos..end))
            } else {
                if pos == 0 {
                    return None;
                }
                let start = if word {
                    next_word_boundary(&snap, pos, false)
                } else {
                    prev_char_boundary(&snap, pos)
                };
                if start >= pos {
                    return None;
                }
                Some(Edit::delete(start..pos))
            }
        });
        self.ensure_cursor_visible();
    }

    fn insert_newline(&mut self) {
        if self.read_only {
            return;
        }
        let snap = self.text_snapshot.clone();
        let indent_step: String = if self.insert_spaces {
            " ".repeat(self.tab_width as usize)
        } else {
            "\t".to_string()
        };
        let buf = self.buffer.clone();
        self.apply_per_cursor(move |c| {
            let pos = c.pos;
            let (line_idx, _) = buf.byte_to_line_col(pos);
            let line_text = buf.line_str(line_idx);
            let leading: String = line_text
                .chars()
                .take_while(|ch| *ch == ' ' || *ch == '\t')
                .collect();

            let line_byte_start = buf.line_to_byte(line_idx);
            let cur_byte_in_line = pos.saturating_sub(line_byte_start).min(line_text.len());
            let prefix = &line_text[..cur_byte_in_line];
            let needs_extra = prefix
                .chars()
                .rev()
                .find(|ch| !ch.is_whitespace())
                .map_or(false, |ch| matches!(ch, '{' | '(' | '['));

            let _ = &snap;
            let to_insert = if needs_extra {
                format!("\n{}{}", leading, indent_step)
            } else {
                format!("\n{}", leading)
            };
            let edit = if let Some(range) = c.selection_range() {
                Edit::replace(range, to_insert)
            } else {
                Edit::insert(c.pos, to_insert)
            };
            Some(edit)
        });
        self.ensure_cursor_visible();
        self.undo.commit_group();
    }

    fn insert_tab(&mut self) {
        if self.read_only {
            return;
        }
        if self.insert_spaces {
            let s: String = " ".repeat(self.tab_width as usize);
            self.replace_selection_or_insert(&s);
        } else {
            self.replace_selection_or_insert("\t");
        }
    }

    fn select_all(&mut self) {
        self.cursors.clear_secondary();
        let len = self.buffer.len_bytes();
        let cursor = self.cursors.primary_mut();
        cursor.anchor = Some(0);
        cursor.pos = len;
        cursor.sticky_col = None;
    }

    fn process_command(&mut self, cmd: super::widget::EditorCommand) {
        use super::widget::EditorCommand as C;
        match cmd {
            C::Copy => {
                let parts: Vec<String> = self
                    .cursors
                    .iter()
                    .filter_map(|c| c.selection_range().map(|r| self.buffer.byte_slice(r)))
                    .collect();
                if !parts.is_empty() {
                    crate::clipboard::copy(&parts.join("\n"));
                }
            }
            C::Cut => {
                if self.read_only { return; }
                let parts: Vec<String> = self
                    .cursors
                    .iter()
                    .filter_map(|c| c.selection_range().map(|r| self.buffer.byte_slice(r)))
                    .collect();
                if parts.is_empty() { return; }
                crate::clipboard::copy(&parts.join("\n"));
                self.apply_per_cursor(|c| c.selection_range().map(|r| Edit::replace(r, "")));
                self.ensure_cursor_visible();
            }
            C::Paste => {
                if self.read_only { return; }
                if let Some(text) = crate::clipboard::paste() {
                    self.undo.commit_group();
                    self.apply_per_cursor(|c| {
                        let edit = if let Some(range) = c.selection_range() {
                            Edit::replace(range, text.clone())
                        } else {
                            Edit::insert(c.pos, text.clone())
                        };
                        Some(edit)
                    });
                    self.ensure_cursor_visible();
                    self.undo.commit_group();
                }
            }
            C::SelectAll => {
                self.select_all();
            }
            C::Reload(new_text) => {
                self.apply_external_reload(new_text);
            }
        }
    }

    fn apply_external_reload(&mut self, new_text: String) {
        if new_text == self.text_snapshot {
            return;
        }

        let new_len = new_text.len();

        let mut pos = self.cursors.primary().pos.min(new_len);
        while pos > 0 && !new_text.is_char_boundary(pos) {
            pos -= 1;
        }
        self.cursors = Cursors::single(Cursor::new(pos));

        if let Some(h) = &mut self.highlighter {
            h.reparse(&new_text);
        }

        self.buffer = RopeBuffer::from_str(&new_text);
        self.text_snapshot = new_text;

        self.undo = UndoStack::new();

        self.wrap_cache.clear();
        self.total_visual_lines = 0;
        self.bracket_match = None;

        self.last_state_snapshot = EditorPersistedState {
            cursor_offset: pos,
            scroll_lines: self.scroll_offset_lines,
            scroll_x: self.scroll_offset_x,
        };
        if let Some(sig) = self.state_signal {
            sig.set(self.last_state_snapshot);
        }
    }

    fn copy_to_clipboard(&self, ctx: &EventContext) {
        let parts: Vec<String> = self
            .cursors
            .iter()
            .filter_map(|c| c.selection_range().map(|r| self.buffer.byte_slice(r)))
            .collect();
        if parts.is_empty() {
            return;
        }
        ctx.copy_to_clipboard(&parts.join("\n"));
    }

    fn cut_to_clipboard(&mut self, ctx: &EventContext) {
        if self.read_only {
            return;
        }
        self.copy_to_clipboard(ctx);
        let any_selection = self.cursors.iter().any(|c| c.has_selection());
        if !any_selection {
            return;
        }
        self.apply_per_cursor(|c| c.selection_range().map(|r| Edit::replace(r, "")));
        self.ensure_cursor_visible();
    }

    fn paste_from_clipboard(&mut self, ctx: &EventContext) {
        if self.read_only {
            return;
        }
        if let Some(text) = ctx.paste_from_clipboard() {
            self.undo.commit_group();
            self.apply_per_cursor(|c| {
                let edit = if let Some(range) = c.selection_range() {
                    Edit::replace(range, text.clone())
                } else {
                    Edit::insert(c.pos, text.clone())
                };
                Some(edit)
            });
            self.ensure_cursor_visible();
            self.undo.commit_group();
        }
    }

    fn undo(&mut self) {
        if self.read_only {
            return;
        }
        let Some(group) = self.undo.pop_undo() else {
            return;
        };
        let mut redo_inverses = Vec::with_capacity(group.edits.len());
        for inv in group.edits.iter().rev() {
            let edit = Edit::from(inv.clone());
            let inverse = self.buffer.apply_edit(&edit);
            redo_inverses.push(inverse);
        }
        self.cursors = group.cursors_before.clone();
        self.refresh_snapshot();
        if let Some(h) = self.highlighter.as_mut() {
            h.reparse(&self.text_snapshot);
        }
        let redo_group = super::buffer::UndoGroup {
            edits: redo_inverses.into_iter().rev().collect(),
            cursors_before: group.cursors_after,
            cursors_after: group.cursors_before,
        };
        self.undo.push_redo(redo_group);
        self.fire_on_change();
        self.fire_on_cursor();
        self.ensure_cursor_visible();
    }

    fn redo(&mut self) {
        if self.read_only {
            return;
        }
        let Some(group) = self.undo.pop_redo() else {
            return;
        };
        let mut undo_inverses = Vec::with_capacity(group.edits.len());
        for inv in group.edits.iter().rev() {
            let edit = Edit::from(inv.clone());
            let inverse = self.buffer.apply_edit(&edit);
            undo_inverses.push(inverse);
        }
        self.cursors = group.cursors_before.clone();
        self.refresh_snapshot();
        if let Some(h) = self.highlighter.as_mut() {
            h.reparse(&self.text_snapshot);
        }
        let undo_group = super::buffer::UndoGroup {
            edits: undo_inverses.into_iter().rev().collect(),
            cursors_before: group.cursors_after,
            cursors_after: group.cursors_before,
        };
        self.undo.push_undo(undo_group);
        self.fire_on_change();
        self.fire_on_cursor();
        self.ensure_cursor_visible();
    }

    fn handle_motion(&mut self, action: KeyAction) {
        let text_buf = self.text_snapshot.clone();
        let total_bytes = self.buffer.len_bytes();
        let total_lines = self.buffer.len_lines();
        let visible = self.visible_lines_count();

        for idx in 0..self.cursors.len() {
            let (cur_pos, cur_sticky_col, cur_anchor) = {
                let c = self.cursors.get(idx).unwrap();
                (c.pos, c.sticky_col, c.anchor)
            };
            let _ = cur_anchor;
            match action {
                KeyAction::Move {
                    granularity,
                    forward,
                    extend_selection,
                } => {
                    let new_pos = match granularity {
                        MotionGranularity::Char => {
                            if forward {
                                next_char_boundary(&text_buf, cur_pos)
                            } else {
                                prev_char_boundary(&text_buf, cur_pos)
                            }
                        }
                        MotionGranularity::Word => {
                            next_word_boundary(&text_buf, cur_pos, forward)
                        }
                        MotionGranularity::Line => {
                            let (line, col) = self.buffer.byte_to_line_col(cur_pos);
                            if self.soft_wrap {
                                let vis_row = self.logical_to_visual_line(line, col);
                                let (l, seg_start, seg_end) = self.visual_to_logical(vis_row);
                                let target_col = if forward { seg_end } else { seg_start };
                                self.buffer.line_col_to_byte(l, target_col)
                            } else if forward {
                                self.buffer.line_byte_range(line).end
                            } else {
                                self.buffer.line_byte_range(line).start
                            }
                        }
                        MotionGranularity::Document => {
                            if forward {
                                total_bytes
                            } else {
                                0
                            }
                        }
                        MotionGranularity::Page => cur_pos,
                    };
                    self.cursors
                        .get_mut(idx)
                        .unwrap()
                        .move_to(new_pos, extend_selection);
                }
                KeyAction::MoveVertical {
                    down,
                    page,
                    extend_selection,
                } => {
                    let (line, col) = self.buffer.byte_to_line_col(cur_pos);
                    let delta = if page { visible.max(1) as isize } else { 1 };
                    if self.soft_wrap {
                        let vis = self.logical_to_visual_line(line, col);
                        let (_l, seg_start, _seg_end) = self.visual_to_logical(vis);
                        let local_col = col.saturating_sub(seg_start);
                        let target_vis_isize = vis as isize + if down { delta } else { -delta };
                        let max_vis = self.total_visual_lines.saturating_sub(1);
                        let target_vis = target_vis_isize.clamp(0, max_vis as isize) as usize;
                        let (new_logical, new_seg_start, new_seg_end) =
                            self.visual_to_logical(target_vis);
                        let sticky = cur_sticky_col.unwrap_or(local_col as u32);
                        let new_local =
                            (sticky as usize).min(new_seg_end.saturating_sub(new_seg_start));
                        let new_col = new_seg_start + new_local;
                        let new_pos = self.buffer.line_col_to_byte(new_logical, new_col);
                        self.cursors.get_mut(idx).unwrap().move_vertical(
                            new_pos,
                            sticky,
                            extend_selection,
                        );
                    } else {
                        let new_line = if down {
                            (line as isize + delta)
                                .clamp(0, total_lines.saturating_sub(1) as isize)
                                as usize
                        } else {
                            (line as isize - delta).max(0) as usize
                        };
                        let sticky = cur_sticky_col.unwrap_or(col as u32);
                        let new_pos = self.buffer.line_col_to_byte(new_line, sticky as usize);
                        self.cursors.get_mut(idx).unwrap().move_vertical(
                            new_pos,
                            sticky,
                            extend_selection,
                        );
                    }
                }
                _ => {}
            }
        }
        self.cursors.merge_overlapping();
        self.ensure_cursor_visible();
        self.fire_on_cursor();
        self.undo.commit_group();
    }

    fn ensure_cursor_visible(&mut self) {
        let (line, col) = self.buffer.byte_to_line_col(self.primary_byte());
        let visual_row = self.logical_to_visual_line(line, col);
        let visible = self.visible_lines_count();
        let prev = self.scroll_offset_lines;
        if visual_row < self.scroll_offset_lines {
            self.scroll_offset_lines = visual_row;
        } else if visual_row >= self.scroll_offset_lines + visible {
            self.scroll_offset_lines = visual_row + 1 - visible;
        }
        if self.scroll_offset_lines != prev {
            self.scrollbar_fader.flash();
        }
    }

    fn click_to_cursor(&self, pos: Point) -> usize {
        let lh = self.line_height().max(1.0);
        let rel_y = (pos.y - self.bounds.y()).max(0.0);
        let row_in_view = (rel_y / lh).floor() as usize;
        let total_visual = self.total_visual_lines.max(1);
        let visual_row = (self.scroll_offset_lines + row_in_view).min(total_visual - 1);
        let (logical, seg_start, seg_end) = self.visual_to_logical(visual_row);
        let line_text = self.buffer.line_str(logical);
        let (seg_byte_start, seg_byte_end) =
            self.segment_byte_range(&line_text, seg_start, seg_end);
        let segment = &line_text[seg_byte_start..seg_byte_end];
        let rel_x = (pos.x - self.text_origin_x()).max(0.0);
        let local_col = self
            .text_measure
            .as_ref()
            .map(|tm| tm.hit_test_char_styled(segment, self.font_size(), rel_x, self.font_family_str()))
            .unwrap_or_else(|| {
                let approx = self.font_size() * 0.6;
                ((rel_x / approx).round() as usize).min(segment.chars().count())
            });
        self.buffer.line_col_to_byte(logical, seg_start + local_col)
    }

    fn recompute_wraps(&mut self) {
        let total_lines = self.buffer.len_lines();
        self.wrap_cache.clear();
        let avail = self.text_area_width;
        if avail <= 0.0 || !self.soft_wrap || self.text_measure.is_none() {
            self.wrap_cache.resize(total_lines, Vec::new());
            self.total_visual_lines = total_lines.max(1);
            return;
        }
        let tm = self.text_measure.as_ref().unwrap().clone();
        let font_size = self.font_size();
        let font_family = self.font_family_str().map(|s| s.to_string());
        let mut total = 0usize;
        for i in 0..total_lines {
            let line = self.buffer.line_str(i);
            let breaks = Self::word_wrap_breaks(&line, avail, &*tm, font_size, font_family.as_deref());
            total += breaks.len() + 1;
            self.wrap_cache.push(breaks);
        }
        self.total_visual_lines = total.max(1);
    }

    fn word_wrap_breaks(
        line: &str,
        avail_width: f32,
        tm: &dyn TextMeasure,
        font_size: f32,
        font_family: Option<&str>,
    ) -> Vec<usize> {
        if line.is_empty() {
            return Vec::new();
        }
        let chars: Vec<char> = line.chars().collect();
        let full_w = tm.measure_text_width_styled(line, font_size, chars.len(), false, font_family);
        if full_w <= avail_width {
            return Vec::new();
        }

        let mut breaks = Vec::new();
        let mut seg_start: usize = 0;
        let mut last_space: Option<usize> = None;

        for i in 0..chars.len() {
            if chars[i] == ' ' {
                last_space = Some(i);
            }
            let seg: String = chars[seg_start..=i].iter().collect();
            let seg_w = tm.measure_text_width_styled(
                &seg,
                font_size,
                i + 1 - seg_start,
                false,
                font_family,
            );
            if seg_w > avail_width && i > seg_start {
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

    fn visual_lines_for(&self, logical: usize) -> usize {
        self.wrap_cache.get(logical).map(|b| b.len() + 1).unwrap_or(1)
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

    fn visual_to_logical(&self, visual_row: usize) -> (usize, usize, usize) {
        let mut remaining = visual_row;
        for (logical, breaks) in self.wrap_cache.iter().enumerate() {
            let vlines = breaks.len() + 1;
            if remaining < vlines {
                let start = if remaining == 0 { 0 } else { breaks[remaining - 1] };
                let end = breaks
                    .get(remaining)
                    .copied()
                    .unwrap_or_else(|| self.buffer.line_str(logical).chars().count());
                return (logical, start, end);
            }
            remaining -= vlines;
        }
        let last = self.buffer.len_lines().saturating_sub(1);
        let end = self.buffer.line_str(last).chars().count();
        (last, 0, end)
    }

    fn visual_line_y(&self, visual_row: usize) -> f32 {
        self.bounds.y()
            + (visual_row as isize - self.scroll_offset_lines as isize) as f32
                * self.line_height()
    }

    fn measure_max_visible_line_width(&self, first_logical: usize, last_logical: usize) -> f32 {
        let Some(tm) = self.text_measure.as_ref() else {
            return 0.0;
        };
        let font_size = self.font_size();
        let font_family = self.font_family_str();
        let mut max_w: f32 = 0.0;
        for i in first_logical..last_logical {
            let line = self.buffer.line_str(i);
            if line.is_empty() {
                continue;
            }
            let w = tm.measure_text_width_styled(
                &line,
                font_size,
                line.chars().count(),
                false,
                font_family,
            );
            if w > max_w {
                max_w = w;
            }
        }
        max_w
    }

    fn segment_byte_range(&self, line_text: &str, seg_start: usize, seg_end: usize) -> (usize, usize) {
        let mut byte_start = 0usize;
        let mut byte_end = line_text.len();
        let mut start_found = seg_start == 0;
        let mut end_found = false;
        for (idx, (byte, _ch)) in line_text.char_indices().enumerate() {
            if !start_found && idx == seg_start {
                byte_start = byte;
                start_found = true;
            }
            if idx == seg_end {
                byte_end = byte;
                end_found = true;
                break;
            }
        }
        let _ = end_found;
        (byte_start, byte_end)
    }
}

fn clip_spans_to_segment(
    spans: &super::syntax::LineSpans,
    seg_byte_start: usize,
    seg_byte_end: usize,
) -> super::syntax::LineSpans {
    let mut out = super::syntax::LineSpans::new();
    let s = seg_byte_start as u32;
    let e = seg_byte_end as u32;
    for span in spans.iter() {
        if span.byte_end <= s || span.byte_start >= e {
            continue;
        }
        let cs = span.byte_start.max(s) - s;
        let ce = span.byte_end.min(e) - s;
        if ce > cs {
            out.push(super::syntax::Span::new(cs, ce, span.class));
        }
    }
    out
}

fn next_char_boundary(text: &str, byte: usize) -> usize {
    let len = text.len();
    if byte >= len {
        return len;
    }
    let mut b = byte + 1;
    while b < len && !text.is_char_boundary(b) {
        b += 1;
    }
    b
}

fn prev_char_boundary(text: &str, byte: usize) -> usize {
    if byte == 0 {
        return 0;
    }
    let mut b = byte - 1;
    while b > 0 && !text.is_char_boundary(b) {
        b -= 1;
    }
    b
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn next_word_boundary(text: &str, byte: usize, forward: bool) -> usize {
    let len = text.len();
    if forward {
        if byte >= len {
            return len;
        }
        let mut b = byte;
        while b < len {
            let c = text[b..].chars().next().unwrap();
            if is_word_char(c) {
                break;
            }
            b += c.len_utf8();
        }
        while b < len {
            let c = text[b..].chars().next().unwrap();
            if !is_word_char(c) {
                break;
            }
            b += c.len_utf8();
        }
        b
    } else {
        if byte == 0 {
            return 0;
        }
        let mut b = byte;
        while b > 0 {
            let prev = text[..b].chars().next_back().unwrap();
            if is_word_char(prev) {
                break;
            }
            b -= prev.len_utf8();
        }
        while b > 0 {
            let prev = text[..b].chars().next_back().unwrap();
            if !is_word_char(prev) {
                break;
            }
            b -= prev.len_utf8();
        }
        b
    }
}

impl Element for CodeEditorElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(w) = widget.as_any().downcast_ref::<CodeEditor>() {
            self.read_only = w.read_only || self.over_size_limit;
            self.show_line_numbers = w.show_line_numbers;
            let wrap_changed = self.soft_wrap != w.soft_wrap;
            self.soft_wrap = w.soft_wrap;
            if wrap_changed {
                self.recompute_wraps();
                if self.soft_wrap {
                    self.scroll_offset_x = 0.0;
                }
                self.ensure_cursor_visible();
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
            self.size_limit_mb = w.size_limit_mb;
            self.tab_width = w.tab_width;
            self.insert_spaces = w.insert_spaces;

            self.command_signal = w.command_signal;
            self.state_signal = w.state_signal;

            if w.initial_text != self.text_snapshot {
                self.buffer = RopeBuffer::from_str(&w.initial_text);
                self.text_snapshot = w.initial_text.clone();
                let bytes_len = self.buffer.len_bytes();
                let limit_bytes = w.size_limit_mb.saturating_mul(1024 * 1024);
                self.over_size_limit = bytes_len > limit_bytes;
                self.read_only = w.read_only || self.over_size_limit;
                self.cursors = Cursors::at_origin();
                self.undo.clear();
                self.scroll_offset_lines = 0;
                self.scroll_offset_x = 0.0;
                self.find = super::find::FindState::new();
                self.preedit = None;
                self.goto_buffer = None;
                self.bracket_match = None;
                self.recompute_wraps();
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }

            let need_lang_refresh =
                w.language != self.language || self.highlighter.is_none() && w.language.is_some();
            if need_lang_refresh {
                self.language = w.language;
                self.highlighter = if self.over_size_limit {
                    None
                } else {
                    self.language.map(|lang| {
                        let mut h = Highlighter::new(lang, self.tab_width as usize);
                        h.reparse(&self.text_snapshot);
                        h
                    })
                };
            } else if let Some(h) = self.highlighter.as_mut() {
                h.reparse(&self.text_snapshot);
            }

            self.on_change = w.on_change.clone();
            self.on_save = w.on_save.clone();
            self.on_cursor = w.on_cursor.clone();
            self.mark_dirty(DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = constraints.max_width;
        let probe_h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            0.0
        };
        self.bounds = Rect::new(Point::zero(), Size::new(w, probe_h));

        let new_text_area = self.visible_text_width();
        let needs_wrap_recompute =
            (new_text_area - self.text_area_width).abs() > 0.5 || self.wrap_cache.is_empty();
        if needs_wrap_recompute {
            self.text_area_width = new_text_area;
            self.recompute_wraps();
        }

        let h = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            self.total_visual_lines.max(1) as f32 * self.line_height()
        };
        self.bounds = Rect::new(Point::zero(), Size::new(w, h));
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let theme = self.theme();
        let bg = theme.bg(&self.mss);
        let radius = self.mss.border_radius_uniform(self.bounds.size.width.min(self.bounds.size.height), 0.0);
        list.push_rect(self.bounds, bg, [radius; 4]);

        list.push_clip(self.bounds);

        gutter::paint_gutter_bg(list, self.bounds, &theme, &self.mss, self.show_line_numbers);

        let scrollbar_w = self.mss.scrollbar_style(Color::default()).width;
        let text_clip = Rect::new(
            self.bounds.origin,
            Size::new(
                (self.bounds.size.width - scrollbar_w).max(0.0),
                self.bounds.size.height,
            ),
        );
        list.push_clip(text_clip);

        let total_visual = self.total_visual_lines.max(1);
        let visible_count = self.visible_lines_count();
        let first_visual = self.scroll_offset_lines.min(total_visual.saturating_sub(1));
        let last_visual = (first_visual + visible_count + 2).min(total_visual);

        let line_height = self.line_height();
        let font_size = self.font_size();
        let font_family = self.font_family_str();

        let (first_logical, _, _) = self.visual_to_logical(first_visual);
        let (last_logical_inc, _, _) =
            self.visual_to_logical(last_visual.saturating_sub(1));
        let last_logical = (last_logical_inc + 1).min(self.buffer.len_lines());
        let highlights = if let Some(h) = &self.highlighter {
            h.highlight_lines(&self.text_snapshot, first_logical..last_logical)
        } else {
            Vec::new()
        };

        let primary_pos = self.primary_byte();
        let (primary_line, _primary_col_chars) = self.buffer.byte_to_line_col(primary_pos);
        let cursor_lines: Vec<(usize, usize)> = self
            .cursors
            .iter()
            .map(|c| {
                let (line, _) = self.buffer.byte_to_line_col(c.pos);
                (line, c.pos)
            })
            .collect();
        let cursor_selections: Vec<Option<std::ops::Range<usize>>> =
            self.cursors.iter().map(|c| c.selection_range()).collect();

        let bracket_positions: Vec<(usize, usize)> = if let Some((a, b)) = self.bracket_match {
            let (la, _) = self.buffer.byte_to_line_col(a);
            let (lb, _) = self.buffer.byte_to_line_col(b);
            vec![(la, a), (lb, b)]
        } else {
            Vec::new()
        };

        for vis_row in first_visual..last_visual {
            let (logical, seg_start_char, seg_end_char) = self.visual_to_logical(vis_row);
            let line_text = self.buffer.line_str(logical);
            let line_byte_start = self.buffer.line_to_byte(logical);
            let line_chars_count = line_text.chars().count();
            let (seg_byte_start, seg_byte_end) =
                self.segment_byte_range(&line_text, seg_start_char, seg_end_char);
            let segment = &line_text[seg_byte_start..seg_byte_end];
            let abs_seg_start = line_byte_start + seg_byte_start;
            let abs_seg_end = line_byte_start + seg_byte_end;
            let is_last_visual_seg = seg_end_char >= line_chars_count;

            let y = self.visual_line_y(vis_row);
            let text_origin = Point::new(self.text_origin_x(), y);

            if self.focused && logical == primary_line {
                overlay::paint_current_line_bg(
                    list,
                    self.bounds.x(),
                    self.bounds.size.width,
                    y,
                    line_height,
                    &theme,
                    &self.mss,
                );
            }

            for sel in cursor_selections.iter().filter_map(|s| s.as_ref()) {
                if sel.start < abs_seg_end && sel.end > abs_seg_start {
                    let local_start = sel.start.saturating_sub(abs_seg_start).min(segment.len());
                    let local_end = sel.end.saturating_sub(abs_seg_start).min(segment.len());
                    overlay::paint_selection_for_line(
                        list,
                        segment,
                        local_start,
                        local_end,
                        text_origin,
                        line_height,
                        font_size,
                        font_family,
                        &theme,
                        &self.mss,
                    );
                }
            }

            if seg_start_char == 0 {
                let leading_chars = line_text
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .count();
                let levels = leading_chars / self.tab_width.max(1) as usize;
                if levels > 0 {
                    let char_w = self
                        .text_measure
                        .as_ref()
                        .map(|tm| {
                            tm.measure_text_width_styled("M", font_size, 1, false, font_family)
                        })
                        .unwrap_or(font_size * 0.6);
                    overlay::paint_indent_guides(
                        list,
                        text_origin.x,
                        y,
                        line_height,
                        char_w,
                        self.tab_width,
                        levels.saturating_sub(1),
                        &theme,
                        &self.mss,
                    );
                }
            }

            if self.find.visible && !self.find.matches.is_empty() {
                let current_idx = self.find.current;
                for (m_idx, m) in self.find.matches.iter().enumerate() {
                    if m.start < abs_seg_end && m.end > abs_seg_start {
                        let local_start = m.start.saturating_sub(abs_seg_start).min(segment.len());
                        let local_end = m.end.saturating_sub(abs_seg_start).min(segment.len());
                        overlay::paint_find_match(
                            list,
                            segment,
                            local_start,
                            local_end,
                            text_origin,
                            line_height,
                            font_size,
                            font_family,
                            &theme,
                            &self.mss,
                            current_idx == Some(m_idx),
                        );
                    }
                }
            }

            let logical_spans = highlights.get(logical - first_logical);
            let segment_spans = match logical_spans {
                Some(s) => clip_spans_to_segment(s, seg_byte_start, seg_byte_end),
                None => Default::default(),
            };
            line_render::paint_line(
                list,
                segment,
                &segment_spans,
                text_origin,
                line_height,
                font_size,
                font_family,
                &theme,
                &self.mss,
                self.text_measure.as_deref(),
            );

            if self.show_line_numbers && seg_start_char == 0 {
                gutter::paint_line_number(
                    list,
                    logical,
                    self.bounds.x(),
                    y,
                    line_height,
                    font_size.min(12.0),
                    &theme,
                    &self.mss,
                );
            }

            if self.focused {
                for (c_line, c_pos) in &cursor_lines {
                    if *c_line != logical {
                        continue;
                    }
                    let c_byte_in_line = c_pos.saturating_sub(line_byte_start);
                    let in_seg = if is_last_visual_seg {
                        c_byte_in_line >= seg_byte_start && c_byte_in_line <= seg_byte_end
                    } else {
                        c_byte_in_line >= seg_byte_start && c_byte_in_line < seg_byte_end
                    };
                    if in_seg {
                        let local = c_byte_in_line - seg_byte_start;
                        overlay::paint_cursor(
                            list,
                            segment,
                            local,
                            text_origin,
                            line_height,
                            font_size,
                            font_family,
                            &theme,
                            &self.mss,
                        );
                    }
                }
            }

            for (b_line, b_pos) in &bracket_positions {
                if *b_line != logical {
                    continue;
                }
                let b_byte_in_line = b_pos.saturating_sub(line_byte_start);
                if b_byte_in_line < seg_byte_start || b_byte_in_line >= seg_byte_end {
                    continue;
                }
                let local = b_byte_in_line - seg_byte_start;
                let prefix = &segment[..local];
                let prefix_w = self
                    .text_measure
                    .as_ref()
                    .map(|tm| {
                        tm.measure_text_width_styled(
                            prefix,
                            font_size,
                            prefix.chars().count(),
                            false,
                            font_family,
                        )
                    })
                    .unwrap_or_else(|| prefix.chars().count() as f32 * font_size * 0.6);
                let bracket_chars: String = segment
                    .get(local..)
                    .and_then(|s| s.chars().next())
                    .map(|c| c.to_string())
                    .unwrap_or_default();
                let char_w = if !bracket_chars.is_empty() {
                    self.text_measure
                        .as_ref()
                        .map(|tm| {
                            tm.measure_text_width_styled(
                                &bracket_chars,
                                font_size,
                                1,
                                false,
                                font_family,
                            )
                        })
                        .unwrap_or(font_size * 0.6)
                } else {
                    font_size * 0.6
                };
                overlay::paint_bracket_highlight(
                    list,
                    text_origin.x + prefix_w,
                    y,
                    char_w,
                    line_height,
                    &theme,
                    &self.mss,
                );
            }

            if self.focused && logical == primary_line {
                if let Some(preedit) = &self.preedit {
                    let c_byte_in_line = primary_pos.saturating_sub(line_byte_start);
                    let in_seg = if is_last_visual_seg {
                        c_byte_in_line >= seg_byte_start && c_byte_in_line <= seg_byte_end
                    } else {
                        c_byte_in_line >= seg_byte_start && c_byte_in_line < seg_byte_end
                    };
                    if in_seg {
                        let local = c_byte_in_line - seg_byte_start;
                        let prefix = &segment[..local];
                        let prefix_w = self
                            .text_measure
                            .as_ref()
                            .map(|tm| {
                                tm.measure_text_width_styled(
                                    prefix,
                                    font_size,
                                    prefix.chars().count(),
                                    false,
                                    font_family,
                                )
                            })
                            .unwrap_or_else(|| prefix.chars().count() as f32 * font_size * 0.6);
                        let preedit_rect = Rect::new(
                            Point::new(text_origin.x + prefix_w, y),
                            Size::new(10_000.0, line_height),
                        );
                        list.push_text_aligned(
                            preedit,
                            preedit_rect,
                            theme.fg(&self.mss),
                            font_size,
                            crate::mss::TextAlign::DEFAULT,
                            crate::mss::TextDecoration::Underline,
                            400,
                        );
                    }
                }
            }
        }

        list.pop_clip();

        find_toolbar::paint_find_toolbar(
            list,
            self.bounds,
            &self.find,
            &theme,
            &self.mss,
            font_size,
        );

        if let Some(goto) = &self.goto_buffer {
            find_toolbar::paint_goto_toolbar(
                list,
                self.bounds,
                goto,
                self.buffer.len_lines(),
                &theme,
                &self.mss,
                font_size,
            );
        }

        let line_height = self.line_height();
        let content_h = self.total_visual_lines.max(1) as f32 * line_height;
        let scroll_y = self.scroll_offset_lines as f32 * line_height;
        let fg = self.mss.color.unwrap_or(Color::from_hex("#9CA3AF"));
        let style = self.mss.scrollbar_style(fg);
        let opacity = crate::widgets::scroll::effective_opacity(&self.scrollbar_fader, &style);
        if opacity > 0.0 {
            crate::widgets::scroll::render_vertical(
                list,
                self.bounds,
                content_h,
                scroll_y,
                &style,
                &self.scrollbar_fader,
                opacity,
            );
        }

        if !self.soft_wrap {
            let max_line_w = self.measure_max_visible_line_width(first_logical, last_logical);
            let viewport_w = self.visible_text_width();
            if max_line_w > viewport_w + 0.5 {
                let opacity_h =
                    crate::widgets::scroll::effective_opacity(&self.scrollbar_fader_h, &style);
                if opacity_h > 0.0 {
                    crate::widgets::scroll::render_horizontal(
                        list,
                        self.bounds,
                        max_line_w,
                        self.scroll_offset_x,
                        &style,
                        &self.scrollbar_fader_h,
                        opacity_h,
                    );
                }
            }
        }

        list.pop_clip();
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::FocusGained => {
                self.focused = true;
                ctx.request_paint();
                EventResult::Handled
            }
            Event::FocusLost => {
                self.focused = false;
                self.undo.commit_group();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::MouseMove(pos) => {
                let sb_geom = self.scrollbar_geom();
                let sb_style = self.scrollbar_style_now();

                if let Some((new_y, _)) = self.scrollbar_interaction.update_drag(
                    &mut self.scrollbar_fader, &sb_geom, &sb_style, *pos,
                ) {
                    let line_h = self.line_height().max(1.0);
                    let visible = self.visible_lines_count();
                    let max_scroll = self.total_visual_lines.saturating_sub(visible);
                    let new_off = (new_y / line_h).round() as i64;
                    let new_off = new_off.clamp(0, max_scroll as i64) as usize;
                    if new_off != self.scroll_offset_lines {
                        self.scroll_offset_lines = new_off;
                    }
                    ctx.request_paint();
                    return EventResult::Captured;
                }

                let was_hover = self.hover;
                self.hover = self.bounds.contains(*pos);
                if self.hover {
                    ctx.set_cursor(CursorIcon::Text);
                }
                if self.mouse_selecting && self.focused {
                    let new_pos = self.click_to_cursor(*pos);
                    self.cursors.primary_mut().move_to(new_pos, true);
                    self.fire_on_cursor();
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.hover {
                    if self.scrollbar_interaction.update_hover(
                        &mut self.scrollbar_fader, &sb_geom, &sb_style, *pos,
                        crate::widgets::scroll::SCROLLBAR_HIT_MARGIN,
                    ) {
                        ctx.request_paint();
                    }
                } else if self.scrollbar_interaction.clear_hover(&mut self.scrollbar_fader) {
                    ctx.request_paint();
                }

                if was_hover != self.hover {
                    ctx.request_paint();
                }
                if self.hover {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    let sb_geom = self.scrollbar_geom();
                    let sb_style = self.scrollbar_style_now();
                    if self.scrollbar_interaction.try_begin_drag(
                        &mut self.scrollbar_fader, &sb_geom, &sb_style, *position,
                    ) {
                        ctx.request_paint();
                        return EventResult::Captured;
                    }

                    self.focused = true;
                    let new_pos = self.click_to_cursor(*position);
                    let alt = ctx.modifiers.alt;
                    let shift = ctx.modifiers.shift;

                    if alt && !shift {
                        self.cursors.add_cursor(new_pos);
                        self.mouse_selecting = false;
                    } else {
                        if !alt {
                            self.cursors.clear_secondary();
                        }
                        self.cursors.primary_mut().move_to(new_pos, shift);
                        if !shift {
                            self.cursors.primary_mut().anchor = Some(new_pos);
                        }
                        self.mouse_selecting = true;
                    }
                    self.undo.commit_group();
                    self.fire_on_cursor();
                    ctx.set_cursor(CursorIcon::Text);
                    ctx.request_paint();
                    return EventResult::Handled;
                } else if self.focused {
                    self.focused = false;
                    self.undo.commit_group();
                    ctx.request_paint();
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } => {
                if *button == MouseButton::Left {
                    if self.scrollbar_interaction.end_drag(&mut self.scrollbar_fader) {
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    if self.mouse_selecting {
                        self.mouse_selecting = false;
                        let pri = self.cursors.primary_mut();
                        if pri.anchor == Some(pri.pos) {
                            pri.anchor = None;
                        }
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            Event::DoubleClick { button, position } => {
                if *button == MouseButton::Left && self.bounds.contains(*position) {
                    self.focused = true;
                    let click_pos = self.click_to_cursor(*position);
                    let text = self.snapshot_text();
                    let (start, end) = crate::widget::selection::TextSelectionState::find_word_boundaries(text, click_pos);
                    let pri = self.cursors.primary_mut();
                    pri.anchor = Some(start);
                    pri.pos = end;
                    pri.sticky_col = None;
                    self.fire_on_cursor();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseWheel { delta, position, .. } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                let visible = self.visible_lines_count();
                let max_scroll = self.total_visual_lines.saturating_sub(visible);
                let step = 3i32;
                let new = if *delta < 0.0 {
                    (self.scroll_offset_lines as i32 + step).min(max_scroll as i32) as usize
                } else if *delta > 0.0 {
                    (self.scroll_offset_lines as i32 - step).max(0) as usize
                } else {
                    self.scroll_offset_lines
                };
                if new != self.scroll_offset_lines {
                    self.scroll_offset_lines = new;
                    self.scrollbar_fader.flash();
                    ctx.request_paint();
                }
                EventResult::Handled
            }
            Event::KeyDown(key) => {
                if !self.focused {
                    return EventResult::Ignored;
                }
                let Some(action) = map_key(*key, ctx.modifiers) else {
                    return EventResult::Ignored;
                };
                self.handle_action(action, ctx);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::CharInput(ch) => {
                if !self.focused || ch.is_control() || ctx.modifiers.ctrl {
                    return EventResult::Ignored;
                }
                if let Some(buf) = self.goto_buffer.as_mut() {
                    if ch.is_ascii_digit() {
                        buf.push(*ch);
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }
                if self.find.visible {
                    let mut q = self.find.query.clone();
                    q.push(*ch);
                    self.find.update_query(&self.text_snapshot, q);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.read_only {
                    return EventResult::Ignored;
                }
                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                self.replace_selection_or_insert(s);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImeCommit(text) => {
                if !self.focused || self.read_only {
                    return EventResult::Ignored;
                }
                self.preedit = None;
                self.replace_selection_or_insert(text);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImePreedit { text, .. } => {
                if !self.focused {
                    return EventResult::Ignored;
                }
                self.preedit = if text.is_empty() {
                    None
                } else {
                    Some(text.clone())
                };
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImeEnabled => {
                if self.focused {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::ImeDisabled => {
                self.preedit = None;
                if self.focused {
                    ctx.request_paint();
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
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

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::TextField,
            state: crate::a11y::NodeState {
                focused: self.focused,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties::default(),
        })
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "CodeEditor"
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
        self.palette.reset();
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.palette = CodeEditorPalette::from_style(style);
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

    fn wants_tab(&self) -> bool {
        self.focused && !self.read_only
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        let style = self.mss.scrollbar_style(self.mss.color.unwrap_or(Color::from_hex("#9CA3AF")));
        let mut needs_repaint = self.scrollbar_fader.tick(dt.as_secs_f32(), &style);

        if let Some(sig) = self.command_signal {
            if let Some(cmd) = sig.get_untracked() {
                self.process_command(cmd);
                sig.set(None);
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
                needs_repaint = true;
            }
        }

        self.push_state_to_signal();
        needs_repaint
    }

    fn needs_repaint(&self) -> bool {
        self.scrollbar_fader.opacity > 0.0
    }
}

impl CodeEditorElement {
    fn handle_action(&mut self, action: KeyAction, ctx: &mut EventContext) {
        if self.goto_buffer.is_some() {
            match action {
                KeyAction::Escape => {
                    self.goto_buffer = None;
                    return;
                }
                KeyAction::InsertNewline => {
                    if let Some(buf) = self.goto_buffer.take() {
                        if let Ok(n) = buf.parse::<usize>() {
                            let total = self.buffer.len_lines();
                            let line = n.saturating_sub(1).min(total.saturating_sub(1));
                            let pos = self.buffer.line_to_byte(line);
                            self.cursors.clear_secondary();
                            let p = self.cursors.primary_mut();
                            p.pos = pos;
                            p.anchor = None;
                            p.sticky_col = None;
                            self.ensure_cursor_visible();
                            self.fire_on_cursor();
                        }
                    }
                    return;
                }
                KeyAction::DeleteChar { forward: false, .. } => {
                    if let Some(buf) = self.goto_buffer.as_mut() {
                        buf.pop();
                    }
                    return;
                }
                _ => return,
            }
        }

        if self.find.visible {
            match action {
                KeyAction::Escape => {
                    self.find.close();
                    return;
                }
                KeyAction::InsertNewline => {
                    let range = if ctx.modifiers.shift {
                        self.find.prev_match()
                    } else {
                        self.find.next_match()
                    };
                    if let Some(r) = range {
                        self.jump_to_match(r);
                    }
                    return;
                }
                KeyAction::DeleteChar { forward: false, .. } => {
                    self.find.query.pop();
                    let q = self.find.query.clone();
                    self.find.update_query(&self.text_snapshot, q);
                    return;
                }
                KeyAction::FindOpen => {
                    if let Some(r) = self.find.next_match() {
                        self.jump_to_match(r);
                    }
                    return;
                }
                _ => {}
            }
        }

        match action {
            KeyAction::Move { .. } | KeyAction::MoveVertical { .. } => {
                self.handle_motion(action);
            }
            KeyAction::DeleteChar { forward, word } => {
                self.delete_char(forward, word);
            }
            KeyAction::InsertNewline => self.insert_newline(),
            KeyAction::InsertTab => self.insert_tab(),
            KeyAction::SelectAll => {
                self.select_all();
                self.fire_on_cursor();
            }
            KeyAction::Copy => self.copy_to_clipboard(ctx),
            KeyAction::Cut => self.cut_to_clipboard(ctx),
            KeyAction::Paste => self.paste_from_clipboard(ctx),
            KeyAction::Undo => self.undo(),
            KeyAction::Redo => self.redo(),
            KeyAction::Save => {
                self.undo.commit_group();
                if let Some(cb) = &self.on_save {
                    if let Ok(mut f) = cb.lock() {
                        f(&self.text_snapshot);
                    }
                }
            }
            KeyAction::Escape => {
                if !self.cursors.is_single() {
                    self.cursors.clear_secondary();
                } else {
                    self.cursors.primary_mut().clear_selection();
                }
                self.fire_on_cursor();
            }
            KeyAction::FindOpen => {
                self.find.open();
                if let Some(r) = self.cursors.primary().selection_range() {
                    let q = self.buffer.byte_slice(r);
                    if !q.is_empty() && !q.contains('\n') {
                        self.find.update_query(&self.text_snapshot, q);
                        if let Some(m) = self.find.current_match() {
                            self.jump_to_match(m);
                        }
                    }
                }
            }
            KeyAction::GoToLineOpen => {
                if self.find.visible {
                    self.find.close();
                }
                self.goto_buffer = Some(String::new());
            }
        }
    }

    fn jump_to_match(&mut self, range: std::ops::Range<usize>) {
        self.cursors.clear_secondary();
        let p = self.cursors.primary_mut();
        p.anchor = Some(range.start);
        p.pos = range.end;
        p.sticky_col = None;
        self.ensure_cursor_visible();
        self.fire_on_cursor();
    }
}

impl StyledElement for CodeEditorElement {
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
    use crate::widget::UpdateContext;

    fn make_text(n: usize) -> String {
        (0..n)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn wrap_cache_resets_on_file_switch() {
        let editor40 = CodeEditor::new().text(make_text(40));
        let mut el = CodeEditorElement::new(&editor40);
        el.recompute_wraps();
        let n40 = el.buffer.len_lines();
        assert_eq!(el.total_visual_lines, n40.max(1));

        let editor10 = CodeEditor::new().text(make_text(10));
        let mut ctx = UpdateContext::new(el.id);
        el.update(&editor10, &mut ctx);

        let n10 = el.buffer.len_lines();
        assert_eq!(
            el.total_visual_lines,
            n10.max(1),
            "total_visual_lines должен соответствовать новому файлу сразу после update"
        );

        let editor40_again = CodeEditor::new().text(make_text(40));
        el.update(&editor40_again, &mut ctx);
        let n40_again = el.buffer.len_lines();
        assert_eq!(el.total_visual_lines, n40_again.max(1));
    }
}
