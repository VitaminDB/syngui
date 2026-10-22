//! Языковой сервер в редакторе: диагностики, список дополнения, подсказка
//! при наведении, переход к определению и установка каретки по команде.

use super::super::lsp::{
    diag_cols_on_line, filter_completion, is_ident_char, word_range, wrap_text, CompletionList,
    CompletionTrigger, Diagnostic, HoverInfo,
};
use super::super::render::popup;
use super::super::syntax::TokenClass;
use super::super::widget::CodeEditor;
use super::CodeEditorElement;
use crate::core::sync::Mutex;
use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{Key, Modifiers};
use crate::render::DisplayList;
use crate::signal::RwSignal;
use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

/// Строк в списке дополнения без прокрутки.
pub(super) const COMPLETION_ROWS: usize = 10;
/// Мышь неподвижна над словом столько — подсказка.
const HOVER_DELAY: Duration = Duration::from_millis(500);
const POPUP_PAD: f32 = 4.0;
const TIP_MAX_CHARS: usize = 60;
const TIP_MAX_LINES: usize = 16;

/// Где нарисован список дополнения в прошлом кадре — для щелчка и колеса.
#[derive(Clone, Copy, Debug)]
pub(super) struct PopupGeom {
    rect: Rect,
    row_h: f32,
    top: usize,
    total: usize,
}

/// Что видно в списке: строка, начало заменяемого, набранный фильтр и
/// номера подходящих вариантов.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CompletionView {
    pub line: usize,
    pub start_col: usize,
    pub filter: String,
    pub indices: Vec<usize>,
}

type PosCb = Arc<Mutex<dyn FnMut(usize, usize) + Send>>;

pub(super) struct LspState {
    pub diagnostics: Vec<Diagnostic>,
    diag_signal: Option<RwSignal<Vec<Diagnostic>>>,
    /// Номера диагностик по возрастанию начальной строки.
    diag_order: Vec<usize>,

    pub completion: Option<RwSignal<Option<CompletionList>>>,
    triggers: Vec<char>,
    on_request: Option<Arc<Mutex<dyn FnMut(CompletionTrigger) + Send>>>,
    pub sel: usize,
    top: usize,
    /// По чему узнаём, что список сменился (выбор — на первый).
    view_key: Option<CompletionView>,
    /// Запрос за слово (строка, начало) отправлен, ответа нет — не повторять.
    awaiting: Option<(usize, usize)>,
    popup: Cell<Option<PopupGeom>>,

    pub hover: Option<RwSignal<Option<HoverInfo>>>,
    on_hover: Option<PosCb>,
    on_goto: Option<PosCb>,
    mouse_pos: Option<Point>,
    /// Слово под мышью (строка, начало, конец в символах) и сам символ.
    hover_word: Option<(usize, usize, usize)>,
    hover_char: Option<(usize, usize)>,
    hover_elapsed: Duration,
    /// Задержка прошла: `on_hover` вызван, диагностики показаны.
    hover_fired: bool,
    /// Слово под мышью с зажатым Ctrl — подчёркнуто, курсор «рука».
    goto_word: Option<(usize, usize, usize)>,
}

impl LspState {
    pub fn from_widget(w: &CodeEditor) -> Self {
        let mut s = Self {
            diagnostics: w.diagnostics_signal.map(|s| s.get_untracked()).unwrap_or_else(|| w.diagnostics.clone()),
            diag_signal: w.diagnostics_signal,
            diag_order: Vec::new(),
            completion: w.completion,
            triggers: w.completion_triggers.clone(),
            on_request: w.on_completion_request.clone(),
            sel: 0,
            top: 0,
            view_key: None,
            awaiting: None,
            popup: Cell::new(None),
            hover: w.hover,
            on_hover: w.on_hover.clone(),
            on_goto: w.on_goto.clone(),
            mouse_pos: None,
            hover_word: None,
            hover_char: None,
            hover_elapsed: Duration::ZERO,
            hover_fired: false,
            goto_word: None,
        };
        s.rebuild_order();
        s
    }

    fn rebuild_order(&mut self) {
        let mut order: Vec<usize> = (0..self.diagnostics.len()).collect();
        order.sort_by_key(|&i| self.diagnostics[i].line);
        self.diag_order = order;
    }

