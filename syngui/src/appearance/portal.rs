//! XDG Desktop Portal — источник системного оформления на Linux.
//!
//! `org.freedesktop.portal.Settings`, namespace `org.freedesktop.appearance`:
//! ключи `color-scheme` (u), `accent-color` ((ddd) в sRGB 0..1), `contrast` (u)
//! и `reduced-motion` (u). Портал работает и под X11, и под Wayland, и в
//! flatpak-песочнице — в отличие от winit'овского `Window::theme()`.
//!
//! Соединение блокирующее и живёт в отдельном потоке (`syngui-appearance`),
//! поэтому здесь нет ни async-рантайма, ни зависимости от event loop.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedValue, Value};

use super::{ColorScheme, SystemAppearance};
use crate::core::Color;

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";

/// Разовое чтение. `None` — портала нет (или он не отдаёт namespace), вызывающий
/// уходит на fallback по конфигам DE.
pub(super) fn read() -> Option<SystemAppearance> {
    let connection = connect()?;
    let proxy = proxy(&connection)?;
    read_all(&proxy)
}

/// Блокирует поток на сигнале `SettingChanged` и зовёт `on_change` при каждом
/// реальном изменении. `None` — портал недоступен, слежение не началось.
pub(super) fn watch<F>(on_change: &F, stop: &AtomicBool) -> Option<()>
where
    F: Fn(SystemAppearance),
{
    let connection = connect()?;
    let proxy = proxy(&connection)?;
    let mut current = read_all(&proxy)?;

    let signals = match proxy.receive_signal("SettingChanged") {
        Ok(s) => s,
        Err(e) => {
            log::debug!("[syngui] portal SettingChanged subscribe failed: {e}");
            return None;
        }
    };

    for message in signals {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let body = message.body();
        let Ok((namespace, key, value)) = body.deserialize::<(String, String, Value<'_>)>() else {
            continue;
        };
        if namespace != NAMESPACE {
            continue;
        }
        let previous = current;
        apply(&mut current, &key, &value);
        if current != previous {
            on_change(current);
        }
    }
    Some(())
}

fn connect() -> Option<Connection> {
    match Connection::session() {
        Ok(c) => Some(c),
        Err(e) => {
            log::debug!("[syngui] no session bus for appearance portal: {e}");
            None
        }
    }
}

fn proxy(connection: &Connection) -> Option<Proxy<'static>> {
    Proxy::new(connection, DESTINATION, PATH, INTERFACE)
        .map_err(|e| log::debug!("[syngui] appearance portal proxy failed: {e}"))
        .ok()
}

fn read_all(proxy: &Proxy<'_>) -> Option<SystemAppearance> {
    // ReadAll есть с первой версии интерфейса и отдаёт все четыре ключа одним
    // вызовом — дешевле, чем четыре ReadOne (последний к тому же появился
    // только в версии 2).
    let reply: HashMap<String, HashMap<String, OwnedValue>> =
        match proxy.call("ReadAll", &(vec![NAMESPACE],)) {
            Ok(r) => r,
            Err(e) => {
                log::debug!("[syngui] portal ReadAll failed: {e}");
                return None;
            }
        };

    let namespace = reply.get(NAMESPACE)?;
    let mut appearance = SystemAppearance::default();
    for (key, value) in namespace {
        apply(&mut appearance, key, value);
    }
    Some(appearance)
}

fn apply(appearance: &mut SystemAppearance, key: &str, value: &Value<'_>) {
    match key {
        "color-scheme" => {
            if let Some(v) = as_u32(value) {
                appearance.color_scheme = ColorScheme::from_portal_u32(v);
            }
        }
        "accent-color" => appearance.accent = as_accent(value),
        "contrast" => {
            if let Some(v) = as_u32(value) {
                appearance.high_contrast = v == 1;
            }
        }
        "reduced-motion" => {
            if let Some(v) = as_u32(value) {
                appearance.reduced_motion = v == 1;
            }
        }
        _ => {}
    }
}

/// Часть реализаций портала кладёт значение в variant внутри variant'а —
/// разворачиваем до конца.
fn unwrap_variant<'a>(value: &'a Value<'a>) -> &'a Value<'a> {
    match value {
        Value::Value(inner) => unwrap_variant(inner),
        other => other,
    }
}

fn as_u32(value: &Value<'_>) -> Option<u32> {
    match unwrap_variant(value) {
        Value::U32(v) => Some(*v),
        Value::I32(v) => u32::try_from(*v).ok(),
        Value::U8(v) => Some(*v as u32),
        _ => None,
    }
}

/// `(ddd)` в sRGB 0..1. Спека разрешает отдать значения вне диапазона — это
/// означает «акцент не задан».
fn as_accent(value: &Value<'_>) -> Option<Color> {
    let fields = match unwrap_variant(value) {
        Value::Structure(s) => s.fields(),
        _ => return None,
    };
    if fields.len() != 3 {
        return None;
    }
    let mut rgb = [0.0f64; 3];
    for (slot, field) in rgb.iter_mut().zip(fields) {
        match unwrap_variant(field) {
            Value::F64(v) => *slot = *v,
            _ => return None,
        }
    }
    if rgb.iter().any(|c| !(0.0..=1.0).contains(c)) {
        return None;
    }
    Some(Color::from_srgb_f32(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32))
}
