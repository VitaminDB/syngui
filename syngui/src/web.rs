//! Веб (WebAssembly в браузере): HTTP через `window.fetch`, запуск async-задач и Telegram Mini App.
//!
//! Модуль есть на всех целях, чтобы код приложения собирался без `#[cfg]`: на не-wasm функции
//! ничего не делают и возвращают `None`/`false`, а [`fetch_json`] — `Err`. Признак — [`is_web`].
//!
//! Идиоматичный запрос из обработчика UI — [`spawn_local`] + сигнал: на wasm всё однопоточно,
//! `RwSignal::set` внутри задачи сразу помечает виджеты и запрашивает кадр.
//!
//! ```ignore
//! let status = use_signal(String::from("…"));
//! Button::new("Обновить").on_click(move || {
//!     syngui::web::spawn_local(async move {
//!         let auth = syngui::web::telegram::init_data().unwrap_or_default();
//!         match syngui::web::fetch_json("GET", "/api/status", &[("X-Init-Data", &auth)], None).await {
//!             Ok((200, body)) => status.set(body),
//!             Ok((code, body)) => status.set(format!("HTTP {code}: {body}")),
//!             Err(e) => status.set(e),
//!         }
//!     });
//! })
//! ```

use std::future::Future;

/// Приложение работает в браузере (wasm32).
pub const fn is_web() -> bool {
    cfg!(target_arch = "wasm32")
}

/// Запустить async-задачу в текущем потоке.
///
/// wasm: `wasm_bindgen_futures::spawn_local` (задача идёт в цикле событий браузера, UI не
/// блокируется). Не-wasm: future выполняется сразу до конца (`pollster::block_on`) — этого
/// хватает для общего с вебом кода (функции модуля там завершаются мгновенно); настоящая
/// фоновая работа на десктопе — `syngui::async_runtime::spawn` (tokio) или поток.
pub fn spawn_local(fut: impl Future<Output = ()> + 'static) {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(fut);
    #[cfg(not(target_arch = "wasm32"))]
    pollster::block_on(fut);
}

/// HTTP-запрос через `window.fetch`: `(код ответа, тело текстом)`.
///
/// `body: Some(..)` без своего `Content-Type` в `headers` уходит как `application/json`;
/// `Accept: application/json` ставится, если не задан. Ответ с любым кодом (в том числе 4xx/5xx) —
/// `Ok`; `Err` — сетевая ошибка, CORS, отказ браузера. Не-wasm: всегда `Err`.
pub async fn fetch_json(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<String>,
) -> Result<(u16, String), String> {
    #[cfg(target_arch = "wasm32")]
    {
        imp::fetch_text(method, url, headers, body).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (method, headers, body);
        Err(format!("fetch {url}: только в браузере (wasm32)"))
    }
}

/// Скачать файл целиком (`GET`); код не 2xx — `Err("HTTP …")`. Не-wasm: всегда `Err`.
pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        imp::fetch_bytes(url).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Err(format!("fetch {url}: только в браузере (wasm32)"))
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) mod imp {
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    fn js_err(what: &str, e: JsValue) -> String {
        let msg = e
            .as_string()
            .or_else(|| {
                js_sys::Reflect::get(&e, &"message".into())
                    .ok()
                    .and_then(|m| m.as_string())
            })
            .unwrap_or_else(|| format!("{e:?}"));
        format!("{what}: {msg}")
    }

    pub async fn fetch_text(
        method: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<String>,
    ) -> Result<(u16, String), String> {
        let window = web_sys::window().ok_or("нет window")?;
        let h = web_sys::Headers::new().map_err(|e| js_err("Headers", e))?;
        let has = |name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
        for (k, v) in headers {
            h.append(k, v).map_err(|e| js_err("заголовок", e))?;
        }
        if !has("accept") {
            let _ = h.append("Accept", "application/json");
        }
        if body.is_some() && !has("content-type") {
            let _ = h.append("Content-Type", "application/json");
        }
        let init = web_sys::RequestInit::new();
        init.set_method(method);
        init.set_headers(&h);
        if let Some(b) = &body {
            init.set_body(&JsValue::from_str(b));
        }
        let resp = JsFuture::from(window.fetch_with_str_and_init(url, &init))
            .await
            .map_err(|e| js_err("fetch", e))?;
        let resp: web_sys::Response = resp.dyn_into().map_err(|_| "fetch: не Response")?;
        let status = resp.status();
        let text = JsFuture::from(resp.text().map_err(|e| js_err("text", e))?)
            .await
            .map_err(|e| js_err("text", e))?;
        Ok((status, text.as_string().unwrap_or_default()))
    }

    pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
        let window = web_sys::window().ok_or("нет window")?;
        let resp = JsFuture::from(window.fetch_with_str(url))
            .await
            .map_err(|e| js_err("fetch", e))?;
        let resp: web_sys::Response = resp.dyn_into().map_err(|_| "fetch: не Response")?;
        if !resp.ok() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let buf = JsFuture::from(resp.array_buffer().map_err(|e| js_err("array_buffer", e))?)
            .await
            .map_err(|e| js_err("array_buffer", e))?;
        Ok(js_sys::Uint8Array::new(&buf).to_vec())
    }
}

