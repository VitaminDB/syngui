//! Размещение всплывающих слоёв внутри окна.
//!
//! Меню, поповеры и дропдауны раскрываются от якоря и обязаны остаться в
//! пределах окна. Правило одно для обеих осей:
//!
//! 1. пробуем предпочтительную сторону;
//! 2. не влезает — переворачиваемся на другую сторону якоря;
//! 3. не влезает и так — прижимаемся к краю окна.
//!
//! Пропуск третьего шага и был причиной того, что меню выбора языка уезжало
//! за нижнюю границу окна: перевёрнутый вариант тоже не влезал, и слой
//! оставался там, где не помещался.

/// Начало отрезка длиной `len` внутри `0..viewport_len`.
///
/// `flip_end` — конец «перевёрнутого» варианта: слой займёт
/// `flip_end - len ..= flip_end`. Для меню, раскрывающегося вниз от якоря,
/// это верхняя граница якоря; для подменю, уходящего вправо, — левая.
///
/// `viewport_len <= 0.0` означает «размер окна ещё не известен» — тогда
/// возвращается предпочтительное значение без правок.
pub fn fit_span(preferred_start: f32, len: f32, flip_end: f32, viewport_len: f32) -> f32 {
    if viewport_len <= 0.0 {
        return preferred_start;
    }
    if preferred_start + len <= viewport_len {
        return preferred_start.max(0.0);
    }
    let flipped = flip_end - len;
    if flipped >= 0.0 {
        return flipped;
    }
    (viewport_len - len).max(0.0)
}

/// То же, но без переворота — только прижать к границам окна.
///
/// Если слой длиннее окна, он прижимается к началу: видно первый экран
/// содержимого, а не середина.
pub fn clamp_span(start: f32, len: f32, viewport_len: f32) -> f32 {
    if viewport_len <= 0.0 {
        return start;
    }
    start.max(0.0).min((viewport_len - len).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_preferred_side_when_it_fits() {
        assert_eq!(fit_span(100.0, 120.0, 100.0, 800.0), 100.0);
    }

    #[test]
    fn flips_when_preferred_side_overflows() {
        // Якорь внизу окна: 700 + 120 > 800, переворачиваемся вверх до 700.
        assert_eq!(fit_span(700.0, 120.0, 700.0, 800.0), 580.0);
    }

    #[test]
    fn pins_to_edge_when_neither_side_fits() {
        // Ни вниз (300 + 600 > 800), ни вверх (300 - 600 < 0) — прижимаем к низу.
        assert_eq!(fit_span(300.0, 600.0, 300.0, 800.0), 200.0);
    }

    #[test]
    fn pins_to_start_when_longer_than_window() {
        assert_eq!(fit_span(100.0, 900.0, 100.0, 800.0), 0.0);
    }

    #[test]
    fn unknown_viewport_leaves_position_alone() {
        assert_eq!(fit_span(700.0, 120.0, 700.0, 0.0), 700.0);
        assert_eq!(clamp_span(700.0, 120.0, 0.0), 700.0);
    }

    #[test]
    fn clamp_pulls_back_from_both_edges() {
        assert_eq!(clamp_span(-20.0, 100.0, 800.0), 0.0);
        assert_eq!(clamp_span(760.0, 100.0, 800.0), 700.0);
        assert_eq!(clamp_span(300.0, 100.0, 800.0), 300.0);
    }
}
