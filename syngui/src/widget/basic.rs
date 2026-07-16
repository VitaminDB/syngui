use super::{Element, Widget, styled::StyledElement};
use crate::core::{Color, Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::render::DisplayList;
use crate::widget::{DirtyFlags, ElementId, UpdateContext, EventContext};
use crate::widget::selection::TextSelectionState;
use std::any::Any;
use std::sync::Arc;
use std::time::Instant;
use crate::core::sync::Mutex;

const DEFAULT_FONT_SIZE: f32 = 16.0;

fn count_visual_lines_via_measure(
    text: &str,
    available_width: f32,
    font_size: f32,
    bold: bool,
    font_family: Option<&str>,
    tm: &dyn crate::widget::context::TextMeasure,
) -> usize {
    if text.is_empty() {
        return 1;
    }
    let mut total: usize = 0;
    for segment in text.split('\n') {
        total += visual_lines_in_segment(segment, available_width, font_size, bold, font_family, tm);
    }
    total.max(1)
}

fn visual_lines_in_segment(
    line: &str,
    available_width: f32,
    font_size: f32,
    bold: bool,
    font_family: Option<&str>,
    tm: &dyn crate::widget::context::TextMeasure,
) -> usize {
    if line.is_empty() {
        return 1;
    }
    let full = tm.measure_text_width_styled(line, font_size, line.chars().count(), bold, font_family);
    if full <= available_width {
        return 1;
    }

    let mut visual_lines: usize = 1;
    let mut x: f32 = 0.0;
    let mut word_width: f32 = 0.0;
    let mut word_chars: usize = 0;
    let mut buf = [0u8; 4];

    for ch in line.chars() {
        let ch_str = ch.encode_utf8(&mut buf);
        let advance = tm.measure_text_width_styled(ch_str, font_size, 1, bold, font_family);

        if ch == ' ' {
            if word_chars > 0 {
                if x + word_width > available_width && x > 0.0 {
                    visual_lines += 1;
                    x = 0.0;
                }
                x += word_width;
                word_width = 0.0;
                word_chars = 0;
            }
            x += advance;
            continue;
        }

        word_width += advance;
        word_chars += 1;

        if x + word_width > available_width && x > 0.0 {
            visual_lines += 1;
            x = 0.0;
        }

        if word_width > available_width && word_chars > 1 {
            visual_lines += 1;
            x = 0.0;
            word_width = advance;
            word_chars = 1;
        }
    }
    if word_chars > 0 && x + word_width > available_width && x > 0.0 {
        visual_lines += 1;
    }
    visual_lines
}

const ELLIPSIS: &str = "\u{2026}";

fn truncate_to_lines<'a>(
    text: &'a str,
    available_width: f32,
    max_lines: usize,
    font_size: f32,
    bold: bool,
    font_family: Option<&str>,
    tm: Option<&dyn crate::widget::context::TextMeasure>,
) -> std::borrow::Cow<'a, str> {
    let max_lines = max_lines.max(1);
    if text.is_empty() {
        return std::borrow::Cow::Borrowed(text);
    }
    let Some(tm) = tm else {
        return truncate_by_logical_lines(text, max_lines);
    };
    if !available_width.is_finite() || available_width <= 0.0 {
        return truncate_by_logical_lines(text, max_lines);
    }

    let ellipsis_w =
        tm.measure_text_width_styled(ELLIPSIS, font_size, ELLIPSIS.chars().count(), bold, font_family);
    if ellipsis_w >= available_width {
        return std::borrow::Cow::Owned(ELLIPSIS.to_string());
    }
    let last_budget = available_width - ellipsis_w;

    let mut line_idx: usize = 0;
    let mut x: f32 = 0.0;
    let mut word_chars: usize = 0;
    let mut word_width: f32 = 0.0;
    let mut last_committed_byte: usize = 0;
    let mut buf = [0u8; 4];

    let on_last = |idx: usize| idx == max_lines - 1;
    let budget_for = |idx: usize| if on_last(idx) { last_budget } else { available_width };

    let mut iter = text.char_indices().peekable();
    while let Some((byte_idx, ch)) = iter.next() {
        let next_byte = iter.peek().map(|&(b, _)| b).unwrap_or(text.len());

        if ch == '\n' {
            if word_chars > 0 {
                if x + word_width > budget_for(line_idx) && x > 0.0 {
                    if on_last(line_idx) {
                        return ellipsize(text, last_committed_byte);
                    }
                    line_idx += 1;
                    x = 0.0;
                }
                if on_last(line_idx) && x + word_width > last_budget {
                    return ellipsize(text, last_committed_byte);
                }
                if on_last(line_idx) {
                    last_committed_byte = byte_idx;
                }
                word_width = 0.0;
                word_chars = 0;
            }
            if on_last(line_idx) {
                return ellipsize(text, last_committed_byte);
            }
            line_idx += 1;
            x = 0.0;
            continue;
        }

        let ch_str = ch.encode_utf8(&mut buf);
        let advance = tm.measure_text_width_styled(ch_str, font_size, 1, bold, font_family);

        if ch == ' ' {
            if word_chars > 0 {
                if x + word_width > budget_for(line_idx) && x > 0.0 {
                    if on_last(line_idx) {
                        return ellipsize(text, last_committed_byte);
                    }
                    line_idx += 1;
                    x = 0.0;
                }
                if on_last(line_idx) && x + word_width > last_budget {
                    return ellipsize(text, last_committed_byte);
                }
                x += word_width;
                if on_last(line_idx) {
                    last_committed_byte = byte_idx;
                }
                word_width = 0.0;
                word_chars = 0;
            }
            if on_last(line_idx) && x + advance > last_budget {
                return ellipsize(text, last_committed_byte);
            }
            x += advance;
            if on_last(line_idx) {
                last_committed_byte = next_byte;
            }
            continue;
        }

        word_width += advance;
        word_chars += 1;

        if x + word_width > budget_for(line_idx) && x > 0.0 {
            if on_last(line_idx) {
                return ellipsize(text, last_committed_byte);
            }
            line_idx += 1;
            x = 0.0;
        }

        if word_width > budget_for(line_idx) && word_chars > 1 {
            if on_last(line_idx) {
                return ellipsize(text, last_committed_byte);
            }
            line_idx += 1;
            if line_idx >= max_lines {
                return ellipsize(text, byte_idx);
            }
            x = 0.0;
            word_width = advance;
            word_chars = 1;
        }

        if on_last(line_idx) {
            last_committed_byte = next_byte;
        }
    }

    std::borrow::Cow::Borrowed(text)
}

