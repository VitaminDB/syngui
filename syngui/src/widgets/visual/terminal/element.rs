use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use web_time::Instant;

use crate::core::{Color, Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, MssFields};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt, TextMeasure, UpdateContext};
use crate::widget::{
    DirtyFlags, Element, ElementId, ElementTree, StyledElement, Widget,
};

use super::clipboard_filter;
use super::config::TerminalConfig;
use super::grid::{Cell, CellFlags};
use super::input as kb;
use super::mouse::{self, MouseAction, MouseMode};
use super::palette;
use super::selection::{GridPos, SelectionMode};
use super::session::{SessionState, TerminalSession};
use super::Terminal;

const CURSOR_BLINK_PERIOD: f32 = 1.0;
const FALLBACK_BG: Color = Color::new(0.067, 0.067, 0.078, 1.0);
const FALLBACK_FG: Color = Color::new(0.9, 0.91, 0.93, 1.0);
const MIN_CELL_WIDTH: f32 = 4.0;
const MULTI_CLICK_WINDOW: Duration = Duration::from_millis(400);
const FALLBACK_LINK_HOVER: Color = Color::new(0.231, 0.510, 0.965, 1.0);

pub struct TerminalElement {
    id: ElementId,
    config: TerminalConfig,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,

    session: Option<TerminalSession>,

    cols: u16,
    rows: u16,
    cell_w: f32,
    cell_h: f32,

    text_measure: Option<Arc<dyn TextMeasure>>,
    cursor_phase: f32,
    cursor_blink_visible: bool,

    rendered_revision: u64,

    left_button_held: bool,
    mouse_down_link: Option<u32>,
    hovered_link: Option<u32>,
    last_click: Option<(Instant, GridPos, u8)>,
    auto_scroll: i32,

    pub(super) scrollbar_fader: crate::widgets::scroll::ScrollbarFader,
    pub(super) scrollbar_interaction: crate::widgets::scroll::ScrollbarInteraction,
    pub(super) command_signal: Option<crate::signal::RwSignal<Option<super::TerminalCommand>>>,

    pub(super) autofocus: bool,
    pub(super) focus_request_pending: bool,
}

impl TerminalElement {
    pub(super) fn new(config: TerminalConfig, session: Option<TerminalSession>) -> Self {
        Self {
            id: ElementId::new(),
            config,
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            session,
            cols: 80,
            rows: 24,
            cell_w: 8.0,
            cell_h: 16.0,
            text_measure: None,
            cursor_phase: 0.0,
            cursor_blink_visible: true,
            rendered_revision: 0,
            left_button_held: false,
            mouse_down_link: None,
            hovered_link: None,
            last_click: None,
            auto_scroll: 0,
            scrollbar_fader: crate::widgets::scroll::ScrollbarFader::default(),
            scrollbar_interaction: crate::widgets::scroll::ScrollbarInteraction::default(),
            command_signal: None,
            autofocus: false,
            focus_request_pending: false,
        }
    }

    fn refresh_cell_metrics(&mut self) {
        let font_size = self.config.font_size;
        let height = (font_size * self.config.line_height).max(1.0).round();
        let width = if let Some(tm) = self.text_measure.as_ref() {
            tm.measure_text_width_styled(
                "M",
                font_size,
                1,
                false,
                Some(self.config.font_family.as_str()),
            )
        } else {
            (font_size * 0.6).max(MIN_CELL_WIDTH)
        };
        self.cell_w = width.max(MIN_CELL_WIDTH).round();
        self.cell_h = height;
    }

    fn ensure_session(&mut self) {
        if self.session.is_some() {
            return;
        }
        match TerminalSession::new(self.config.clone()) {
            Ok(s) => {
                if self.cols >= 1 && self.rows >= 1 {
                    s.resize(self.cols, self.rows);
                }
                self.session = Some(s);
            }
            Err(e) => {
                log::error!("[syngui terminal] open session failed: {e}");
            }
        }
    }

    fn padding_lrtb(&self) -> (f32, f32, f32, f32) {
        (
            self.mss.padding_left.unwrap_or(0.0),
            self.mss.padding_right.unwrap_or(0.0),
            self.mss.padding_top.unwrap_or(0.0),
            self.mss.padding_bottom.unwrap_or(0.0),
        )
    }

    fn inner_origin(&self) -> Point {
        let (l, _, t, _) = self.padding_lrtb();
        Point::new(self.bounds.origin.x + l, self.bounds.origin.y + t)
    }

    fn recompute_size(&mut self) {
        let (pl, pr, pt, pb) = self.padding_lrtb();
        let inner_w = (self.bounds.size.width - pl - pr).max(0.0);
        let inner_h = (self.bounds.size.height - pt - pb).max(0.0);
        let cols = ((inner_w / self.cell_w).floor() as u16).max(1);
        let rows = ((inner_h / self.cell_h).floor() as u16).max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        if let Some(s) = self.session.as_ref() {
            s.resize(cols, rows);
        }
    }