/// Telegram Mini App: `window.Telegram.WebApp` (скрипт telegram-web-app.js подключает
/// `syngui build -t web` при `[package.metadata.syngui.web] telegram = true`).
///
/// Всё — через `js_sys::Reflect`, без своих зависимостей. Вне Telegram (и не на wasm) функции
/// возвращают `None`/`false` и ничего не делают; [`theme_mss`] отдаёт запасную палитру,
/// чтобы приложение выглядело одинаково при отладке на десктопе.
pub mod telegram {
    use crate::core::Color;

    /// Цвета темы Telegram (`WebApp.themeParams`); поле `None` — клиент его не прислал.
    #[derive(Debug, Clone, Copy, PartialEq, Default)]
    pub struct ThemeColors {
        pub bg_color: Option<Color>,
        pub secondary_bg_color: Option<Color>,
        pub text_color: Option<Color>,
        pub hint_color: Option<Color>,
        pub link_color: Option<Color>,
        pub button_color: Option<Color>,
        pub button_text_color: Option<Color>,
        pub header_bg_color: Option<Color>,
        pub accent_text_color: Option<Color>,
        pub section_bg_color: Option<Color>,
        pub section_header_text_color: Option<Color>,
        pub subtitle_text_color: Option<Color>,
        pub destructive_text_color: Option<Color>,
    }

    impl ThemeColors {
        /// Имена полей = ключи `themeParams` = имена MSS-переменных (`--tg-bg-color`).
        pub const KEYS: [&'static str; 13] = [
            "bg_color",
            "secondary_bg_color",
            "text_color",
            "hint_color",
            "link_color",
            "button_color",
            "button_text_color",
            "header_bg_color",
            "accent_text_color",
            "section_bg_color",
            "section_header_text_color",
            "subtitle_text_color",
            "destructive_text_color",
        ];

        pub fn get(&self, key: &str) -> Option<Color> {
            match key {
                "bg_color" => self.bg_color,
                "secondary_bg_color" => self.secondary_bg_color,
                "text_color" => self.text_color,
                "hint_color" => self.hint_color,
                "link_color" => self.link_color,
                "button_color" => self.button_color,
                "button_text_color" => self.button_text_color,
                "header_bg_color" => self.header_bg_color,
                "accent_text_color" => self.accent_text_color,
                "section_bg_color" => self.section_bg_color,
                "section_header_text_color" => self.section_header_text_color,
                "subtitle_text_color" => self.subtitle_text_color,
                "destructive_text_color" => self.destructive_text_color,
                _ => None,
            }
        }

