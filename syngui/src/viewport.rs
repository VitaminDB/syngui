//! Реактивный размер вьюпорта главного окна.
//!
//! Фреймворк публикует сюда логический размер области, которую получает
//! корневой layout (за вычетом safe area), при старте и при каждом ресайзе.
//! Виджеты читают его через [`viewport_size`] и строят адаптивные раскладки:
//! телефон в портрете, узкое окно на десктопе, браузер любой ширины.
//!
//! Для брейкпоинтов есть [`viewport_below`]: производный bool-сигнал, который
//! меняется только при пересечении порога — подписчики не пересобираются на
//! каждый пиксель ресайза.

use std::cell::Cell;

use crate::core::Size;
use crate::signal::{create_effect, use_signal, RwSignal};

thread_local! {
    static VIEWPORT: Cell<Option<RwSignal<Size>>> = const { Cell::new(None) };
}

/// Логический размер вьюпорта главного окна. Сигнал живёт в runtime главного
/// потока; читать (как и любой сигнал) можно только на главном потоке.
///
/// Значение до первого layout — 1280×720 (дефолт `BuildContext`).
pub fn viewport_size() -> RwSignal<Size> {
    VIEWPORT.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let sig = use_signal(Size::new(1280.0, 720.0));
        slot.set(Some(sig));
        sig
    })
}

/// `true`, пока ширина вьюпорта меньше `width`.
///
/// Каждый вызов создаёт свой сигнал и эффект, привязанные к текущему
/// element-scope, — вызывать один раз при сборке компонента (не внутри
/// часто перезапускаемых реактивных замыканий). Подписчики пересобираются
/// только при пересечении порога, а не на каждый шаг ресайза.
pub fn viewport_below(width: f32) -> RwSignal<bool> {
    let vp = viewport_size();
    let sig = use_signal(vp.get_untracked().width < width);
    create_effect(move || {
        sig.set(vp.get().width < width);
    });
    sig
}

/// Публикация нового размера фреймворком. Дедупликация в `set()`: подписчики
/// будятся только при реальном изменении размера.
pub(crate) fn publish(size: Size) {
    viewport_size().set(size);
}
