use crate::core::{Color, Point, Rect, RectExt, Size};
use crate::input::{CursorIcon, Event, EventResult, Key, MouseButton};
use crate::layout::Constraints;
use crate::mss::{ComputedStyle, Dimension, MssFields, TextAlign, TextDecoration};
use crate::render::{Border, DisplayList};
use crate::widget::context::{EventContext, EventContextExt};
use crate::widget::selection::TextSelectionState;
use crate::widget::{DirtyFlags, Element, ElementId, ElementTree, StyledElement, UpdateContext, Widget};
use crate::widgets::containers::IntoWidget;
use std::any::Any;
use std::sync::Arc;
use crate::core::sync::Mutex;

pub struct TextField {
    pub text: String,
    pub placeholder: String,
    pub disabled: bool,
    pub read_only: bool,
    pub width: Option<Dimension>,
    pub prefix: Option<Box<dyn Widget>>,
    pub suffix: Option<Box<dyn Widget>>,
    pub obscure: bool,
    pub on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    pub on_submit: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    pub submit_on_focus_lost: bool,
    pub on_escape: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    pub on_prefix_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    pub helper_text: Option<String>,
    pub error_text: Option<String>,
    pub input_filter: Option<Arc<dyn Fn(char) -> bool + Send + Sync>>,
    pub on_filter_reject: Option<Arc<Mutex<dyn FnMut(char) + Send>>>,
    /// Забрать клавиатурный фокус при монтировании — для полей, с которых
    /// начинается ввод (палитра команд, диалог поиска).
    pub autofocus: bool,
    /// Показывать при фокусе всплывашку с текстом из буфера обмена: тап по
    /// ней вставляет текст. Также включается из MSS: `clipboard-hint: on`.
    pub clipboard_hint: bool,
}

