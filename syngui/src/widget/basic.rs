use super::{Element, Widget, styled::StyledElement};
use crate::core::{Color, Point, Rect, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::ComputedStyle;
use crate::render::DisplayList;
use crate::text::line_break::breaks_before;
use crate::widget::{DirtyFlags, ElementId, UpdateContext, EventContext};
use crate::widget::selection::TextSelectionState;
use std::any::Any;
use std::sync::Arc;
use std::time::Instant;
use crate::core::sync::Mutex;

const DEFAULT_FONT_SIZE: f32 = 16.0;

/// Сколько визуальных строк займёт текст в заданной ширине. Алгоритм
/// переноса тот же, что в `FontAtlas::shape_text`, — иначе расчёт разошёлся
/// бы с тем, что реально рисует рендерер.
pub(crate) fn count_visual_lines_via_measure(
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
    let mut prev: Option<char> = None;

    for ch in line.chars() {
        let ch_str = ch.encode_utf8(&mut buf);
        let advance = tm.measure_text_width_styled(ch_str, font_size, 1, bold, font_family);
        let prev_ch = prev.replace(ch);

        if (ch == ' ' || breaks_before(prev_ch, ch)) && word_chars > 0 {
            if x + word_width > available_width && x > 0.0 {
                visual_lines += 1;
                x = 0.0;
            }
            x += word_width;
            word_width = 0.0;
            word_chars = 0;
        }
        if ch == ' ' {
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

    let mut prev: Option<char> = None;
    let mut iter = text.char_indices().peekable();
    while let Some((byte_idx, ch)) = iter.next() {
        let next_byte = iter.peek().map(|&(b, _)| b).unwrap_or(text.len());
        let prev_ch = prev.replace(ch);

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

        if (ch == ' ' || breaks_before(prev_ch, ch)) && word_chars > 0 {
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
        if ch == ' ' {
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

/// Где ставить многоточие, когда однострочный текст не влезает в отведённую
/// ширину.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Elide {
    /// Обрезать хвост: `/home/master/Pro…`. Поведение по умолчанию.
    #[default]
    End,
    /// Схлопывать середину, сохраняя начало и конец: `/home/…/synthos`. Для
    /// путей (есть `/` или `\`) выкидываются целые сегменты, для остального —
    /// символы. Включает однострочный режим, даже если `max_lines` не задан.
    Middle,
}

/// Сжимает середину строки, пока она не влезет в `available_width`.
fn elide_middle<'a>(
    text: &'a str,
    available_width: f32,
    font_size: f32,
    bold: bool,
    font_family: Option<&str>,
    tm: &dyn crate::widget::context::TextMeasure,
) -> std::borrow::Cow<'a, str> {
    let measure = |s: &str| {
        tm.measure_text_width_styled(s, font_size, s.chars().count(), bold, font_family)
    };
    // Многострочный текст сжимать по середине бессмысленно — берём первую строку.
    let text = match text.split_once('\n') {
        Some((first, _)) => first,
        None => text,
    };
    if !available_width.is_finite() || available_width <= 0.0 || measure(text) <= available_width {
        return std::borrow::Cow::Borrowed(text);
    }
    if let Some(s) = elide_path_segments(text, available_width, &measure) {
        return std::borrow::Cow::Owned(s);
    }
    std::borrow::Cow::Owned(elide_chars(text, available_width, &measure))
}

/// Путь: выкидываем сегменты из середины, сохраняя корень и хвост.
/// `/home/master/Projects/2027/synthos` → `/home/…/2027/synthos` → `…/synthos`.
/// Возвращает `None`, если строка не похожа на путь.
fn elide_path_segments(
    text: &str,
    available_width: f32,
    measure: &dyn Fn(&str) -> f32,
) -> Option<String> {
    let sep = if text.contains('/') {
        '/'
    } else if text.contains('\\') {
        '\\'
    } else {
        return None;
    };
    // Хвостовой разделитель («…/synthos/») в сжатом виде только мешает.
    let trimmed = text.strip_suffix(sep).unwrap_or(text);
    let segments: Vec<&str> = trimmed.split(sep).filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }
    let absolute = trimmed.starts_with(sep);
    let root = if absolute { String::from(sep) } else { String::new() };
    let joined = |parts: &[&str]| parts.join(&sep.to_string());

    // Сначала держим первый сегмент («~», «home»), отдавая хвост по одному.
    // Верхняя граница — `len - 1`: при большем `keep` выкидывать из середины
    // нечего, и кандидат выходит длиннее исходной строки, которая уже не
    // влезла.
    let head = segments[0];
    for keep in (1..segments.len() - 1).rev() {
        let tail = joined(&segments[segments.len() - keep..]);
        let candidate = format!("{root}{head}{sep}{ELLIPSIS}{sep}{tail}");
        if measure(&candidate) <= available_width {
            return Some(candidate);
        }
    }
    // Не влезло даже с одним сегментом хвоста — жертвуем началом.
    for keep in (1..segments.len()).rev() {
        let tail = joined(&segments[segments.len() - keep..]);
        let candidate = format!("{ELLIPSIS}{sep}{tail}");
        if measure(&candidate) <= available_width {
            return Some(candidate);
        }
    }
    // Остался один сегмент, и тот длинный — режем его посимвольно.
    Some(elide_chars(segments[segments.len() - 1], available_width, measure))
}

/// Посимвольное сжатие середины: наращиваем начало и конец от `…`, пока влезает.
fn elide_chars(text: &str, available_width: f32, measure: &dyn Fn(&str) -> f32) -> String {
    if measure(ELLIPSIS) > available_width {
        return ELLIPSIS.to_string();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    // head — сколько символов слева, tail — сколько справа; растим по очереди,
    // начиная с хвоста: он информативнее (имя файла, расширение).
    let mut head = 0usize;
    let mut tail = 0usize;
    loop {
        let grow_tail = tail <= head;
        let (nh, nt) = if grow_tail { (head, tail + 1) } else { (head + 1, tail) };
        if nh + nt >= n {
            break;
        }
        let start = chars[nh].0;
        let end = chars[n - nt].0;
        let candidate = format!("{}{ELLIPSIS}{}", &text[..start], &text[end..]);
        if measure(&candidate) > available_width {
            break;
        }
        head = nh;
        tail = nt;
    }
    if head + tail == 0 {
        return ELLIPSIS.to_string();
    }
    let start = chars[head].0;
    let end = chars[n - tail].0;
    format!("{}{ELLIPSIS}{}", &text[..start], &text[end..])
}

pub struct Text {
    text: String,
    color: Option<Color>,
    font_weight: Option<u16>,
    dark_color: Option<Color>,
    theme: Option<Arc<Mutex<bool>>>,
    max_lines: Option<usize>,
    elide: Elide,
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
            elide: Elide::End,
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

    /// Как сжимать текст, который не влезает. [`Elide::Middle`] подразумевает
    /// одну строку, поэтому отдельный `max_lines(1)` не нужен.
    pub fn elide(mut self, elide: Elide) -> Self {
        self.elide = elide;
        self
    }
}

impl Text {
    /// Собрать элемент. Отдельно от [`Widget::create_element`], чтобы тесты
    /// могли работать с конкретным типом, а не с `Box<dyn Element>`.
    fn element(&self) -> TextElement {
        TextElement {
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
            mss_max_lines: match self.elide {
                Elide::Middle => Some(self.max_lines.unwrap_or(1)),
                Elide::End => self.max_lines,
            },
            mss_elide: self.elide,
            mss_width: None,
            mss_height: None,
            selectable: self.selectable,
            selection: TextSelectionState::new(),
            cursor_pos: 0,
            mouse_selecting: false,
            mss_selection_color: Color::new(0.231, 0.510, 0.965, 0.30),
            last_click_at: None,
            click_count: 0,
        }
    }
}

impl Widget for Text {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(self.element())
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
    mss_elide: Elide,
    mss_width: Option<crate::mss::Dimension>,
    mss_height: Option<crate::mss::Dimension>,

    selectable: bool,
    selection: TextSelectionState,
    cursor_pos: usize,
    mouse_selecting: bool,
    mss_selection_color: Color,
    last_click_at: Option<Instant>,
    click_count: u8,
}

impl TextElement {
    /// Текст в том виде, в каком он попадёт на экран: с применённым
    /// `text-transform`. Мерить надо именно его — заглавные шире строчных,
    /// и по неперевёрнутой строке ширина выходит заниженной.
    fn display_text(&self) -> std::borrow::Cow<'_, str> {
        match self.mss_text_transform {
            Some(crate::mss::fields::TextTransform::Uppercase) => {
                std::borrow::Cow::Owned(self.text.to_uppercase())
            }
            Some(crate::mss::fields::TextTransform::Lowercase) => {
                std::borrow::Cow::Owned(self.text.to_lowercase())
            }
            Some(crate::mss::fields::TextTransform::Capitalize) => {
                let mut result = String::with_capacity(self.text.len());
                let mut cap_next = true;
                for c in self.text.chars() {
                    if cap_next && c.is_alphabetic() {
                        for uc in c.to_uppercase() {
                            result.push(uc);
                        }
                        cap_next = false;
                    } else {
                        result.push(c);
                        if c.is_whitespace() {
                            cap_next = true;
                        }
                    }
                }
                std::borrow::Cow::Owned(result)
            }
            _ => std::borrow::Cow::Borrowed(&self.text),
        }
    }

    /// Ширина строки с учётом разрядки: `FontAtlas::emit_glyph_spaced`
    /// добавляет `letter-spacing` после каждого глифа, последний включительно.
    fn measure_line(&self, line: &str, bold: bool) -> f32 {
        let chars = line.chars().count();
        let base = self
            .text_measure
            .as_ref()
            .map(|tm| {
                tm.measure_text_width_styled(
                    line,
                    self.font_size,
                    chars,
                    bold,
                    self.mss_font_family.as_deref(),
                )
            })
            .unwrap_or_else(|| chars as f32 * self.font_size * 0.65);
        base + self.mss_letter_spacing * chars as f32
    }

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
            let new_max_lines = match widget.elide {
                Elide::Middle => Some(widget.max_lines.unwrap_or(1)),
                Elide::End => widget.max_lines,
            };
            let max_lines_changed =
                new_max_lines.is_some() && self.mss_max_lines != new_max_lines;
            if let Some(n) = new_max_lines {
                self.mss_max_lines = Some(n);
            }
            let elide_changed = self.mss_elide != widget.elide;
            self.mss_elide = widget.elide;
            let mut flags = DirtyFlags::RENDER;
            if text_changed || max_lines_changed || elide_changed {
                flags |= DirtyFlags::LAYOUT;
            }
            self.mark_dirty(flags);
        }
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let pad_h = self.mss_padding_left + self.mss_padding_right;
        let pad_v = self.mss_padding_top + self.mss_padding_bottom;
        let bold = self.mss_font_weight >= 700;
        let measured = self.display_text();
        let text_width = measured
            .split('\n')
            .map(|line| self.measure_line(line, bold))
            .fold(0.0f32, f32::max);
        let line_height = self.mss_line_height
            .map(|lh| lh.resolve(self.font_size))
            .unwrap_or(self.font_size * 1.3);
        // Явные размеры (MSS width/height) резолвим относительно родителя.
        let explicit_w = self.mss_width.and_then(|d| d.resolve_opt(constraints.containing_block.width));
        let explicit_h = self.mss_height.and_then(|d| d.resolve_opt(constraints.containing_block.height));
        let available_width = if let Some(w) = explicit_w {
            (w - pad_h).max(1.0)
        } else if constraints.max_width.is_finite() {
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
        let width = if let Some(w) = explicit_w {
            w
        } else if aligned && constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            (text_width + pad_h).min(constraints.max_width)
        };
        let height = if let Some(h) = explicit_h {
            h
        } else if aligned && constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            natural_height.min(constraints.max_height)
        };
        // Отрисовка/выравнивание внутри явной ширины, а не всего max_width.
        self.max_render_width = explicit_w.unwrap_or(constraints.max_width);
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

        // Та же строка, по которой считалась ширина в `layout` — иначе бокс
        // и надпись расходятся.
        let display_text = self.display_text();

        let bold = self.mss_font_weight >= 700;
        let tm: Option<&dyn crate::widget::context::TextMeasure> =
            self.text_measure.as_deref();
        let display_text: std::borrow::Cow<str> = if let (Elide::Middle, Some(tm)) =
            (self.mss_elide, tm)
        {
            match elide_middle(
                display_text.as_ref(),
                render_width,
                self.font_size,
                bold,
                self.mss_font_family.as_deref(),
                tm,
            ) {
                std::borrow::Cow::Borrowed(s) if s.len() == display_text.len() => display_text,
                other => std::borrow::Cow::Owned(other.into_owned()),
            }
        } else if let Some(n) = self.mss_max_lines {
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

    fn explicit_dimensions(&self, parent_width: f32, parent_height: f32) -> (Option<f32>, Option<f32>) {
        (
            self.mss_width.and_then(|d| d.resolve_opt(parent_width)),
            self.mss_height.and_then(|d| d.resolve_opt(parent_height)),
        )
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
        // Явные размеры (MSS width/height). Раньше Text их игнорировал, из-за чего
        // text-align растягивал текст на весь max_width и центрированный глиф уезжал.
        self.mss_width = style.width();
        self.mss_height = style.height();
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
        // Center заполняет то, что ему дали, но «дали бесконечность» —
        // не размер: в неограниченной по ширине строке (Row меряет
        // не-flex детей с `max_width = INFINITY`) вернуть `inf` значит
        // отправить элемент и всё его содержимое в бесконечность. Тогда
        // размер определяет содержимое — его подставит `measure_center`.
        // Та же развилка, что в `RowElement::layout`.
        let width = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            constraints.min_width.max(0.0)
        };
        let height = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            constraints.min_height.max(0.0)
        };
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

    #[test]
    fn visual_lines_cjk_without_spaces_wraps() {
        let line = "日本語のテキストは空白なしで折り返されます";
        let n = count_visual_lines_via_measure(line, 100.0, 12.0, false, None, &MonoMeasure);
        assert_eq!(n, 3, "22 ideographs at 10 per line, got {n}");
    }

    #[test]
    fn visual_lines_hangul_without_spaces_wraps() {
        let n = count_visual_lines_via_measure("한국어텍스트는띄어쓰기없이", 50.0, 12.0, false, None, &MonoMeasure);
        assert_eq!(n, 3);
    }

    #[test]
    fn visual_lines_latin_word_after_cjk_stays_whole() {
        let n = count_visual_lines_via_measure("日本語ですabcdefghijk", 100.0, 12.0, false, None, &MonoMeasure);
        assert_eq!(n, 3, "expected 日本語です / abcdefghij / k, got {n}");
    }

    #[test]
    fn truncate_cjk_without_spaces_appends_ellipsis() {
        let out = trunc("日本語のテキストです", 40.0, 1);
        assert_eq!(out.as_ref(), "日本語\u{2026}");
    }

    #[test]
    fn truncate_cjk_two_lines_cuts_at_ideograph() {
        let out = trunc("日本語のテキストですよ", 40.0, 2);
        assert_eq!(out.as_ref(), "日本語のテキス\u{2026}");
    }

    #[test]
    fn truncate_latin_word_after_cjk_wraps_as_a_word() {
        let out = trunc("日本語Hello", 50.0, 2);
        assert_eq!(out.as_ref(), "日本語Hell\u{2026}");
        let latin = trunc("abc Hello", 50.0, 2);
        assert_eq!(latin.as_ref(), "abc Hell\u{2026}");
    }

    // --- сжатие середины (Elide::Middle) ---
    // MonoMeasure: 10px на символ, поэтому ширина = 10 * (число символов).

    fn mid(text: &str, width: f32) -> String {
        elide_middle(text, width, 12.0, false, None, &MonoMeasure).into_owned()
    }

    #[test]
    fn elide_middle_keeps_text_that_fits() {
        assert_eq!(mid("~/Projects/2027/synthos", 300.0), "~/Projects/2027/synthos");
    }

    #[test]
    fn elide_middle_drops_path_segments_one_by_one() {
        let path = "~/Projects/2027/synthos";
        // 23 символа → 230px. Ужимаем шаг за шагом.
        assert_eq!(mid(path, 220.0), "~/\u{2026}/2027/synthos");
        assert_eq!(mid(path, 150.0), "~/\u{2026}/synthos");
        assert_eq!(mid(path, 100.0), "\u{2026}/synthos");
    }

    #[test]
    fn elide_middle_keeps_absolute_root_slash() {
        let path = "/home/master/Projects/2027/synthos";
        assert_eq!(mid(path, 250.0), "/home/\u{2026}/2027/synthos");
        assert_eq!(mid(path, 180.0), "/home/\u{2026}/synthos");
    }

    #[test]
    fn elide_middle_ignores_trailing_separator() {
        assert_eq!(mid("/home/master/2027/synthos/", 190.0), "/home/\u{2026}/synthos");
    }

    /// Два сегмента: выкидывать из середины нечего, жертвуем головой.
    #[test]
    fn elide_middle_two_segments_drops_the_head() {
        // 14 символов не влезают в 100px, «…/Projects» — ровно 10.
        assert_eq!(mid("/home/Projects", 100.0), "\u{2026}/Projects");
    }

    #[test]
    fn elide_middle_falls_back_to_chars_for_last_segment() {
        // Один сегмент не влезает целиком — режем его посимвольно.
        let out = mid("/verylongsingledirectoryname", 100.0);
        assert!(out.contains('\u{2026}'), "{out}");
        assert!(out.chars().count() <= 10, "{out}");
    }

    #[test]
    fn elide_middle_on_plain_text_cuts_chars() {
        let out = mid("abcdefghijklmnop", 100.0);
        assert_eq!(out.chars().count(), 10);
        assert!(out.starts_with("abcd") && out.ends_with("mnop"), "{out}");
    }

    #[test]
    fn elide_middle_windows_separator() {
        assert_eq!(
            mid("C:\\Users\\master\\Projects\\synthos", 200.0),
            "C:\\\u{2026}\\synthos"
        );
    }

    #[test]
    fn elide_middle_uses_first_line_only() {
        assert_eq!(mid("~/Projects/2027/synthos\nвторая", 150.0), "~/\u{2026}/synthos");
    }

    #[test]
    fn elide_middle_narrower_than_ellipsis_returns_just_ellipsis() {
        assert_eq!(mid("abcdefgh", 5.0), "\u{2026}");
    }

    // --- Center в неограниченной строке ---

    fn center_layout(max_w: f32, max_h: f32) -> Size {
        let mut el = *Box::new(CenterElement {
            id: ElementId::new(),
            bounds: Rect::zero(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::empty(),
        });
        el.layout(Constraints {
            min_width: 0.0,
            max_width: max_w,
            min_height: 0.0,
            max_height: max_h,
            containing_block: Size::new(100.0, 100.0),
        })
    }

    #[test]
    fn center_fills_finite_constraints() {
        let size = center_layout(200.0, 40.0);
        assert_eq!(size, Size::new(200.0, 40.0));
    }

    /// Row меряет не-flex детей с `max_width = INFINITY`. Вернуть оттуда
    /// бесконечность значит отправить элемент и всё его содержимое в
    /// бесконечность — так пилюли в заголовке окна получали `w = inf` и
    /// просто не рисовались. Размер в этом случае задаёт содержимое.
    #[test]
    fn center_does_not_return_infinity() {
        let size = center_layout(f32::INFINITY, f32::INFINITY);
        assert!(size.width.is_finite() && size.height.is_finite(), "{size:?}");
        assert_eq!(size, Size::new(0.0, 0.0));
    }

    #[test]
    fn center_handles_one_unbounded_axis() {
        let size = center_layout(f32::INFINITY, 40.0);
        assert_eq!(size, Size::new(0.0, 40.0));
    }

    // --- ширина мерится по тому, что рисуется ---

    fn measured_width(text: &str, transform: Option<crate::mss::fields::TextTransform>, spacing: f32) -> f32 {
        let widget = Text::new(text);
        let mut el = widget.element();
        el.text_measure = Some(Arc::new(MonoMeasure));
        el.mss_text_transform = transform;
        el.mss_letter_spacing = spacing;
        el.font_size = 12.0;
        el.layout(Constraints {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
            containing_block: Size::new(500.0, 500.0),
        })
        .width
    }

    /// `MonoMeasure` даёт 10px на символ, поэтому «beta» — 40px.
    #[test]
    fn width_without_transform_is_plain() {
        assert_eq!(measured_width("beta", None, 0.0), 40.0);
    }

    /// Разрядка добавляется после каждого глифа (`emit_glyph_spaced`), и
    /// ширина бокса обязана её учитывать — иначе надпись вылезает за край.
    #[test]
    fn width_includes_letter_spacing() {
        assert_eq!(measured_width("beta", None, 2.0), 40.0 + 8.0);
    }

    /// Ширину «beta» мерить нельзя, когда на экране «BETA»: у MonoMeasure
    /// это видно только через число символов, но в реальном шрифте
    /// заглавные шире, и текст уезжал за правый край пилюли.
    #[test]
    fn width_is_measured_after_text_transform() {
        let mut el = Text::new("beta").element();
        el.text_measure = Some(Arc::new(UppercaseAware));
        el.mss_text_transform = Some(crate::mss::fields::TextTransform::Uppercase);
        el.font_size = 12.0;
        let w = el
            .layout(Constraints {
                min_width: 0.0,
                max_width: f32::INFINITY,
                min_height: 0.0,
                max_height: f32::INFINITY,
                containing_block: Size::new(500.0, 500.0),
            })
            .width;
        assert_eq!(w, 4.0 * 15.0, "мерить надо «BETA», а не «beta»: {w}");
    }

    /// Заглавные шире строчных — как в настоящем шрифте.
    struct UppercaseAware;
    impl TextMeasure for UppercaseAware {
        fn measure_text_width(&self, text: &str, _font_size: f32, char_count: usize) -> f32 {
            text.chars()
                .take(char_count)
                .map(|c| if c.is_uppercase() { 15.0 } else { 10.0 })
                .sum()
        }
        fn hit_test_char(&self, _text: &str, _font_size: f32, x_offset: f32) -> usize {
            (x_offset / 10.0).floor().max(0.0) as usize
        }
    }
}
