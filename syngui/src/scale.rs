//! Пользовательский масштаб интерфейса.
//!
//! Множитель поверх системного DPI-масштаба окна: приложение целиком
//! (шрифты, отступы, толщина рамок, иконки) становится крупнее или мельче.
//! Реализован не пересчётом стилей, а изменением эффективного scale factor —
//! логический вьюпорт при масштабе 1.25 становится в 1.25 раза меньше,
//! поэтому адаптивные раскладки ([`crate::viewport::viewport_below`])
//! перестраиваются сами, как при уменьшении окна.
//!
//! Стартовое значение задаётся билдером
//! ([`AppBuilder::ui_scale`](crate::app::AppBuilder::ui_scale)), в рантайме
//! меняется через [`set_ui_scale`] — приложение сохраняет его само.

use std::cell::Cell;

/// Нижняя граница масштаба: мельче интерфейс уже не читается.
pub const MIN_UI_SCALE: f32 = 0.5;
/// Верхняя граница масштаба.
pub const MAX_UI_SCALE: f32 = 3.0;

thread_local! {
    static UI_SCALE: Cell<f32> = const { Cell::new(1.0) };
    /// Взведён после смены масштаба; снимается кадром, который его применил.
    static DIRTY: Cell<bool> = const { Cell::new(false) };
}

/// Текущий пользовательский масштаб (1.0 — «100%»).
pub fn ui_scale() -> f32 {
    UI_SCALE.with(|c| c.get())
}

/// Задать масштаб интерфейса. Значение зажимается в
/// [`MIN_UI_SCALE`]..=[`MAX_UI_SCALE`]; применяется следующим кадром.
///
/// Вызывать на главном потоке (как и работу с сигналами).
pub fn set_ui_scale(scale: f32) {
    let scale = clamp(scale);
    let changed = UI_SCALE.with(|c| {
        if (c.get() - scale).abs() < f32::EPSILON {
            false
        } else {
            c.set(scale);
            true
        }
    });
    if changed {
        DIRTY.with(|c| c.set(true));
        crate::signal::request_redraw();
    }
}

/// Стартовое значение из билдера — до создания окна, без запроса кадра.
pub(crate) fn set_initial_ui_scale(scale: f32) {
    UI_SCALE.with(|c| c.set(clamp(scale)));
    DIRTY.with(|c| c.set(false));
}

/// Забрать флаг «масштаб сменился» (кадр применяет его один раз).
pub(crate) fn take_ui_scale_dirty() -> bool {
    DIRTY.with(|c| c.replace(false))
}

fn clamp(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(MIN_UI_SCALE, MAX_UI_SCALE)
    } else {
        1.0
    }
}