    fn default_colors(&self) -> (Color, Color) {
        let fg = self.mss.color.unwrap_or(FALLBACK_FG);
        let bg = self.mss.background_color.unwrap_or(FALLBACK_BG);
        (fg, bg)
    }

    fn link_hover_color(&self) -> Color {
        self.mss.accent_color.unwrap_or(FALLBACK_LINK_HOVER)
    }

    fn point_to_local(&self, p: Point) -> Option<(u16, u16)> {
        if !self.bounds.contains(p) {
            return None;
        }
        if self.cell_w <= 0.0 || self.cell_h <= 0.0 {
            return None;
        }
        let inner = self.inner_origin();
        let dx = p.x - inner.x;
        let dy = p.y - inner.y;
        let col = ((dx / self.cell_w).floor() as i32).clamp(0, self.cols as i32 - 1) as u16;
        let row = ((dy / self.cell_h).floor() as i32).clamp(0, self.rows as i32 - 1) as u16;
        Some((col + 1, row + 1))
    }

    fn point_to_grid(&self, s: &SessionState, p: Point) -> Option<GridPos> {
        if self.cell_w <= 0.0 || self.cell_h <= 0.0 {
            return None;
        }
        let inner = self.inner_origin();
        let dx = p.x - inner.x;
        let dy = p.y - inner.y;
        let row_in_view = (dy / self.cell_h).floor() as i32;
        let cols = self.cols as i32;
        let rows = self.rows as i32;
        let col = (dx / self.cell_w).floor() as i32;
        let col = col.clamp(0, cols.saturating_sub(1)) as u16;
        let row_in_view = row_in_view.clamp(0, rows.saturating_sub(1)) as i32;

        let total = if s.grid.on_alt() {
            s.grid.rows() as i32
        } else {
            s.grid.total_lines() as i32
        };
        let view = rows;
        let off = s.scroll_offset as i32;
        let top_line = (total - view - off).max(0);
        let line = top_line + row_in_view;
        Some(GridPos::new(line, col))
    }

    fn link_at(s: &SessionState, pos: GridPos) -> Option<u32> {
        s.grid.cell_at_global(pos.line, pos.col).and_then(|c| c.link_id)
    }

    fn update_click_count(&mut self, pos: GridPos) -> u8 {
        let now = Instant::now();
        let count = match self.last_click {
            Some((prev_t, prev_pos, prev_n))
                if now.duration_since(prev_t) <= MULTI_CLICK_WINDOW
                    && (prev_pos.line - pos.line).abs() <= 1
                    && (prev_pos.col as i32 - pos.col as i32).abs() <= 1
                    && prev_n < 3 =>
            {
                prev_n + 1
            }
            _ => 1,
        };
        self.last_click = Some((now, pos, count));
        count
    }

    pub(super) fn scrollbar_geom_now(&self) -> Option<crate::widgets::scroll::ScrollbarGeom> {
        let s = self.session.as_ref()?;
        s.with_state_ref(|st| {
            if st.grid.on_alt() {
                return None;
            }
            let scrollback = st.grid.scrollback_len();
            if scrollback == 0 {
                return None;
            }
            let total = scrollback + st.grid.rows();
            let content_h = total as f32 * self.cell_h;
            let scroll_y = (scrollback - st.scroll_offset.min(scrollback)) as f32 * self.cell_h;
            Some(crate::widgets::scroll::ScrollbarGeom {
                viewport: self.bounds,
                content_w: 0.0,
                content_h,
                scroll_x: 0.0,
                scroll_y,
            })
        })
    }

    pub(super) fn scrollbar_style_now(&self) -> crate::widgets::scroll::ScrollbarStyle {
        let (default_fg, _) = self.default_colors();
        let fg = self.mss.color.unwrap_or(default_fg);
        self.mss.scrollbar_style(fg)
    }

    pub(super) fn apply_scrollbar_drag_y(&self, new_y_px: f32) -> bool {
        let cell_h = self.cell_h.max(1.0);
        if let Some(s) = self.session.as_ref() {
            let mut changed = false;
            s.with_state(|st| {
                let scrollback = st.grid.scrollback_len();
                let lines_from_top = (new_y_px / cell_h).round() as i64;
                let lines_from_top = lines_from_top.clamp(0, scrollback as i64) as usize;
                let new_off = scrollback.saturating_sub(lines_from_top).min(scrollback);
                if new_off != st.scroll_offset {
                    st.scroll_offset = new_off;
                    changed = true;
                }
            });
            changed
        } else {
            false
        }
    }
}

impl Element for TerminalElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(t) = widget.as_any().downcast_ref::<Terminal>() {
            self.config.font_size = t.config.font_size;
            self.config.font_size_explicit = t.config.font_size_explicit;
            self.config.font_family = t.config.font_family.clone();
            self.config.font_family_explicit = t.config.font_family_explicit;
            self.config.line_height = t.config.line_height;

