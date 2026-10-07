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
/// `SYNGUI_ANIMATIONS` (`0`/`off`/`false` — выключены), иначе включены; скорость — `SYNGUI_ANIMATION_SPEED`.
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
            if let Some(k) = std::env::var("SYNGUI_ANIMATION_SPEED").ok().and_then(|v| v.trim().parse::<f32>().ok()) {
                set_speed(k);
            }
            on
        }
    }
}

/// Шаг времени для переходной анимации виджета: при выключенных анимациях — заведомо больше любой длительности,
/// и переход завершается за один кадр. Для непрерывных анимаций (индикаторы, курсор, частицы, инерция прокрутки)
/// не применять.
pub fn effective_dt(dt: std::time::Duration) -> std::time::Duration {
    if enabled() {
        std::time::Duration::from_secs_f32(scaled_secs(dt.as_secs_f32()))
    } else {
        std::time::Duration::from_secs(3600)
    }
}

/// Множитель длительности анимаций (системная «Скорость»: 2 — вдвое медленнее, 0.5 — вдвое быстрее), в битах f32.
static SPEED_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000); // 1.0

/// Скорость анимаций syngui: множитель длительности (переходы MSS, [`Animation`], конечные ключевые кадры).
pub fn set_speed(factor: f32) {
    let f = if factor.is_finite() { factor.clamp(0.1, 10.0) } else { 1.0 };
    SPEED_BITS.store(f.to_bits(), Ordering::Relaxed);
}

/// Текущий множитель длительности (см. [`set_speed`]).
pub fn speed() -> f32 {
    f32::from_bits(SPEED_BITS.load(Ordering::Relaxed))
}

/// Шаг времени анимации с учётом скорости: длительность ×k ⇔ шаг ÷k.
pub fn scaled_secs(dt_secs: f32) -> f32 {
    dt_secs / speed()
}
