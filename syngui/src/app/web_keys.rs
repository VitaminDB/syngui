//! Веб: функциональные клавиши, оставленные браузеру.
//!
//! winit вешает `keydown`/`keyup` на canvas и вызывает `preventDefault()` для
//! каждой клавиши. Чтобы F5, F11, F12 и прочие F-клавиши вне набора
//! [`captured_function_keys`] по-прежнему выполняли действие браузера,
//! ставим capture-слушатели на `window`: они срабатывают раньше canvas и
//! останавливают распространение события — до winit оно не доходит, а
//! действие по умолчанию остаётся в силе.

use std::cell::Cell;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::input::{captured_function_keys, FunctionKeys};

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

/// Ставит слушатели один раз; они живут до конца страницы.
pub(crate) fn install() {
    if INSTALLED.with(|c| c.replace(true)) {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    for name in ["keydown", "keyup"] {
        let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |event: web_sys::KeyboardEvent| {
                let Some(key) = FunctionKeys::key_from_code(&event.code()) else {
                    return;
                };
                if !captured_function_keys().contains(key) {
                    // Клавиша остаётся браузеру: событие не доходит до canvas,
                    // где winit вызвал бы preventDefault().
                    event.stop_propagation();
                }
            },
        );
        if let Err(err) = window.add_event_listener_with_callback_and_bool(
            name,
            closure.as_ref().unchecked_ref(),
            true,
        ) {
            web_sys::console::warn_2(&"[syngui] function keys listener:".into(), &err);
        }
        closure.forget();
    }
}
