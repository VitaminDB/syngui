//! Веб: экранная клавиатура через скрытый `<input>` — «агент ввода».
//!
//! Мобильный браузер поднимает экранную клавиатуру только при фокусе на
//! редактируемом DOM-элементе; canvas winit'а (`tabindex="0"`) её не
//! вызывает, поэтому в web-сборке тап по текстовому полю не давал набрать
//! ни символа. Схема — аналог невидимого `SynGuiInputView` на Android:
//!
//! 1. По запросу виджета (`ctx.set_virtual_keyboard_visible(true)` — общий
//!    канал с Android) [`show`] наполняет невидимый `<input>` текстом поля,
//!    кладёт его поверх поля (браузер сам подвинет его в видимую область над
//!    клавиатурой) и даёт ему фокус. Вызов синхронный, внутри обработчика
//!    тапа — так браузер засчитывает жест пользователя.
//! 2. Набор идёт в `<input>`. Событие `input` сравнивает прежнее и новое
//!    значение ([`crate::input::edit_diff`]) и превращает разницу в
//!    синтетические keydown для canvas: `Backspace` на каждый удалённый
//!    символ и keydown с `key = символ` на каждый вставленный — winit
//!    доставляет их приложению обычным путём (`event.text` →
//!    `Event::CharInput`). Composition IME (Gboard подчёркивает набираемое
//!    слово, автозамена при подтверждении) сводится к той же разнице.
//! 3. Клавиши без текста (Enter, стрелки, Tab, Escape, Backspace при пустом
//!    агенте, любые Ctrl/Cmd-сочетания) пробрасываются в canvas как есть с
//!    `preventDefault()`, чтобы браузер не правил агент сам. Печатные
//!    клавиши пробрасываются без `key` (виджет получает `KeyDown(Key::A)`,
//!    а сам символ придёт через `input`). Ctrl/Cmd+V сюда не доходит —
//!    его перехватывает [`super::web_clipboard`] на стадии capture.
//! 4. [`hide`] снимает фокус с агента и возвращает его canvas'у —
//!    физическая клавиатура снова идёт в winit напрямую.
//!
//! Закрытие клавиатуры пользователем (кнопка «назад» Android) браузер не
//! сообщает — фокус остаётся на `<input>`. Оно распознаётся по возврату
//! высоты `visualViewport` к полной; [`take_dismissed`] отдаёт факт
//! приложению, и оно снимает фокус с поля, как на Android. Повторный тап
//! по уже фокусному полю при закрытой клавиатуре делает `blur` + `focus`:
//! повторный `focus()` без потери фокуса клавиатуру не поднимает.
//!
//! Ограничение: каретка агента всегда в конце; перемещение каретки в самом
//! агенте (жест по пробелу Gboard) в виджет не передаётся.

use std::cell::{Cell, RefCell};

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::core::Rect;
use crate::input::edit_diff;

/// Ниже этой доли полной высоты visualViewport считается, что экранная
/// клавиатура открыта. Панель адреса браузера (~56px) порог не проходит.
const KEYBOARD_OPEN_RATIO: f64 = 0.85;

struct Agent {
    input: web_sys::HtmlInputElement,
    canvas: web_sys::HtmlCanvasElement,
}

/// Слежение за visualViewport: ширина (смена — поворот/зум, отсчёт заново),
/// полная высота без клавиатуры и последняя виденная высота.
#[derive(Clone, Copy)]
struct Viewport {
    width: f64,
    full_height: f64,
    last_height: f64,
}

impl Viewport {
    fn open(&self, height: f64) -> bool {
        self.full_height > 0.0 && height < self.full_height * KEYBOARD_OPEN_RATIO
    }
}

thread_local! {
    static AGENT: RefCell<Option<Agent>> = const { RefCell::new(None) };
    /// Значение агента после последней обработанной правки.
    static LAST_VALUE: RefCell<String> = const { RefCell::new(String::new()) };
    static LAST_RECT: Cell<Option<(i32, i32, i32, i32)>> = const { Cell::new(None) };
    static SHOWN: Cell<bool> = const { Cell::new(false) };
    static DISMISSED: Cell<bool> = const { Cell::new(false) };
    static VIEWPORT: Cell<Viewport> = const {
        Cell::new(Viewport { width: 0.0, full_height: 0.0, last_height: 0.0 })
    };
}