        fn slot(&mut self, key: &str) -> Option<&mut Option<Color>> {
            Some(match key {
                "bg_color" => &mut self.bg_color,
                "secondary_bg_color" => &mut self.secondary_bg_color,
                "text_color" => &mut self.text_color,
                "hint_color" => &mut self.hint_color,
                "link_color" => &mut self.link_color,
                "button_color" => &mut self.button_color,
                "button_text_color" => &mut self.button_text_color,
                "header_bg_color" => &mut self.header_bg_color,
                "accent_text_color" => &mut self.accent_text_color,
                "section_bg_color" => &mut self.section_bg_color,
                "section_header_text_color" => &mut self.section_header_text_color,
                "subtitle_text_color" => &mut self.subtitle_text_color,
                "destructive_text_color" => &mut self.destructive_text_color,
                _ => return None,
            })
        }

        /// Из пар «ключ themeParams → "#rrggbb"» (неизвестные ключи и не-цвета пропускаются).
        pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
            let mut t = Self::default();
            for (k, v) in pairs {
                if let (Some(slot), Some(c)) = (t.slot(k), parse_hex(v)) {
                    *slot = Some(c);
                }
            }
            t
        }

        /// Палитра Telegram по умолчанию (светлая/тёмная) — запасные значения.
        pub fn fallback(dark: bool) -> Self {
            let light = [
                ("bg_color", "#ffffff"),
                ("secondary_bg_color", "#f1f1f4"),
                ("text_color", "#000000"),
                ("hint_color", "#999999"),
                ("link_color", "#2481cc"),
                ("button_color", "#2481cc"),
                ("button_text_color", "#ffffff"),
                ("header_bg_color", "#ffffff"),
                ("accent_text_color", "#2481cc"),
                ("section_bg_color", "#ffffff"),
                ("section_header_text_color", "#6d6d72"),
                ("subtitle_text_color", "#999999"),
                ("destructive_text_color", "#ff3b30"),
            ];
            let darkp = [
                ("bg_color", "#212121"),
                ("secondary_bg_color", "#181818"),
                ("text_color", "#ffffff"),
                ("hint_color", "#aaaaaa"),
                ("link_color", "#8774e1"),
                ("button_color", "#8774e1"),
                ("button_text_color", "#ffffff"),
                ("header_bg_color", "#212121"),
                ("accent_text_color", "#8774e1"),
                ("section_bg_color", "#212121"),
                ("section_header_text_color", "#aaaaaa"),
                ("subtitle_text_color", "#aaaaaa"),
                ("destructive_text_color", "#ff595a"),
            ];
            Self::from_pairs(if dark { darkp } else { light })
        }

        /// Пустые поля — из `other`.
        pub fn or(mut self, other: &ThemeColors) -> Self {
            for k in Self::KEYS {
                if let Some(slot) = self.slot(k) {
                    if slot.is_none() {
                        *slot = other.get(k);
                    }
                }
            }
            self
        }

