//! Экранная клавиатура для пульта (Android TV, приставки) и мыши.
//!
//! Раскладки настраиваются ([`KeyboardLayout`] — строки [`KeyDef`] с
//! действием и шириной в юнитах), состояние живёт в сигналах
//! ([`KeyboardState`]) и переживает пересборки. Клавиатура сама обрабатывает
//! D-pad, пока [`KeyboardState::active`] = true; на краю сетки (вверх с
//! первого ряда, вниз с последнего, влево/вправо с крайних клавиш) событие
//! **не** поглощается — родительский `EventHook` переводит фокус в другую
//! зону экрана (поле, список результатов, кнопку).
//!
//! Вид задаётся MSS-классами (см. [`DEFAULT_MSS`]): `.osk`, `.osk-row`,
//! `.osk-key`, `.osk-key-focused`, `.osk-key-action`, `.osk-key-w{n}`,
//! `.osk-key-text`; поле ввода — [`input_field`]: `.osk-field`,
//! `.osk-field-focused`, `.osk-field-text`, `.osk-field-placeholder`,
//! `.osk-field-caret`.
//!
//! ```ignore
//! let kb = KeyboardState::new("");
//! on_screen_keyboard(kb, KeyboardLayout::text_ru_en(), "Найти")
//!     .on_change(move |q| query.set(q.to_string()))
//!     .on_submit(move |q| run_search(q))
//! ```

use std::sync::Arc;

use crate::signal::{use_signal, RwSignal};
use crate::widget::{Text, Widget, WidgetExt};
use crate::widgets::containers::gesture_detector::GestureDetector;
use crate::widgets::input::event_hook::{EventHook, RemoteKey};
use crate::widgets::{Column, CrossAxisAlignment, DecoratedBox, Reactive, Row};

/// Что делает клавиша.
#[derive(Clone, Debug, PartialEq)]
pub enum KeyAction {
    /// Вставить символ(ы). Под Shift первая буква поднимается в верхний регистр.
    Char(String),
    Space,
    Backspace,
    /// Очистить всё поле.
    Clear,
    /// Подтвердить ввод (`on_submit`).
    Submit,
    /// Переключить раскладку по индексу в списке раскладок.
    Layout(usize),
    /// Следующая раскладка по кругу.
    NextLayout,
    /// Одноразовый верхний регистр для следующей буквы.
    Shift,
}

#[derive(Clone, Debug)]
pub struct KeyDef {
    pub label: String,
    pub action: KeyAction,
    /// Ширина в юнитах сетки (1 — обычная буква). Классы `.osk-key-w2`…`w6`.
    pub width: u8,
}

impl KeyDef {
    pub fn ch(c: char) -> Self {
        Self {
            label: c.to_string(),
            action: KeyAction::Char(c.to_string()),
            width: 1,
        }
    }

    pub fn text(label: impl Into<String>, insert: impl Into<String>, width: u8) -> Self {
        Self {
            label: label.into(),
            action: KeyAction::Char(insert.into()),
            width,
        }
    }

    pub fn action(label: impl Into<String>, action: KeyAction, width: u8) -> Self {
        Self {
            label: label.into(),
            action,
            width,
        }
    }

    fn is_action(&self) -> bool {
        !matches!(self.action, KeyAction::Char(_))
    }
}

#[derive(Clone, Debug)]
pub struct KeyboardLayout {
    /// Подпись, которой другие раскладки ссылаются на эту (клавиша «АБВ»).
    pub name: String,
    pub rows: Vec<Vec<KeyDef>>,
}

impl KeyboardLayout {
    pub fn new(name: impl Into<String>, rows: Vec<Vec<KeyDef>>) -> Self {
        Self {
            name: name.into(),
            rows,
        }
    }

    fn row_of_chars(s: &str) -> Vec<KeyDef> {
        s.chars().map(KeyDef::ch).collect()
    }

