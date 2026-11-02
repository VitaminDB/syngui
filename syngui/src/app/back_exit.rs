//! «Назад» на корневом экране телефона: первый жест, который не обработало приложение, показывает поверх
//! кадра подсказку «Нажмите «назад» ещё раз, чтобы закрыть», второй в течение [`WINDOW`] закрывает окно —
//! как кнопка закрытия (с `close_allowed` и сворачиванием в трей). Без этого программу на телефоне нельзя
//! было закрыть жестом: synshell шлёт окну клавишу XF86Back, а необработанный «назад» молча пропадал.
//!
//! Только на телефоне (`SYNSHELL_FORM_FACTOR=phone`): на ПК XF86Back — боковая кнопка мыши, закрытие
//! окна по ней было бы сюрпризом. На Android необработанный «назад» закрывает приложение сразу (система).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::core::{Color, Point, Rect, Size};
use crate::render::DisplayList;

/// Сколько ждать второго «назад».
const WINDOW: Duration = Duration::from_millis(2000);
/// После подсказки ещё столько кадров идёт непрерывно, чтобы она исчезла с экрана.
const TAIL: Duration = Duration::from_millis(250);

static ARMED: Mutex<Option<Instant>> = Mutex::new(None);

fn armed() -> std::sync::MutexGuard<'static, Option<Instant>> {
    ARMED.lock().unwrap_or_else(|e| e.into_inner())
}

/// Включено ли для этого процесса (телефон).
pub(crate) fn enabled() -> bool {
    std::env::var("SYNSHELL_FORM_FACTOR").is_ok_and(|v| v.trim().eq_ignore_ascii_case("phone"))
}

/// Необработанный «назад». `true` — второй подряд: закрыть окно.
pub(crate) fn press() -> bool {
    let mut a = armed();
    if a.is_some_and(|t| t.elapsed() < WINDOW) {
        *a = None;
        return true;
    }
    *a = Some(Instant::now());
    false
}

/// Приложение обработало «назад» (вернулось на экран выше) — подсказку прочь.
pub(crate) fn reset() {
    *armed() = None;
}

/// Нужны ли кадры: подсказка на экране или только что ушла.
pub(crate) fn active() -> bool {
    armed().is_some_and(|t| t.elapsed() < WINDOW + TAIL)
}

/// Подсказка внизу экрана, по центру.
pub(crate) fn draw(dl: &mut DisplayList) {
    if !armed().is_some_and(|t| t.elapsed() < WINDOW) {
        return;
    }
    let surface = dl.surface_size();
    if surface.width <= 0.0 || surface.height <= 0.0 {
        return;
    }
    let text = "Нажмите «назад» ещё раз, чтобы закрыть";
    let font = 14.0;
    let pad_x = 18.0;
    let h = 40.0;
    // ширина текста — с запасом по средней ширине глифа (атласа шрифтов здесь нет; кириллица до ~0.6 кегля,
    // при 0.56 строка на телефоне переносилась)
    let tw = text.chars().count() as f32 * font * 0.66;
    let w = (tw + pad_x * 2.0).min(surface.width - 24.0);
    // над нижними кнопками приложений (запись, плеер, панель действий), а не поверх них
    let bottom = (surface.height * 0.2).clamp(96.0, 200.0);
    let rect = Rect::new(Point::new((surface.width - w) * 0.5, surface.height - h - bottom), Size::new(w, h));
    dl.begin_overlay_absolute();
    dl.push_rect(rect, Color::new(0.12, 0.12, 0.14, 0.94), [h * 0.5; 4]);
    let tr = Rect::new(Point::new(rect.origin.x + pad_x, rect.origin.y + (h - font * 1.3) * 0.5), Size::new(w - pad_x * 2.0, font * 1.3));
    dl.push_text(text, tr, Color::new(0.95, 0.95, 0.97, 1.0), font);
    dl.end_overlay();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twice_closes() {
        reset();
        assert!(!press());
        assert!(active());
        assert!(press());
        assert!(!active());
        assert!(!press());
        reset();
        assert!(!active());
    }
}