        /// `:root { --tg-bg-color: #FFFFFF; … }` (только заданные поля).
        pub fn to_mss(&self) -> String {
            let mut s = String::from(":root {\n");
            for k in Self::KEYS {
                if let Some(c) = self.get(k) {
                    s.push_str(&format!(
                        "    --tg-{}: {};\n",
                        k.replace('_', "-"),
                        c.to_hex()
                    ));
                }
            }
            s.push_str("}\n");
            s
        }
    }

    /// `"#rrggbb"` / `"#rgb"` → цвет.
    pub fn parse_hex(s: &str) -> Option<Color> {
        let h = s.trim().strip_prefix('#')?;
        let full = match h.len() {
            3 => h.chars().flat_map(|c| [c, c]).collect::<String>(),
            6 | 8 => h.to_string(),
            _ => return None,
        };
        if !full.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        Some(Color::from_hex(&full))
    }

    /// Запущено внутри Telegram: есть `Telegram.WebApp` и непустой `initData`.
    pub fn is_available() -> bool {
        init_data().is_some()
    }

    /// Сырая строка `initData` (query string с `hash`) — для проверки подписи на сервере.
    pub fn init_data() -> Option<String> {
        imp::str_prop(&["initData"]).filter(|s| !s.is_empty())
    }

    /// Имя пользователя (`initDataUnsafe.user.first_name`; не проверено — только для показа).
    pub fn user_first_name() -> Option<String> {
        imp::str_prop(&["initDataUnsafe", "user", "first_name"])
    }

    /// id пользователя (`initDataUnsafe.user.id`; не проверено).
    pub fn user_id() -> Option<i64> {
        imp::num_prop(&["initDataUnsafe", "user", "id"]).map(|v| v as i64)
    }

    /// Язык клиента (`initDataUnsafe.user.language_code`).
    pub fn user_language_code() -> Option<String> {
        imp::str_prop(&["initDataUnsafe", "user", "language_code"])
    }

    /// `"light"` или `"dark"`.
    pub fn color_scheme() -> Option<String> {
        imp::str_prop(&["colorScheme"])
    }

    /// Тёмная тема Telegram (вне Telegram — `false`).
    pub fn is_dark() -> bool {
        color_scheme().as_deref() == Some("dark")
    }

    /// Платформа клиента (`ios`, `android`, `tdesktop`, `web`…).
    pub fn platform() -> Option<String> {
        imp::str_prop(&["platform"])
    }

    /// Версия Bot API клиента (`"8.0"`).
    pub fn version() -> Option<String> {
        imp::str_prop(&["version"])
    }

    /// Цвета темы (`themeParams`); вне Telegram — `None`.
    pub fn theme_colors() -> Option<ThemeColors> {
        if !imp::has_webapp() {
            return None;
        }
        let mut pairs = Vec::new();
        for k in ThemeColors::KEYS {
            if let Some(v) = imp::str_prop(&["themeParams", k]) {
                pairs.push((k, v));
            }
        }
        Some(ThemeColors::from_pairs(
            pairs.iter().map(|(k, v)| (*k, v.as_str())),
        ))
    }

    /// MSS-переменные темы `--tg-*` (`--tg-bg-color`, `--tg-text-color`, `--tg-button-color`…):
    /// цвета Telegram, недостающие — из его палитры по умолчанию (светлой/тёмной по
    /// `colorScheme`); вне Telegram — палитра по умолчанию целиком. Заодно выставляет
    /// [`crate::theme_fallback::set_dark_theme`] — запасные цвета виджетов под тему.
    pub fn theme_mss() -> String {
        let dark = is_dark();
        crate::theme_fallback::set_dark_theme(dark);
        theme_colors()
            .unwrap_or_default()
            .or(&ThemeColors::fallback(dark))
            .to_mss()
    }

    /// Сообщить Telegram, что приложение готово (убирает заставку загрузки).
    pub fn ready() {
        imp::call(&[], "ready", &[]);
    }

    /// Развернуть Mini App на всю высоту.
    pub fn expand() {
        imp::call(&[], "expand", &[]);
    }

    /// Закрыть Mini App.
    pub fn close() {
        imp::call(&[], "close", &[]);
    }

    /// Цвет шапки Telegram (`"#rrggbb"` или `"bg_color"`/`"secondary_bg_color"`).
    pub fn set_header_color(color: &str) {
        imp::call(&[], "setHeaderColor", &[color]);
    }

    /// Цвет фона Telegram под приложением.
    pub fn set_background_color(color: &str) {
        imp::call(&[], "setBackgroundColor", &[color]);
    }

    /// Отдача: `"light"`, `"medium"`, `"heavy"`, `"rigid"`, `"soft"`.
    pub fn haptic_impact(style: &str) {
        imp::call(&["HapticFeedback"], "impactOccurred", &[style]);
    }

    /// Отдача: `"success"`, `"error"`, `"warning"`.
    pub fn haptic_notification(kind: &str) {
        imp::call(&["HapticFeedback"], "notificationOccurred", &[kind]);
    }

    /// Отдача при смене выбора.
    pub fn haptic_selection() {
        imp::call(&["HapticFeedback"], "selectionChanged", &[]);
    }

    /// Окно-сообщение Telegram (`showAlert`).
    pub fn show_alert(message: &str) {
        imp::call(&[], "showAlert", &[message]);
    }

    /// Подписка на событие `themeChanged` (пользователь сменил тему Telegram). Колбэк живёт
    /// до конца программы. Обычно: `on_theme_changed(move || theme.set(app_mss(&theme_mss())))`.
    pub fn on_theme_changed(callback: impl Fn() + 'static) {
        imp::on_event("themeChanged", Box::new(callback));
    }

    /// Подписка на любое событие WebApp (`viewportChanged`, `mainButtonClicked`, `backButtonClicked`…).
    pub fn on_event(event: &str, callback: impl Fn() + 'static) {
        imp::on_event(event, Box::new(callback));
    }

    #[cfg(target_arch = "wasm32")]
    mod imp {
        use js_sys::{Function, Reflect};
        use wasm_bindgen::{closure::Closure, JsCast, JsValue};

        fn webapp() -> Option<JsValue> {
            let tg = Reflect::get(&js_sys::global(), &"Telegram".into()).ok()?;
            if tg.is_undefined() || tg.is_null() {
                return None;
            }
            let w = Reflect::get(&tg, &"WebApp".into()).ok()?;
            (!w.is_undefined() && !w.is_null()).then_some(w)
        }

        pub fn has_webapp() -> bool {
            webapp().is_some()
        }

        fn walk(path: &[&str]) -> Option<JsValue> {
            let mut v = webapp()?;
            for p in path {
                v = Reflect::get(&v, &JsValue::from_str(p)).ok()?;
                if v.is_undefined() || v.is_null() {
                    return None;
                }
            }
            Some(v)
        }

        pub fn str_prop(path: &[&str]) -> Option<String> {
            walk(path)?.as_string()
        }

        pub fn num_prop(path: &[&str]) -> Option<f64> {
            walk(path)?.as_f64()
        }

        pub fn call(obj: &[&str], method: &str, args: &[&str]) {
            let Some(o) = walk(obj) else { return };
            let Some(f) = Reflect::get(&o, &JsValue::from_str(method))
                .ok()
                .and_then(|f| f.dyn_into::<Function>().ok())
            else {
                return;
            };
            let r = match args {
                [] => f.call0(&o),
                [a] => f.call1(&o, &JsValue::from_str(a)),
                [a, b, ..] => f.call2(&o, &JsValue::from_str(a), &JsValue::from_str(b)),
            };
            if let Err(e) = r {
                log::warn!("Telegram.WebApp.{method}: {e:?}");
            }
        }

        pub fn on_event(event: &str, cb: Box<dyn Fn()>) {
            let Some(o) = webapp() else { return };
            let Some(f) = Reflect::get(&o, &"onEvent".into())
                .ok()
                .and_then(|f| f.dyn_into::<Function>().ok())
            else {
                return;
            };
            let c = Closure::<dyn Fn()>::new(cb);
            let _ = f.call2(&o, &JsValue::from_str(event), c.as_ref().unchecked_ref());
            c.forget();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod imp {
        pub fn has_webapp() -> bool {
            false
        }
        pub fn str_prop(_: &[&str]) -> Option<String> {
            None
        }
        pub fn num_prop(_: &[&str]) -> Option<f64> {
            None
        }
        pub fn call(_: &[&str], _: &str, _: &[&str]) {}
        pub fn on_event(_: &str, _: Box<dyn Fn()>) {}
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn theme_mss_from_pairs() {
            let t = ThemeColors::from_pairs([
                ("bg_color", "#17212b"),
                ("text_color", "#fff"),
                ("junk", "#000"),
            ]);
            assert_eq!(t.bg_color.unwrap().to_hex(), "#17212B");
            assert_eq!(t.text_color.unwrap().to_hex(), "#FFFFFF");
            assert!(t.hint_color.is_none());
            let mss = t.or(&ThemeColors::fallback(true)).to_mss();
            assert!(mss.contains("--tg-bg-color: #17212B;"));
            assert!(mss.contains("--tg-hint-color: #AAAAAA;"));
            assert!(parse_hex("red").is_none());
        }

        #[test]
        fn desktop_noop() {
            assert!(!is_available());
            assert!(theme_colors().is_none());
            ready();
            haptic_impact("light");
            assert!(theme_mss().contains("--tg-button-color: #2481CC;"));
        }
    }
}