impl TextField {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            placeholder: String::new(),
            disabled: false,
            read_only: false,
            obscure: false,
            width: None,
            prefix: None,
            suffix: None,
            on_change: None,
            on_submit: None,
            submit_on_focus_lost: false,
            on_escape: None,
            on_prefix_click: None,
            helper_text: None,
            error_text: None,
            input_filter: None,
            on_filter_reject: None,
            autofocus: false,
            clipboard_hint: false,
        }
    }

    pub fn with_text(text: impl Into<String>) -> Self {
        Self::new().text(text)
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn obscure(mut self, obscure: bool) -> Self {
        self.obscure = obscure;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(Dimension::Px(width));
        self
    }

    pub fn prefix<M>(mut self, widget: impl IntoWidget<M>) -> Self {
        self.prefix = Some(widget.into_widget());
        self
    }

    pub fn suffix<M>(mut self, widget: impl IntoWidget<M>) -> Self {
        self.suffix = Some(widget.into_widget());
        self
    }

    pub fn prefix_icon(self, icon: impl Into<String>) -> Self {
        use crate::widgets::visual::icon::Icon;
        use crate::widget::styled::WidgetExt;
        self.prefix(Icon::new(icon).style("icon-size", FONT_SIZE + 2.0))
    }

    pub fn suffix_icon(self, icon: impl Into<String>) -> Self {
        use crate::widgets::visual::icon::Icon;
        use crate::widget::styled::WidgetExt;
        self.suffix(Icon::new(icon).style("icon-size", FONT_SIZE + 2.0))
    }

    pub fn on_change(mut self, callback: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_change = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn on_submit(mut self, callback: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_submit = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn submit_on_focus_lost(mut self, enabled: bool) -> Self {
        self.submit_on_focus_lost = enabled;
        self
    }

    /// Escape в поле. Сам Escape только снимает фокус; хосту inline-правки
    /// (переименование чата, подпись графа) этого мало — ему нужно закрыть
    /// режим правки, не сохраняя текст.
    pub fn on_escape(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_escape = Some(Arc::new(Mutex::new(callback)));
        self
    }

    /// Поле забирает клавиатурный фокус сразу при появлении на экране.
    pub fn autofocus(mut self, on: bool) -> Self {
        self.autofocus = on;
        self
    }

    /// Всплывашка с текстом из буфера обмена при фокусе (тап вставляет
    /// текст). Эквивалент MSS-свойства `clipboard-hint: on`.
    pub fn clipboard_hint(mut self, on: bool) -> Self {
        self.clipboard_hint = on;
        self
    }

    pub fn on_prefix_click(mut self, callback: impl FnMut() + Send + 'static) -> Self {
        self.on_prefix_click = Some(Arc::new(Mutex::new(callback)));
        self
    }

    pub fn helper_text(mut self, text: impl Into<String>) -> Self {
        self.helper_text = Some(text.into());
        self
    }

    pub fn error(mut self, text: impl Into<String>) -> Self {
        self.error_text = Some(text.into());
        self
    }

    pub fn input_filter<F>(mut self, f: F) -> Self
    where
        F: Fn(char) -> bool + Send + Sync + 'static,
    {
        self.input_filter = Some(Arc::new(f));
        self
    }

    pub fn on_filter_reject(mut self, callback: impl FnMut(char) + Send + 'static) -> Self {
        self.on_filter_reject = Some(Arc::new(Mutex::new(callback)));
        self
    }
}

impl Default for TextField {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TextField {
    fn create_element(&self) -> Box<dyn Element> {
        Box::new(TextFieldElement {
            id: ElementId::new(),
            text: self.text.clone(),
            placeholder: self.placeholder.clone(),
            disabled: self.disabled,
            read_only: self.read_only,
            obscure: self.obscure,
            width: self.width,
            prefix_element: self.prefix.as_ref().map(|w| w.create_element()),
            suffix_element: self.suffix.as_ref().map(|w| w.create_element()),
            prefix_width: 0.0,
            suffix_width: 0.0,
            bounds: Rect::zero(),
            hover: false,
            focused: self.autofocus,
            autofocus: self.autofocus,
            focus_request_pending: self.autofocus,
            cursor_pos: self.text.len(),
            selection: TextSelectionState::new(),
            on_change: self.on_change.clone(),
            on_submit: self.on_submit.clone(),
            submit_on_focus_lost: self.submit_on_focus_lost,
            on_escape: self.on_escape.clone(),
            on_prefix_click: self.on_prefix_click.clone(),
            helper_text: self.helper_text.clone(),
            error_text: self.error_text.clone(),
            input_filter: self.input_filter.clone(),
            on_filter_reject: self.on_filter_reject.clone(),
            classes: Vec::new(),
            dirty_flags: DirtyFlags::LAYOUT | DirtyFlags::RENDER,
            mss: MssFields::new(),
            preedit_text: None,
            preedit_cursor: None,
            text_measure: None,
            scroll_offset: 0.0,
            clipboard_hint_prop: self.clipboard_hint,
            hint_text: None,
            hint_visible: false,
            hint_hover: false,
            hint_above: false,
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

pub struct TextFieldElement {
    id: ElementId,
    text: String,
    placeholder: String,
    disabled: bool,
    read_only: bool,
    obscure: bool,
    width: Option<Dimension>,
    prefix_element: Option<Box<dyn Element>>,
    suffix_element: Option<Box<dyn Element>>,
    prefix_width: f32,
    suffix_width: f32,
    bounds: Rect,
    hover: bool,
    focused: bool,
    /// Поле просит фокус при монтировании; заявка забирается деревом один раз.
    autofocus: bool,
    focus_request_pending: bool,
    cursor_pos: usize,
    selection: TextSelectionState,
    on_change: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    on_submit: Option<Arc<Mutex<dyn FnMut(&str) + Send>>>,
    submit_on_focus_lost: bool,
    on_escape: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    on_prefix_click: Option<Arc<Mutex<dyn FnMut() + Send>>>,
    helper_text: Option<String>,
    error_text: Option<String>,
    input_filter: Option<Arc<dyn Fn(char) -> bool + Send + Sync>>,
    on_filter_reject: Option<Arc<Mutex<dyn FnMut(char) + Send>>>,
    classes: Vec<String>,
    dirty_flags: DirtyFlags,
    mss: MssFields,
    text_measure: Option<std::sync::Arc<dyn crate::widget::context::TextMeasure>>,
    scroll_offset: f32,
    preedit_text: Option<String>,
    preedit_cursor: Option<(usize, usize)>,
    /// Подсказка буфера обмена включена через builder (MSS может переопределить).
    clipboard_hint_prop: bool,
    /// Текст из буфера обмена, предлагаемый к вставке.
    hint_text: Option<String>,
    hint_visible: bool,
    hint_hover: bool,
    /// Чип рисуется над полем (снизу не влезает / Android-клавиатура).
    hint_above: bool,
}

const FONT_SIZE: f32 = 14.0;
const TEXT_PADDING: f32 = 12.0;

const HELPER_FONT_SIZE: f32 = 12.0;
const HELPER_LINE_HEIGHT: f32 = 18.0;
const HELPER_TOP_GAP: f32 = 4.0;
const ERROR_COLOR_HEX: &str = "#EF4444";

const OBSCURE_CHAR: &str = "\u{2022}";

const HINT_HEIGHT: f32 = 34.0;
const HINT_GAP: f32 = 6.0;
const HINT_FONT_SIZE: f32 = 13.0;
const HINT_PADDING_X: f32 = 12.0;
const HINT_ICON_SIZE: f32 = 15.0;
const HINT_ICON_GAP: f32 = 7.0;
/// Material Icons `content_paste`.
const HINT_ICON: &str = "\u{E14F}";
/// Максимум символов подсказки в чипе.
const HINT_MAX_CHARS: usize = 80;

impl TextFieldElement {
    fn visual_text(&self) -> String {
        if self.obscure {
            OBSCURE_CHAR.repeat(self.text.chars().count())
        } else {
            self.text.clone()
        }
    }

    fn map_pos_to_visual(&self, byte_pos: usize) -> usize {
        if self.obscure {
            self.text[..byte_pos].chars().count() * OBSCURE_CHAR.len()
        } else {
            byte_pos
        }
    }

    fn trigger_change(&mut self) {
        if let Some(ref callback) = self.on_change {
            if let Ok(mut cb) = callback.lock() {
                cb(&self.text);
            }
        }
    }

    fn accept_char(&self, ch: char) -> bool {
        self.input_filter.as_ref().map(|f| f(ch)).unwrap_or(true)
    }

    fn apply_input_filter(&mut self, s: &str) -> String {
        let Some(ref f) = self.input_filter else { return s.to_string(); };
        let mut out = String::with_capacity(s.len());
        let mut first_rejected: Option<char> = None;
        for ch in s.chars() {
            if f(ch) {
                out.push(ch);
            } else if first_rejected.is_none() {
                first_rejected = Some(ch);
            }
        }
        if let Some(rej) = first_rejected {
            self.trigger_filter_reject(rej);
        }
        out
    }

    fn trigger_filter_reject(&mut self, ch: char) {
        if let Some(ref cb) = self.on_filter_reject {
            if let Ok(mut f) = cb.lock() {
                f(ch);
            }
        }
    }

    fn char_idx_to_byte(&self, char_idx: usize) -> usize {
        self.text.char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    fn hit_test_cursor(&self, rel_x: f32, _ctx: &EventContext) -> usize {
        let font_size = self.mss.font_size_or(FONT_SIZE);
        let vis = self.visual_text();
        if let Some(ref tm) = self.text_measure {
            let char_idx = tm.hit_test_char_styled(&vis, font_size, rel_x, self.mss.font_family.as_deref());
            self.char_idx_to_byte(char_idx)
        } else {
            let char_w = font_size * 0.6;
            let char_idx = (rel_x / char_w).round() as usize;
            self.char_idx_to_byte(char_idx.min(self.text.chars().count()))
        }
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

    fn text_left_offset(&self) -> f32 {
        TEXT_PADDING + if self.prefix_width > 0.0 { self.prefix_width + 6.0 } else { 0.0 }
    }

    fn text_area_width(&self) -> f32 {
        let prefix_gap = if self.prefix_width > 0.0 { self.prefix_width + 6.0 } else { 0.0 };
        let suffix_gap = if self.suffix_width > 0.0 { self.suffix_width + 6.0 } else { 0.0 };
        (self.bounds.size.width - TEXT_PADDING * 2.0 - prefix_gap - suffix_gap).max(0.0)
    }

    fn cursor_x(&self) -> f32 {
        let font_size = self.mss.font_size_or(FONT_SIZE);
        let char_count = self.text[..self.cursor_pos].chars().count();
        let vis = self.visual_text();
        if let Some(ref tm) = self.text_measure {
            let bold = self.mss.font_weight_or(400) >= 700;
            tm.measure_text_width_styled(&vis, font_size, char_count, bold, self.mss.font_family.as_deref())
        } else {
            char_count as f32 * font_size * 0.6
        }
    }

    fn prefix_hit_rect(&self) -> Option<Rect> {
        if self.prefix_width <= 0.0 || self.prefix_element.is_none() {
            return None;
        }
        let field = self.field_rect();
        let x = field.x() + TEXT_PADDING;
        let w = self.prefix_width + 6.0;
        let y = field.y();
        let h = field.size.height;
        Some(Rect::new(Point::new(x, y), Size::new(w, h)))
    }

    fn helper_extra(&self) -> f32 {
        if self.helper_text.is_some() || self.error_text.is_some() {
            HELPER_LINE_HEIGHT + HELPER_TOP_GAP
        } else {
            0.0
        }
    }

    fn field_rect(&self) -> Rect {
        let extra = self.helper_extra();
        Rect::new(
            self.bounds.origin,
            Size::new(self.bounds.size.width, (self.bounds.size.height - extra).max(0.0)),
        )
    }

    fn ensure_cursor_visible(&mut self) {
        let cursor_x = self.cursor_x();
        let text_w = self.text_area_width();
        let margin = 2.0;

        if cursor_x - self.scroll_offset > text_w - margin {
            self.scroll_offset = cursor_x - text_w + margin;
        }
        if cursor_x - self.scroll_offset < margin {
            self.scroll_offset = (cursor_x - margin).max(0.0);
        }
    }

    fn clipboard_hint_enabled(&self) -> bool {
        self.mss.clipboard_hint.unwrap_or(self.clipboard_hint_prop)
    }

    /// Текст чипа: одна строка, обрезанная до [`HINT_MAX_CHARS`].
    fn hint_display_text(&self) -> String {
        let raw = self.hint_text.as_deref().unwrap_or("");
        let trimmed = raw.trim();
        let mut s: String = trimmed
            .chars()
            .take(HINT_MAX_CHARS)
            .map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c })
            .collect();
        if trimmed.chars().count() > HINT_MAX_CHARS {
            s.push('…');
        }
        s
    }

    fn hint_chip_rect(&self) -> Rect {
        let field = self.field_rect();
        let display = self.hint_display_text();
        let text_w = if let Some(ref tm) = self.text_measure {
            tm.measure_text_width(&display, HINT_FONT_SIZE, display.chars().count())
        } else {
            display.chars().count() as f32 * HINT_FONT_SIZE * 0.6
        };
        let intrinsic = HINT_PADDING_X * 2.0 + HINT_ICON_SIZE + HINT_ICON_GAP + text_w;
        let w = intrinsic.min(field.size.width).max(HINT_HEIGHT);
        let y = if self.hint_above {
            field.y() - HINT_GAP - HINT_HEIGHT
        } else {
            field.y() + field.size.height + HINT_GAP
        };
        Rect::new(Point::new(field.x(), y), Size::new(w, HINT_HEIGHT))
    }

    /// Границы overlay для маршрутизации событий: поле + чип.
    fn hint_overlay_bounds(&self) -> Rect {
        let field = self.field_rect();
        let chip = self.hint_chip_rect();
        let x0 = field.x().min(chip.x());
        let y0 = field.y().min(chip.y());
        let x1 = (field.x() + field.size.width).max(chip.x() + chip.size.width);
        let y1 = (field.y() + field.size.height).max(chip.y() + chip.size.height);
        Rect::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0))
    }

    /// Обновляет подсказку буфера обмена при получении фокуса.
    fn refresh_clipboard_hint(&mut self, ctx: &mut EventContext) {
        if !self.clipboard_hint_enabled() || self.read_only || self.obscure {
            return;
        }
        // Веб: readText() асинхронный — кэш подтянется, и FocusGained
        // повторится из AppHandler::update() уже со свежим текстом.
        #[cfg(target_arch = "wasm32")]
        crate::clipboard::request_refresh();
        self.hint_text = ctx
            .paste_from_clipboard()
            .filter(|t| !t.trim().is_empty() && *t != self.text);
        self.hint_visible = self.hint_text.is_some();
        self.hint_hover = false;
        if self.hint_visible {
            let field = self.field_rect();
            // Координаты поля глобальные, viewport_size — размер layout-области:
            // сравнение ведётся в границах `[origin .. origin + size]`.
            let origin = crate::viewport::viewport_origin();
            let below_fits = field.y() + field.size.height + HINT_GAP + HINT_HEIGHT
                <= origin.y + ctx.viewport_size().height;
            let above_fits = field.y() - origin.y >= HINT_GAP + HINT_HEIGHT;
            // Android: над полем, чтобы не уйти под экранную клавиатуру.
            self.hint_above = if cfg!(target_os = "android") {
                above_fits
            } else {
                !below_fits && above_fits
            };
        }
    }

    /// Вставляет текст подсказки в позицию курсора (как обычную вставку).
    fn insert_hint_text(&mut self, ctx: &mut EventContext) {
        if let Some(text) = self.hint_text.take() {
            let text = text.replace('\n', " ").replace('\r', "");
            let filtered = self.apply_input_filter(&text);
            if !filtered.is_empty() || self.input_filter.is_none() {
                self.selection.replace_selection(&mut self.text, &mut self.cursor_pos, &filtered);
                self.trigger_change();
                self.ensure_cursor_visible();
            }
        }
        self.hint_visible = false;
        self.hint_hover = false;
        ctx.request_paint();
    }

    fn dismiss_hint(&mut self, ctx: &mut EventContext) {
        if self.hint_visible {
            self.hint_visible = false;
            self.hint_text = None;
            self.hint_hover = false;
            ctx.request_paint();
        }
    }
}

impl Element for TextFieldElement {
    fn update(&mut self, widget: &dyn Widget, _ctx: &mut UpdateContext) {
        if let Some(tf) = widget.as_any().downcast_ref::<TextField>() {
            self.placeholder = tf.placeholder.clone();
            self.disabled = tf.disabled;
            self.read_only = tf.read_only;
            self.obscure = tf.obscure;
            self.width = tf.width;
            if tf.text != self.text {
                self.text = tf.text.clone();
                self.cursor_pos = self.text.len().min(self.cursor_pos);
                self.selection.clear();
            }
            self.prefix_element = tf.prefix.as_ref().map(|w| w.create_element());
            self.suffix_element = tf.suffix.as_ref().map(|w| w.create_element());
            self.on_change = tf.on_change.clone();
            self.on_submit = tf.on_submit.clone();
            self.submit_on_focus_lost = tf.submit_on_focus_lost;
            self.on_escape = tf.on_escape.clone();
            self.on_prefix_click = tf.on_prefix_click.clone();
            self.helper_text = tf.helper_text.clone();
            self.error_text = tf.error_text.clone();
            self.input_filter = tf.input_filter.clone();
            self.on_filter_reject = tf.on_filter_reject.clone();
            self.clipboard_hint_prop = tf.clipboard_hint;
            if tf.autofocus && !self.autofocus {
                self.focus_request_pending = true;
                self.focused = true;
            }
            self.autofocus = tf.autofocus;
            self.mark_dirty(DirtyFlags::LAYOUT | DirtyFlags::RENDER);
        }
    }

    fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_request_pending)
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let field_height = self.mss.height.map(|d| d.resolve(constraints.max_height))
            .unwrap_or(40.0_f32.max(constraints.min_height))
            .clamp(constraints.min_height, constraints.max_height);
        let width = self.width.or(self.mss.width)
            .map(|d| d.resolve(constraints.max_width))
            .unwrap_or(constraints.max_width)
            .clamp(constraints.min_width, constraints.max_width);

        let extra = if self.helper_text.is_some() || self.error_text.is_some() {
            HELPER_LINE_HEIGHT + HELPER_TOP_GAP
        } else {
            0.0
        };
        let total_height = (field_height + extra).clamp(constraints.min_height, constraints.max_height);

        let fg = self.mss.color.unwrap_or(Color::from_hex("#374151"));
        let affix_fg = fg.with_alpha(0.6);
        let mut inherited = crate::mss::ComputedStyle::with_color(affix_fg);
        if let Some(icon_sz) = self.mss.icon_size {
            inherited.set("font-size", crate::mss::StyleValue::from(icon_sz));
        }

        let inner_constraints = Constraints::loose(Size::new(field_height, field_height));
        if let Some(ref mut el) = self.prefix_element {
            el.apply_computed_style(&inherited);
            let sz = el.layout(inner_constraints);
            self.prefix_width = sz.width;
        } else {
            self.prefix_width = 0.0;
        }
        if let Some(ref mut el) = self.suffix_element {
            el.apply_computed_style(&inherited);
            let sz = el.layout(inner_constraints);
            self.suffix_width = sz.width;
        } else {
            self.suffix_width = 0.0;
        }

        self.bounds = Rect::new(Point::zero(), Size::new(width, total_height));
        Size::new(width, total_height)
    }

    fn build_display_list(&self, list: &mut DisplayList, _clip: Rect) {
        let primary = self.mss.accent_color.unwrap_or(Color::from_hex("#3B82F6"));

        let field = self.field_rect();
        let resolve_base = field.size.width.min(field.size.height);
        let radius = self.mss.border_radius_uniform(resolve_base, 6.0);

        let (bg_color, fg, border_color, draw_bw) = if self.mss.has_mss_styles {
            let target = self.mss.target_props(self.hover, false, self.focused, false);
            let mut bg = self.mss.effective_bg(&target, Color::WHITE);
            let mut fg = self.mss.effective_fg(&target, Color::from_hex("#374151"));
            let mut bc = self.mss.effective_border_color(&target, Color::TRANSPARENT);
            // Явная ширина из MSS уважается и в фокусе: утолщение до 2px —
            // это фокус-подсказка для полей без своего стиля, а не закон.
            let draw_bw = match self.mss.border_width {
                Some(w) => w,
                None => if self.focused { 2.0 } else { 1.0 },
            };
            if self.disabled {
                bg = bg.with_alpha(bg.a * 0.5);
                fg = fg.with_alpha(fg.a * 0.5);
                bc = bc.with_alpha(bc.a * 0.5);
            }
            (bg, fg, bc, draw_bw)
        } else {
            let has_custom_styles = self.mss.background_color.is_some() || self.mss.color.is_some() || self.mss.border_color.is_some();
            let default_border = if has_custom_styles { Color::TRANSPARENT } else { Color::from_hex("#D1D5DB") };
            let bg = self.mss.background_color.unwrap_or(Color::WHITE);
            let fg = self.mss.color.unwrap_or(Color::from_hex("#374151"));
            let border_base = self.mss.border_color.unwrap_or(default_border);
            let (bg_color, border_color) = if self.disabled {
                (bg.darken(0.1), border_base)
            } else if self.focused {
                let focus_color = if border_base.a < 0.01 { primary } else { primary.lerp(&border_base, 0.3) };
                (bg, focus_color)
            } else {
                (bg, border_base)
            };
            // Явная ширина из MSS уважается и в фокусе: утолщение до 2px —
            // это фокус-подсказка для полей без своего стиля, а не закон.
            let draw_bw = match self.mss.border_width {
                Some(w) => w,
                None => if self.focused { 2.0 } else { 1.0 },
            };
            (bg_color, fg, border_color, draw_bw)
        };
        let (border_color, draw_bw) = if self.error_text.is_some() {
            (Color::from_hex(ERROR_COLOR_HEX), draw_bw.max(2.0))
        } else {
            (border_color, draw_bw)
        };
        if draw_bw > 0.0 && border_color.a > 0.01 {
            list.push_rect_bordered(
                field,
                bg_color,
                [radius; 4],
                Border { width: draw_bw, color: border_color },
            );
        } else {
            list.push_rect(field, bg_color, [radius; 4]);
        }

        let prefix_gap = if self.prefix_width > 0.0 { self.prefix_width + 6.0 } else { 0.0 };
        let _suffix_gap = if self.suffix_width > 0.0 { self.suffix_width + 6.0 } else { 0.0 };
        let text_left = field.x() + TEXT_PADDING + prefix_gap;
        let text_w = self.text_area_width();

        if let Some(ref el) = self.prefix_element {
            let prefix_x = field.x() + TEXT_PADDING;
            let prefix_y = field.y() + (field.size.height - el.bounds().size.height) / 2.0;
            list.push_transform(crate::core::Transform::translation(prefix_x, prefix_y));
            el.build_display_list(list, _clip);
            list.pop_transform();
        }

        if let Some(ref el) = self.suffix_element {
            let suffix_x = field.x() + field.size.width - TEXT_PADDING - self.suffix_width;
            let suffix_y = field.y() + (field.size.height - el.bounds().size.height) / 2.0;
            list.push_transform(crate::core::Transform::translation(suffix_x, suffix_y));
            el.build_display_list(list, _clip);
            list.pop_transform();
        }

        let font_size = self.mss.font_size_or(FONT_SIZE);
        let font_weight = self.mss.font_weight_or(400);
        let text_height = font_size + 4.0;
        let text_y = field.y() + (field.size.height - text_height) / 2.0;

        let clip_rect = Rect::new(
            Point::new(text_left, field.y()),
            Size::new(text_w, field.size.height),
        );
        list.push_clip(clip_rect);

        let scrolled_text_left = text_left - self.scroll_offset;
        let text_rect = Rect::new(
            Point::new(scrolled_text_left, text_y),
            Size::new(f32::INFINITY, text_height),
        );

        let placeholder_color = fg.with_alpha(0.5);

        if self.text.is_empty() && !self.focused {
            let placeholder_rect = Rect::new(
                Point::new(text_left, text_y),
                Size::new(text_w, text_height),
            );
            list.push_text_styled(&self.placeholder, placeholder_rect, placeholder_color, font_size,
                TextAlign::DEFAULT, TextDecoration::None, font_weight, self.mss.font_family.clone());
        } else {
            let vis = self.visual_text();

            if let Some((sel_start, sel_end)) = self.selection.range(self.cursor_pos) {
                let vis_sel_start = self.map_pos_to_visual(sel_start);
                let vis_sel_end = self.map_pos_to_visual(sel_end);
                let sel_color = self.mss.selection_color_or_default();
                list.push_text_selection_styled(
                    &vis,
                    vis_sel_start,
                    vis_sel_end,
                    scrolled_text_left,
                    text_y - 1.0,
                    text_height + 2.0,
                    font_size,
                    sel_color,
                    self.mss.font_family.clone(),
                );
            }

            let text_color = if self.disabled { placeholder_color } else { fg };

            let display_text;
            let preedit_byte_start;
            let preedit_byte_end;
            if !self.obscure {
                if let Some(ref preedit) = self.preedit_text {
                    let mut t = vis.clone();
                    let insert_pos = self.cursor_pos.min(t.len());
                    t.insert_str(insert_pos, preedit);
                    preedit_byte_start = insert_pos;
                    preedit_byte_end = insert_pos + preedit.len();
                    display_text = t;
                } else {
                    preedit_byte_start = 0;
                    preedit_byte_end = 0;
                    display_text = vis.clone();
                }
            } else {
                preedit_byte_start = 0;
                preedit_byte_end = 0;
                display_text = vis.clone();
            }

            list.push_text_styled(&display_text, text_rect, text_color, font_size,
                TextAlign::DEFAULT, TextDecoration::None, font_weight, self.mss.font_family.clone());

            if !self.obscure && self.preedit_text.is_some() && preedit_byte_end > preedit_byte_start {
                if let Some(ref tm) = self.text_measure {
                    let pre_preedit = &display_text[..preedit_byte_start];
                    let preedit_str = &display_text[preedit_byte_start..preedit_byte_end];
                    let pre_w = tm.measure_text_width(pre_preedit, font_size, pre_preedit.chars().count());
                    let preedit_w = tm.measure_text_width(preedit_str, font_size, preedit_str.chars().count());
                    let underline_y = text_y + text_height - 1.0;
                    let underline_rect = Rect::new(
                        Point::new(scrolled_text_left + pre_w, underline_y),
                        Size::new(preedit_w, 1.0),
                    );
                    list.push_rect(underline_rect, text_color, [0.0; 4]);
                }
            }

            if self.focused && !self.disabled {
                let cursor_color = self.mss.caret_color_or(primary);
                let vis_cursor = self.map_pos_to_visual(self.cursor_pos);
                list.push_text_cursor_styled(
                    &vis,
                    vis_cursor,
                    scrolled_text_left,
                    text_y - 1.0,
                    text_height + 2.0,
                    font_size,
                    font_weight,
                    cursor_color,
                    self.mss.font_family.clone(),
                );
            }
        }

        list.pop_clip();

        if self.error_text.is_some() || self.helper_text.is_some() {
            let helper_x = field.x() + TEXT_PADDING;
            let helper_y = field.y() + field.size.height + HELPER_TOP_GAP;
            let helper_w = (self.bounds.size.width - TEXT_PADDING * 2.0).max(0.0);
            let helper_rect = Rect::new(
                Point::new(helper_x, helper_y),
                Size::new(helper_w, HELPER_LINE_HEIGHT),
            );
            let (text, color) = if let Some(ref e) = self.error_text {
                (e.as_str(), Color::from_hex(ERROR_COLOR_HEX))
            } else {
                (self.helper_text.as_deref().unwrap_or(""), fg.with_alpha(0.6))
            };
            list.push_text_styled(
                text,
                helper_rect,
                color,
                HELPER_FONT_SIZE,
                TextAlign::DEFAULT,
                TextDecoration::None,
                400,
                self.mss.font_family.clone(),
            );
        }

        // Чип подсказки буфера обмена: рисуется в overlay-слое поверх всего.
        if self.focused && self.hint_visible && self.hint_text.is_some() {
            let chip = self.hint_chip_rect();
            let chip_radius = 10.0_f32.min(chip.size.height / 2.0);
            let chip_bg = if self.hint_hover { bg_color.darken(0.05) } else { bg_color };
            let chip_border = if border_color.a > 0.01 {
                border_color
            } else {
                fg.with_alpha(0.15)
            };

            list.begin_overlay();
            list.push_shadow(
                chip,
                Color::new(0.0, 0.0, 0.0, 0.18),
                10.0,
                (0.0, 3.0),
                [chip_radius; 4],
            );
            list.push_rect_bordered(
                chip,
                chip_bg,
                [chip_radius; 4],
                Border { width: 1.0, color: chip_border },
            );

            let icon_rect = Rect::new(
                Point::new(chip.x() + HINT_PADDING_X, chip.y()),
                Size::new(HINT_ICON_SIZE + 2.0, chip.size.height),
            );
            list.push_text_centered(HINT_ICON, icon_rect, primary, HINT_ICON_SIZE);

            let text_x = chip.x() + HINT_PADDING_X + HINT_ICON_SIZE + HINT_ICON_GAP;
            let hint_text_h = HINT_FONT_SIZE + 4.0;
            let hint_rect = Rect::new(
                Point::new(text_x, chip.y() + (chip.size.height - hint_text_h) / 2.0),
                Size::new((chip.x() + chip.size.width - HINT_PADDING_X - text_x).max(0.0), hint_text_h),
            );
            list.push_text_styled_singleline(
                &self.hint_display_text(),
                hint_rect,
                fg,
                HINT_FONT_SIZE,
                TextAlign::DEFAULT,
                TextDecoration::None,
                400,
                self.mss.font_family.clone(),
            );
            list.end_overlay();
        }
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut EventContext) -> EventResult {
        if self.disabled {
            return EventResult::Ignored;
        }

        match event {
            Event::MouseMove(pos) => {
                if self.hint_visible {
                    let over_chip = self.hint_chip_rect().contains(*pos);
                    if over_chip != self.hint_hover {
                        self.hint_hover = over_chip;
                        ctx.request_paint();
                    }
                    if over_chip {
                        ctx.set_cursor(CursorIcon::Pointer);
                        return EventResult::Handled;
                    }
                }
                let was_hover = self.hover;
                self.hover = self.field_rect().contains(*pos);
                if self.selection.mouse_selecting && self.focused {
                    let text_x = self.bounds.x() + self.text_left_offset();
                    let rel_x = (pos.x - text_x + self.scroll_offset).max(0.0);
                    self.cursor_pos = self.hit_test_cursor(rel_x, ctx);
                    self.ensure_cursor_visible();
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                if self.hover {
                    let over_prefix = self.on_prefix_click.is_some()
                        && self.prefix_hit_rect().map(|r| r.contains(*pos)).unwrap_or(false);
                    if over_prefix {
                        ctx.set_cursor(CursorIcon::Pointer);
                    } else {
                        ctx.set_cursor(CursorIcon::Text);
                    }
                }
                if self.hover != was_hover {
                    self.mss.start_transition_to(self.hover, false, self.focused, false);
                    ctx.request_paint();
                }
                if self.hover {
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::FocusGained => {
                self.focused = true;
                self.mss.start_transition_to(self.hover, false, true, false);
                ctx.set_virtual_keyboard_visible(true);
                ctx.set_numeric_keyboard(false);
                ctx.set_focused_text(self.text.clone());
                self.refresh_clipboard_hint(ctx);

                ctx.request_paint();
                EventResult::Handled
            }
            Event::FocusLost => {
                self.focused = false;
                self.mss.start_transition_to(self.hover, false, false, false);
                ctx.set_virtual_keyboard_visible(false);
                self.dismiss_hint(ctx);
                self.selection.clear();
                if self.submit_on_focus_lost {
                    if let Some(ref callback) = self.on_submit {
                        if let Ok(mut f) = callback.lock() {
                            f(&self.text);
                        }
                    }
                }
                ctx.request_paint();
                EventResult::Handled
            }
            Event::MouseDown { button, position } => {
                if *button == MouseButton::Left && self.hint_visible {
                    if self.hint_chip_rect().contains(*position) {
                        self.insert_hint_text(ctx);
                        return EventResult::Handled;
                    }
                    // Клик мимо чипа и мимо поля — подсказка больше не нужна.
                    // Клик в само поле подсказку не прячет: FocusGained
                    // приходит раньше MouseDown, иначе она исчезала бы сразу.
                    if !self.field_rect().contains(*position) {
                        self.dismiss_hint(ctx);
                    }
                }
                if *button == MouseButton::Left && self.on_prefix_click.is_some() {
                    if let Some(rect) = self.prefix_hit_rect() {
                        if rect.contains(*position) {
                            if let Some(ref cb) = self.on_prefix_click {
                                if let Ok(mut f) = cb.lock() {
                                    f();
                                }
                            }
                            ctx.request_paint();
                            return EventResult::Handled;
                        }
                    }
                }
                if *button == MouseButton::Left && self.field_rect().contains(*position) {
                    self.focused = true;
                    let text_x = self.bounds.x() + self.text_left_offset();
                    let rel_x = (position.x - text_x + self.scroll_offset).max(0.0);
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
                } else if self.focused {
                    self.focused = false;
                    self.selection.clear();
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
            Event::DoubleClick { button, position } => {
                if *button == MouseButton::Left && self.field_rect().contains(*position) {
                    self.focused = true;
                    let text_x = self.bounds.x() + self.text_left_offset();
                    let rel_x = (position.x - text_x + self.scroll_offset).max(0.0);
                    let click_pos = self.hit_test_cursor(rel_x, ctx);
                    let word_end = self.selection.select_word(&self.text, click_pos);
                    self.cursor_pos = word_end;
                    ctx.request_paint();
                    return EventResult::Handled;
                }
                EventResult::Ignored
            }
            Event::KeyDown(key) => {
                if !self.focused {
                    return EventResult::Ignored;
                }
                // Начали работать с клавиатуры — подсказка буфера не нужна.
                self.dismiss_hint(ctx);

                let shift = ctx.modifiers.shift;
                let ctrl = ctx.modifiers.ctrl;

                if ctrl && matches!(key, Key::A) {
                    self.selection.select_all();
                    self.cursor_pos = self.text.len();
                    self.ensure_cursor_visible();
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                if ctrl && matches!(key, Key::C) {
                    if !self.obscure {
                        if let Some(selected) = self.selection.selected_text(&self.text, self.cursor_pos) {
                            ctx.copy_to_clipboard(selected);
                        }
                    }
                    return EventResult::Handled;
                }

                if ctrl && matches!(key, Key::X) && !self.read_only {
                    if !self.obscure {
                        if let Some(selected) = self.selection.selected_text(&self.text, self.cursor_pos) {
                            ctx.copy_to_clipboard(selected);
                        }
                    }
                    if self.selection.delete_selection(&mut self.text, &mut self.cursor_pos) {
                        self.trigger_change();
                        self.ensure_cursor_visible();
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }

                if ctrl && matches!(key, Key::V) && !self.read_only {
                    if let Some(paste_text) = ctx.paste_from_clipboard() {
                        let paste_text = paste_text.replace('\n', " ").replace('\r', "");
                        let filtered = self.apply_input_filter(&paste_text);
                        if !filtered.is_empty()
                            || (filtered.is_empty() && self.input_filter.is_none())
                        {
                            self.selection.replace_selection(&mut self.text, &mut self.cursor_pos, &filtered);
                            self.trigger_change();
                            self.ensure_cursor_visible();
                        }
                        ctx.request_paint();
                    }
                    return EventResult::Handled;
                }

                if self.read_only {
                    match key {
                        Key::Left | Key::Right | Key::Home | Key::End => {
                        }
                        _ => return EventResult::Ignored,
                    }
                }

                match key {
                    Key::Backspace => {
                        if self.selection.delete_selection(&mut self.text, &mut self.cursor_pos) {
                            self.trigger_change();
                        } else if self.cursor_pos > 0 {
                            let prev = self.text[..self.cursor_pos]
                                .char_indices()
                                .next_back()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            self.text.remove(prev);
                            self.cursor_pos = prev;
                            self.trigger_change();
                        }
                        self.ensure_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Delete => {
                        if self.selection.delete_selection(&mut self.text, &mut self.cursor_pos) {
                            self.trigger_change();
                        } else if self.cursor_pos < self.text.len() {
                            if self.text.is_char_boundary(self.cursor_pos) {
                                self.text.remove(self.cursor_pos);
                                self.trigger_change();
                            }
                        }
                        self.ensure_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Left => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_pos);
                            self.move_cursor_left();
                        } else {
                            if self.selection.has_selection(self.cursor_pos) {
                                if let Some((start, _)) = self.selection.range(self.cursor_pos) {
                                    self.cursor_pos = start;
                                }
                                self.selection.clear();
                            } else {
                                self.move_cursor_left();
                            }
                        }
                        self.ensure_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Right => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_pos);
                            self.move_cursor_right();
                        } else {
                            if self.selection.has_selection(self.cursor_pos) {
                                if let Some((_, end)) = self.selection.range(self.cursor_pos) {
                                    self.cursor_pos = end;
                                }
                                self.selection.clear();
                            } else {
                                self.move_cursor_right();
                            }
                        }
                        self.ensure_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Home => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_pos);
                        } else {
                            self.selection.clear();
                        }
                        self.cursor_pos = 0;
                        self.ensure_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::End => {
                        if shift {
                            self.selection.extend_or_start(self.cursor_pos);
                        } else {
                            self.selection.clear();
                        }
                        self.cursor_pos = self.text.len();
                        self.ensure_cursor_visible();
                        ctx.request_paint();
                        EventResult::Handled
                    }
                    Key::Enter => {
                        if let Some(ref callback) = self.on_submit {
                            if let Ok(mut cb) = callback.lock() {
                                cb(&self.text);
                            }
                        }
                        EventResult::Handled
                    }
                    Key::Escape => {
                        self.focused = false;
                        self.selection.clear();
                        if let Some(ref callback) = self.on_escape {
                            if let Ok(mut cb) = callback.lock() {
                                cb();
                            }
                        }
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
                self.dismiss_hint(ctx);

                if !self.accept_char(*ch) {
                    self.trigger_filter_reject(*ch);
                    ctx.request_paint();
                    return EventResult::Handled;
                }

                let mut ch_buf = [0u8; 4];
                let ch_str = ch.encode_utf8(&mut ch_buf);
                self.selection.replace_selection(&mut self.text, &mut self.cursor_pos, ch_str);
                self.preedit_text = None;
                self.preedit_cursor = None;
                self.trigger_change();
                self.ensure_cursor_visible();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImeReplace(text) => {
                if !self.focused || self.read_only {
                    return EventResult::Ignored;
                }
                self.dismiss_hint(ctx);
                self.text = self.apply_input_filter(text);
                self.cursor_pos = self.text.len();
                self.selection.clear();
                self.preedit_text = None;
                self.preedit_cursor = None;
                self.trigger_change();
                self.ensure_cursor_visible();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImeCommit(text) => {
                if !self.focused || self.read_only {
                    return EventResult::Ignored;
                }
                self.dismiss_hint(ctx);
                self.preedit_text = None;
                self.preedit_cursor = None;
                let filtered = self.apply_input_filter(text);
                self.selection.replace_selection(&mut self.text, &mut self.cursor_pos, &filtered);
                self.trigger_change();
                self.ensure_cursor_visible();
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImePreedit { text, cursor } => {
                if !self.focused {
                    return EventResult::Ignored;
                }
                if text.is_empty() {
                    self.preedit_text = None;
                    self.preedit_cursor = None;
                } else {
                    self.preedit_text = Some(text.clone());
                    self.preedit_cursor = *cursor;
                }
                ctx.request_paint();
                EventResult::Handled
            }
            Event::ImeEnabled | Event::ImeDisabled => {
                if self.focused { EventResult::Handled } else { EventResult::Ignored }
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

    fn set_classes(&mut self, classes: Vec<String>) {
        self.classes = classes;
        self.mark_dirty(DirtyFlags::RENDER);
    }

    fn get_classes(&self) -> &[String] {
        &self.classes
    }

    fn element_type_name(&self) -> &str { "TextField" }

    /// Декларативный overlay для чипа подсказки буфера обмена: события в его
    /// границах маршрутизируются полю, а `sync_overlay_stack` держит границы
    /// актуальными при любом реflow (например, при появлении клавиатуры).
    fn overlay_request(&self) -> Option<(Rect, bool)> {
        if self.focused && self.hint_visible {
            Some((self.hint_overlay_bounds(), false))
        } else {
            None
        }
    }

    fn animate(&mut self, dt: std::time::Duration) -> bool {
        self.mss.transition.tick(dt.as_secs_f32())
    }

    fn needs_repaint(&self) -> bool {
        self.mss.transition.is_animating()
    }

    fn reset_mss_styles(&mut self) { self.mss.reset(); }
    fn mss(&self) -> Option<&crate::mss::MssFields> { Some(&self.mss) }
    fn apply_computed_style(&mut self, style: &ComputedStyle) {
        self.mss.apply(style);
        self.apply_style(style);
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
                disabled: self.disabled,
                focused: self.focused,
                ..Default::default()
            },
            properties: crate::a11y::NodeProperties {
                value: if self.text.is_empty() || self.obscure { None } else { Some(self.text.clone()) },
                placeholder: if self.placeholder.is_empty() { None } else { Some(self.placeholder.clone()) },
                ..Default::default()
            },
        })
    }
}

impl StyledElement for TextFieldElement {
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