    /// Нижний ряд: переключатели раскладок, пробел, стереть, подтвердить.
    /// `switches` — (подпись, индекс раскладки).
    fn bottom_row(switches: &[(&str, usize)], submit_label: &str) -> Vec<KeyDef> {
        let mut row: Vec<KeyDef> = switches
            .iter()
            .map(|(l, i)| KeyDef::action(*l, KeyAction::Layout(*i), 2))
            .collect();
        row.push(KeyDef::action("Aa", KeyAction::Shift, 1));
        row.push(KeyDef::action("Пробел", KeyAction::Space, 4));
        row.push(KeyDef::action("Стереть", KeyAction::Backspace, 2));
        row.push(KeyDef::action(submit_label, KeyAction::Submit, 2));
        row
    }

    /// Русская раскладка (индексы: 0 — эта, 1 — латиница, 2 — цифры).
    pub fn cyrillic(submit: &str) -> Self {
        Self::new(
            "АБВ",
            vec![
                Self::row_of_chars("йцукенгшщзхъ"),
                Self::row_of_chars("фывапролджэё"),
                Self::row_of_chars("ячсмитьбю-"),
                Self::bottom_row(&[("ABC", 1), ("123", 2)], submit),
            ],
        )
    }

    /// Латиница (индексы: 0 — кириллица, 1 — эта, 2 — цифры).
    pub fn latin(submit: &str) -> Self {
        Self::new(
            "ABC",
            vec![
                Self::row_of_chars("qwertyuiop"),
                Self::row_of_chars("asdfghjkl'"),
                Self::row_of_chars("zxcvbnm,.-"),
                Self::bottom_row(&[("АБВ", 0), ("123", 2)], submit),
            ],
        )
    }

    /// Цифры и знаки (индексы: 0 — кириллица, 1 — латиница).
    pub fn digits(submit: &str) -> Self {
        Self::new(
            "123",
            vec![
                Self::row_of_chars("1234567890"),
                Self::row_of_chars("@#$%&*()_+"),
                Self::row_of_chars("!?:;\"'/\\=~"),
                Self::bottom_row(&[("АБВ", 0), ("ABC", 1)], submit),
            ],
        )
    }

    /// Раскладка для адресов: латиница, цифры и частые куски URL.
    /// Индексы: 0 — эта, 1 — цифры/знаки.
    pub fn url(submit: &str) -> Self {
        let mut rows = vec![
            Self::row_of_chars("1234567890"),
            Self::row_of_chars("qwertyuiop"),
            Self::row_of_chars("asdfghjkl_"),
            Self::row_of_chars("zxcvbnm-.:"),
            vec![
                KeyDef::text("https://", "https://", 3),
                KeyDef::text("http://", "http://", 3),
                KeyDef::text("www.", "www.", 2),
                KeyDef::text(".com", ".com", 2),
                KeyDef::text(".ru", ".ru", 2),
                KeyDef::text("/", "/", 1),
                KeyDef::text("?", "?", 1),
                KeyDef::text("=", "=", 1),
                KeyDef::text("&", "&", 1),
            ],
        ];
        let mut bottom = vec![KeyDef::action("#+=", KeyAction::Layout(1), 2)];
        bottom.push(KeyDef::action("Aa", KeyAction::Shift, 1));
        bottom.push(KeyDef::action("Стереть", KeyAction::Backspace, 2));
        bottom.push(KeyDef::action("Очистить", KeyAction::Clear, 2));
        bottom.push(KeyDef::action(submit, KeyAction::Submit, 3));
        rows.push(bottom);
        Self::new("URL", rows)
    }

    /// Знаки для URL-набора (индекс 0 — обратно к URL).
    fn url_symbols(submit: &str) -> Self {
        let mut bottom = vec![KeyDef::action("URL", KeyAction::Layout(0), 2)];
        bottom.push(KeyDef::action("Стереть", KeyAction::Backspace, 2));
        bottom.push(KeyDef::action("Очистить", KeyAction::Clear, 2));
        bottom.push(KeyDef::action(submit, KeyAction::Submit, 3));
        Self::new(
            "#+=",
            vec![
                Self::row_of_chars("1234567890"),
                Self::row_of_chars("@#$%^&*()+"),
                Self::row_of_chars("!;,'\"[]{}~"),
                bottom,
            ],
        )
    }