/// Создаёт агент рядом с canvas и ставит слушатели; один раз на страницу.
pub(crate) fn install(canvas: web_sys::HtmlCanvasElement) {
    if AGENT.with(|a| a.borrow().is_some()) {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Ok(input) = document
        .create_element("input")
        .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().map_err(Into::into))
    else {
        return;
    };
    input.set_type("text");
    for (name, value) in [
        ("autocomplete", "off"),
        ("autocorrect", "off"),
        ("autocapitalize", "off"),
        ("spellcheck", "false"),
        ("inputmode", "text"),
        ("aria-hidden", "true"),
        ("tabindex", "-1"),
        ("data-syngui", "text-agent"),
    ] {
        let _ = input.set_attribute(name, value);
    }
    let style = input.style();
    for (prop, value) in [
        ("position", "fixed"),
        ("left", "0px"),
        ("top", "0px"),
        ("width", "1px"),
        ("height", "1px"),
        ("opacity", "0"),
        ("pointer-events", "none"),
        ("border", "0"),
        ("outline", "none"),
        ("padding", "0"),
        ("margin", "0"),
        ("background", "transparent"),
        ("color", "transparent"),
        ("caret-color", "transparent"),
        // 16px: iOS Safari приближает страницу при фокусе на поле с меньшим шрифтом.
        ("font-size", "16px"),
        ("z-index", "0"),
    ] {
        let _ = style.set_property(prop, value);
    }
    let parent: Option<web_sys::Node> = canvas
        .parent_node()
        .or_else(|| document.body().map(web_sys::Node::from));
    let Some(parent) = parent else {
        return;
    };
    if let Err(err) = parent.append_child(&input) {
        web_sys::console::warn_2(&"[syngui] text agent append:".into(), &err);
        return;
    }

    for name in ["keydown", "keyup"] {
        let input_ref = input.clone();
        let canvas_ref = canvas.clone();
        let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |event: web_sys::KeyboardEvent| on_key(&event, &input_ref, &canvas_ref),
        );
        add_listener(&input, name, closure.as_ref().unchecked_ref());
        closure.forget();
    }
    {
        let input_ref = input.clone();
        let canvas_ref = canvas.clone();
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
            on_input(&input_ref, &canvas_ref)
        });
        add_listener(&input, "input", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    {
        // winit на каждом pointerdown вызывает `canvas.focus()` (после
        // preventDefault), и агент терял бы фокус — экранная клавиатура
        // моргала бы при тапе по тому же полю. Слушатель стоит после
        // winit'овского и тут же возвращает фокус; если тап уйдёт мимо
        // полей, `hide()` на отпускании снимет его сам.
        let input_ref = input.clone();
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
            if SHOWN.with(|s| s.get()) {
                let _ = input_ref.focus();
            }
        });
        add_listener(&canvas, "pointerdown", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    if let Some(viewport) = window.visual_viewport() {
        VIEWPORT.with(|v| {
            v.set(Viewport {
                width: viewport.width(),
                full_height: viewport.height(),
                last_height: viewport.height(),
            })
        });
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            if let Some(target) = event.target() {
                if let Ok(viewport) = target.dyn_into::<web_sys::VisualViewport>() {
                    on_viewport_resize(&viewport);
                }
            }
        });
        add_listener(&viewport, "resize", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    AGENT.with(|a| *a.borrow_mut() = Some(Agent { input, canvas }));
}

fn add_listener(target: &web_sys::EventTarget, name: &str, callback: &web_sys::js_sys::Function) {
    if let Err(err) = target.add_event_listener_with_callback(name, callback) {
        web_sys::console::warn_2(&format!("[syngui] text agent {name} listener:").into(), &err);
    }
}

