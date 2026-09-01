//! Веб: вставка из системного буфера через DOM-событие `paste`.
//!
//! winit вызывает `preventDefault()` для каждого keydown на canvas, поэтому
//! браузер не генерирует `paste`, а `navigator.clipboard.readText()` асинхронен
//! и в Firefox страницам недоступен. Схема обхода:
//!
//! 1. capture-слушатель `keydown` на `window` останавливает распространение
//!    доверенного Ctrl/Cmd+V — до canvas (и winit) оно не доходит, действие
//!    по умолчанию остаётся, и браузер генерирует `paste`;
//! 2. слушатель `paste` кладёт текст из `clipboardData` в кэш
//!    [`crate::clipboard`] и диспатчит в canvas синтетический keydown
//!    Ctrl+V — winit доставляет его приложению обычным путём, а
//!    `paste_from_clipboard()` виджета читает уже наполненный кэш.
//!
//! Синтетическое событие не доверенное (`isTrusted == false`), поэтому
//! слушатель из шага 1 его пропускает. Копирование (Ctrl+C/X) в обходе не
//! нуждается: виджет сам зовёт `navigator.clipboard.writeText`.

use std::cell::{Cell, RefCell};

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    static CANVAS: RefCell<Option<web_sys::HtmlCanvasElement>> = const { RefCell::new(None) };
}

/// Ставит слушатели один раз; живут до конца страницы.
pub(crate) fn install(canvas: web_sys::HtmlCanvasElement) {
    CANVAS.with(|c| *c.borrow_mut() = Some(canvas));
    if INSTALLED.with(|c| c.replace(true)) {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };

    let keydown = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
        |event: web_sys::KeyboardEvent| {
            if !event.is_trusted() {
                return;
            }
            if (event.ctrl_key() || event.meta_key())
                && !event.alt_key()
                && event.code() == "KeyV"
            {
                event.stop_propagation();
            }
        },
    );
    if let Err(err) = window.add_event_listener_with_callback_and_bool(
        "keydown",
        keydown.as_ref().unchecked_ref(),
        true,
    ) {
        web_sys::console::warn_2(&"[syngui] clipboard keydown listener:".into(), &err);
    }
    keydown.forget();

    let paste = Closure::<dyn FnMut(web_sys::ClipboardEvent)>::new(
        |event: web_sys::ClipboardEvent| {
            let Some(data) = event.clipboard_data() else {
                return;
            };
            let Ok(text) = data.get_data("text/plain") else {
                return;
            };
            event.prevent_default();
            if text.is_empty() {
                return;
            }
            crate::clipboard::set_cached(text);

            // Синтетический Ctrl+V в canvas: winit доведёт его до фокусного
            // виджета, тот заберёт текст из кэша.
            CANVAS.with(|c| {
                let Some(canvas) = c.borrow().clone() else {
                    return;
                };
                let init = web_sys::KeyboardEventInit::new();
                init.set_key("v");
                init.set_code("KeyV");
                init.set_ctrl_key(true);
                init.set_bubbles(true);
                init.set_cancelable(true);
                let Ok(synthetic) = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(
                    "keydown", &init,
                ) else {
                    return;
                };
                let _ = canvas.dispatch_event(&synthetic);
                if let Ok(up) = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(
                    "keyup", &init,
                ) {
                    let _ = canvas.dispatch_event(&up);
                }
            });
        },
    );
    if let Err(err) =
        window.add_event_listener_with_callback("paste", paste.as_ref().unchecked_ref())
    {
        web_sys::console::warn_2(&"[syngui] clipboard paste listener:".into(), &err);
    }
    paste.forget();
}