    /// Набор для текста: русская, латиница, цифры.
    pub fn text_ru_en(submit: &str) -> Vec<Self> {
        vec![
            Self::cyrillic(submit),
            Self::latin(submit),
            Self::digits(submit),
        ]
    }

    /// Набор для логина/пароля: латиница первой.
    pub fn text_en_ru(submit: &str) -> Vec<Self> {
        let mut v = Self::text_ru_en(submit);
        v.swap(0, 1);
        // После перестановки индексы в переключателях остались прежними:
        // 0 — кириллица, 1 — латиница. Поправляем на новые места.
        for layout in &mut v {
            for row in &mut layout.rows {
                for key in row.iter_mut() {
                    if let KeyAction::Layout(i) = key.action {
                        key.action = KeyAction::Layout(match i {
                            0 => 1,
                            1 => 0,
                            other => other,
                        });
                    }
                }
            }
        }
        v
    }

    /// Набор для адресов: URL-раскладка и знаки.
    pub fn url_set(submit: &str) -> Vec<Self> {
        vec![Self::url(submit), Self::url_symbols(submit)]
    }

    fn row_len(&self, row: usize) -> usize {
        self.rows.get(row).map(|r| r.len()).unwrap_or(0)
    }

    /// Центр клавиши в юнитах — для перехода между рядами разной ширины.
    fn key_center(&self, row: usize, col: usize) -> f32 {
        let Some(r) = self.rows.get(row) else {
            return 0.0;
        };
        let mut x = 0.0f32;
        for (i, k) in r.iter().enumerate() {
            let w = k.width.max(1) as f32;
            if i == col {
                return x + w / 2.0;
            }
            x += w;
        }
        x
    }

    fn nearest_col(&self, row: usize, center: f32) -> usize {
        let Some(r) = self.rows.get(row) else {
            return 0;
        };
        let mut best = 0usize;
        let mut best_d = f32::INFINITY;
        for i in 0..r.len() {
            let d = (self.key_center(row, i) - center).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        best
    }
}

/// Состояние клавиатуры: создаётся один раз (в контексте приложения),
/// переживает пересборки экрана.
#[derive(Clone, Copy)]
pub struct KeyboardState {
    pub text: RwSignal<String>,
    pub layout: RwSignal<usize>,
    /// (ряд, клавиша) под фокусом.
    pub focus: RwSignal<(usize, usize)>,
    pub shift: RwSignal<bool>,
    /// Пока `true`, клавиатура поглощает D-pad и OK.
    pub active: RwSignal<bool>,
}

impl KeyboardState {
    pub fn new(initial: &str) -> Self {
        Self {
            text: use_signal(initial.to_string()),
            layout: use_signal(0usize),
            focus: use_signal((0usize, 0usize)),
            shift: use_signal(false),
            active: use_signal(true),
        }
    }

    pub fn reset(&self, text: &str) {
        self.text.set(text.to_string());
        self.layout.set(0);
        self.focus.set((0, 0));
        self.shift.set(false);
    }
}

type TextCb = Arc<dyn Fn(&str) + Send + Sync>;

pub struct OnScreenKeyboard {
    state: KeyboardState,
    layouts: Arc<Vec<KeyboardLayout>>,
    on_change: Option<TextCb>,
    on_submit: Option<TextCb>,
    /// Зазор между клавишами и рядами (px). MSS `margin` контейнеры не
    /// учитывают, поэтому расстояние задаётся здесь.
    gap: f32,
}

/// Управление клавиатурой снаружи виджета: те же действия, что и по D-pad
/// (отладка, физическая клавиатура, скрипты). Дёшево клонируется.
#[derive(Clone)]
pub struct KeyboardController {
    state: KeyboardState,
    layouts: Arc<Vec<KeyboardLayout>>,
    on_change: Option<TextCb>,
    on_submit: Option<TextCb>,
}

impl KeyboardController {
    /// Нажатие пульта. `false` — фокус упёрся в край сетки или клавиатура
    /// неактивна.
    pub fn press(&self, key: RemoteKey) -> bool {
        if !self.state.active.get_untracked() {
            return false;
        }
        handle_remote(
            self.state,
            &self.layouts,
            key,
            &self.on_change,
            &self.on_submit,
        )
    }