/// Запрос клавиатуры для фокусного поля. `text` — текст поля (None —
/// повторный запрос для того же поля, содержимое агента сохраняется),
/// `rect` — его положение в CSS-пикселях.
pub(crate) fn show(text: Option<&str>, numeric: bool, secret: bool, rect: Option<Rect>) {
    AGENT.with(|a| {
        let Some(agent) = a.borrow().as_ref().map(|a| (a.input.clone(), a.canvas.clone())) else {
            return;
        };
        let (input, _canvas) = agent;
        let kind = if secret { "password" } else { "text" };
        if input.type_() != kind {
            input.set_type(kind);
        }
        let _ = input.set_attribute("inputmode", if numeric { "numeric" } else { "text" });
        if let Some(text) = text {
            input.set_value(text);
            LAST_VALUE.with(|v| *v.borrow_mut() = text.to_string());
            let end = text.encode_utf16().count() as u32;
            let _ = input.set_selection_range(end, end);
        }
        if let Some(rect) = rect {
            apply_rect(&input, rect);
        }
        SHOWN.with(|s| s.set(true));
        DISMISSED.with(|d| d.set(false));
        if is_active(&input) {
            // Повторный тап по фокусному полю: клавиатура могла быть закрыта
            // кнопкой «назад» — тогда только потеря и возврат фокуса поднимут её.
            if !keyboard_open() {
                let _ = input.blur();
                let _ = input.focus();
            }
        } else {
            let _ = input.focus();
        }
    });
}

/// Снимает фокус с агента и возвращает его canvas'у.
pub(crate) fn hide() {
    SHOWN.with(|s| s.set(false));
    DISMISSED.with(|d| d.set(false));
    LAST_RECT.with(|r| r.set(None));
    AGENT.with(|a| {
        let Some(agent) = a.borrow().as_ref().map(|a| (a.input.clone(), a.canvas.clone())) else {
            return;
        };
        let (input, canvas) = agent;
        if is_active(&input) {
            let _ = input.blur();
            let _ = canvas.focus();
        }
        input.set_value("");
        LAST_VALUE.with(|v| v.borrow_mut().clear());
    });
}

pub(crate) fn is_shown() -> bool {
    SHOWN.with(|s| s.get())
}

/// Положение фокусного поля изменилось (скролл, relayout) — агент следует
/// за ним, чтобы браузер прокручивал к нему правильно.
pub(crate) fn sync_rect(rect: Rect) {
    AGENT.with(|a| {
        if let Some(agent) = a.borrow().as_ref() {
            apply_rect(&agent.input, rect);
        }
    });
}

/// Пользователь закрыл экранную клавиатуру сам (viewport вернулся к полной
/// высоте при фокусе на агенте). Флаг сбрасывается при чтении.
pub(crate) fn take_dismissed() -> bool {
    DISMISSED.with(|d| d.replace(false))
}

fn apply_rect(input: &web_sys::HtmlInputElement, rect: Rect) {
    let key = (
        rect.origin.x.round() as i32,
        rect.origin.y.round() as i32,
        rect.size.width.round().max(1.0) as i32,
        rect.size.height.round().max(1.0) as i32,
    );
    if LAST_RECT.with(|r| r.get()) == Some(key) {
        return;
    }
    LAST_RECT.with(|r| r.set(Some(key)));
    let style = input.style();
    let _ = style.set_property("left", &format!("{}px", key.0));
    let _ = style.set_property("top", &format!("{}px", key.1));
    let _ = style.set_property("width", &format!("{}px", key.2));
    let _ = style.set_property("height", &format!("{}px", key.3));
}

fn is_active(input: &web_sys::HtmlInputElement) -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.active_element())
        .map(|el| el.is_same_node(Some(input.as_ref())))
        .unwrap_or(false)
}

/// Экранная клавиатура открыта — по высоте visualViewport.
pub(crate) fn keyboard_open() -> bool {
    let Some(viewport) = web_sys::window().and_then(|w| w.visual_viewport()) else {
        return false;
    };
    VIEWPORT.with(|v| v.get().open(viewport.height()))
}

fn on_viewport_resize(viewport: &web_sys::VisualViewport) {
    let (width, height) = (viewport.width(), viewport.height());
    let mut track = VIEWPORT.with(|v| v.get());
    if (width - track.width).abs() > 1.0 {
        // Поворот или зум: прежняя полная высота больше не показательна.
        track = Viewport { width, full_height: height, last_height: height };
        VIEWPORT.with(|v| v.set(track));
        return;
    }
    let was_open = track.open(track.last_height);
    track.full_height = track.full_height.max(height);
    let now_open = track.open(height);
    track.last_height = height;
    VIEWPORT.with(|v| v.set(track));
    if was_open && !now_open && SHOWN.with(|s| s.get()) {
        let agent_focused = AGENT.with(|a| a.borrow().as_ref().map(|a| is_active(&a.input)).unwrap_or(false));
        if agent_focused {
            DISMISSED.with(|d| d.set(true));
        }
    }
}

