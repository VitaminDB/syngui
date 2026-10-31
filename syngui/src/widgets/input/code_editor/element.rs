use super::buffer::{Edit, RopeBuffer, UndoStack};
use super::find::FindState;
use super::input::keymap::map_key;
use super::input::{Cursor, Cursors, KeyAction, MotionGranularity};
use super::render::{bracket, find_toolbar, gutter, line as line_render, overlay};
use super::syntax::{Highlighter, Language, LineHighlighter};
use super::theme::Theme;
use super::widget::{CodeEditor, CodeEditorChange, CursorInfo, EditorPersistedState};
use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
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

#[path = "element_lsp.rs"]
mod lsp_ui;

pub struct CodeEditorElement {
    id: ElementId,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    palette: CodeEditorPalette,
    text_measure: Option<Arc<dyn TextMeasure>>,
    /// Главная каретка с последней отрисовки (координаты окна) — для IME.
    ime_caret: std::cell::Cell<Option<Rect>>,

    focused: bool,
    hover: bool,
    scroll_offset_lines: usize,
    scroll_offset_x: f32,
    mouse_selecting: bool,
    scrollbar_fader: crate::widgets::scroll::ScrollbarFader,
    scrollbar_interaction: crate::widgets::scroll::ScrollbarInteraction,
    scrollbar_fader_h: crate::widgets::scroll::ScrollbarFader,
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
    custom_highlighter: Option<std::sync::Arc<dyn LineHighlighter>>,
    line_marks: Vec<(usize, Color)>,
    breakpoints: Vec<usize>,
    on_gutter_click: Option<Arc<Mutex<dyn FnMut(usize) + Send>>>,
    /// Строка, к которой уже прокручено, и ожидающая прокрутка.
    revealed: Option<usize>,
    pending_reveal: Option<usize>,

    on_change: Option<Arc<Mutex<dyn FnMut(CodeEditorChange) + Send>>>,
    on_save: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    key_hook: Option<super::widget::EditorKeyHook>,
    on_cursor: Option<Arc<Mutex<dyn FnMut(CursorInfo) + Send>>>,
    /// Диагностики, дополнение, подсказки, переход к определению.
    lsp: lsp_ui::LspState,
}