fn ellipsize(text: &str, cut: usize) -> std::borrow::Cow<'_, str> {
    if cut == 0 {
        return std::borrow::Cow::Owned(ELLIPSIS.to_string());
    }
    debug_assert!(text.is_char_boundary(cut), "cut must be on a UTF-8 char boundary");
    let trimmed = text[..cut].trim_end_matches(|c: char| c == ' ' || c == '\t');
    let mut s = String::with_capacity(trimmed.len() + ELLIPSIS.len());
    s.push_str(trimmed);
    s.push_str(ELLIPSIS);
    std::borrow::Cow::Owned(s)
}

fn truncate_by_logical_lines(text: &str, max_lines: usize) -> std::borrow::Cow<'_, str> {
    let mut newline_count: usize = 0;
    let mut cut: usize = text.len();
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            newline_count += 1;
            if newline_count == max_lines {
                cut = i;
                break;
            }
        }
    }
    if cut == text.len() {
        std::borrow::Cow::Borrowed(text)
    } else {
        std::borrow::Cow::Owned(text[..cut].to_string())
    }
}

fn count_visual_lines_approx(text: &str, available_width: f32, font_size: f32) -> usize {
    let avg_char_w = (font_size * 0.65).max(1.0);
    let mut total: usize = 0;
    for segment in text.split('\n') {
        let seg_chars = segment.chars().count() as f32;
        let seg_width = seg_chars * avg_char_w;
        let n = if seg_width <= available_width {
            1
        } else {
            (seg_width / available_width).ceil() as usize
        };
        total += n.max(1);
    }
    total.max(1)
}