            self.command_signal = t.command_signal;

            if let Some(new_session) = t.session.clone() {
                self.session = Some(new_session);
                self.rendered_revision = 0;
                if self.cols >= 1 && self.rows >= 1 {
                    if let Some(s) = self.session.as_ref() {
                        s.resize(self.cols, self.rows);
                    }
                }
                if t.autofocus {
                    if let Some(s) = self.session.as_ref() {
                        if s.try_consume_autofocus() {
                            self.focus_request_pending = true;
                            s.with_state(|st| st.focused = true);
                        }
                    }
                }
            }
            self.autofocus = t.autofocus;

            self.refresh_cell_metrics();
            self.recompute_size();
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let w = self
            .mss
            .width
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(constraints.max_width)
            .min(constraints.max_width);
        let h = self
            .mss
            .height
            .map(|d| d.resolve(constraints.max_height))
            .unwrap_or(constraints.max_height)
            .min(constraints.max_height);
        let w = if w.is_finite() { w.max(0.0) } else { 600.0 };
        let h = if h.is_finite() { h.max(0.0) } else { 320.0 };
        self.bounds = Rect::new(self.bounds.origin, Size::new(w, h));
        self.refresh_cell_metrics();
        self.recompute_size();
        Size::new(w, h)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let (default_fg, default_bg) = self.default_colors();

        let radius = self
            .mss
            .border_radius
            .map(|d| [
                d[0].resolve(self.bounds.size.width),
                d[1].resolve(self.bounds.size.width),
                d[2].resolve(self.bounds.size.width),
                d[3].resolve(self.bounds.size.width),
            ])
            .unwrap_or([0.0; 4]);
        let border = self
            .mss
            .border_color
            .map(|bc| Border::new(self.mss.border_width.unwrap_or(0.0), bc))
            .unwrap_or_else(|| Border::new(0.0, Color::TRANSPARENT));
        list.push_rect_bordered(
            self.bounds,
            self.mss.background_color.unwrap_or(default_bg),
            radius,
            border,
        );

        list.push_clip(self.bounds);

        let Some(session) = self.session.as_ref() else {
            list.pop_clip();
            return;
        };

        session.with_state_ref(|s| {
            let view_rows = self.rows as usize;
            let scrollback_len = if s.grid.on_alt() {
                0
            } else {
                s.grid.scrollback_len()
            };
            let total_lines = scrollback_len + s.grid.rows();
            let top_line = total_lines.saturating_sub(view_rows + s.scroll_offset);

            let origin = self.inner_origin();
            let cell_w = self.cell_w;
            let cell_h = self.cell_h;

            for row_in_view in 0..view_rows {
                let line_idx = top_line + row_in_view;
                if line_idx >= total_lines {
                    break;
                }
                let cells = if line_idx < scrollback_len {
                    s.grid.scrollback_line(line_idx)
                } else {
                    s.grid.line(line_idx - scrollback_len)
                };
                self.draw_line(
                    list, cells, origin, row_in_view, cell_w, cell_h, default_fg, default_bg,
                );
            }

            let has_real_selection = s
                .selection
                .range()
                .map(|(a, b)| a != b)
                .unwrap_or(false);
            if has_real_selection {
                let accent = self.mss.accent_color.unwrap_or(default_fg);
                let overlay_color = accent.with_alpha(0.3);
                for row_in_view in 0..view_rows {
                    let global = (top_line + row_in_view) as i32;
                    let row_cells = if (global as usize) < scrollback_len {
                        s.grid.scrollback_line(global as usize)
                    } else if (global as usize) < total_lines {
                        s.grid.line(global as usize - scrollback_len)
                    } else {
                        continue;
                    };
                    if let Some((c0, c1)) = s.selection.cells_in_row(global, row_cells.len()) {
                        let x = origin.x + c0 as f32 * cell_w;
                        let y = origin.y + row_in_view as f32 * cell_h;
                        let w = (c1 - c0 + 1) as f32 * cell_w;
                        let rect = Rect::new(Point::new(x, y), Size::new(w, cell_h));
                        list.push_rect(rect, overlay_color, [0.0; 4]);
                    }
                }
            }

            if s.scroll_offset == 0
                && s.focused
                && self.cursor_blink_visible
                && s.grid.cursor_visible()
            {
                let cursor = s.grid.cursor();
                let x = origin.x + cursor.col as f32 * cell_w;
                let y = origin.y + cursor.row as f32 * cell_h;
                let cursor_rect = Rect::new(Point::new(x, y), Size::new(cell_w, cell_h));
                let cursor_color = self.mss.accent_color.unwrap_or(default_fg);
                list.push_rect(cursor_rect, cursor_color.with_alpha(0.45), [0.0; 4]);
            }

            let scrollback = if s.grid.on_alt() { 0 } else { s.grid.scrollback_len() };
            if scrollback > 0 {
                let total = scrollback + s.grid.rows();
                let content_h = total as f32 * cell_h;
                let scroll_y = (scrollback - s.scroll_offset.min(scrollback)) as f32 * cell_h;
                let fg = self.mss.color.unwrap_or(default_fg);
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
            }
        });