    /// Символ с физической клавиатуры.
    pub fn type_char(&self, c: char) {
        apply_action(
            self.state,
            &self.layouts,
            &KeyAction::Char(c.to_string()),
            &self.on_change,
            &self.on_submit,
        );
    }

    pub fn apply(&self, action: &KeyAction) {
        apply_action(
            self.state,
            &self.layouts,
            action,
            &self.on_change,
            &self.on_submit,
        );
    }

    pub fn state(&self) -> KeyboardState {
        self.state
    }
}

/// Экранная клавиатура. `layouts` — список раскладок, индексы в
/// `KeyAction::Layout` ссылаются на него.
pub fn on_screen_keyboard(state: KeyboardState, layouts: Vec<KeyboardLayout>) -> OnScreenKeyboard {
    OnScreenKeyboard {
        state,
        layouts: Arc::new(layouts),
        on_change: None,
        on_submit: None,
        gap: 10.0,
    }
}

impl OnScreenKeyboard {
    /// Вызывается после каждого изменения текста.
    pub fn on_change(mut self, cb: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(cb));
        self
    }

    /// Клавиша подтверждения (и Enter с физической клавиатуры на desktop).
    pub fn on_submit(mut self, cb: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_submit = Some(Arc::new(cb));
        self
    }

    /// Зазор между клавишами и рядами (по умолчанию 10 px).
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap.max(0.0);
        self
    }

    /// Контроллер с теми же раскладками и колбэками, что у виджета.
    pub fn controller(&self) -> KeyboardController {
        KeyboardController {
            state: self.state,
            layouts: self.layouts.clone(),
            on_change: self.on_change.clone(),
            on_submit: self.on_submit.clone(),
        }
    }

    /// Собрать виджет. Клавиши перерисовываются при смене фокуса/раскладки.
    pub fn build(self) -> impl Widget {
        let state = self.state;
        let layouts = self.layouts.clone();
        let on_change = self.on_change.clone();
        let on_submit = self.on_submit.clone();

        let ctl = self.controller();
        let ctl_chars = ctl.clone();
        let gap = self.gap;

        EventHook::new()
            .on_remote(move |key| ctl.press(key))
            // Физическая клавиатура (desktop): буквы печатаются напрямую,
            // Backspace/Enter — через RemoteKey (Back/Select) не идут, так как
            // Back закрывает экран; ловим их как символы.
            .on_char(move |c| {
                if !ctl_chars.state.active.get_untracked() {
                    return false;
                }
                match c {
                    '\u{8}' => ctl_chars.apply(&KeyAction::Backspace),
                    '\r' | '\n' => ctl_chars.apply(&KeyAction::Submit),
                    c if !c.is_control() => ctl_chars.type_char(c),
                    _ => return false,
                }
                true
            })
            .child(Reactive::new(move || -> Vec<Box<dyn Widget>> {
                let layout_idx = state.layout.get().min(layouts.len().saturating_sub(1));
                let (fr, fc) = state.focus.get();
                let active = state.active.get();
                let shift = state.shift.get();
                let Some(layout) = layouts.get(layout_idx) else {
                    return vec![Box::new(DecoratedBox::new())];
                };

                let mut col = Column::new()
                    .gap(gap)
                    .cross_axis_alignment(CrossAxisAlignment::Start);
                for (r, row) in layout.rows.iter().enumerate() {
                    let mut row_w = Row::new()
                        .gap(gap)
                        .cross_axis_alignment(CrossAxisAlignment::Center);
                    for (c, key) in row.iter().enumerate() {
                        let focused = active && (fr, fc) == (r, c);
                        let mut cls = String::from("osk-key");
                        if key.is_action() {
                            cls.push_str(" osk-key-action");
                        }
                        if key.width > 1 {
                            cls.push_str(&format!(" osk-key-w{}", key.width.min(6)));
                        }
                        if focused {
                            cls.push_str(" osk-key-focused");
                        }
                        if shift && matches!(key.action, KeyAction::Shift) {
                            cls.push_str(" osk-key-on");
                        }
                        let label = match &key.action {
                            KeyAction::Char(s) if shift && !key.is_action() => uppercase_first(s),
                            _ => key.label.clone(),
                        };
                        let action = key.action.clone();
                        let layouts_c = layouts.clone();
                        let change_c = on_change.clone();
                        let submit_c = on_submit.clone();
                        row_w = row_w.child(
                            GestureDetector::new()
                                .on_click(move || {
                                    state.focus.set((r, c));
                                    apply_action(state, &layouts_c, &action, &change_c, &submit_c);
                                })
                                .child(DecoratedBox::new().class(cls.as_str()).child(
                                    Text::new(label).max_lines(1).class(if focused {
                                        "osk-key-text osk-key-text-focused"
                                    } else {
                                        "osk-key-text"
                                    }),
                                )),
                        );
                    }
                    col = col.child(row_w.class("osk-row"));
                }
                vec![Box::new(col.class("osk"))]
            }))
    }
}