pub struct Text {
    text: String,
    color: Option<Color>,
    font_weight: Option<u16>,
    dark_color: Option<Color>,
    theme: Option<Arc<Mutex<bool>>>,
    max_lines: Option<usize>,
    selectable: bool,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            font_weight: None,
            dark_color: None,
            theme: None,
            max_lines: None,
            selectable: false,
        }
    }

    pub fn selectable(mut self, on: bool) -> Self {
        self.selectable = on;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn font_weight(mut self, weight: u16) -> Self {
        self.font_weight = Some(weight);
        self
    }

    pub fn bold(self) -> Self {
        self.font_weight(700)
    }

    pub fn dark_mode(mut self, dark_color: Color, theme: Arc<Mutex<bool>>) -> Self {
        self.dark_color = Some(dark_color);
        self.theme = Some(theme);
        self
    }

    pub fn max_lines(mut self, n: usize) -> Self {
        self.max_lines = Some(n.max(1));
        self
    }
}

impl Widget for Text {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(TextElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            text: self.text.clone(),
            color: self.color.unwrap_or(Color::rgb(0.0, 0.0, 0.0)),
            font_size: DEFAULT_FONT_SIZE,
            dark_color: self.dark_color,
            theme: self.theme.clone(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss_font_weight: self.font_weight.unwrap_or(400),
            mss_text_align: None,
            mss_text_decoration: crate::mss::TextDecoration::None,
            mss_font_family: None,
            mss_letter_spacing: 0.0,
            mss_text_transform: None,
            mss_text_shadow: None,
            mss_line_height: None,
            mss_padding_left: 0.0,
            mss_padding_right: 0.0,
            mss_padding_top: 0.0,
            mss_padding_bottom: 0.0,
            max_render_width: f32::INFINITY,
            text_measure: None,
            mss_max_lines: self.max_lines,
            selectable: self.selectable,
            selection: TextSelectionState::new(),
            cursor_pos: 0,
            mouse_selecting: false,
            mss_selection_color: Color::new(0.231, 0.510, 0.965, 0.30),
            last_click_at: None,
            click_count: 0,
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

    fn mount(&self, _tree: &mut super::ElementTree, _parent_id: ElementId) {
    }
}

struct TextElement {
    id: ElementId,
    bounds: Rect,
    text: String,
    color: Color,
    font_size: f32,
    dark_color: Option<Color>,
    theme: Option<Arc<Mutex<bool>>>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss_font_weight: u16,
    mss_text_align: Option<crate::mss::TextAlign>,
    mss_text_decoration: crate::mss::TextDecoration,
    mss_font_family: Option<String>,
    mss_letter_spacing: f32,
    mss_text_transform: Option<crate::mss::fields::TextTransform>,
    mss_text_shadow: Option<crate::mss::fields::TextShadow>,
    mss_line_height: Option<crate::mss::fields::LineHeight>,
    mss_padding_left: f32,
    mss_padding_right: f32,
    mss_padding_top: f32,
    mss_padding_bottom: f32,
    max_render_width: f32,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
    mss_max_lines: Option<usize>,

    selectable: bool,
    selection: TextSelectionState,
    cursor_pos: usize,
    mouse_selecting: bool,
    mss_selection_color: Color,
    last_click_at: Option<Instant>,
    click_count: u8,
}

impl TextElement {
    fn effective_color(&self) -> Color {
        if let (Some(dark), Some(theme)) = (&self.dark_color, &self.theme) {
            if *theme.lock().unwrap() { *dark } else { self.color }
        } else {
            self.color
        }
    }

    fn hit_test_byte(&self, pos: Point) -> usize {
        let pl = self.mss_padding_left;
        let x_local = (pos.x - self.bounds.origin.x - pl)
            .max(0.0)
            .min((self.bounds.size.width - pl - self.mss_padding_right).max(0.0));
        let char_idx = if let Some(tm) = self.text_measure.as_ref() {
            tm.hit_test_char_styled(
                &self.text,
                self.font_size,
                x_local,
                self.mss_font_family.as_deref(),
            )
        } else {
            let avg = self.font_size * 0.55;
            (x_local / avg.max(1.0)) as usize
        };
        self.text
            .char_indices()
            .nth(char_idx)
            .map(|(b, _)| b)
            .unwrap_or(self.text.len())
    }
}

impl Element for TextElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(widget) = widget.as_any().downcast_ref::<Text>() {
            let text_changed = self.text != widget.text;
            if text_changed {
                self.selection.clear();
                self.cursor_pos = 0;
                self.mouse_selecting = false;
            }
            self.selectable = widget.selectable;
            self.text = widget.text.clone();
            if let Some(c) = widget.color {
                self.color = c;
            }
            if let Some(fw) = widget.font_weight {
                self.mss_font_weight = fw;
            }
            self.dark_color = widget.dark_color;
            self.theme = widget.theme.clone();
            let max_lines_changed = widget.max_lines.is_some()
                && self.mss_max_lines != widget.max_lines;
            if let Some(n) = widget.max_lines {
                self.mss_max_lines = Some(n);
            }
            let mut flags = DirtyFlags::RENDER;
            if text_changed || max_lines_changed {
                flags |= DirtyFlags::LAYOUT;
            }
            self.mark_dirty(flags);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let pad_h = self.mss_padding_left + self.mss_padding_right;
        let pad_v = self.mss_padding_top + self.mss_padding_bottom;
        let bold = self.mss_font_weight >= 700;
        let text_width = self.text_measure.as_ref()
            .map(|tm| tm.measure_text_width_styled(
                &self.text, self.font_size, self.text.chars().count(),
                bold, self.mss_font_family.as_deref(),
            ))
            .unwrap_or_else(|| self.text.chars().count() as f32 * self.font_size * 0.65);
        let line_height = self.mss_line_height
            .map(|lh| lh.resolve(self.font_size))
            .unwrap_or(self.font_size * 1.3);
        let available_width = if constraints.max_width.is_finite() {
            (constraints.max_width - pad_h).max(1.0)
        } else {
            f32::INFINITY
        };
        let line_count = if available_width.is_finite() {
            self.text_measure
                .as_ref()
                .map(|tm| {
                    count_visual_lines_via_measure(
                        &self.text,
                        available_width,
                        self.font_size,
                        bold,
                        self.mss_font_family.as_deref(),
                        tm.as_ref(),
                    )
                })
                .unwrap_or_else(|| {
                    count_visual_lines_approx(&self.text, available_width, self.font_size)
                })
        } else {
            self.text.chars().filter(|&c| c == '\n').count() + 1
        };
        let line_count = line_count.min(self.mss_max_lines.unwrap_or(usize::MAX));
        let natural_height = line_height * line_count as f32 + pad_v;
        let aligned = self.mss_text_align.is_some();
        let width = if aligned && constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            (text_width + pad_h).min(constraints.max_width)
        };
        let height = if aligned && constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            natural_height.min(constraints.max_height)
        };
        self.max_render_width = constraints.max_width;
        let size = Size::new(width, height);
        self.bounds = Rect::new(self.bounds.origin, size);
        size
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let pl = self.mss_padding_left;
        let pr = self.mss_padding_right;
        let pt = self.mss_padding_top;
        let pb = self.mss_padding_bottom;
        let pad_h = pl + pr;
        let pad_v = pt + pb;
        let render_width = if self.max_render_width.is_finite() {
            (self.max_render_width - pad_h).max(self.bounds.size.width - pad_h)
        } else {
            (self.bounds.size.width - pad_h).max(10000.0)
        };
        let render_bounds = Rect::new(
            Point::new(self.bounds.origin.x + pl, self.bounds.origin.y + pt),
            Size::new(render_width, (self.bounds.size.height - pad_v).max(0.0)),
        );