/// Как поступить с клавишей, пришедшей в агент.
enum KeyRoute {
    /// Пробросить в canvas; `mask` прячет `key`, чтобы winit не сделал из
    /// него текст (символ придёт через `input`); `prevent` не даёт браузеру
    /// применить клавишу к агенту.
    Forward { mask: bool, prevent: bool },
    /// Оставить браузеру: правка агента придёт событием `input`.
    Browser,
}

fn route_key(key: &str, ctrl: bool, alt: bool, meta: bool, agent_has_text: bool) -> KeyRoute {
    // Как в AppHandler: Ctrl (кроме AltGr = Ctrl+Alt) и Cmd — не набор.
    let combo = (ctrl && !alt) || meta;
    let printable = key.chars().count() == 1
        || matches!(key, "Dead" | "Process" | "Unidentified");
    if combo {
        KeyRoute::Forward { mask: false, prevent: true }
    } else if printable {
        KeyRoute::Forward { mask: true, prevent: false }
    } else if key == "Backspace" && !alt && agent_has_text {
        KeyRoute::Browser
    } else {
        KeyRoute::Forward { mask: false, prevent: true }
    }
}

fn on_key(
    event: &web_sys::KeyboardEvent,
    input: &web_sys::HtmlInputElement,
    canvas: &web_sys::HtmlCanvasElement,
) {
    let key = event.key();
    let agent_has_text = input
        .selection_start()
        .ok()
        .flatten()
        .map(|start| start > 0)
        .unwrap_or_else(|| !input.value().is_empty());
    match route_key(&key, event.ctrl_key(), event.alt_key(), event.meta_key(), agent_has_text) {
        KeyRoute::Browser => {}
        KeyRoute::Forward { mask, prevent } => {
            let is_down = event.type_() == "keydown";
            if prevent && is_down {
                event.prevent_default();
            }
            let init = web_sys::KeyboardEventInit::new();
            init.set_key(if mask { "Unidentified" } else { key.as_str() });
            init.set_code(&event.code());
            init.set_ctrl_key(event.ctrl_key());
            init.set_shift_key(event.shift_key());
            init.set_alt_key(event.alt_key());
            init.set_meta_key(event.meta_key());
            init.set_repeat(event.repeat());
            init.set_location(event.location());
            init.set_bubbles(true);
            init.set_cancelable(true);
            if let Ok(synthetic) =
                web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(&event.type_(), &init)
            {
                let _ = canvas.dispatch_event(&synthetic);
            }
        }
    }
}

fn on_input(input: &web_sys::HtmlInputElement, canvas: &web_sys::HtmlCanvasElement) {
    let value = input.value();
    let old = LAST_VALUE.with(|v| std::mem::replace(&mut *v.borrow_mut(), value.clone()));
    let diff = edit_diff(&old, &value);
    for _ in 0..diff.removed {
        dispatch_key(canvas, "keydown", "Backspace", "Backspace");
        dispatch_key(canvas, "keyup", "Backspace", "Backspace");
    }
    for ch in diff.inserted.chars() {
        if ch.is_control() {
            continue;
        }
        // Без `code`: winit отдаст PhysicalKey::Unidentified, и AppHandler
        // возьмёт из события только текст (`Event::CharInput`).
        dispatch_key(canvas, "keydown", &ch.to_string(), "");
    }
}

fn dispatch_key(canvas: &web_sys::HtmlCanvasElement, kind: &str, key: &str, code: &str) {
    let init = web_sys::KeyboardEventInit::new();
    init.set_key(key);
    init.set_code(code);
    init.set_bubbles(true);
    init.set_cancelable(true);
    if let Ok(synthetic) = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(kind, &init) {
        let _ = canvas.dispatch_event(&synthetic);
    }
}