fn uppercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn apply_action(
    state: KeyboardState,
    layouts: &Arc<Vec<KeyboardLayout>>,
    action: &KeyAction,
    on_change: &Option<TextCb>,
    on_submit: &Option<TextCb>,
) {
    let mut changed = false;
    match action {
        KeyAction::Char(s) => {
            let shift = state.shift.get_untracked();
            let insert = if shift { uppercase_first(s) } else { s.clone() };
            let mut t = state.text.get_untracked();
            t.push_str(&insert);
            state.text.set(t);
            if shift {
                state.shift.set(false);
            }
            changed = true;
        }
        KeyAction::Space => {
            let mut t = state.text.get_untracked();
            t.push(' ');
            state.text.set(t);
            changed = true;
        }
        KeyAction::Backspace => {
            let mut t = state.text.get_untracked();
            if t.pop().is_some() {
                state.text.set(t);
                changed = true;
            }
        }
        KeyAction::Clear => {
            if !state.text.get_untracked().is_empty() {
                state.text.set(String::new());
                changed = true;
            }
        }
        KeyAction::Submit => {
            if let Some(cb) = on_submit {
                cb(&state.text.get_untracked());
            }
        }
        KeyAction::Layout(i) => {
            if *i < layouts.len() {
                state.layout.set(*i);
                clamp_focus(state, layouts);
            }
        }
        KeyAction::NextLayout => {
            let n = layouts.len().max(1);
            state.layout.set((state.layout.get_untracked() + 1) % n);
            clamp_focus(state, layouts);
        }
        KeyAction::Shift => {
            state.shift.set(!state.shift.get_untracked());
        }
    }
    if changed {
        if let Some(cb) = on_change {
            cb(&state.text.get_untracked());
        }
    }
}

fn clamp_focus(state: KeyboardState, layouts: &Arc<Vec<KeyboardLayout>>) {
    let Some(layout) = layouts.get(state.layout.get_untracked()) else {
        return;
    };
    let (r, c) = state.focus.get_untracked();
    let r = r.min(layout.rows.len().saturating_sub(1));
    let c = c.min(layout.row_len(r).saturating_sub(1));
    state.focus.set((r, c));
}