    /// Новые параметры виджета; `true` — перерисовать. Текст и каретку
    /// не трогает.
    pub fn update(&mut self, w: &CodeEditor) -> bool {
        let mut changed = false;
        self.diag_signal = w.diagnostics_signal;
        let wanted = w.diagnostics_signal.map(|s| s.get_untracked()).unwrap_or_else(|| w.diagnostics.clone());
        if self.diagnostics != wanted {
            self.diagnostics = wanted;
            self.rebuild_order();
            changed = true;
        }
        self.completion = w.completion;
        self.triggers = w.completion_triggers.clone();
        self.on_request = w.on_completion_request.clone();
        self.hover = w.hover;
        self.on_hover = w.on_hover.clone();
        self.on_goto = w.on_goto.clone();
        changed
    }

    /// Диагностики из сигнала изменились — взять новые; `true` — перерисовать.
    pub fn sync_diagnostics(&mut self) -> bool {
        let Some(s) = self.diag_signal else { return false };
        let d = s.get_untracked();
        if d == self.diagnostics {
            return false;
        }
        self.diagnostics = d;
        self.rebuild_order();
        true
    }

    pub fn subscribe(&self, id: crate::widget::ElementId) {
        if let Some(s) = self.diag_signal {
            s.subscribe_element(id);
        }
        if let Some(s) = self.completion {
            s.subscribe_element(id);
        }
        if let Some(s) = self.hover {
            s.subscribe_element(id);
        }
    }

    pub fn has_signals(&self) -> bool {
        self.completion.is_some() || self.hover.is_some()
    }

    pub fn has_diag_signal(&self) -> bool {
        self.diag_signal.is_some()
    }

    /// Ждём задержку наведения — нужны тики `animate`.
    pub fn hover_pending(&self) -> bool {
        self.hover_word.is_some() && !self.hover_fired
    }

    pub fn can_request(&self) -> bool {
        self.on_request.is_some()
    }

    pub fn wants_goto(&self) -> bool {
        self.on_goto.is_some()
    }

    fn tracks_hover(&self) -> bool {
        self.on_hover.is_some() || !self.diagnostics.is_empty()
    }

    /// Диагностики, задевающие строки `first..last`.
    pub fn visible(&self, first: usize, last: usize) -> Vec<usize> {
        let n = self.diag_order.partition_point(|&i| self.diagnostics[i].line < last);
        self.diag_order[..n]
            .iter()
            .copied()
            .filter(|&i| {
                let d = &self.diagnostics[i];
                d.end_line.max(d.line) >= first
            })
            .collect()
    }
}

fn call_pos(cb: &Option<PosCb>, line: usize, col: usize) {
    if let Some(cb) = cb {
        if let Ok(mut f) = cb.lock() {
            f(line, col);
        }
    }
}

fn kind_class(kind: &str) -> Option<TokenClass> {
    Some(match kind {
        "fn" | "function" | "method" | "constructor" => TokenClass::Function,
        "macro" => TokenClass::FunctionMacro,
        "var" | "variable" | "local" | "param" => TokenClass::Variable,
        "field" | "property" | "prop" | "member" => TokenClass::Property,
        "kw" | "keyword" => TokenClass::Keyword,
        "type" | "struct" | "class" | "enum" | "interface" | "trait" | "typeparam" => TokenClass::Type,
        "const" | "constant" | "enummember" | "value" => TokenClass::Constant,
        "mod" | "module" | "namespace" | "package" => TokenClass::Namespace,
        "snippet" | "text" | "str" => TokenClass::String,
        _ => return None,
    })
}

