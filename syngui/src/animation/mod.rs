pub mod animation;
pub mod easing;
pub mod keyframe;
pub mod spring;
pub mod transition;

pub use animation::Animation;
pub use easing::Easing;
pub use keyframe::KeyframeAnimation;
pub use spring::Spring;
pub use transition::TransitionState;

use std::sync::atomic::{AtomicU8, Ordering};

/// 0 — не задано (берётся из `SYNGUI_ANIMATIONS`), 1 — включены, 2 — выключены.
static ENABLED: AtomicU8 = AtomicU8::new(0);

/// Глобально включить/выключить анимации (системная настройка «Анимации»): переходы MSS (`transition`),
/// пружины и твины виджетов ([`Animation`]) и конечные ключевые кадры сразу приходят в конечное состояние.
/// Бесконечные ключевые кадры (индикаторы загрузки) продолжают идти. По умолчанию — переменная окружения
/// `SYNGUI_ANIMATIONS` (`0`/`off`/`false` — выключены), иначе включены.
pub fn set_enabled(on: bool) {
    ENABLED.store(if on { 1 } else { 2 }, Ordering::Release);
}

/// Включены ли анимации (см. [`set_enabled`]).
pub fn enabled() -> bool {
    match ENABLED.load(Ordering::Acquire) {
        1 => true,
        2 => false,
        _ => {
            let on = !std::env::var("SYNGUI_ANIMATIONS")
                .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "off" | "false" | "no"))
                .unwrap_or(false);
            ENABLED.store(if on { 1 } else { 2 }, Ordering::Release);
            on
        }
    }
}
