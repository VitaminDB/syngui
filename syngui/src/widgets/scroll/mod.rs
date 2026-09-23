pub mod scroll_view;
pub mod scrollbar;

pub use crate::widgets::containers::page::ScrollbarPolicy;
pub use scroll_view::{ScrollDirection, ScrollView};
pub use scrollbar::{
    effective_opacity, horizontal_thumb_rect, horizontal_track_rect, render_horizontal,
    render_vertical, show_horizontal, show_vertical, vertical_thumb_rect, vertical_track_rect,
    ScrollbarFader, ScrollbarGeom, ScrollbarInteraction, ScrollbarStyle, SCROLLBAR_HIT_MARGIN,
};

use std::sync::atomic::{AtomicU8, Ordering};

/// Инерция после прокрутки колесом мыши.
///
/// Колесо прокручивает дискретными щелчками, и докат после каждого щелчка
/// привычен не всем: на длинных списках он уводит содержимое на страницу
/// дальше нужного, а анимация тянется секунды. Тач-жесты (свайп) настройка
/// не затрагивает — там инерция нужна.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelMomentum {
    /// Без доката: сдвиг ровно на шаг колеса.
    Off,
    /// Короткий докат — доли секунды.
    Short,
    /// Прежнее поведение: длинный докат, как у свайпа.
    Normal,
}

static WHEEL_MOMENTUM: AtomicU8 = AtomicU8::new(2);

/// Задать инерцию колеса для всех прокручиваемых областей приложения.
pub fn set_wheel_momentum(mode: WheelMomentum) {
    let v = match mode {
        WheelMomentum::Off => 0,
        WheelMomentum::Short => 1,
        WheelMomentum::Normal => 2,
    };
    WHEEL_MOMENTUM.store(v, Ordering::Relaxed);
}

pub fn wheel_momentum() -> WheelMomentum {
    match WHEEL_MOMENTUM.load(Ordering::Relaxed) {
        0 => WheelMomentum::Off,
        1 => WheelMomentum::Short,
        _ => WheelMomentum::Normal,
    }
}

/// Параметры доката после щелчка колеса: множитель импульса и трение за
/// кадр 60 Гц. `None` — доката нет.
pub(crate) fn wheel_coast(normal_friction: f32) -> Option<(f32, f32)> {
    match wheel_momentum() {
        WheelMomentum::Off => None,
        WheelMomentum::Short => Some((0.5, 0.9)),
        WheelMomentum::Normal => Some((1.0, normal_friction)),
    }
}