        let display_text: std::borrow::Cow<str> = match self.mss_text_transform {
            Some(crate::mss::fields::TextTransform::Uppercase) => std::borrow::Cow::Owned(self.text.to_uppercase()),
            Some(crate::mss::fields::TextTransform::Lowercase) => std::borrow::Cow::Owned(self.text.to_lowercase()),
            Some(crate::mss::fields::TextTransform::Capitalize) => {
                let mut result = String::with_capacity(self.text.len());
                let mut cap_next = true;
                for c in self.text.chars() {
                    if cap_next && c.is_alphabetic() {
                        for uc in c.to_uppercase() { result.push(uc); }
                        cap_next = false;
                    } else {
                        result.push(c);
                        if c.is_whitespace() { cap_next = true; }
                    }
                }
                std::borrow::Cow::Owned(result)
            }
            _ => std::borrow::Cow::Borrowed(&self.text),
        };

        let display_text: std::borrow::Cow<str> = if let Some(n) = self.mss_max_lines {
            let bold = self.mss_font_weight >= 700;
            let tm: Option<&dyn crate::widget::context::TextMeasure> =
                self.text_measure.as_deref();
            match truncate_to_lines(
                display_text.as_ref(),
                render_width,
                n,
                self.font_size,
                bold,
                self.mss_font_family.as_deref(),
                tm,
            ) {
                std::borrow::Cow::Borrowed(_) => display_text,
                std::borrow::Cow::Owned(s) => std::borrow::Cow::Owned(s),
            }
        } else {
            display_text
        };