        list.pop_clip();
    }

    /// Терминал живёт кадрами: в `animate` он подхватывает новую ревизию
    /// сессии (вывод PTY), мигает курсором, гасит скроллбар и выполняет
    /// команды из `command_signal` (Copy/Paste/Clear контекстного меню).
    /// Без этой заявки точечный реестр анимаций элемент не обходит.
    fn wants_animate_tick(&self) -> bool {
        true
    }

    fn animate(&mut self, dt: Duration) -> bool {
        if self.session.is_none() && self.cols > 0 && self.rows > 0 {
            self.ensure_session();
        }

        let mut dirty = false;

        if let Some(s) = self.session.as_ref() {
            let rev = s.revision();
            if rev != self.rendered_revision {
                self.rendered_revision = rev;
                self.mark_dirty(DirtyFlags::RENDER);
                dirty = true;
            }
        }

        if let Some(s) = self.session.as_ref() {
            s.with_state(|st| {
                let on_alt_now = st.grid.on_alt();
                if on_alt_now != st.last_on_alt {
                    st.last_on_alt = on_alt_now;
                    st.scroll_offset = 0;
                    if st.selection.is_active() {
                        st.selection.clear();
                    }
                }
            });
        }

        let prev = self.cursor_blink_visible;
        self.cursor_phase += dt.as_secs_f32();
        if self.cursor_phase >= CURSOR_BLINK_PERIOD {
            self.cursor_phase -= CURSOR_BLINK_PERIOD;
        }
        self.cursor_blink_visible = self.cursor_phase < CURSOR_BLINK_PERIOD * 0.5;
        if prev != self.cursor_blink_visible {
            let need_blink_repaint = self
                .session
                .as_ref()
                .map(|s| {
                    s.with_state_ref(|st| {
                        st.focused && st.scroll_offset == 0 && st.grid.cursor_visible()
                    })
                })
                .unwrap_or(false);
            if need_blink_repaint {
                self.mark_dirty(DirtyFlags::RENDER);
                dirty = true;
            }
        }

        if self.auto_scroll != 0 {
            if let Some(s) = self.session.as_ref() {
                s.with_state(|st| {
                    if st.selection.mouse_selecting {
                        let max = st.grid.scrollback_len() as i32;
                        let new = (st.scroll_offset as i32 + self.auto_scroll).clamp(0, max)
                            as usize;
                        if new != st.scroll_offset {
                            st.scroll_offset = new;
                        }
                    }
                });
                self.mark_dirty(DirtyFlags::RENDER);
                dirty = true;
            }
        }

        let style = self.mss.scrollbar_style(self.mss.color.unwrap_or(Color::from_hex("#9CA3AF")));
        if self.scrollbar_fader.tick(dt.as_secs_f32(), &style) {
            self.mark_dirty(DirtyFlags::RENDER);
            dirty = true;
        }

        if let Some(sig) = self.command_signal {
            if let Some(cmd) = sig.get_untracked() {
                self.process_command(cmd);
                sig.set(None);
                self.mark_dirty(DirtyFlags::RENDER);
                dirty = true;
            }
        }

        let _ = dirty;
        let alive = self
            .session
            .as_ref()
            .map(|s| s.is_alive())
            .unwrap_or(false);
        let focused = self
            .session
            .as_ref()
            .map(|s| s.with_state_ref(|st| st.focused))
            .unwrap_or(false);
        alive || focused || self.scrollbar_fader.opacity > 0.0
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        match event {
            Event::KeyDown(key) => {
                let focused = self
                    .session
                    .as_ref()
                    .map(|s| s.with_state_ref(|st| st.focused))
                    .unwrap_or(false);
                if !focused {
                    return EventResult::Ignored;
                }
                let mods = ctx.modifiers;
                use crate::input::Key;
                if mods.ctrl && mods.alt && matches!(key, Key::C) {
                    if let Some(text) = self.collect_selection() {
                        if !text.is_empty() {
                            ctx.copy_to_clipboard(&text);
                            return EventResult::Handled;
                        }
                    }
                }
                if mods.ctrl && mods.alt && matches!(key, Key::V) {
                    self.do_paste(ctx);
                    self.reset_scroll_offset_if_any(ctx);
                    return EventResult::Handled;
                }
                if let Some(bytes) = kb::map_key(*key, mods) {
                    self.clear_selection_if_any(ctx);
                    self.write_pty(&bytes);
                    self.reset_scroll_offset_if_any(ctx);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::CharInput(c) => {
                let focused = self
                    .session
                    .as_ref()
                    .map(|s| s.with_state_ref(|st| st.focused))
                    .unwrap_or(false);
                if !focused {
                    return EventResult::Ignored;
                }
                let mods = ctx.modifiers;
                if mods.ctrl && c.is_ascii_alphabetic() {
                    return EventResult::Handled;
                }
                self.clear_selection_if_any(ctx);
                let bytes = kb::map_char(*c, mods);
                self.write_pty(&bytes);
                self.reset_scroll_offset_if_any(ctx);
                EventResult::Handled
            }

            Event::MouseDown { button, position }
                if *button == MouseButton::Left && self.bounds.contains(*position) =>
            {
                if let Some(geom) = self.scrollbar_geom_now() {
                    let style = self.scrollbar_style_now();
                    if self.scrollbar_interaction.try_begin_drag(
                        &mut self.scrollbar_fader, &geom, &style, *position,
                    ) {
                        ctx.request_paint();
                        return EventResult::Captured;
                    }
                }

                if let Some(s) = self.session.as_ref() {
                    s.with_state(|st| st.focused = true);
                }
                self.left_button_held = true;
                ctx.request_paint();

                if self.try_forward_mouse(event, ctx) {
                    return EventResult::Handled;
                }

                let pos_grid = self
                    .session
                    .as_ref()
                    .and_then(|s| s.with_state_ref(|st| self.point_to_grid(st, *position)))
                    .unwrap_or_default();
                let link_under = self
                    .session
                    .as_ref()
                    .and_then(|s| s.with_state_ref(|st| Self::link_at(st, pos_grid)));
                self.mouse_down_link = link_under;

                let count = self.update_click_count(pos_grid);
                let mode = if ctx.modifiers.alt {
                    SelectionMode::Block
                } else {
                    match count {
                        2 => SelectionMode::Word,
                        3 => SelectionMode::Line,
                        _ => SelectionMode::Simple,
                    }
                };
                if let Some(s) = self.session.as_ref() {
                    s.with_state(|st| {
                        st.selection.start(pos_grid, mode);
                        st.selection.mouse_selecting = true;
                        match mode {
                            SelectionMode::Word => st.selection.extend_word(&st.grid),
                            SelectionMode::Line => st.selection.extend_line(&st.grid),
                            _ => {}
                        }
                    });
                }
                ctx.set_cursor(if self.mouse_down_link.is_some() {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Text
                });
                EventResult::Handled
            }

            Event::MouseUp { button, position } if *button == MouseButton::Left => {
                if self.scrollbar_interaction.end_drag(&mut self.scrollbar_fader) {
                    self.left_button_held = false;
                    self.auto_scroll = 0;
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                self.left_button_held = false;
                self.auto_scroll = 0;

                if self.try_forward_mouse(event, ctx) {
                    if let Some(s) = self.session.as_ref() {
                        s.with_state(|st| st.selection.mouse_selecting = false);
                    }
                    return EventResult::Handled;
                }

                let (pos, was_dragged, up_link, link_uri) = if let Some(s) = self.session.as_ref()
                {
                    s.with_state_ref(|st| {
                        let pos = self.point_to_grid(st, *position).unwrap_or_default();
                        let up_link = Self::link_at(st, pos);
                        let was_dragged = st
                            .selection
                            .range()
                            .map(|(a, b)| a != b)
                            .unwrap_or(false);
                        let uri = up_link.and_then(|id| st.grid.link(id).map(str::to_owned));
                        (pos, was_dragged, up_link, uri)
                    })
                } else {
                    (GridPos::default(), false, None, None)
                };
                let _ = pos;

                if let (Some(down), Some(up)) = (self.mouse_down_link, up_link) {
                    if down == up && !was_dragged {
                        if let Some(uri) = link_uri {
                            if let Err(e) = crate::open_url(&uri) {
                                log::warn!(
                                    "[syngui terminal] open link `{uri}` failed: {e}"
                                );
                            }
                        }
                        if let Some(s) = self.session.as_ref() {
                            s.with_state(|st| st.selection.clear());
                        }
                        ctx.request_paint();
                    }
                }
                self.mouse_down_link = None;
                if let Some(s) = self.session.as_ref() {
                    s.with_state(|st| st.selection.mouse_selecting = false);
                }
                EventResult::Handled
            }

            Event::MouseMove(pos) => {
                if self.scrollbar_interaction.dragging() {
                    if let Some(geom) = self.scrollbar_geom_now() {
                        let style = self.scrollbar_style_now();
                        if let Some((new_y, _)) = self.scrollbar_interaction.update_drag(
                            &mut self.scrollbar_fader, &geom, &style, *pos,
                        ) {
                            self.apply_scrollbar_drag_y(new_y);
                            ctx.request_paint();
                            return EventResult::Captured;
                        }
                    }
                }

                if self.try_forward_mouse(event, ctx) {
                    return EventResult::Handled;
                }

                if let Some(geom) = self.scrollbar_geom_now() {
                    let style = self.scrollbar_style_now();
                    if self.bounds.contains(*pos) {
                        if self.scrollbar_interaction.update_hover(
                            &mut self.scrollbar_fader, &geom, &style, *pos,
                            crate::widgets::scroll::SCROLLBAR_HIT_MARGIN,
                        ) {
                            ctx.request_paint();
                        }
                    } else if self.scrollbar_interaction.clear_hover(&mut self.scrollbar_fader) {
                        ctx.request_paint();
                    }
                }

                let mouse_selecting = self
                    .session
                    .as_ref()
                    .map(|s| s.with_state_ref(|st| st.selection.mouse_selecting))
                    .unwrap_or(false);
                if mouse_selecting {
                    if let Some(s) = self.session.as_ref() {
                        s.with_state(|st| {
                            if let Some(grid_pos) = self.point_to_grid(st, *pos) {
                                st.selection.update_cursor(grid_pos);
                            }
                        });
                        ctx.request_paint();
                    }
                    self.auto_scroll = if pos.y < self.bounds.origin.y {
                        1
                    } else if pos.y > self.bounds.origin.y + self.bounds.size.height {
                        -1
                    } else {
                        0
                    };
                    ctx.set_cursor(CursorIcon::Text);
                    return EventResult::Handled;
                }

                if !self.bounds.contains(*pos) {
                    if self.hovered_link.is_some() {
                        self.hovered_link = None;
                        ctx.request_paint();
                    }
                    return EventResult::Ignored;
                }

                let new_link = self
                    .session
                    .as_ref()
                    .and_then(|s| {
                        s.with_state_ref(|st| {
                            let grid_pos = self.point_to_grid(st, *pos)?;
                            Some(Self::link_at(st, grid_pos))
                        })
                    })
                    .flatten();
                if new_link != self.hovered_link {
                    self.hovered_link = new_link;
                    ctx.request_paint();
                }
                ctx.set_cursor(if new_link.is_some() {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Text
                });
                EventResult::Ignored
            }

            Event::MouseWheel { delta, position, .. } if self.bounds.contains(*position) => {
                if self.try_forward_mouse(event, ctx) {
                    return EventResult::Handled;
                }
                let lines = (delta.abs() / 30.0).max(1.0) as i32;
                let dir_up = *delta > 0.0;

                let (on_alt, alt_scroll, scrollback_len, cur_off) =
                    if let Some(s) = self.session.as_ref() {
                        s.with_state_ref(|st| {
                            (
                                st.grid.on_alt(),
                                st.grid.alt_scroll(),
                                st.grid.scrollback_len(),
                                st.scroll_offset,
                            )
                        })
                    } else {
                        return EventResult::Ignored;
                    };

                if on_alt && alt_scroll && !ctx.modifiers.shift {
                    let arrow: &[u8] = if dir_up { b"\x1b[A" } else { b"\x1b[B" };
                    let mut payload = Vec::with_capacity(arrow.len() * lines as usize);
                    for _ in 0..lines {
                        payload.extend_from_slice(arrow);
                    }
                    self.write_pty(&payload);
                    return EventResult::Handled;
                }

                if on_alt {
                    return EventResult::Handled;
                }
                let signed = lines * if dir_up { 1 } else { -1 };
                let max = scrollback_len as i32;
                let new = (cur_off as i32 + signed).clamp(0, max) as usize;
                if new != cur_off {
                    if let Some(s) = self.session.as_ref() {
                        s.with_state(|st| st.scroll_offset = new);
                    }
                    self.scrollbar_fader.flash();
                    ctx.request_paint();
                }
                EventResult::Handled
            }

            Event::FocusGained => {
                let send_focus_in = if let Some(s) = self.session.as_ref() {
                    s.with_state(|st| {
                        st.focused = true;
                        st.grid.focus_events()
                    })
                } else {
                    false
                };
                if send_focus_in {
                    self.write_pty(b"\x1b[I");
                }
                ctx.request_paint();
                EventResult::Handled
            }
            Event::FocusLost => {
                self.left_button_held = false;
                self.auto_scroll = 0;
                let send_focus_out = if let Some(s) = self.session.as_ref() {
                    s.with_state(|st| {
                        st.focused = false;
                        st.grid.focus_events()
                    })
                } else {
                    false
                };
                if send_focus_out {
                    self.write_pty(b"\x1b[O");
                }
                ctx.request_paint();
                EventResult::Handled
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
    fn clip_content(&self) -> bool {
        true
    }
    fn wants_tab(&self) -> bool {
        self.session
            .as_ref()
            .map(|s| s.with_state_ref(|st| st.focused))
            .unwrap_or(false)
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        let focused = self
            .session
            .as_ref()
            .map(|s| s.with_state_ref(|st| st.focused))
            .unwrap_or(false);
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::Terminal,
            state: crate::a11y::NodeState {
                focused,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties::default(),
        })
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
        self.refresh_cell_metrics();
        if self.focus_request_pending {
            if let Some(s) = self.session.as_ref() {
                s.with_state(|st| st.focused = true);
            }
        }
    }

    fn take_focus_request(&mut self) -> bool {
        if self.focus_request_pending {
            self.focus_request_pending = false;
            true
        } else {
            false
        }
    }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str {
        "Terminal"
    }

    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }

    fn reset_mss_styles(&mut self) {
        self.mss.reset();
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);

        let mut metrics_changed = false;
        if !self.config.font_family_explicit {
            if let Some(ref family) = self.mss.font_family {
                if family != &self.config.font_family {
                    self.config.font_family = family.clone();
                    metrics_changed = true;
                }
            }
        }
        if !self.config.font_size_explicit {
            if let Some(size) = self.mss.font_size {
                let clamped = size.max(6.0);
                if (clamped - self.config.font_size).abs() > f32::EPSILON {
                    self.config.font_size = clamped;
                    metrics_changed = true;
                }
            }
        }
        if metrics_changed {
            self.refresh_cell_metrics();
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
        self.mss.apply_transitions(base, hover, active, focus, selected);
    }
}

impl TerminalElement {
    fn write_pty(&self, bytes: &[u8]) {
        if let Some(s) = self.session.as_ref() {
            s.write(bytes);
        }
    }

    fn reset_scroll_offset_if_any(&self, ctx: &mut EventContext) {
        if let Some(s) = self.session.as_ref() {
            let changed = s.with_state(|st| {
                if st.scroll_offset != 0 {
                    st.scroll_offset = 0;
                    true
                } else {
                    false
                }
            });
            if changed {
                ctx.request_paint();
            }
        }
    }

    fn clear_selection_if_any(&self, ctx: &mut EventContext) {
        if let Some(s) = self.session.as_ref() {
            let changed = s.with_state(|st| {
                if st.selection.is_active() {
                    st.selection.clear();
                    true
                } else {
                    false
                }
            });
            if changed {
                ctx.request_paint();
            }
        }
    }

    fn collect_selection(&self) -> Option<String> {
        self.session
            .as_ref()
            .and_then(|s| s.with_state_ref(|st| st.selection.collect_text(&st.grid)))
    }

    fn process_command(&mut self, cmd: super::TerminalCommand) {
        use super::TerminalCommand as C;
        match cmd {
            C::Copy => {
                if let Some(text) = self.collect_selection() {
                    if !text.is_empty() {
                        crate::clipboard::copy(&text);
                    }
                }
            }
            C::Paste => {
                if let Some(text) = crate::clipboard::paste() {
                    let cleaned = clipboard_filter::sanitize_paste(&text);
                    let bracketed = self
                        .session
                        .as_ref()
                        .map(|s| s.with_state_ref(|st| st.grid.bracketed_paste()))
                        .unwrap_or(false);
                    let bytes = if bracketed {
                        clipboard_filter::wrap_bracketed(&cleaned)
                    } else {
                        cleaned.into_bytes()
                    };
                    self.write_pty(&bytes);
                }
            }
            C::Clear => {
                self.write_pty(b"\x1b[H\x1b[2J");
            }
        }
    }

    fn do_paste(&mut self, ctx: &mut EventContext) {
        let Some(text) = ctx.paste_from_clipboard() else { return };
        let cleaned = clipboard_filter::sanitize_paste(&text);
        let bracketed = self
            .session
            .as_ref()
            .map(|s| s.with_state_ref(|st| st.grid.bracketed_paste()))
            .unwrap_or(false);
        let bytes = if bracketed {
            clipboard_filter::wrap_bracketed(&cleaned)
        } else {
            cleaned.into_bytes()
        };
        self.write_pty(&bytes);
    }

    fn try_forward_mouse(&mut self, event: &Event, ctx: &EventContext) -> bool {
        let (mode, encoding) = if let Some(s) = self.session.as_ref() {
            s.with_state_ref(|st| (st.grid.mouse_mode(), st.grid.mouse_encoding()))
        } else {
            return false;
        };
        if mode == MouseMode::Off {
            return false;
        }
        if ctx.modifiers.shift {
            return false;
        }
        let (point, action) = match event {
            Event::MouseDown { button, position } => {
                let (col, row) = match self.point_to_local(*position) {
                    Some(v) => v,
                    None => return false,
                };
                if !mouse::should_report(mode, MouseAction::Press(*button), self.left_button_held) {
                    return false;
                }
                ((col, row), MouseAction::Press(*button))
            }
            Event::MouseUp { button, position } => {
                let (col, row) = match self.point_to_local(*position) {
                    Some(v) => v,
                    None => return false,
                };
                if !mouse::should_report(mode, MouseAction::Release(*button), self.left_button_held) {
                    return false;
                }
                ((col, row), MouseAction::Release(*button))
            }
            Event::MouseMove(p) => {
                let (col, row) = match self.point_to_local(*p) {
                    Some(v) => v,
                    None => return false,
                };
                let action = MouseAction::Motion {
                    button: if self.left_button_held { Some(MouseButton::Left) } else { None },
                };
                if !mouse::should_report(mode, action, self.left_button_held) {
                    return false;
                }
                ((col, row), action)
            }
            Event::MouseWheel { delta, position, .. } => {
                let (col, row) = match self.point_to_local(*position) {
                    Some(v) => v,
                    None => return false,
                };
                let dir = if *delta > 0.0 { 1 } else { -1 };
                let action = MouseAction::Wheel(dir);
                if !mouse::should_report(mode, action, self.left_button_held) {
                    return false;
                }
                ((col, row), action)
            }
            _ => return false,
        };

        let bytes = mouse::encode_event(encoding, action, point.0, point.1, ctx.modifiers);
        if let Some(bytes) = bytes {
            self.write_pty(&bytes);
            true
        } else {
            true
        }
    }

    fn draw_line(
        &self,
        list: &mut DisplayList,
        cells: &[Cell],
        origin: Point,
        row_in_view: usize,
        cell_w: f32,
        cell_h: f32,
        default_fg: Color,
        default_bg: Color,
    ) {
        let y = origin.y + row_in_view as f32 * cell_h;

        let row_text: String = cells.iter().map(|c| c.ch).collect();
        let bbox_sample: &str = if row_text.trim().is_empty() {
            "Mgyqlj"
        } else {
            row_text.as_str()
        };

        let effective_bg = |c: &Cell| -> Color {
            if c.flags.contains(CellFlags::REVERSE) {
                palette::resolve(c.fg, default_fg, default_bg, true)
            } else {
                palette::resolve(c.bg, default_fg, default_bg, false)
            }
        };
        let mut col = 0;
        while col < cells.len() {
            let bg = effective_bg(&cells[col]);
            let mut span_end = col + 1;
            while span_end < cells.len() && effective_bg(&cells[span_end]) == bg {
                span_end += 1;
            }
            if bg != default_bg {
                let rect = Rect::new(
                    Point::new(origin.x + col as f32 * cell_w, y),
                    Size::new((span_end - col) as f32 * cell_w, cell_h),
                );
                list.push_rect(rect, bg, [0.0; 4]);
            }
            col = span_end;
        }

        let mut buf = String::with_capacity(4);
        for (col, cell) in cells.iter().enumerate() {
            if cell.ch == ' ' && cell.flags.is_empty() {
                continue;
            }
            let mut fg = palette::resolve(cell.fg, default_fg, default_bg, true);
            let mut bg_for_reverse =
                palette::resolve(cell.bg, default_fg, default_bg, false);
            if cell.flags.contains(CellFlags::REVERSE) {
                std::mem::swap(&mut fg, &mut bg_for_reverse);
            }
            if cell.flags.contains(CellFlags::FAINT) {
                fg = fg.with_alpha(fg.a * 0.6);
            }
            let weight = if cell.flags.contains(CellFlags::BOLD) {
                700
            } else {
                400
            };

            let rect = Rect::new(
                Point::new(origin.x + col as f32 * cell_w, y),
                Size::new(cell_w, cell_h),
            );
            buf.clear();
            buf.push(cell.ch);
            list.push_text_with_bbox(
                buf.as_str(),
                rect,
                fg,
                self.config.font_size,
                crate::mss::TextAlign::DEFAULT,
                Default::default(),
                weight,
                Some(self.config.font_family.clone()),
                bbox_sample,
            );
        }

        let mut col = 0;
        while col < cells.len() {
            let start_cell = &cells[col];
            if let Some(link_id) = start_cell.link_id {
                if !start_cell.flags.contains(CellFlags::UNDERLINE) {
                    let start = col;
                    let mut end = col + 1;
                    while end < cells.len() && cells[end].link_id == Some(link_id) {
                        end += 1;
                    }
                    let fg = palette::resolve(start_cell.fg, default_fg, default_bg, true);
                    let is_hovered = Some(link_id) == self.hovered_link;
                    let underline_color = if is_hovered {
                        self.link_hover_color()
                    } else {
                        fg.with_alpha(fg.a * 0.7)
                    };
                    let thickness = if is_hovered { 2.0 } else { 1.0 };
                    let underline_y = y + cell_h - thickness - 1.0;
                    let underline_rect = Rect::new(
                        Point::new(origin.x + start as f32 * cell_w, underline_y),
                        Size::new((end - start) as f32 * cell_w, thickness),
                    );
                    list.push_rect(underline_rect, underline_color, [0.0; 4]);
                    col = end;
                    continue;
                }
            }
            col += 1;
        }
    }
}

impl StyledElement for TerminalElement {
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

#[allow(dead_code)]
fn _any_anchor(_a: &dyn Any) {}