/// D-pad внутри сетки. `false` — фокус упёрся в край, пусть решает родитель.
fn handle_remote(
    state: KeyboardState,
    layouts: &Arc<Vec<KeyboardLayout>>,
    key: RemoteKey,
    on_change: &Option<TextCb>,
    on_submit: &Option<TextCb>,
) -> bool {
    let Some(layout) = layouts.get(state.layout.get_untracked()) else {
        return false;
    };
    let (r, c) = state.focus.get_untracked();
    let rows = layout.rows.len();
    match key {
        RemoteKey::Left => {
            if c == 0 {
                return false;
            }
            state.focus.set((r, c - 1));
            true
        }
        RemoteKey::Right => {
            if c + 1 >= layout.row_len(r) {
                return false;
            }
            state.focus.set((r, c + 1));
            true
        }
        RemoteKey::Up => {
            if r == 0 {
                return false;
            }
            let center = layout.key_center(r, c);
            state.focus.set((r - 1, layout.nearest_col(r - 1, center)));
            true
        }
        RemoteKey::Down => {
            if r + 1 >= rows {
                return false;
            }
            let center = layout.key_center(r, c);
            state.focus.set((r + 1, layout.nearest_col(r + 1, center)));
            true
        }
        RemoteKey::Select => {
            if let Some(key) = layout.rows.get(r).and_then(|row| row.get(c)) {
                let action = key.action.clone();
                apply_action(state, layouts, &action, on_change, on_submit);
            }
            true
        }
        RemoteKey::Rewind => {
            apply_action(state, layouts, &KeyAction::Backspace, on_change, on_submit);
            true
        }
        _ => false,
    }
}

/// Поле ввода для ТВ: показывает текст (или placeholder), каретку и рамку
/// фокуса. `masked` — пароль, символы заменяются точками.
pub fn input_field(
    value: RwSignal<String>,
    placeholder: &'static str,
    masked: bool,
    focused: bool,
) -> impl Widget {
    Reactive::new(move || -> Vec<Box<dyn Widget>> {
        let v = value.get();
        let shown = if masked {
            "•".repeat(v.chars().count())
        } else {
            v.clone()
        };
        let mut row = Row::new()
            .gap(2.0)
            .cross_axis_alignment(CrossAxisAlignment::Center);
        if shown.is_empty() {
            row = row.child(
                Text::new(placeholder)
                    .max_lines(1)
                    .class("osk-field-placeholder"),
            );
        } else {
            row = row.child(
                Text::new(shown)
                    .max_lines(1)
                    .elide(crate::widget::Elide::Middle)
                    .class("osk-field-text"),
            );
        }
        if focused {
            row = row.child(DecoratedBox::new().class("osk-field-caret"));
        }
        vec![Box::new(
            DecoratedBox::new()
                .class(if focused {
                    "osk-field osk-field-focused"
                } else {
                    "osk-field"
                })
                .child(row),
        )]
    })
}

/// Базовые стили клавиатуры и поля — приложение подключает и переопределяет
/// (`with_additional_styles_str`).
pub const DEFAULT_MSS: &str = r#"
.osk { padding: 0px; }
.osk-row { padding: 0px; }
.osk-key {
    width: 64px;
    height: 56px;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.10);
    border: 2px solid rgba(255, 255, 255, 0);
    justify-content: center;
    align-items: center;
    transition: background 120ms ease-out, border-color 120ms ease-out;
}
.osk-key-action { background: rgba(255, 255, 255, 0.05); }
/* Ширина клавиши в n юнитов = n·64 + (n−1)·gap при gap 10. */
.osk-key-w2 { width: 138px; }
.osk-key-w3 { width: 212px; }
.osk-key-w4 { width: 286px; }
.osk-key-w5 { width: 360px; }
.osk-key-w6 { width: 434px; }
.osk-key-on { border-color: rgba(255, 255, 255, 0.45); }
.osk-key-focused { background: #ffffff; border-color: #ffffff; }
.osk-key-text { color: #f4f4f8; font-size: 22px; font-weight: 600; }
.osk-key-text-focused { color: #0b0c14; }

.osk-field {
    width: 100%;
    height: 64px;
    padding: 0px 20px;
    border-radius: 14px;
    background: rgba(255, 255, 255, 0.08);
    border: 2px solid rgba(255, 255, 255, 0.12);
    align-items: center;
    transition: border-color 120ms ease-out, background 120ms ease-out;
}
.osk-field-focused { border-color: #ffffff; background: rgba(255, 255, 255, 0.14); }
.osk-field-text { color: #f4f4f8; font-size: 26px; }
.osk-field-placeholder { color: rgba(244, 244, 248, 0.4); font-size: 26px; }
.osk-field-caret { width: 2px; height: 32px; background: #ffffff; }
"#;