        let align = self.mss_text_align.unwrap_or(crate::mss::TextAlign::DEFAULT);
        let has_extra = self.mss_letter_spacing != 0.0
            || self.mss_text_shadow.is_some()
            || self.mss_text_transform.is_some();

        if self.selectable {
            if let Some((sel_start, sel_end)) = self.selection.range(self.cursor_pos) {
                let sel_text = self.text.as_str();
                let s = sel_start.min(sel_text.len());
                let e = sel_end.min(sel_text.len());
                if e > s {
                    list.push_text_selection(
                        sel_text,
                        s,
                        e,
                        render_bounds.origin.x,
                        render_bounds.origin.y,
                        self.font_size + 2.0,
                        self.font_size,
                        self.mss_selection_color,
                    );
                }
            }
        }

        if has_extra
            || self.mss_text_align.is_some()
            || self.mss_font_family.is_some()
            || self.mss_font_weight >= 700
            || self.mss_text_decoration != crate::mss::TextDecoration::None
        {
            list.push_text_full(
                &display_text, render_bounds, self.effective_color(), self.font_size,
                align, self.mss_text_decoration, self.mss_font_weight,
                self.mss_font_family.clone(),
                self.mss_letter_spacing,
                self.mss_text_shadow.clone(),
            );
        } else {
            list.push_text(&display_text, render_bounds, self.effective_color(), self.font_size);
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if !self.selectable {
            return EventResult::Ignored;
        }

        match event {
            Event::MouseMove(pos) => {
                if self.mouse_selecting {
                    self.cursor_pos = self.hit_test_byte(*pos);
                    self.mark_dirty(DirtyFlags::RENDER);
                    ctx.set_cursor(CursorIcon::Text);
                    return EventResult::Handled;
                }
                if self.bounds.contains(*pos) {
                    ctx.set_cursor(CursorIcon::Text);
                }
                EventResult::Ignored
            }
            Event::MouseDown { button: MouseButton::Left, position } => {
                if !self.bounds.contains(*position) {
                    return EventResult::Ignored;
                }
                let byte = self.hit_test_byte(*position);
                let now = Instant::now();
                let within = self
                    .last_click_at
                    .map(|t| now.duration_since(t).as_millis() < 300)
                    .unwrap_or(false);
                if within {
                    self.click_count = (self.click_count + 1).min(3);
                } else {
                    self.click_count = 1;
                }
                self.last_click_at = Some(now);

                match self.click_count {
                    2 => {
                        self.selection.select_word(&self.text, byte);
                        self.cursor_pos = self.selection.range(byte).map(|(_, e)| e).unwrap_or(byte);
                        self.mouse_selecting = false;
                    }
                    3 => {
                        self.selection.select_all();
                        self.cursor_pos = self.text.len();
                        self.mouse_selecting = false;
                    }
                    _ => {
                        self.selection.start(byte);
                        self.cursor_pos = byte;
                        self.mouse_selecting = true;
                    }
                }
                self.mark_dirty(DirtyFlags::RENDER);
                EventResult::Handled
            }
            Event::MouseUp { button: MouseButton::Left, .. } => {
                if self.mouse_selecting {
                    self.mouse_selecting = false;
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::C) if ctx.modifiers.ctrl => {
                if let Some(text) = self.selection.selected_text(&self.text, self.cursor_pos) {
                    #[cfg(feature = "clipboard")]
                    ctx.copy_to_clipboard(text);
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(Key::A) if ctx.modifiers.ctrl => {
                self.selection.select_all();
                self.cursor_pos = self.text.len();
                self.mark_dirty(DirtyFlags::RENDER);
                EventResult::Handled
            }
            Event::FocusLost => {
                self.mouse_selecting = false;
                self.last_click_at = None;
                self.click_count = 0;
                EventResult::Ignored
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
        self.bounds = Rect::new(pos, self.bounds.size);
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

    fn mount(&mut self, tree: &mut super::ElementTree) {
        self.text_measure = tree.text_measure.clone();
    }

    fn element_type_name(&self) -> &str { "Text" }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.apply_style(style);
    }

    fn accessibility_info(&self) -> Option<crate::a11y::AccessibilityInfo> {
        Some(crate::a11y::AccessibilityInfo {
            role: crate::a11y::Role::StaticText,
            state: crate::a11y::NodeState::default(),
            properties: crate::a11y::NodeProperties {
                label: Some(self.text.clone()),
                ..Default::default()
            },
        })
    }
}

impl StyledElement for TextElement {
    fn apply_style(&mut self, style: &ComputedStyle) {
        if let Some(color) = style.color() {
            self.color = Color::from_srgb(color.r, color.g, color.b, color.a as f32 / 255.0);
        }
        self.font_size = style.font_size();
        if let Some(fw) = style.font_weight() {
            self.mss_font_weight = fw;
        }
        if let Some(ta) = style.text_align() {
            self.mss_text_align = Some(ta);
        }
        if let Some(td) = style.text_decoration() {
            self.mss_text_decoration = td;
        }
        if let Some(ff) = style.font_family() {
            self.mss_font_family = Some(ff.to_string());
        }
        if let Some(v) = style.get("letter-spacing").and_then(|v| v.as_px()) {
            self.mss_letter_spacing = v;
        }
        if let Some(s) = style.get("text-transform").and_then(|v| v.as_string()) {
            self.mss_text_transform = match s {
                "uppercase" => Some(crate::mss::fields::TextTransform::Uppercase),
                "lowercase" => Some(crate::mss::fields::TextTransform::Lowercase),
                "capitalize" => Some(crate::mss::fields::TextTransform::Capitalize),
                "none" => Some(crate::mss::fields::TextTransform::None),
                _ => None,
            };
        }
        if let Some(s) = style.get("text-shadow").and_then(|v| v.as_string()) {
            self.mss_text_shadow = crate::mss::fields::TextShadow::parse(s);
        }
        if let Some(v) = style.get("line-height") {
            self.mss_line_height = match v {
                crate::mss::StyleValue::Number(m) =>
                    Some(crate::mss::fields::LineHeight::Multiplier(*m)),
                crate::mss::StyleValue::Length(px, crate::mss::Unit::Px) =>
                    Some(crate::mss::fields::LineHeight::Px(*px)),
                _ => self.mss_line_height,
            };
        }
        if let Some(v) = style.get("line-clamp").and_then(|v| v.as_px()) {
            let n = (v.max(1.0)) as usize;
            self.mss_max_lines = Some(n);
        }
        let p = style.padding();
        if p > 0.0 {
            self.mss_padding_left = p;
            self.mss_padding_right = p;
            self.mss_padding_top = p;
            self.mss_padding_bottom = p;
        }
        if let Some(v) = style.get("padding-left").and_then(|v| v.as_px()) {
            self.mss_padding_left = v;
        }
        if let Some(v) = style.get("padding-right").and_then(|v| v.as_px()) {
            self.mss_padding_right = v;
        }
        if let Some(v) = style.get("padding-top").and_then(|v| v.as_px()) {
            self.mss_padding_top = v;
        }
        if let Some(v) = style.get("padding-bottom").and_then(|v| v.as_px()) {
            self.mss_padding_bottom = v;
        }
        if let Some(c) = style.get("selection-color").and_then(|v| v.as_color()) {
            self.mss_selection_color =
                Color::from_srgb(c.r, c.g, c.b, c.a as f32 / 255.0);
        }
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

pub struct Center {
    child: Option<Box<dyn Widget>>,
}

impl Center {
    pub fn new() -> Self {
        Self {
            child: None,
        }
    }

    pub fn child(mut self, child: impl Widget + 'static) -> Self {
        self.child = Some(Box::new(child));
        self
    }
}

impl Default for Center {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Center {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(CenterElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
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

    fn mount(&self, tree: &mut super::ElementTree, parent_id: ElementId) {
        if let Some(child) = &self.child {
            let child_element = child.create_element();
            let child_id = tree.insert_with_type_id(child_element, Some(parent_id), child.as_any().type_id());
            child.mount(tree, child_id);
        }
    }

    fn child_widgets(&self) -> Vec<&dyn Widget> {
        self.child.as_ref().map(|c| vec![c.as_ref() as &dyn Widget]).unwrap_or_default()
    }
}

struct CenterElement {
    id: ElementId,
    bounds: Rect,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
}

impl Element for CenterElement {
    fn update(&mut self, _widget: &dyn Widget, _ctx: &mut UpdateContext) {
        self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let width = constraints.max_width;
        let height = constraints.max_height;
        self.bounds = Rect::new(Point::zero(), Size::new(width, height));
        Size::new(width, height)
    }

    fn build_display_list(&self, _list: &mut DisplayList, _clip: Rect) {
    }

    fn handle_event(&mut self, _event: &Event, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }

    fn passthrough_hit_test(&self) -> bool { true }

    fn children(&self) -> &[ElementId] {
        &[]
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_position(&mut self, pos: Point) {
        self.bounds = Rect::new(pos, self.bounds.size);
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

    fn mount(&mut self, _tree: &mut super::ElementTree) {
    }

    fn element_type_name(&self) -> &str { "Center" }

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }
}

impl StyledElement for CenterElement {
    fn apply_style(&mut self, _style: &ComputedStyle) {
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
    use crate::widget::context::TextMeasure;

    struct MonoMeasure;
    impl TextMeasure for MonoMeasure {
        fn measure_text_width(&self, text: &str, _font_size: f32, char_count: usize) -> f32 {
            (text.chars().take(char_count).count() as f32) * 10.0
        }
        fn hit_test_char(&self, _text: &str, _font_size: f32, x_offset: f32) -> usize {
            (x_offset / 10.0).floor().max(0.0) as usize
        }
    }

    #[test]
    fn visual_lines_single_short_line_is_one() {
        let n = count_visual_lines_via_measure("hello", 1000.0, 12.0, false, None, &MonoMeasure);
        assert_eq!(n, 1);
    }

    #[test]
    fn visual_lines_multiple_newlines_counted_when_fit() {
        let n = count_visual_lines_via_measure("a\nb\nc", 1000.0, 12.0, false, None, &MonoMeasure);
        assert_eq!(n, 3);
    }

    #[test]
    fn visual_lines_word_wrap_at_space() {
        let line = "aaa bbb ccc ddd eee fff";
        let n = count_visual_lines_via_measure(line, 100.0, 12.0, false, None, &MonoMeasure);
        assert!(n >= 2, "expected ≥2 visual lines, got {n}");
    }

    #[test]
    fn visual_lines_long_word_breaks_char_level() {
        let n = count_visual_lines_via_measure("a".repeat(20).as_str(), 50.0, 12.0, false, None, &MonoMeasure);
        assert!(n >= 4, "expected ≥4 for 20-char word in 50px, got {n}");
    }

    #[test]
    fn visual_lines_per_segment_accumulates() {
        let text = format!("fit\n{}", "a".repeat(20));
        let n = count_visual_lines_via_measure(&text, 50.0, 12.0, false, None, &MonoMeasure);
        assert!(n >= 5, "expected ≥5, got {n}");
    }

    #[test]
    fn visual_lines_empty_text_is_one() {
        let n = count_visual_lines_via_measure("", 100.0, 12.0, false, None, &MonoMeasure);
        assert_eq!(n, 1);
    }

    fn trunc(text: &str, w: f32, n: usize) -> std::borrow::Cow<'_, str> {
        let tm: &dyn TextMeasure = &MonoMeasure;
        truncate_to_lines(text, w, n, 12.0, false, None, Some(tm))
    }

    #[test]
    fn truncate_under_limit_returns_borrowed() {
        let out = trunc("abc", 100.0, 3);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
        assert_eq!(out.as_ref(), "abc");
    }

    #[test]
    fn truncate_exact_limit_returns_borrowed() {
        let out = trunc("aaa\nbbb\nccc", 100.0, 3);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
        assert_eq!(out.as_ref(), "aaa\nbbb\nccc");
    }

    #[test]
    fn truncate_above_limit_appends_ellipsis() {
        let out = trunc("aaa\nbbb\nccc\nddd", 30.0, 2);
        assert!(out.ends_with('\u{2026}'));
        assert!(out.starts_with("aaa"));
    }

    #[test]
    fn truncate_long_unbreakable_word_clamped_with_ellipsis() {
        let s = "a".repeat(20);
        let out = trunc(&s, 30.0, 1);
        assert!(out.ends_with('\u{2026}'));
        let body = out.trim_end_matches('\u{2026}');
        assert!(body.chars().count() <= 3, "{body:?}");
    }

    #[test]
    fn truncate_explicit_newlines_respect_max_lines() {
        let out = truncate_to_lines("a\nb\nc\nd\ne", 100.0, 3, 12.0, false, None, None);
        assert_eq!(out.as_ref(), "a\nb\nc");
    }

    #[test]
    fn truncate_empty_text_is_borrowed() {
        let out = trunc("", 100.0, 3);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
        assert_eq!(out.as_ref(), "");
    }

    #[test]
    fn truncate_max_zero_normalized_to_one() {
        let out = trunc("abcdefghij", 30.0, 0);
        assert!(out.ends_with('\u{2026}'));
    }

    #[test]
    fn truncate_narrower_than_ellipsis_returns_just_ellipsis() {
        let out = trunc("hello world", 5.0, 2);
        assert_eq!(out.as_ref(), "\u{2026}");
    }

    #[test]
    fn text_max_lines_normalizes_zero_to_one() {
        let t = Text::new("hi").max_lines(0);
        assert_eq!(t.max_lines, Some(1));
    }

    #[test]
    fn text_max_lines_stores_value() {
        let t = Text::new("hi").max_lines(3);
        assert_eq!(t.max_lines, Some(3));
    }
}