fn make_highlighter(
    custom: Option<&std::sync::Arc<dyn LineHighlighter>>,
    language: Option<Language>,
    tab_width: u8,
    text: &str,
) -> Option<Highlighter> {
    let mut h = match custom {
        Some(c) => Highlighter::custom(c.clone()),
        None => Highlighter::new(language?, tab_width as usize),
    };
    h.reparse(text);
    Some(h)
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
            make_highlighter(widget.highlighter.as_ref(), widget.language, widget.tab_width, &widget.initial_text)
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
                (
                    Cursors::at_origin(),
                    0usize,
                    0.0f32,
                    EditorPersistedState::default(),
                )
            };

        let element = Self {
            id: ElementId::new(),
            bounds: Rect::zero(),
            classes: widget.classes.clone(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            palette: CodeEditorPalette::default(),
            text_measure: None,
            ime_caret: std::cell::Cell::new(None),

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
            custom_highlighter: widget.highlighter.clone(),
            line_marks: widget.line_marks.clone(),
            breakpoints: widget.breakpoints.clone(),
            on_gutter_click: widget.on_gutter_click.clone(),
            revealed: widget.reveal_line,
            pending_reveal: widget.reveal_line,

            on_change: widget.on_change.clone(),
            on_save: widget.on_save.clone(),
            key_hook: widget.key_hook.clone(),
            on_cursor: widget.on_cursor.clone(),
            lsp: lsp_ui::LspState::from_widget(widget),
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

    fn gutter_w(&self) -> f32 {
        if self.show_line_numbers {
            gutter::GUTTER_WIDTH
        } else {
            0.0
        }
    }

    fn text_origin_x(&self) -> f32 {
        self.bounds.x() + self.gutter_w() + 8.0 - self.scroll_offset_x
    }

    /// Логические строки, попадающие на экран: `first..last`.
    fn visible_logical_range(&self) -> (usize, usize) {
        let total_visual = self.total_visual_lines.max(1);
        let first_visual = self.scroll_offset_lines.min(total_visual - 1);
        let last_visual = (first_visual + self.visible_lines_count() + 2).min(total_visual);
        let (first, _, _) = self.visual_to_logical(first_visual);
        let (last, _, _) = self.visual_to_logical(last_visual.saturating_sub(1));
        (first, (last + 1).min(self.buffer.len_lines()))
    }

    fn char_width(&self) -> f32 {
        let fs = self.font_size();
        self.text_measure
            .as_ref()
            .map(|tm| tm.measure_text_width_styled("M", fs, 1, false, self.font_family_str()))
            .unwrap_or(fs * 0.6)
    }

    /// Насколько можно сдвинуть текст влево: по самой длинной строке на экране
    /// (плюс место под каретку в конце). Без переноса строк.
    fn max_scroll_x(&self) -> f32 {
        if self.soft_wrap {
            return 0.0;
        }
        let (first, last) = self.visible_logical_range();
        let content = self.measure_max_visible_line_width(first, last) + self.char_width() * 2.0;
        (content - self.visible_text_width()).max(0.0)
    }

    /// Геометрия горизонтальной полосы: под текстом, от края gutter'а до
    /// вертикальной полосы. Уже прокрученное за текущий максимум (строки на
    /// экране стали короче) остаётся в диапазоне — ползунок не прыгает.
    fn h_scrollbar_geom(&self) -> crate::widgets::scroll::ScrollbarGeom {
        let gutter_w = self.gutter_w();
        let sb_w = self.mss.scrollbar_style(Color::default()).width;
        let viewport = Rect::new(
            Point::new(self.bounds.x() + gutter_w, self.bounds.y()),
            Size::new(
                (self.bounds.size.width - gutter_w - sb_w - 4.0).max(0.0),
                self.bounds.size.height,
            ),
        );
        let max = self.max_scroll_x().max(self.scroll_offset_x);
        crate::widgets::scroll::ScrollbarGeom {
            viewport,
            content_w: viewport.size.width + max,
            content_h: 0.0,
            scroll_x: self.scroll_offset_x,
            scroll_y: 0.0,
        }
    }

    /// Горизонтально подвинуть текст так, чтобы главная каретка была видна,
    /// с запасом `margin_chars` символов от края.
    fn ensure_cursor_visible_x(&mut self, margin_chars: f32) {
        if self.soft_wrap {
            if self.scroll_offset_x != 0.0 {
                self.scroll_offset_x = 0.0;
            }
            return;
        }
        let Some(tm) = self.text_measure.as_ref() else { return };
        let (line, col) = self.buffer.byte_to_line_col(self.primary_byte());
        let line_text = self.buffer.line_str(line);
        let prefix: String = line_text.chars().take(col).collect();
        let x = tm.measure_text_width_styled(
            &prefix,
            self.font_size(),
            prefix.chars().count(),
            false,
            self.font_family_str(),
        );
        let margin = self.char_width() * margin_chars;
        let view_w = self.visible_text_width();
        let prev = self.scroll_offset_x;
        if x - margin < self.scroll_offset_x {
            self.scroll_offset_x = (x - margin).max(0.0);
        } else if x + margin > self.scroll_offset_x + view_w {
            self.scroll_offset_x = (x + margin - view_w).max(0.0);
        }
        if (self.scroll_offset_x - prev).abs() > 0.01 {
            self.scrollbar_fader_h.flash();
        }
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
            self.undo
                .push_group(inverses, cursors_before, cursors_after);
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

    /// Правки по позициям исходного текста — одним шагом отмены.
    fn apply_text_edits(&mut self, edits: Vec<super::widget::TextEdit>) {
        if self.read_only || edits.is_empty() {
            return;
        }
        let cursors_before = self.cursors.clone();
        let mut ranges: Vec<Edit> = edits
            .iter()
            .map(|e| {
                let a = self.buffer.line_col_to_byte(e.start_line, e.start_col);
                let b = self.buffer.line_col_to_byte(e.end_line, e.end_col).max(a);
                Edit::replace(a..b, e.text.clone())
            })
            .collect();
        // с конца, чтобы ранние смещения не съезжали; пересекающиеся — пропустить
        ranges.sort_by(|x, y| y.range.start.cmp(&x.range.start).then(y.range.end.cmp(&x.range.end)));
        let mut inverses = Vec::with_capacity(ranges.len());
        let mut floor = usize::MAX;
        for edit in &ranges {
            if edit.range.end > floor {
                continue;
            }
            floor = edit.range.start;
            let inverse = self.buffer.apply_edit(edit);
            let removed = inverse.replacement.len();
            let delta = edit.replacement.len() as isize - removed as isize;
            self.cursors.shift_after(edit.range.start, edit.range.start + removed, delta);
            inverses.push(inverse);
        }
        if inverses.is_empty() {
            return;
        }
        self.cursors.merge_overlapping();
        let cursors_after = self.cursors.clone();
        self.undo.commit_group();
        self.undo.push_group(inverses, cursors_before, cursors_after);
        self.undo.commit_group();
        self.refresh_snapshot();
        if let Some(h) = self.highlighter.as_mut() {
            h.reparse(&self.text_snapshot);
        }
        if self.find.visible {
            self.find.search(&self.text_snapshot);
        }
        self.wrap_cache.clear();
        self.total_visual_lines = 0;
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
                if self.read_only {
                    return;
                }
                let parts: Vec<String> = self
                    .cursors
                    .iter()
                    .filter_map(|c| c.selection_range().map(|r| self.buffer.byte_slice(r)))
                    .collect();
                if parts.is_empty() {
                    return;
                }
                crate::clipboard::copy(&parts.join("\n"));
                self.apply_per_cursor(|c| c.selection_range().map(|r| Edit::replace(r, "")));
                self.ensure_cursor_visible();
            }
            C::Paste => {
                if self.read_only {
                    return;
                }
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
            C::Undo => {
                if !self.read_only {
                    self.undo();
                    self.ensure_cursor_visible();
                }
            }
            C::Redo => {
                if !self.read_only {
                    self.redo();
                    self.ensure_cursor_visible();
                }
            }
            C::Delete => {
                if self.read_only {
                    return;
                }
                self.undo.commit_group();
                self.apply_per_cursor(|c| c.selection_range().map(|r| Edit::replace(r, "")));
                self.undo.commit_group();
                self.ensure_cursor_visible();
            }
            C::Reload(new_text) => {
                self.apply_external_reload(new_text);
            }
            C::SetCursor { line, col } => {
                self.set_cursor_to(line, col);
            }
            C::ApplyEdits(edits) => {
                self.apply_text_edits(edits);
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
                        MotionGranularity::Word => next_word_boundary(&text_buf, cur_pos, forward),
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
                            (line as isize + delta).clamp(0, total_lines.saturating_sub(1) as isize)
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
        self.ensure_cursor_visible_x(4.0);
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
            .map(|tm| {
                tm.hit_test_char_styled(segment, self.font_size(), rel_x, self.font_family_str())
            })
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
            let breaks =
                Self::word_wrap_breaks(&line, avail, &*tm, font_size, font_family.as_deref());
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
        self.wrap_cache
            .get(logical)
            .map(|b| b.len() + 1)
            .unwrap_or(1)
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
                let start = if remaining == 0 {
                    0
                } else {
                    breaks[remaining - 1]
                };
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
            + (visual_row as isize - self.scroll_offset_lines as isize) as f32 * self.line_height()
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

    fn segment_byte_range(
        &self,
        line_text: &str,
        seg_start: usize,
        seg_end: usize,
    ) -> (usize, usize) {
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
            if self.line_marks != w.line_marks {
                self.line_marks = w.line_marks.clone();
                self.mark_dirty(DirtyFlags::RENDER);
            }
            if self.breakpoints != w.breakpoints {
                self.breakpoints = w.breakpoints.clone();
                self.mark_dirty(DirtyFlags::RENDER);
            }
            self.on_gutter_click = w.on_gutter_click.clone();
            if w.reveal_line != self.revealed {
                self.revealed = w.reveal_line;
                self.pending_reveal = w.reveal_line;
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
            }
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
            if let Some(sig) = self.command_signal {
                sig.subscribe_element(self.id);
            }
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

            let custom_changed = match (&w.highlighter, &self.custom_highlighter) {
                (Some(a), Some(b)) => !std::sync::Arc::ptr_eq(a, b),
                (None, None) => false,
                _ => true,
            };
            let need_lang_refresh = custom_changed
                || w.language != self.language
                || self.highlighter.is_none() && (w.language.is_some() || w.highlighter.is_some());
            if need_lang_refresh {
                self.language = w.language;
                self.custom_highlighter = w.highlighter.clone();
                self.highlighter = if self.over_size_limit {
                    None
                } else {
                    make_highlighter(self.custom_highlighter.as_ref(), self.language, self.tab_width, &self.text_snapshot)
                };
            } else if let Some(h) = self.highlighter.as_mut() {
                h.reparse(&self.text_snapshot);
            }

            self.on_change = w.on_change.clone();
            self.on_save = w.on_save.clone();
            self.key_hook = w.key_hook.clone();
            self.on_cursor = w.on_cursor.clone();
            self.lsp.update(w);
            self.lsp.subscribe(self.id);
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
        if let Some(line) = self.pending_reveal.take() {
            // строка вне экрана — показать её в верхней трети
            let row = self.logical_to_visual_line(line.min(self.buffer.len_lines().saturating_sub(1)), 0);
            let visible = self.visible_lines_count().max(1);
            if row < self.scroll_offset_lines || row >= self.scroll_offset_lines + visible {
                self.scroll_offset_lines = row.saturating_sub(visible / 3);
            }
        }
        Size::new(w, h)
    }

    fn ime_cursor_area(&self) -> Option<Rect> {
        if self.focused {
            self.ime_caret.get()
        } else {
            None
        }
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let theme = self.theme();
        let bg = theme.bg(&self.mss);
        let radius = self
            .mss
            .border_radius_uniform(self.bounds.size.width.min(self.bounds.size.height), 0.0);
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
        let (last_logical_inc, _, _) = self.visual_to_logical(last_visual.saturating_sub(1));
        let last_logical = (last_logical_inc + 1).min(self.buffer.len_lines());
        let highlights = if let Some(h) = &self.highlighter {
            h.highlight_lines(&self.text_snapshot, first_logical..last_logical)
        } else {
            Vec::new()
        };

        let diags = self.visible_diagnostics(first_logical, last_logical);

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

        // фон строк и номера — под всей шириной, текст — только правее gutter'а:
        // при горизонтальной прокрутке он уходит под номера, а не поверх них
        for vis_row in first_visual..last_visual {
            let (logical, seg_start_char, _) = self.visual_to_logical(vis_row);
            let y = self.visual_line_y(vis_row);
            if let Some((_, color)) = self.line_marks.iter().find(|(l, _)| *l == logical) {
                list.push_rect(Rect::new(Point::new(self.bounds.x(), y), Size::new(self.bounds.size.width, line_height)), color.with_alpha(0.08), [0.0; 4]);
                list.push_rect(Rect::new(Point::new(self.bounds.x(), y), Size::new(3.0, line_height)), *color, [0.0; 4]);
            }
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
            let bp = seg_start_char == 0 && self.show_line_numbers && self.breakpoints.contains(&logical);
            if bp {
                let d = (line_height - 6.0).clamp(6.0, 11.0);
                let r = Rect::new(Point::new(self.bounds.x() + 4.0, y + (line_height - d) / 2.0), Size::new(d, d));
                list.push_rect(r, Color::new(0.90, 0.25, 0.25, 1.0), [d / 2.0; 4]);
            } else if seg_start_char == 0 && !diags.is_empty() {
                self.paint_diag_gutter(list, &diags, logical, y);
            }
        }
        let gutter_w = self.gutter_w();
        list.push_clip(Rect::new(
            Point::new(self.bounds.x() + gutter_w, self.bounds.y()),
            Size::new((text_clip.size.width - gutter_w).max(0.0), self.bounds.size.height),
        ));

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

            self.paint_row_underlines(
                list,
                &diags,
                logical,
                &line_text,
                segment,
                seg_start_char,
                seg_end_char,
                is_last_visual_seg,
                text_origin,
            );

            if self.focused {
                for (ci, (c_line, c_pos)) in cursor_lines.iter().enumerate() {
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
                        if ci == 0 {
                            let before = &segment[..local.min(segment.len())];
                            let w = match &self.text_measure {
                                Some(tm) => {
                                    tm.measure_text_width(before, font_size, before.chars().count())
                                }
                                None => before.chars().count() as f32 * font_size * 0.6,
                            };
                            self.ime_caret.set(Some(Rect::new(
                                Point::new(text_origin.x + w, text_origin.y),
                                Size::new(1.0, line_height),
                            )));
                        }
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
            let opacity_h = crate::widgets::scroll::effective_opacity(&self.scrollbar_fader_h, &style);
            if opacity_h > 0.0 {
                let g = self.h_scrollbar_geom();
                crate::widgets::scroll::render_horizontal(
                    list,
                    g.viewport,
                    g.content_w,
                    g.scroll_x,
                    &style,
                    &self.scrollbar_fader_h,
                    opacity_h,
                );
            }
        }

        list.pop_clip();

        self.paint_lsp_overlays(list);
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
                self.hide_completion();
                self.reset_hover();
                self.goto_track(false);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::MouseMove(pos) => {
                let sb_geom = self.scrollbar_geom();
                let sb_style = self.scrollbar_style_now();
                let h_geom = self.h_scrollbar_geom();

                if let Some((_, new_x)) = self.scrollbar_interaction_h.update_drag(
                    &mut self.scrollbar_fader_h,
                    &h_geom,
                    &sb_style,
                    *pos,
                ) {
                    self.scroll_offset_x = new_x;
                    ctx.request_paint();
                    return EventResult::Captured;
                }

                if let Some((new_y, _)) = self.scrollbar_interaction.update_drag(
                    &mut self.scrollbar_fader,
                    &sb_geom,
                    &sb_style,
                    *pos,
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
                if !self.mouse_selecting {
                    let mut lsp_repaint = self.hover_track(self.hover.then_some(*pos));
                    lsp_repaint |= self.goto_track(ctx.modifiers.ctrl || ctx.modifiers.meta);
                    if self.hover && self.goto_hovering() {
                        ctx.set_cursor(CursorIcon::Pointer);
                    }
                    if lsp_repaint {
                        ctx.request_paint();
                    }
                }
                if self.mouse_selecting && self.focused {
                    let new_pos = self.click_to_cursor(*pos);
                    self.cursors.primary_mut().move_to(new_pos, true);
                    self.ensure_cursor_visible_x(0.0);
                    self.fire_on_cursor();
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if self.hover {
                    if self.scrollbar_interaction.update_hover(
                        &mut self.scrollbar_fader,
                        &sb_geom,
                        &sb_style,
                        *pos,
                        crate::widgets::scroll::SCROLLBAR_HIT_MARGIN,
                    ) {
                        ctx.request_paint();
                    }
                    if !self.soft_wrap
                        && self.scrollbar_interaction_h.update_hover(
                            &mut self.scrollbar_fader_h,
                            &h_geom,
                            &sb_style,
                            *pos,
                            crate::widgets::scroll::SCROLLBAR_HIT_MARGIN,
                        )
                    {
                        ctx.request_paint();
                    }
                } else {
                    let v = self.scrollbar_interaction.clear_hover(&mut self.scrollbar_fader);
                    let h = self.scrollbar_interaction_h.clear_hover(&mut self.scrollbar_fader_h);
                    if v || h {
                        ctx.request_paint();
                    }
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
                    if self.completion_click(*position) {
                        ctx.request_paint();
                        return EventResult::Handled;
                    }
                    let sb_geom = self.scrollbar_geom();
                    let sb_style = self.scrollbar_style_now();
                    if self.scrollbar_interaction.try_begin_drag(
                        &mut self.scrollbar_fader,
                        &sb_geom,
                        &sb_style,
                        *position,
                    ) {
                        ctx.request_paint();
                        return EventResult::Captured;
                    }
                    let h_geom = self.h_scrollbar_geom();
                    if !self.soft_wrap
                        && self.scrollbar_interaction_h.try_begin_drag(
                            &mut self.scrollbar_fader_h,
                            &h_geom,
                            &sb_style,
                            *position,
                        )
                    {
                        ctx.request_paint();
                        return EventResult::Captured;
                    }

                    // щелчок по полю номеров — приложению (точка останова)
                    if self.show_line_numbers && position.x < self.bounds.x() + self.gutter_w() {
                        if let Some(cb) = self.on_gutter_click.clone() {
                            let line = self.buffer.byte_to_line(self.click_to_cursor(*position));
                            if let Ok(mut f) = cb.lock() {
                                f(line);
                            }
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                    self.focused = true;
                    let new_pos = self.click_to_cursor(*position);
                    let alt = ctx.modifiers.alt;
                    let shift = ctx.modifiers.shift;
                    self.reset_hover();

                    // Ctrl+щелчок — переход к определению символа под мышью
                    if (ctx.modifiers.ctrl || ctx.modifiers.meta) && !alt && !shift && self.lsp.wants_goto() {
                        let target = self
                            .char_at_point(*position)
                            .unwrap_or_else(|| self.buffer.byte_to_line_col(new_pos));
                        self.cursors.clear_secondary();
                        self.cursors.primary_mut().move_to(new_pos, false);
                        self.cursors.primary_mut().anchor = None;
                        self.mouse_selecting = false;
                        self.undo.commit_group();
                        self.fire_on_cursor();
                        self.sync_completion();
                        self.goto_track(false);
                        self.goto_at(target.0, target.1);
                        ctx.request_paint();
                        return EventResult::Handled;
                    }

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
                    self.sync_completion();
                    ctx.set_cursor(CursorIcon::Text);
                    ctx.request_paint();
                    return EventResult::Handled;
                } else if *button == MouseButton::Right && self.bounds.contains(*position) {
                    // правый щелчок вне выделения — курсор туда (как в редакторах),
                    // событие идёт дальше: контекстное меню обёртки откроется над словом
                    let new_pos = self.click_to_cursor(*position);
                    // правый край включительно: щелчок по правой половине последнего
                    // символа даёт позицию конца выделения — оно должно остаться
                    let inside = self
                        .cursors
                        .iter()
                        .filter_map(|c| c.selection_range())
                        .any(|r| r.start <= new_pos && new_pos <= r.end);
                    if !inside {
                        self.cursors.clear_secondary();
                        self.cursors.primary_mut().move_to(new_pos, false);
                        self.cursors.primary_mut().anchor = None;
                        self.fire_on_cursor();
                    }
                    self.sync_completion();
                    self.reset_hover();
                    self.focused = true;
                    ctx.request_paint();
                    return EventResult::Ignored;
                } else if self.focused {
                    self.focused = false;
                    self.undo.commit_group();
                    ctx.request_paint();
                }
                EventResult::Ignored
            }
            Event::MouseUp { button, .. } => {
                if *button == MouseButton::Left {
                    let v = self.scrollbar_interaction.end_drag(&mut self.scrollbar_fader);
                    let h = self.scrollbar_interaction_h.end_drag(&mut self.scrollbar_fader_h);
                    if v || h {
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
                    let (start, end) =
                        crate::widget::selection::TextSelectionState::find_word_boundaries(
                            text, click_pos,
                        );
                    let pri = self.cursors.primary_mut();
                    pri.anchor = Some(start);
                    pri.pos = end;
                    pri.sticky_col = None;
                    self.fire_on_cursor();
                    self.sync_completion();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::MouseWheel {
                delta, delta_x, position,
            } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                if self.completion_wheel(*position, *delta) {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.reset_hover() {
                    ctx.request_paint();
                }
                // горизонталь: тачпад или Shift+колесо (платформа кладёт его в delta_x)
                if *delta_x != 0.0 && !self.soft_wrap {
                    let old = self.scroll_offset_x;
                    let new = old - *delta_x;
                    self.scroll_offset_x = if new > old {
                        new.min(self.max_scroll_x().max(old))
                    } else {
                        new.max(0.0)
                    };
                    if (self.scroll_offset_x - old).abs() > 0.01 {
                        self.scrollbar_fader_h.flash();
                        ctx.request_paint();
                    }
                    if *delta == 0.0 {
                        return EventResult::Handled;
                    }
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
                if matches!(key, Key::Ctrl | Key::Meta) {
                    if self.goto_track(true) {
                        ctx.request_paint();
                    }
                    return EventResult::Ignored;
                }
                if let Some(hook) = self.key_hook.as_ref() {
                    if hook(*key, ctx.modifiers) {
                        return EventResult::Handled;
                    }
                }
                // открытый список дополнения забирает ↑/↓/Enter/Tab/Esc
                if self.completion_key(*key, ctx.modifiers) {
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                let m = ctx.modifiers;
                let ctrl = m.ctrl || m.meta;
                if *key == Key::Space && ctrl && !m.alt && self.lsp.can_request() {
                    self.reset_hover();
                    self.request_completion(None, true);
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if *key == Key::F12 && !ctrl && !m.alt && !m.shift && self.lsp.wants_goto() {
                    self.goto_caret();
                    return EventResult::Handled;
                }
                let Some(action) = map_key(*key, ctx.modifiers) else {
                    return EventResult::Ignored;
                };
                self.handle_action(action, ctx);
                self.reset_hover();
                self.sync_completion();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::KeyUp(Key::Ctrl | Key::Meta) => {
                if self.goto_track(false) {
                    ctx.request_paint();
                }
                EventResult::Ignored
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
                self.reset_hover();
                self.after_char_typed(*ch);
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImeCommit(text) => {
                if !self.focused || self.read_only {
                    return EventResult::Ignored;
                }
                self.preedit = None;
                self.replace_selection_or_insert(text);
                self.reset_hover();
                self.sync_completion();
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
        if let Some(sig) = self.command_signal {
            sig.subscribe_element(self.id);
        }
        self.lsp.subscribe(self.id);
    }

    /// Команда (пункт контекстного меню, внешний Reload) приходит через
    /// сигнал, а разбирается в `animate`. Без подписки редактор не попадает
    /// в реестр анимаций, и команда ждёт, пока его разбудит что-то другое.
    fn on_signal_change(&mut self) -> bool {
        // диагностики из сигнала — на лету, без пересборки виджета
        let diags = self.lsp.has_diag_signal() && self.lsp.sync_diagnostics();
        // список дополнения или справка от приложения — перерисовать
        if self.lsp.has_signals() {
            self.refresh_completion();
            return true;
        }
        diags || self.has_pending_command()
    }

    fn wants_animate_tick(&self) -> bool {
        self.has_pending_command() || self.lsp.hover_pending()
    }

    fn wants_key(&self, key: Key) -> bool {
        key == Key::F12 && self.focused && self.lsp.wants_goto()
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

    fn mss(&self) -> Option<&crate::mss::MssFields> {
        Some(&self.mss)
    }

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
        self.mss
            .apply_transitions(base, hover, active, focus, selected);
    }

    fn wants_tab(&self) -> bool {
        self.focused && !self.read_only
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        let style = self
            .mss
            .scrollbar_style(self.mss.color.unwrap_or(Color::from_hex("#9CA3AF")));
        let mut needs_repaint = self.scrollbar_fader.tick(dt.as_secs_f32(), &style);
        needs_repaint |= self.scrollbar_fader_h.tick(dt.as_secs_f32(), &style);

        if let Some(sig) = self.command_signal {
            if let Some(cmd) = sig.get_untracked() {
                self.process_command(cmd);
                sig.set(None);
                self.sync_completion();
                self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
                needs_repaint = true;
            }
        }
        if self.hover_tick(dt) {
            self.mark_dirty(DirtyFlags::RENDER);
            needs_repaint = true;
        }

        self.push_state_to_signal();
        needs_repaint
    }

    fn needs_repaint(&self) -> bool {
        self.scrollbar_fader.opacity > 0.0 || self.scrollbar_fader_h.opacity > 0.0
    }
}

impl CodeEditorElement {
    fn has_pending_command(&self) -> bool {
        self.command_signal
            .is_some_and(|sig| sig.with_untracked(|c| c.is_some()))
    }

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

    use super::super::lsp::{
        CompletionItem, CompletionList, CompletionTrigger, Diagnostic, DiagnosticSeverity, HoverInfo,
    };
    use super::super::widget::EditorCommand;
    use crate::input::Modifiers;
    use crate::signal::{allow_signal_reads_on_this_thread, use_signal, RwSignal};
    use std::sync::Mutex as StdMutex;

    fn ctx_with(el: &CodeEditorElement, m: Modifiers) -> EventContext {
        let mut ctx = EventContext::new(el.id);
        ctx.modifiers = m;
        ctx
    }

    fn send(el: &mut CodeEditorElement, ev: Event) -> EventResult {
        let mut ctx = ctx_with(el, Modifiers::empty());
        el.handle_event(&ev, &mut ctx)
    }

    fn send_mod(el: &mut CodeEditorElement, ev: Event, m: Modifiers) -> EventResult {
        let mut ctx = ctx_with(el, m);
        el.handle_event(&ev, &mut ctx)
    }

    fn ctrl() -> Modifiers {
        Modifiers { ctrl: true, ..Modifiers::empty() }
    }

    fn laid_out(editor: &CodeEditor) -> CodeEditorElement {
        let mut el = CodeEditorElement::new(editor);
        el.layout(Constraints::tight(Size::new(800.0, 400.0)));
        send(&mut el, Event::FocusGained);
        el
    }

    fn item(label: &str, kind: &str) -> CompletionItem {
        CompletionItem { label: label.into(), insert_text: label.into(), kind: kind.into(), detail: String::new() }
    }

    fn completion_editor(text: &str) -> (CodeEditor, RwSignal<Option<CompletionList>>, Arc<StdMutex<Vec<CompletionTrigger>>>) {
        allow_signal_reads_on_this_thread();
        let sig = use_signal::<Option<CompletionList>>(None);
        let reqs = Arc::new(StdMutex::new(Vec::new()));
        let r2 = reqs.clone();
        let ed = CodeEditor::new()
            .text(text)
            .completion(sig)
            .completion_triggers(vec!['.'])
            .on_completion_request(move |t| r2.lock().unwrap().push(t));
        (ed, sig, reqs)
    }

    #[test]
    fn completion_filters_and_accept_replaces_range() {
        let (ed, sig, _) = completion_editor("fn main() {\n    pri\n}");
        let changes = Arc::new(StdMutex::new(0usize));
        let c2 = changes.clone();
        let ed = ed.on_change(move |_| *c2.lock().unwrap() += 1);
        let mut el = laid_out(&ed);
        el.process_command(EditorCommand::SetCursor { line: 1, col: 7 });
        sig.set(Some(CompletionList {
            line: 1,
            start_col: 4,
            items: vec![item("eprintln", "macro"), item("println", "macro"), item("print", "fn"), item("len", "fn")],
        }));
        el.refresh_completion();
        let v = el.completion_view().expect("список виден");
        assert_eq!(v.filter, "pri");
        // сначала начинающиеся с «pri», потом содержащие
        assert_eq!(v.indices, vec![1, 2, 0]);

        // список открыт — ↓ двигает выбор, а не каретку
        let caret = el.primary_byte();
        assert_eq!(send(&mut el, Event::KeyDown(Key::Down)), EventResult::Handled);
        assert_eq!(el.primary_byte(), caret);
        assert_eq!(el.lsp.sel, 1);
        send(&mut el, Event::KeyDown(Key::Enter));
        assert_eq!(el.text_snapshot, "fn main() {\n    print\n}");
        assert_eq!(el.buffer.byte_to_line_col(el.primary_byte()), (1, 9));
        assert!(sig.get_untracked().is_none(), "принятый список закрыт");
        assert_eq!(*changes.lock().unwrap(), 1, "on_change вызван");

        // отмена возвращает набранное
        send_mod(&mut el, Event::KeyDown(Key::Z), ctrl());
        assert_eq!(el.text_snapshot, "fn main() {\n    pri\n}");
    }

    #[test]
    fn completion_filter_narrows_while_typing_and_hides_when_empty() {
        let (ed, sig, _) = completion_editor("x\n");
        let mut el = laid_out(&ed);
        sig.set(Some(CompletionList { line: 0, start_col: 1, items: vec![item("count", ""), item("clear", "")] }));
        el.process_command(EditorCommand::SetCursor { line: 0, col: 1 });
        el.refresh_completion();
        assert_eq!(el.completion_view().unwrap().indices, vec![0, 1]);
        send(&mut el, Event::CharInput('c'));
        send(&mut el, Event::CharInput('l'));
        assert_eq!(el.completion_view().unwrap().indices, vec![1]);
        send(&mut el, Event::CharInput('z'));
        assert!(sig.get_untracked().is_none(), "пустой отфильтрованный список закрывается");
    }

    #[test]
    fn escape_closes_completion_without_editing() {
        let (ed, sig, _) = completion_editor("ab");
        let mut el = laid_out(&ed);
        el.process_command(EditorCommand::SetCursor { line: 0, col: 2 });
        sig.set(Some(CompletionList { line: 0, start_col: 0, items: vec![item("abc", "")] }));
        assert!(el.completion_view().is_some());
        assert_eq!(send(&mut el, Event::KeyDown(Key::Escape)), EventResult::Handled);
        assert!(sig.get_untracked().is_none());
        assert_eq!(el.text_snapshot, "ab");
        // каретка ушла левее начала — тоже закрыт
        sig.set(Some(CompletionList { line: 0, start_col: 2, items: vec![item("abc", "")] }));
        send(&mut el, Event::KeyDown(Key::Left));
        assert!(sig.get_untracked().is_none());
    }

    #[test]
    fn ctrl_space_and_typing_request_completion() {
        let (ed, _sig, reqs) = completion_editor("\n");
        let mut el = laid_out(&ed);
        assert_eq!(send_mod(&mut el, Event::KeyDown(Key::Space), ctrl()), EventResult::Handled);
        assert_eq!(
            reqs.lock().unwrap().last().copied(),
            Some(CompletionTrigger { line: 0, col: 0, trigger_char: None, manual: true })
        );
        reqs.lock().unwrap().clear();
        send(&mut el, Event::CharInput('p'));
        assert!(reqs.lock().unwrap().is_empty(), "одна буква — без запроса");
        send(&mut el, Event::CharInput('r'));
        send(&mut el, Event::CharInput('i'));
        assert_eq!(
            *reqs.lock().unwrap(),
            vec![CompletionTrigger { line: 0, col: 2, trigger_char: None, manual: false }],
            "один запрос на слово, пока нет ответа"
        );
        send(&mut el, Event::CharInput('.'));
        assert_eq!(
            reqs.lock().unwrap().last().copied(),
            Some(CompletionTrigger { line: 0, col: 4, trigger_char: Some('.'), manual: false })
        );
    }

    #[test]
    fn f12_and_ctrl_click_call_on_goto() {
        let got = Arc::new(StdMutex::new(Vec::new()));
        let g2 = got.clone();
        let ed = CodeEditor::new().text("let value = 1;\nfoo(value)").on_goto(move |l, c| g2.lock().unwrap().push((l, c)));
        let mut el = laid_out(&ed);
        assert!(el.wants_key(Key::F12));
        el.process_command(EditorCommand::SetCursor { line: 1, col: 5 });
        assert_eq!(send(&mut el, Event::KeyDown(Key::F12)), EventResult::Handled);
        assert_eq!(got.lock().unwrap().last().copied(), Some((1, 5)));

        // щелчок с Ctrl по третьему символу первой строки
        let cw = el.char_width();
        let p = Point::new(el.text_origin_x() + cw * 2.5, el.bounds.y() + el.line_height() * 0.5);
        send_mod(&mut el, Event::MouseDown { button: MouseButton::Left, position: p }, ctrl());
        assert_eq!(got.lock().unwrap().last().copied(), Some((0, 2)));
        assert!(!el.mouse_selecting, "Ctrl+щелчок не начинает выделение");
        assert!(!el.cursors.primary().has_selection());
    }

    #[test]
    fn apply_edits_one_undo_step_and_key_hook() {
        use super::super::widget::TextEdit;
        let hits = Arc::new(StdMutex::new(0));
        let h2 = hits.clone();
        let ed = CodeEditor::new().text("let value = 1;\nfoo(value)").on_key(move |k, _| {
            if k == Key::F2 {
                *h2.lock().unwrap() += 1;
                return true;
            }
            false
        });
        let mut el = laid_out(&ed);
        el.process_command(EditorCommand::SetCursor { line: 1, col: 10 });
        el.process_command(EditorCommand::ApplyEdits(vec![
            TextEdit { start_line: 0, start_col: 4, end_line: 0, end_col: 9, text: "count".into() },
            TextEdit { start_line: 1, start_col: 4, end_line: 1, end_col: 9, text: "count".into() },
        ]));
        assert_eq!(el.text_snapshot, "let count = 1;\nfoo(count)");
        // курсор за «)» второй строки остался за ней
        assert_eq!(el.cursors.primary().pos, el.text_snapshot.len());
        send_mod(&mut el, Event::KeyDown(Key::Z), ctrl());
        assert_eq!(el.text_snapshot, "let value = 1;\nfoo(value)", "одна отмена — обе правки");
        assert_eq!(send(&mut el, Event::KeyDown(Key::F2)), EventResult::Handled);
        assert_eq!(*hits.lock().unwrap(), 1);
    }

    #[test]
    fn set_cursor_command_moves_caret_and_scrolls() {
        allow_signal_reads_on_this_thread();
        let cmd = use_signal::<Option<EditorCommand>>(None);
        let ed = CodeEditor::new().text(make_text(300)).command_signal(cmd);
        let mut el = laid_out(&ed);
        el.cursors.primary_mut().anchor = Some(0);
        cmd.set(Some(EditorCommand::SetCursor { line: 200, col: 3 }));
        el.animate(std::time::Duration::from_millis(16));
        assert!(cmd.get_untracked().is_none());
        assert_eq!(el.primary_byte(), el.buffer.line_col_to_byte(200, 3));
        assert!(!el.cursors.primary().has_selection());
        let visible = el.visible_lines_count();
        assert!(el.scroll_offset_lines <= 200 && 200 < el.scroll_offset_lines + visible);
        assert!(el.scroll_offset_lines > 180, "строка ближе к центру, а не у края");
        // за пределами — к ближайшей позиции
        el.process_command(EditorCommand::SetCursor { line: 10_000, col: 10_000 });
        assert_eq!(el.primary_byte(), el.buffer.len_bytes());
    }

    #[test]
    fn diagnostics_update_keeps_text_and_cursor_and_paints() {
        let text = "let a = 1;\nlet b = a +;\n";
        let mut el = laid_out(&CodeEditor::new().text(text));
        el.process_command(EditorCommand::SetCursor { line: 1, col: 4 });
        let caret = el.primary_byte();
        send(&mut el, Event::CharInput('x'));
        let edited = el.text_snapshot.clone();
        let caret = caret + 1;

        let diags = vec![
            Diagnostic { line: 1, col: 10, end_line: 1, end_col: 11, severity: DiagnosticSeverity::Error, message: "expected expression".into() },
            Diagnostic { line: 0, col: 4, end_line: 0, end_col: 4, severity: DiagnosticSeverity::Warning, message: "unused".into() },
        ];
        let mut ctx = UpdateContext::new(el.id);
        el.update(&CodeEditor::new().text(edited.clone()).diagnostics(diags.clone()), &mut ctx);
        assert_eq!(el.lsp.diagnostics, diags);
        assert_eq!(el.text_snapshot, edited);
        assert_eq!(el.primary_byte(), caret);
        assert_eq!(el.visible_diagnostics(1, 2), vec![0]);

        let mut list = DisplayList::new();
        el.build_display_list(&mut list, el.bounds);
        let strips = list
            .iter_all_commands()
            .filter(|c| matches!(c, crate::render::DrawCommand::LineStrip { .. }))
            .count();
        assert_eq!(strips, 2, "волнистая линия под каждой диагностикой");
    }

    #[test]
    fn hover_calls_on_hover_once_and_hides_on_word_change() {
        allow_signal_reads_on_this_thread();
        let hover = use_signal::<Option<HoverInfo>>(None);
        let got = Arc::new(StdMutex::new(Vec::new()));
        let g2 = got.clone();
        let ed = CodeEditor::new().text("alpha beta").hover(hover).on_hover(move |l, c| g2.lock().unwrap().push((l, c)));
        let mut el = laid_out(&ed);
        let cw = el.char_width();
        let y = el.bounds.y() + el.line_height() * 0.5;
        let at = |c: f32| Point::new(el.text_origin_x() + cw * c, y);
        let (p1, p2, p3) = (at(1.5), at(2.5), at(7.5));
        send(&mut el, Event::MouseMove(p1));
        assert!(el.wants_animate_tick());
        el.animate(std::time::Duration::from_millis(300));
        assert!(got.lock().unwrap().is_empty());
        el.animate(std::time::Duration::from_millis(300));
        assert_eq!(*got.lock().unwrap(), vec![(0, 1)]);
        el.animate(std::time::Duration::from_millis(600));
        assert_eq!(got.lock().unwrap().len(), 1, "один раз на место");

        hover.set(Some(HoverInfo { line: 0, col: 1, text: "fn alpha()".into() }));
        let mut list = DisplayList::new();
        el.build_display_list(&mut list, el.bounds);
        assert!(!list.overlay_commands().is_empty(), "подсказка в слое оверлея");

        // в пределах слова — подсказка остаётся, другое слово — скрыта
        send(&mut el, Event::MouseMove(p2));
        assert!(hover.get_untracked().is_some());
        send(&mut el, Event::MouseMove(p3));
        assert!(hover.get_untracked().is_none());
        // gutter — не текст
        assert_eq!(el.char_at_point(Point::new(el.bounds.x() + 4.0, y)), None);
    }


    #[test]
    fn harness_popup_paints_and_click_accepts() {
        use crate::testing::TestHarness;
        let (ed, sig, reqs) = completion_editor("");
        let changes = Arc::new(StdMutex::new(String::new()));
        let c2 = changes.clone();
        let ed = ed.on_change(move |c| *c2.lock().unwrap() = c.full_text.to_string());
        let mut h = TestHarness::new(Box::new(ed));
        h.layout(600.0, 300.0);
        h.send_events(&crate::testing::click_at(Point::new(200.0, 10.0)));
        h.send_events(&crate::testing::type_text("ve"));
        let t = reqs.lock().unwrap().last().copied().expect("запрос после двух букв");
        sig.set(Some(CompletionList {
            line: t.line,
            start_col: 0,
            items: vec![item("vec", "macro"), item("very_long_name", "var")],
        }));
        let list = h.paint();
        assert!(!list.overlay_commands().is_empty(), "список нарисован поверх");
        // щелчок по второй строке списка (под строкой каретки)
        let lh = (13.0f32 * 1.5).round();
        let y = lh + 2.0 + 4.0 + lh * 1.5;
        h.send_events(&crate::testing::click_at(Point::new(120.0, y)));
        assert_eq!(*changes.lock().unwrap(), "very_long_name");
        assert!(sig.get_untracked().is_none());
    }


    #[test]
    fn gutter_click_and_menu_commands() {
        let got = Arc::new(StdMutex::new(Vec::new()));
        let g2 = got.clone();
        let ed = CodeEditor::new().text("one\ntwo\nthree").breakpoints(vec![1]).on_gutter_click(move |l| g2.lock().unwrap().push(l));
        let mut el = laid_out(&ed);
        // щелчок по полю номеров третьей строки — обработчику, каретка на месте
        let p = Point::new(el.bounds.x() + 10.0, el.bounds.y() + el.line_height() * 2.5);
        assert_eq!(send(&mut el, Event::MouseDown { button: MouseButton::Left, position: p }), EventResult::Handled);
        assert_eq!(*got.lock().unwrap(), vec![2]);
        assert_eq!(el.cursors.primary().pos, 0);

        // «Удалить» выделения, «Отменить», «Повторить» — командами (контекстное меню)
        el.process_command(EditorCommand::SelectAll);
        el.process_command(EditorCommand::Delete);
        assert_eq!(el.text_snapshot, "");
        el.process_command(EditorCommand::Undo);
        assert_eq!(el.text_snapshot, "one\ntwo\nthree");
        el.process_command(EditorCommand::Redo);
        assert_eq!(el.text_snapshot, "");
    }
}
