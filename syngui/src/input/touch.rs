//! Касания пальцем → события дерева: общий автомат для окна winit и для
//! встроенных поверхностей ([`crate::embed::EmbedView`], layer-shell).
//!
//! Дерево получает сырые `TouchStart/TouchMove/TouchEnd` всех пальцев (их
//! разбирают прокрутки, слайдеры, `GestureDetector` с панорамой и щипком),
//! а поверх автомат синтезирует:
//!
//! - **тап** — `MouseDown`+`MouseUp` левой кнопкой при отпускании, если палец
//!   не ушёл дальше [`TouchConfig::tap_slop`] и второй палец не касался;
//! - **двойной тап** — вместо `MouseDown` второго тапа `DoubleClick`, как у
//!   мыши;
//! - **долгое нажатие** — палец держится [`TouchConfig::long_press`] на месте:
//!   сначала `Event::LongPress` (его берёт `GestureDetector::on_long_press`),
//!   а если никто не взял и [`TouchConfig::long_press_as_secondary`] —
//!   нажатие правой кнопки: так контекстные меню (`ContextMenu`,
//!   `on_secondary_click`, меню правки полей) открываются удержанием без
//!   отдельного кода в каждом виджете. После долгого нажатия отпускание
//!   тапом уже не считается.
//!
//! Таймер долгого нажатия ведёт хост: [`TouchTracker::deadline`] — когда
//! проснуться, [`TouchTracker::poll`] — проверить.

use crate::core::Point;
use crate::input::{Event, EventResult, MouseButton};
use std::collections::HashMap;
use std::time::Duration;
use web_time::Instant;

/// Пороги жестов (логические пиксели и время).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchConfig {
    /// Дальше этого палец «поехал» — это уже не тап и не удержание.
    pub tap_slop: f32,
    /// Сколько держать палец для долгого нажатия.
    pub long_press: Duration,
    /// Долгое нажатие, которое никто не обработал, — правая кнопка.
    pub long_press_as_secondary: bool,
    /// Окно двойного тапа.
    pub double_tap: Duration,
    /// Насколько близко должен быть второй тап.
    pub double_tap_slop: f32,
}

impl Default for TouchConfig {
    fn default() -> Self {
        Self {
            tap_slop: 8.0,
            long_press: Duration::from_millis(450),
            long_press_as_secondary: true,
            double_tap: Duration::from_millis(300),
            double_tap_slop: 24.0,
        }
    }
}

static CONFIG: std::sync::Mutex<Option<TouchConfig>> = std::sync::Mutex::new(None);

/// Пороги для всех хостов процесса (например, из настроек оболочки).
pub fn set_touch_config(cfg: TouchConfig) {
    if let Ok(mut c) = CONFIG.lock() {
        *c = Some(cfg);
    }
}

pub fn touch_config() -> TouchConfig {
    CONFIG.lock().ok().and_then(|c| *c).unwrap_or_default()
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    start: Point,
    pos: Point,
    moved: bool,
}

/// Состояние касаний одной поверхности.
#[derive(Debug, Default)]
pub struct TouchTracker {
    fingers: HashMap<u64, Finger>,
    /// Первый палец жеста — только он даёт тап и долгое нажатие.
    primary: Option<u64>,
    /// Когда первый палец коснулся.
    down_at: Option<Instant>,
    /// Жест уже не может быть тапом (второй палец, долгое нажатие).
    tap_cancelled: bool,
    long_press_fired: bool,
    last_tap: Option<(Instant, Point)>,
}

