//! Веб: кнопка «Назад» браузера — то же событие [`Event::BackPressed`], что
//! жест «назад» на Android.
//!
//! Приложение на canvas живёт на одной записи истории браузера, поэтому
//! «Назад» уводит со страницы, и при возврате приложение загружается заново.
//! Поверх текущей записи кладётся запись-страж: «Назад» снимает её, браузер
//! присылает `popstate`, и событие уходит виджетам. Обработал кто-то —
//! страж ставится снова; не обработал никто — шаг назад повторяется, и
//! браузер уходит со страницы, как без приложения.

use std::cell::Cell;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use winit::event_loop::EventLoopProxy;

use super::user_event::SynGuiUserEvent;

const GUARD_KEY: &str = "syngui_back_guard";

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

fn guard_state() -> JsValue {
    let state = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&state, &GUARD_KEY.into(), &JsValue::TRUE);
    state.into()
}

fn is_guard(state: &JsValue) -> bool {
    state.is_object()
        && js_sys::Reflect::get(state, &GUARD_KEY.into())
            .map(|v| v.is_truthy())
            .unwrap_or(false)
}

fn history() -> Option<web_sys::History> {
    web_sys::window()?.history().ok()
}

/// Поставить запись-страж, если текущая запись — не она (после
/// перезагрузки страницы браузер оставляет пользователя на страже).
pub(crate) fn arm() {
    let Some(history) = history() else {
        return;
    };
    if is_guard(&history.state().unwrap_or(JsValue::NULL)) {
        return;
    }
    if let Err(err) = history.push_state(&guard_state(), "") {
        web_sys::console::warn_2(&"[syngui] history.pushState:".into(), &err);
    }
}

/// Никто не обработал «назад»: страж уже снят, ещё один шаг назад уводит
/// со страницы.
pub(crate) fn leave() {
    if let Some(history) = history() {
        if let Err(err) = history.back() {
            web_sys::console::warn_2(&"[syngui] history.back:".into(), &err);
        }
    }
}

/// Ставит страж и слушатель `popstate` один раз; живут до конца страницы.
pub(crate) fn install(proxy: EventLoopProxy<SynGuiUserEvent>) {
    if INSTALLED.with(|c| c.replace(true)) {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    arm();
    let closure =
        Closure::<dyn FnMut(web_sys::PopStateEvent)>::new(move |event: web_sys::PopStateEvent| {
            if is_guard(&event.state()) {
                return;
            }
            let _ = proxy.send_event(SynGuiUserEvent::WebBack);
        });
    if let Err(err) =
        window.add_event_listener_with_callback("popstate", closure.as_ref().unchecked_ref())
    {
        web_sys::console::warn_2(&"[syngui] popstate listener:".into(), &err);
    }
    closure.forget();
}