impl CodeEditorElement {
    fn text_w(&self, s: &str, fs: f32) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        let n = s.chars().count();
        match &self.text_measure {
            Some(tm) => tm.measure_text_width_styled(s, fs, n, false, self.font_family_str()),
            None => n as f32 * fs * 0.6,
        }
    }

    /// Ширина первых `local` символов куска строки (за концом — по ширине
    /// «M» на каждый лишний).
    fn seg_char_x(&self, segment: &str, local: usize) -> f32 {
        let fs = self.font_size();
        let n = segment.chars().count();
        let take = local.min(n);
        let end = segment.char_indices().nth(take).map(|(b, _)| b).unwrap_or(segment.len());
        self.text_w(&segment[..end], fs) + (local - take) as f32 * self.char_width()
    }

    /// Символ текста под точкой `(строка, символ)`; gutter, правее конца
    /// строки и ниже текста — `None`.
    pub(super) fn char_at_point(&self, pos: Point) -> Option<(usize, usize)> {
        if !self.bounds.contains(pos) || pos.x < self.bounds.x() + self.gutter_w() {
            return None;
        }
        let lh = self.line_height().max(1.0);
        let row = ((pos.y - self.bounds.y()).max(0.0) / lh).floor() as usize;
        let visual = self.scroll_offset_lines + row;
        if visual >= self.total_visual_lines.max(1) {
            return None;
        }
        let (logical, seg_start, seg_end) = self.visual_to_logical(visual);
        let line_text = self.buffer.line_str(logical);
        let (b0, b1) = self.segment_byte_range(&line_text, seg_start, seg_end);
        let segment = &line_text[b0..b1];
        let rel_x = pos.x - self.text_origin_x();
        if rel_x < 0.0 {
            return None;
        }
        let n = segment.chars().count();
        let fs = self.font_size();
        let mut col = match &self.text_measure {
            Some(tm) => tm.hit_test_char_styled(segment, fs, rel_x, self.font_family_str()),
            None => (rel_x / (fs * 0.6)).floor() as usize,
        }
        .min(n);
        if col > 0 && self.seg_char_x(segment, col) > rel_x {
            col -= 1;
        }
        if col >= n || rel_x >= self.seg_char_x(segment, col + 1) {
            return None;
        }
        Some((logical, seg_start + col))
    }

    /// Левый верхний угол символа `line:col` на экране (`None` — строка не
    /// видна).
    fn char_screen_pos(&self, line: usize, col: usize) -> Option<Point> {
        if line >= self.buffer.len_lines() {
            return None;
        }
        let row = self.logical_to_visual_line(line, col);
        if row < self.scroll_offset_lines || row > self.scroll_offset_lines + self.visible_lines_count() {
            return None;
        }
        let (_, seg_start, seg_end) = self.visual_to_logical(row);
        let line_text = self.buffer.line_str(line);
        let (b0, b1) = self.segment_byte_range(&line_text, seg_start, seg_end);
        let x = self.text_origin_x() + self.seg_char_x(&line_text[b0..b1], col.saturating_sub(seg_start));
        Some(Point::new(x, self.visual_line_y(row)))
    }

    fn caret_line_col(&self) -> (usize, usize) {
        self.buffer.byte_to_line_col(self.primary_byte())
    }

    // ── дополнение ──────────────────────────────────────────────────────

    /// Список, который сейчас виден, — или `None`, если его нет или каретка
    /// ушла со строки, левее начала, выделение, фильтр ничего не оставил.
    pub(super) fn completion_view(&self) -> Option<CompletionView> {
        let sig = self.lsp.completion?;
        if !self.cursors.is_single() || self.cursors.primary().has_selection() {
            return None;
        }
        let (line, col) = self.caret_line_col();
        sig.with_untracked(|l| {
            let l = l.as_ref()?;
            if l.line != line || col < l.start_col {
                return None;
            }
            let text = self.buffer.line_str(line);
            let filter: String = text.chars().skip(l.start_col).take(col - l.start_col).collect();
            let indices = filter_completion(&l.items, &filter);
            if indices.is_empty() {
                return None;
            }
            Some(CompletionView { line: l.line, start_col: l.start_col, filter, indices })
        })
    }

    fn completion_open(&self) -> bool {
        self.lsp.completion.is_some_and(|s| s.with_untracked(|l| l.is_some()))
    }

    /// Новый список или фильтр — выбор на первый вариант.
    pub(super) fn refresh_completion(&mut self) {
        if self.completion_open() {
            self.lsp.awaiting = None;
        }
        let view = self.completion_view();
        match &view {
            Some(v) => {
                if self.lsp.view_key.as_ref() != Some(v) {
                    self.lsp.sel = 0;
                    self.lsp.top = 0;
                }
                let n = v.indices.len();
                self.lsp.sel = self.lsp.sel.min(n - 1);
                self.lsp.top = self.lsp.top.min(n.saturating_sub(COMPLETION_ROWS));
            }
            None => self.lsp.popup.set(None),
        }
        self.lsp.view_key = view;
    }

    /// После ввода, движения каретки, щелчка: список, который больше не
    /// виден, закрывается (`None` в сигнал).
    pub(super) fn sync_completion(&mut self) {
        if self.completion_open() && self.completion_view().is_none() {
            self.hide_completion();
        } else {
            self.refresh_completion();
        }
    }

    pub(super) fn hide_completion(&mut self) {
        if let Some(s) = self.lsp.completion {
            if s.with_untracked(|l| l.is_some()) {
                s.set(None);
            }
        }
        self.lsp.view_key = None;
        self.lsp.awaiting = None;
        self.lsp.popup.set(None);
    }

    fn word_start_before_caret(&self) -> (usize, usize, usize) {
        let (line, col) = self.caret_line_col();
        let text = self.buffer.line_str(line);
        let before: Vec<char> = text.chars().take(col).collect();
        let n = before.iter().rev().take_while(|c| is_ident_char(**c)).count();
        (line, col, n)
    }

    pub(super) fn request_completion(&mut self, trigger_char: Option<char>, manual: bool) {
        let Some(cb) = self.lsp.on_request.clone() else { return };
        let (line, col, n) = self.word_start_before_caret();
        // повтор гасится только у автоматических запросов по слову
        self.lsp.awaiting = (trigger_char.is_none() && !manual).then_some((line, col - n));
        if let Ok(mut f) = cb.lock() {
            f(CompletionTrigger { line, col, trigger_char, manual });
        };
    }

    /// Символ напечатан: триггер — запрос; буква слова при закрытом списке
    /// и слове от двух букв — запрос (один на слово, пока нет ответа).
    pub(super) fn after_char_typed(&mut self, ch: char) {
        if self.lsp.on_request.is_none() && self.lsp.completion.is_none() {
            return;
        }
        if self.lsp.triggers.contains(&ch) {
            self.hide_completion();
            self.request_completion(Some(ch), false);
            return;
        }
        self.sync_completion();
        if is_ident_char(ch) && !self.completion_open() {
            let (line, col, n) = self.word_start_before_caret();
            if n >= 2 && self.lsp.awaiting != Some((line, col - n)) {
                self.request_completion(None, false);
            }
        }
    }

    fn scroll_completion_to_sel(&mut self) {
        if self.lsp.sel < self.lsp.top {
            self.lsp.top = self.lsp.sel;
        } else if self.lsp.sel >= self.lsp.top + COMPLETION_ROWS {
            self.lsp.top = self.lsp.sel + 1 - COMPLETION_ROWS;
        }
    }

    /// Клавиша при открытом списке; `true` — съедена (обычное действие
    /// редактора не выполняется).
    pub(super) fn completion_key(&mut self, key: Key, m: Modifiers) -> bool {
        if m.ctrl || m.alt || m.meta || m.shift {
            return false;
        }
        if !matches!(
            key,
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Enter | Key::Tab | Key::Escape
        ) {
            return false;
        }
        self.refresh_completion();
        let Some(v) = self.lsp.view_key.clone() else { return false };
        let n = v.indices.len();
        let sel = self.lsp.sel;
        match key {
            Key::Up => self.lsp.sel = if sel == 0 { n - 1 } else { sel - 1 },
            Key::Down => self.lsp.sel = (sel + 1) % n,
            Key::PageUp => self.lsp.sel = sel.saturating_sub(COMPLETION_ROWS),
            Key::PageDown => self.lsp.sel = (sel + COMPLETION_ROWS).min(n - 1),
            Key::Enter | Key::Tab => {
                self.accept_completion(sel);
                return true;
            }
            Key::Escape => {
                self.hide_completion();
                return true;
            }
            _ => return false,
        }
        self.scroll_completion_to_sel();
        true
    }

    /// Принять вариант `view_pos` видимого списка: `[start_col, каретка)`
    /// заменяется его текстом обычной правкой (отмена, `on_change`).
    pub(super) fn accept_completion(&mut self, view_pos: usize) {
        let Some(sig) = self.lsp.completion else { return };
        let Some(v) = self.completion_view() else { return };
        let Some(&idx) = v.indices.get(view_pos) else { return };
        let Some(item) = sig.with_untracked(|l| l.as_ref().and_then(|l| l.items.get(idx).cloned())) else {
            return;
        };
        self.hide_completion();
        let text = if item.insert_text.is_empty() { item.label } else { item.insert_text };
        let caret = self.primary_byte();
        let start = self.buffer.line_col_to_byte(v.line, v.start_col);
        if self.read_only || start > caret || (start == caret && text.is_empty()) {
            return;
        }
        self.cursors.primary_mut().clear_selection();
        self.undo.commit_group();
        self.apply_per_cursor(move |_| Some(super::Edit::replace(start..caret, text.clone())));
        self.undo.commit_group();
        self.ensure_cursor_visible();
    }

    /// Щелчок по списку: вариант принят. `true` — точка над списком.
    pub(super) fn completion_click(&mut self, pos: Point) -> bool {
        let Some(g) = self.lsp.popup.get() else { return false };
        if !g.rect.contains(pos) || self.completion_view().is_none() {
            return false;
        }
        let row = ((pos.y - g.rect.y() - POPUP_PAD) / g.row_h).floor();
        if row >= 0.0 {
            let i = g.top + row as usize;
            if i < g.total {
                self.accept_completion(i);
            }
        }
        true
    }

    /// Колесо над списком прокручивает его. `true` — точка над списком.
    pub(super) fn completion_wheel(&mut self, pos: Point, delta: f32) -> bool {
        let Some(g) = self.lsp.popup.get() else { return false };
        if !g.rect.contains(pos) {
            return false;
        }
        let max_top = g.total.saturating_sub(COMPLETION_ROWS);
        self.lsp.top = if delta < 0.0 { (self.lsp.top + 3).min(max_top) } else { self.lsp.top.saturating_sub(3) };
        true
    }

    fn point_in_popup(&self, pos: Point) -> bool {
        self.lsp.popup.get().is_some_and(|g| g.rect.contains(pos))
    }

    // ── наведение ───────────────────────────────────────────────────────

    /// Мышь сдвинулась (`None` — ушла): другое слово — подсказка скрыта,
    /// задержка заново. `true` — перерисовать.
    pub(super) fn hover_track(&mut self, pos: Option<Point>) -> bool {
        self.lsp.mouse_pos = pos;
        if !self.lsp.tracks_hover() {
            return false;
        }
        let hit = pos.filter(|p| !self.point_in_popup(*p)).and_then(|p| self.char_at_point(p));
        let word = hit.map(|(l, c)| {
            let (s, e) = word_range(&self.buffer.line_str(l), c);
            (l, s, e)
        });
        if word == self.lsp.hover_word {
            return false;
        }
        let repaint = self.hide_hover();
        self.lsp.hover_word = word;
        self.lsp.hover_char = hit;
        self.lsp.hover_elapsed = Duration::ZERO;
        repaint
    }

    /// Скрыть подсказку (сигнал — в `None`). `true` — что-то было видно.
    pub(super) fn hide_hover(&mut self) -> bool {
        let mut changed = std::mem::replace(&mut self.lsp.hover_fired, false);
        if let Some(s) = self.lsp.hover {
            if s.with_untracked(|h| h.is_some()) {
                s.set(None);
                changed = true;
            }
        }
        changed
    }

    /// Ввод, прокрутка: подсказка скрыта, пока мышь не сдвинется.
    pub(super) fn reset_hover(&mut self) -> bool {
        let r = self.hide_hover();
        self.lsp.hover_word = None;
        self.lsp.hover_char = None;
        r
    }

    /// Тик задержки наведения. `true` — подсказка появилась.
    pub(super) fn hover_tick(&mut self, dt: Duration) -> bool {
        if !self.lsp.hover_pending() {
            return false;
        }
        self.lsp.hover_elapsed += dt;
        if self.lsp.hover_elapsed < HOVER_DELAY {
            return false;
        }
        self.lsp.hover_fired = true;
        if let Some((l, c)) = self.lsp.hover_char {
            call_pos(&self.lsp.on_hover, l, c);
        }
        true
    }

    // ── переход к определению ───────────────────────────────────────────

    pub(super) fn goto_at(&self, line: usize, col: usize) {
        call_pos(&self.lsp.on_goto, line, col);
    }

    pub(super) fn goto_caret(&self) {
        let (l, c) = self.caret_line_col();
        self.goto_at(l, c);
    }

    /// Слово под мышью с Ctrl (`ctrl = false` — снять). `true` — перерисовать.
    pub(super) fn goto_track(&mut self, ctrl: bool) -> bool {
        let word = if ctrl && self.lsp.wants_goto() {
            self.lsp.mouse_pos.and_then(|p| self.char_at_point(p)).and_then(|(l, c)| {
                let (s, e) = word_range(&self.buffer.line_str(l), c);
                let t = self.buffer.line_str(l);
                let ch = t.chars().nth(c)?;
                is_ident_char(ch).then_some((l, s, e))
            })
        } else {
            None
        };
        let changed = word != self.lsp.goto_word;
        self.lsp.goto_word = word;
        changed
    }

    pub(super) fn goto_hovering(&self) -> bool {
        self.lsp.goto_word.is_some()
    }

    /// `EditorCommand::SetCursor`: каретка на `line:col`, без выделения, строка
    /// на экране (вне его — по центру).
    pub(super) fn set_cursor_to(&mut self, line: usize, col: usize) {
        let line = line.min(self.buffer.len_lines().saturating_sub(1));
        let pos = self.buffer.line_col_to_byte(line, col);
        self.cursors = super::Cursors::single(super::Cursor::new(pos));
        self.undo.commit_group();
        if self.bounds.size.height <= 0.0 {
            self.pending_reveal = Some(line);
        } else {
            let (_, c) = self.buffer.byte_to_line_col(pos);
            let row = self.logical_to_visual_line(line, c);
            let visible = self.visible_lines_count().max(1);
            if row < self.scroll_offset_lines || row >= self.scroll_offset_lines + visible {
                let max_scroll = self.total_visual_lines.saturating_sub(visible);
                self.scroll_offset_lines = row.saturating_sub(visible / 2).min(max_scroll);
                self.scrollbar_fader.flash();
            }
            self.ensure_cursor_visible_x(4.0);
        }
        self.fire_on_cursor();
    }

    // ── рисование ───────────────────────────────────────────────────────

    /// Диагностики строк `first..last` для кадра.
    pub(super) fn visible_diagnostics(&self, first: usize, last: usize) -> Vec<usize> {
        if self.lsp.diagnostics.is_empty() {
            return Vec::new();
        }
        self.lsp.visible(first, last)
    }

    /// Метка в gutter'е: самая серьёзная диагностика, начинающаяся на строке.
    pub(super) fn paint_diag_gutter(&self, list: &mut DisplayList, visible: &[usize], logical: usize, y: f32) {
        let worst = visible
            .iter()
            .map(|&i| &self.lsp.diagnostics[i])
            .filter(|d| d.line == logical)
            .min_by_key(|d| d.severity.rank());
        let Some(d) = worst else { return };
        let color = self.theme().diagnostic(&d.severity);
        let lh = self.line_height();
        if self.show_line_numbers {
            popup::paint_gutter_marker(list, self.bounds.x(), y, lh, color);
        } else {
            list.push_rect(Rect::new(Point::new(self.bounds.x(), y), Size::new(2.0, lh)), color, [0.0; 4]);
        }
    }

    /// Подчёркивания одного видимого куска строки: диагностики и слово
    /// под мышью с Ctrl.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_row_underlines(
        &self,
        list: &mut DisplayList,
        visible: &[usize],
        logical: usize,
        line_text: &str,
        segment: &str,
        seg_start_char: usize,
        seg_end_char: usize,
        is_last_seg: bool,
        origin: Point,
    ) {
        let lh = self.line_height();
        let base_y = origin.y + lh - 3.0;
        let clip = |s: usize, e: usize| -> Option<(usize, usize)> {
            let s2 = s.max(seg_start_char);
            let e2 = if is_last_seg { e } else { e.min(seg_end_char) };
            (e2 > s2).then_some((s2 - seg_start_char, e2 - seg_start_char))
        };
        if !visible.is_empty() {
            let theme = self.theme();
            // серьёзные — поверх
            let mut ds: Vec<&Diagnostic> = visible.iter().map(|&i| &self.lsp.diagnostics[i]).collect();
            ds.sort_by_key(|d| std::cmp::Reverse(d.severity.rank()));
            for d in ds {
                let Some((s, e)) = diag_cols_on_line(d, logical, line_text) else { continue };
                let Some((ls, le)) = clip(s, e) else { continue };
                let x0 = origin.x + self.seg_char_x(segment, ls);
                let x1 = origin.x + self.seg_char_x(segment, le);
                popup::paint_wavy_underline(list, x0, x1, base_y, theme.diagnostic(&d.severity));
            }
        }
        if let Some((l, s, e)) = self.lsp.goto_word {
            if l == logical {
                if let Some((ls, le)) = clip(s, e) {
                    let x0 = origin.x + self.seg_char_x(segment, ls);
                    let x1 = origin.x + self.seg_char_x(segment, le);
                    popup::paint_straight_underline(list, x0, x1, origin.y + lh - 2.0, self.theme().cursor(&self.mss));
                }
            }
        }
    }

    /// Список дополнения и подсказка — поверх всего (слой оверлея).
    pub(super) fn paint_lsp_overlays(&self, list: &mut DisplayList) {
        self.paint_hover_tip(list);
        self.paint_completion(list);
    }

    fn paint_completion(&self, list: &mut DisplayList) {
        let Some(sig) = self.lsp.completion else { return };
        let Some(v) = self.completion_view() else {
            self.lsp.popup.set(None);
            return;
        };
        let Some(anchor) = self.char_screen_pos(v.line, v.start_col) else {
            self.lsp.popup.set(None);
            return;
        };
        let theme = self.theme();
        let fs = self.font_size();
        let tag_fs = (fs * 0.8).round();
        let family = self.font_family_str();
        let lh = self.line_height();
        let row_h = lh;
        let n = v.indices.len();
        let rows = n.min(COMPLETION_ROWS);
        let top = self.lsp.top.min(n - rows);
        let sel = self.lsp.sel.min(n - 1);

        let items: Vec<(String, String, String)> = sig.with_untracked(|l| {
            let l = l.as_ref();
            v.indices
                .iter()
                .map(|&i| {
                    l.and_then(|l| l.items.get(i))
                        .map(|it| {
                            let detail: String = it.detail.lines().next().unwrap_or("").chars().take(60).collect();
                            (it.label.clone(), it.kind.clone(), detail)
                        })
                        .unwrap_or_default()
                })
                .collect()
        });
        // ширина — по первым вариантам, чтобы окно не прыгало при прокрутке
        let probe = &items[..items.len().min(50)];
        let kind_w = probe
            .iter()
            .map(|(_, k, _)| if k.is_empty() { 0.0 } else { self.text_w(k, tag_fs) + 10.0 })
            .fold(0.0f32, f32::max);
        let label_w = probe.iter().map(|(l, _, _)| self.text_w(l, fs)).fold(0.0f32, f32::max);
        let detail_w = probe.iter().map(|(_, _, d)| self.text_w(d, fs)).fold(0.0f32, f32::max);
        let kind_col = if kind_w > 0.0 { kind_w + 6.0 } else { 0.0 };
        let gap = if detail_w > 0.0 { 20.0 } else { 0.0 };
        let max_w = (self.bounds.size.width - 8.0).max(120.0).min(640.0);
        let w = (POPUP_PAD * 2.0 + 8.0 + kind_col + label_w + gap + detail_w + 8.0).clamp(180.0, max_w);
        let h = rows as f32 * row_h + POPUP_PAD * 2.0;

        let bottom = self.bounds.y() + self.bounds.size.height;
        let below = anchor.y + lh + 2.0;
        let y = if below + h > bottom && anchor.y - h - 2.0 >= self.bounds.y() {
            anchor.y - h - 2.0
        } else {
            below
        };
        let right = self.bounds.x() + self.bounds.size.width - 4.0;
        let x = (anchor.x - POPUP_PAD - 8.0 - kind_col).min(right - w).max(self.bounds.x() + 2.0);
        let rect = Rect::new(Point::new(x, y), Size::new(w, h));
        self.lsp.popup.set(Some(PopupGeom { rect, row_h, top, total: n }));

        list.begin_overlay();
        popup::paint_panel(list, rect, &theme, &self.mss);
        list.push_clip(rect);
        let fg = theme.fg(&self.mss);
        let dim = theme.gutter_fg(&self.mss);
        for (r, (label, kind, detail)) in items.iter().enumerate().skip(top).take(rows) {
            let ry = rect.y() + POPUP_PAD + (r - top) as f32 * row_h;
            if r == sel {
                list.push_rect(
                    Rect::new(Point::new(rect.x() + 3.0, ry), Size::new(w - 6.0, row_h)),
                    theme.selection(&self.mss),
                    [4.0; 4],
                );
            }
            let mut tx = rect.x() + POPUP_PAD + 8.0;
            if kind_w > 0.0 {
                if !kind.is_empty() {
                    let color = kind_class(kind).map(|c| theme.token(c, &self.mss)).unwrap_or(dim);
                    let kw = self.text_w(kind, tag_fs) + 10.0;
                    let tag = Rect::new(Point::new(tx, ry + 3.0), Size::new(kw, row_h - 6.0));
                    list.push_rect(tag, color.with_alpha(0.16), [3.0; 4]);
                    popup::push_text(
                        list,
                        kind,
                        Rect::new(Point::new(tx + 5.0, ry), Size::new(kw, row_h)),
                        color,
                        tag_fs,
                        family,
                    );
                }
                tx += kind_col;
            }
            popup::push_text(list, label, Rect::new(Point::new(tx, ry), Size::new(label_w + 4.0, row_h)), fg, fs, family);
            if !detail.is_empty() {
                let dx = tx + label_w + gap;
                let dw = (rect.x() + w - POPUP_PAD - dx).max(0.0);
                popup::push_text(list, detail, Rect::new(Point::new(dx, ry), Size::new(dw, row_h)), dim, fs, family);
            }
        }
        popup::paint_list_thumb(list, rect, top, rows, n, dim.with_alpha(0.5));
        list.pop_clip();
        list.end_overlay();
    }

    /// Строки подсказки: диагностики под мышью (с цветом важности), затем
    /// справка сервера.
    fn hover_lines(&self) -> (Option<(usize, usize)>, Vec<(String, Option<Color>)>) {
        let mut lines: Vec<(String, Option<Color>)> = Vec::new();
        let mut anchor = None;
        if self.lsp.hover_fired {
            if let Some((l, c)) = self.lsp.hover_char {
                let text = self.buffer.line_str(l);
                let theme = self.theme();
                let mut ds: Vec<&Diagnostic> = self
                    .visible_diagnostics(l, l + 1)
                    .into_iter()
                    .map(|i| &self.lsp.diagnostics[i])
                    .filter(|d| diag_cols_on_line(d, l, &text).is_some_and(|(s, e)| s <= c && c < e))
                    .collect();
                ds.sort_by_key(|d| d.severity.rank());
                for d in ds {
                    for (i, w) in wrap_text(&d.message, TIP_MAX_CHARS, TIP_MAX_LINES).into_iter().enumerate() {
                        lines.push((w, (i == 0).then(|| theme.diagnostic(&d.severity))));
                    }
                }
                anchor = Some((l, c));
            }
        }
        if let Some(info) = self.lsp.hover.and_then(|s| s.get_untracked()) {
            let wrapped = wrap_text(&info.text, TIP_MAX_CHARS, TIP_MAX_LINES * 2);
            if !wrapped.is_empty() {
                if !lines.is_empty() {
                    lines.push((String::new(), None));
                }
                lines.extend(wrapped.into_iter().map(|w| (w, None)));
                anchor = anchor.or(Some((info.line, info.col)));
            }
        }
        lines.truncate(TIP_MAX_LINES + 4);
        (anchor, lines)
    }

    fn paint_hover_tip(&self, list: &mut DisplayList) {
        let (anchor, lines) = self.hover_lines();
        let Some((l, c)) = anchor else { return };
        if lines.is_empty() {
            return;
        }
        let Some(at) = self.char_screen_pos(l, c) else { return };
        let theme = self.theme();
        let fs = self.font_size();
        let family = self.font_family_str();
        let lh = (fs * 1.4).round();
        let marker = if lines.iter().any(|(_, c)| c.is_some()) { 10.0 } else { 0.0 };
        let pad = 8.0;
        let text_w = lines.iter().map(|(t, _)| self.text_w(t, fs)).fold(0.0f32, f32::max);
        let max_w = (self.bounds.size.width - 8.0).max(120.0);
        let w = (text_w + marker + pad * 2.0).min(max_w);
        let h = lines.len() as f32 * lh + pad * 2.0 - 2.0;
        let max_h = (self.bounds.size.height - 8.0).max(lh + pad * 2.0);
        let h = h.min(max_h);
        let editor_lh = self.line_height();
        let y = if at.y - h - 4.0 >= self.bounds.y() {
            at.y - h - 4.0
        } else {
            at.y + editor_lh + 4.0
        };
        let right = self.bounds.x() + self.bounds.size.width - 4.0;
        let x = at.x.min(right - w).max(self.bounds.x() + 2.0);
        let rect = Rect::new(Point::new(x, y), Size::new(w, h));

        list.begin_overlay();
        popup::paint_panel(list, rect, &theme, &self.mss);
        list.push_clip(rect);
        let fg = theme.fg(&self.mss);
        for (i, (text, color)) in lines.iter().enumerate() {
            let ly = rect.y() + pad - 1.0 + i as f32 * lh;
            if ly > rect.y() + h {
                break;
            }
            if let Some(c) = color {
                list.push_rect(
                    Rect::new(Point::new(rect.x() + pad, ly + 4.0), Size::new(3.0, lh - 8.0)),
                    *c,
                    [1.5; 4],
                );
            }
            popup::push_text(
                list,
                text,
                Rect::new(Point::new(rect.x() + pad + marker, ly), Size::new(w - pad * 2.0 - marker, lh)),
                fg,
                fs,
                family,
            );
        }
        list.pop_clip();
        list.end_overlay();
    }
}
