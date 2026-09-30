//! Виброотклик: виджеты и автомат касаний сообщают, что случилось
//! (удержание сработало, переключатель щёлкнул), а чем вибрировать — решает
//! приложение, поставив обработчик [`set_haptic_handler`] (своё устройство,
//! свои настройки). Без обработчика отклики молча пропадают.

use std::sync::{Arc, RwLock};

/// Что произошло.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Haptic {
    /// Долгое нажатие пальцем что-то сделало (выбор, меню).
    LongPress,
    /// Лёгкий щелчок: переключатель, шаг ползунка.
    Tick,
}

type Handler = Arc<dyn Fn(Haptic) + Send + Sync>;

static HANDLER: RwLock<Option<Handler>> = RwLock::new(None);

/// Чем отзываться на [`Haptic`] (одна функция на процесс).
pub fn set_haptic_handler(f: impl Fn(Haptic) + Send + Sync + 'static) {
    if let Ok(mut h) = HANDLER.write() {
        *h = Some(Arc::new(f));
    }
}

/// Отклик (зовут виджеты и автомат касаний).
pub fn haptic(kind: Haptic) {
    let h = HANDLER.read().ok().and_then(|h| h.clone());
    if let Some(f) = h {
        f(kind);
    }
}