impl TouchTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Сколько пальцев сейчас на поверхности.
    pub fn active(&self) -> usize {
        self.fingers.len()
    }

    /// Палец коснулся. `dispatch` — доставка события в дерево.
    pub fn down(&mut self, id: u64, pos: Point, dispatch: &mut dyn FnMut(&Event) -> EventResult) {
        if self.fingers.is_empty() {
            self.primary = Some(id);
            self.down_at = Some(Instant::now());
            self.tap_cancelled = false;
            self.long_press_fired = false;
        } else {
            // Второй палец: это щипок или панорама, не тап.
            self.tap_cancelled = true;
        }
        self.fingers.insert(id, Finger { start: pos, pos, moved: false });
        dispatch(&Event::TouchStart { id, position: pos });
    }

    pub fn motion(&mut self, id: u64, pos: Point, dispatch: &mut dyn FnMut(&Event) -> EventResult) {
        let slop = touch_config().tap_slop;
        let Some(f) = self.fingers.get_mut(&id) else { return };
        f.pos = pos;
        if !f.moved && ((pos.x - f.start.x).abs() > slop || (pos.y - f.start.y).abs() > slop) {
            f.moved = true;
            if self.primary == Some(id) {
                self.tap_cancelled = true;
            }
        }
        dispatch(&Event::TouchMove { id, position: pos });
    }

    /// Станет ли отпускание пальца `id` тапом (хост до `up` переводит фокус
    /// клавиатуры на элемент под пальцем, как при щелчке мышью).
    pub fn would_tap(&self, id: u64) -> bool {
        self.primary == Some(id)
            && !self.tap_cancelled
            && !self.long_press_fired
            && self.fingers.get(&id).is_some_and(|f| !f.moved)
    }

    /// Палец поднят. Возвращает `true`, если синтезирован тап.
    pub fn up(&mut self, id: u64, pos: Option<Point>, dispatch: &mut dyn FnMut(&Event) -> EventResult) -> bool {
        let Some(f) = self.fingers.remove(&id) else { return false };
        let pos = pos.unwrap_or(f.pos);
        dispatch(&Event::TouchEnd { id, position: pos });
        let mut tapped = false;
        if self.primary == Some(id) {
            self.primary = None;
            self.down_at = None;
            if !self.tap_cancelled && !self.long_press_fired && !f.moved {
                tapped = true;
                self.tap(pos, dispatch);
            }
        }
        if self.fingers.is_empty() {
            self.primary = None;
            self.down_at = None;
        }
        tapped
    }

    /// Композитор отменил касания (`wl_touch.cancel`): всем пальцам —
    /// `TouchEnd`, без тапа.
    pub fn cancel(&mut self, dispatch: &mut dyn FnMut(&Event) -> EventResult) {
        for (id, f) in std::mem::take(&mut self.fingers) {
            dispatch(&Event::TouchEnd { id, position: f.pos });
        }
        self.primary = None;
        self.down_at = None;
        self.tap_cancelled = true;
    }

    fn tap(&mut self, pos: Point, dispatch: &mut dyn FnMut(&Event) -> EventResult) {
        let cfg = touch_config();
        let double = self.last_tap.is_some_and(|(t, p)| {
            t.elapsed() < cfg.double_tap
                && (pos.x - p.x).abs() < cfg.double_tap_slop
                && (pos.y - p.y).abs() < cfg.double_tap_slop
        });
        let button = MouseButton::Left;
        if double {
            self.last_tap = None;
            dispatch(&Event::DoubleClick { button, position: pos });
        } else {
            self.last_tap = Some((Instant::now(), pos));
            dispatch(&Event::MouseDown { button, position: pos });
        }
        dispatch(&Event::MouseUp { button, position: pos });
    }

    /// Когда хосту проснуться, чтобы проверить долгое нажатие.
    pub fn deadline(&self) -> Option<Instant> {
        if self.long_press_fired || self.tap_cancelled {
            return None;
        }
        self.down_at.map(|t| t + touch_config().long_press)
    }

    /// Проверить таймер долгого нажатия. `true` — оно сработало (событие
    /// уже доставлено).
    pub fn poll(&mut self, dispatch: &mut dyn FnMut(&Event) -> EventResult) -> bool {
        let Some(deadline) = self.deadline() else { return false };
        if Instant::now() < deadline {
            return false;
        }
        let Some(pos) = self.primary.and_then(|id| self.fingers.get(&id)).map(|f| f.pos) else {
            return false;
        };
        self.long_press_fired = true;
        self.tap_cancelled = true;
        let r = dispatch(&Event::LongPress { position: pos });
        if !r.is_handled() && touch_config().long_press_as_secondary {
            let button = MouseButton::Right;
            dispatch(&Event::MouseDown { button, position: pos });
            dispatch(&Event::MouseUp { button, position: pos });
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(f: impl FnOnce(&mut dyn FnMut(&Event) -> EventResult)) -> Vec<String> {
        let mut out = Vec::new();
        let mut d = |e: &Event| {
            out.push(format!("{e:?}").split([' ', '{', '(']).next().unwrap().to_string());
            EventResult::Ignored
        };
        f(&mut d);
        out
    }

    #[test]
    fn tap_synthesizes_click_on_release() {
        let mut t = TouchTracker::new();
        let ev = collect(|d| {
            t.down(1, Point::new(10.0, 10.0), d);
            t.motion(1, Point::new(12.0, 11.0), d);
            t.up(1, None, d);
        });
        assert_eq!(ev, ["TouchStart", "TouchMove", "TouchEnd", "MouseDown", "MouseUp"]);
    }

    #[test]
    fn drag_is_not_a_tap() {
        let mut t = TouchTracker::new();
        let ev = collect(|d| {
            t.down(1, Point::new(10.0, 10.0), d);
            t.motion(1, Point::new(40.0, 10.0), d);
            t.up(1, None, d);
        });
        assert_eq!(ev, ["TouchStart", "TouchMove", "TouchEnd"]);
    }

    #[test]
    fn second_finger_cancels_tap() {
        let mut t = TouchTracker::new();
        let ev = collect(|d| {
            t.down(1, Point::new(10.0, 10.0), d);
            t.down(2, Point::new(50.0, 10.0), d);
            t.up(2, None, d);
            t.up(1, None, d);
        });
        assert!(!ev.contains(&"MouseDown".to_string()));
    }

    #[test]
    fn long_press_falls_back_to_secondary_click() {
        set_touch_config(TouchConfig { long_press: Duration::from_millis(0), ..Default::default() });
        let mut t = TouchTracker::new();
        let ev = collect(|d| {
            t.down(1, Point::new(10.0, 10.0), d);
            assert!(t.poll(d));
            t.up(1, None, d);
        });
        set_touch_config(TouchConfig::default());
        assert_eq!(ev, ["TouchStart", "LongPress", "MouseDown", "MouseUp", "TouchEnd"]);
    }

    #[test]
    fn double_tap() {
        let mut t = TouchTracker::new();
        let ev = collect(|d| {
            t.down(1, Point::new(10.0, 10.0), d);
            t.up(1, None, d);
            t.down(2, Point::new(12.0, 10.0), d);
            t.up(2, None, d);
        });
        assert_eq!(ev.iter().filter(|e| *e == "DoubleClick").count(), 1);
    }
}
