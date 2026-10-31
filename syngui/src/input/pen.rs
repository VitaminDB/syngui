//! Перо (стилус): родные события с давлением, наклоном и ластиком.
//!
//! На Wayland они приходят по протоколу `zwp_tablet_v2` (фича `pen`,
//! `app/wayland_pen`). Доставка — как у мыши: попадание по позиции, захват
//! с `Down` до `Up` (элемент, взявший `Down`, получает `Move`/`Up`, даже
//! когда перо выходит за его края), координаты — локальные, как у `Mouse*`.
//!
//! Элемент, который пером не рисует, может его не разбирать: если `Down`
//! никто не взял, [`PenTracker`] ведёт этот штрих как мышь (`MouseDown`/
//! `MouseMove`/`MouseUp`, левая кнопка, с нажатой кнопкой пера — правая).
//! Наведение (`Hover`), которое никто не взял, — `MouseMove`.

use super::{Event, EventResult, MouseButton};
use crate::core::Point;

/// Фаза события пера.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenPhase {
    /// Перо над экраном, кончик не касается.
    Hover,
    /// Кончик коснулся.
    Down,
    /// Движение с прижатым кончиком.
    Move,
    /// Кончик оторван (перо ещё рядом — дальше `Hover`).
    Up,
    /// Перо ушло от экрана (или из окна).
    Leave,
}

/// Инструмент: кончик пера или ластик (обратный конец).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PenTool {
    #[default]
    Pen,
    Eraser,
}

/// Отсчёт пера.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenSample {
    /// Логические координаты (у элемента — локальные, как у мыши).
    pub position: Point,
    /// Давление 0..1 (0 при наведении).
    pub pressure: f32,
    /// Наклон, градусы.
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub tool: PenTool,
    /// Нажата кнопка на корпусе пера (нижняя).
    pub barrel: bool,
    /// Время отсчёта, мс (часы композитора).
    pub time_ms: u32,
}

impl PenSample {
    pub fn new(position: Point) -> Self {
        Self {
            position,
            pressure: 0.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
            tool: PenTool::Pen,
            barrel: false,
            time_ms: 0,
        }
    }

    pub fn with_pressure(mut self, pressure: f32) -> Self {
        self.pressure = pressure;
        self
    }

    pub fn with_tool(mut self, tool: PenTool) -> Self {
        self.tool = tool;
        self
    }

    pub fn with_barrel(mut self, barrel: bool) -> Self {
        self.barrel = barrel;
        self
    }

    pub fn with_time(mut self, time_ms: u32) -> Self {
        self.time_ms = time_ms;
        self
    }
}

/// Куда [`PenTracker`] отправляет событие.
#[derive(Debug)]
pub enum PenOut<'a> {
    /// Родное `Event::Pen` — в дерево (по позиции, захватчику).
    Pen(&'a Event),
    /// Событие мыши взамен пера (никто не взял `Down`/`Hover`).
    Mouse(&'a Event),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Idle,
    /// Штрих взят элементом — родные события.
    Pen,
    /// Штрих идёт как мышь с этой кнопкой.
    Mouse(MouseButton),
}

/// Автомат пера: родные события и запасной путь мыши.
#[derive(Debug, Default)]
pub struct PenTracker {
    mode: Mode,
    /// Наведение последним брал элемент (иначе шло как мышь).
    hover_native: bool,
    /// Перо у окна.
    near: bool,
}

impl PenTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Кончик прижат (штрих идёт).
    pub fn is_down(&self) -> bool {
        self.mode != Mode::Idle
    }

    /// Перо рядом с окном (между первым событием и `Leave`).
    pub fn is_near(&self) -> bool {
        self.near
    }

    /// Текущий штрих идёт запасным путём мыши.
    pub fn is_mouse_fallback(&self) -> bool {
        matches!(self.mode, Mode::Mouse(_))
    }

    /// Обработать отсчёт. `dispatch` отправляет событие в дерево и
    /// возвращает, взял ли его кто-то (для `Mouse` ответ не важен).
    pub fn handle(
        &mut self,
        phase: PenPhase,
        sample: PenSample,
        dispatch: &mut dyn FnMut(PenOut<'_>) -> EventResult,
    ) -> EventResult {
        let pos = sample.position;
        let pen = |phase| Event::Pen { phase, sample };
        match phase {
            PenPhase::Hover => {
                self.near = true;
                if self.mode != Mode::Idle {
                    // Пропущенный Up (композитор оборвал касание) — закрыть штрих.
                    self.finish(sample, dispatch);
                }
                let r = dispatch(PenOut::Pen(&pen(PenPhase::Hover)));
                self.hover_native = r.is_handled();
                if !self.hover_native {
                    dispatch(PenOut::Mouse(&Event::MouseMove(pos)));
                }
                r
            }
            PenPhase::Down => {
                self.near = true;
                if self.mode != Mode::Idle {
                    self.finish(sample, dispatch);
                }
                let r = dispatch(PenOut::Pen(&pen(PenPhase::Down)));
                self.hover_native = r.is_handled();
                if r.is_handled() {
                    self.mode = Mode::Pen;
                    return r;
                }
                let button = if sample.barrel {
                    MouseButton::Right
                } else {
                    MouseButton::Left
                };
                self.mode = Mode::Mouse(button);
                dispatch(PenOut::Mouse(&Event::MouseMove(pos)));
                dispatch(PenOut::Mouse(&Event::MouseDown {
                    button,
                    position: pos,
                }))
            }
            PenPhase::Move => match self.mode {
                Mode::Pen => dispatch(PenOut::Pen(&pen(PenPhase::Move))),
                Mode::Mouse(_) => dispatch(PenOut::Mouse(&Event::MouseMove(pos))),
                // Движение без касания — наведение.
                Mode::Idle => self.handle(PenPhase::Hover, sample, dispatch),
            },
            PenPhase::Up => self.finish(sample, dispatch),
            PenPhase::Leave => {
                let r = if self.mode != Mode::Idle {
                    self.finish(sample, dispatch)
                } else {
                    EventResult::Ignored
                };
                self.near = false;
                dispatch(PenOut::Pen(&pen(PenPhase::Leave)));
                if !self.hover_native {
                    // Наведение уходит: точка за экраном, как у CursorLeft.
                    dispatch(PenOut::Mouse(&Event::MouseMove(Point::new(-1.0, -1.0))));
                }
                self.hover_native = false;
                r
            }
        }
    }

    /// Закрыть штрих: `Up` захватчику или отпустить кнопку мыши.
    fn finish(
        &mut self,
        sample: PenSample,
        dispatch: &mut dyn FnMut(PenOut<'_>) -> EventResult,
    ) -> EventResult {
        let mode = std::mem::take(&mut self.mode);
        match mode {
            Mode::Idle => EventResult::Ignored,
            Mode::Pen => dispatch(PenOut::Pen(&Event::Pen {
                phase: PenPhase::Up,
                sample,
            })),
            Mode::Mouse(button) => {
                dispatch(PenOut::Mouse(&Event::MouseMove(sample.position)));
                dispatch(PenOut::Mouse(&Event::MouseUp {
                    button,
                    position: sample.position,
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(
        tr: &mut PenTracker,
        phase: PenPhase,
        x: f32,
        take: bool,
        log: &mut Vec<String>,
    ) {
        let s = PenSample::new(Point::new(x, 0.0)).with_pressure(0.5);
        tr.handle(phase, s, &mut |out| match out {
            PenOut::Pen(Event::Pen { phase, .. }) => {
                log.push(format!("pen {phase:?}"));
                if take {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            PenOut::Mouse(e) => {
                log.push(match e {
                    Event::MouseMove(_) => "mm".into(),
                    Event::MouseDown { button, .. } => format!("md {button:?}"),
                    Event::MouseUp { button, .. } => format!("mu {button:?}"),
                    other => format!("{other:?}"),
                });
                EventResult::Handled
            }
            PenOut::Pen(other) => panic!("{other:?}"),
        });
    }

    #[test]
    fn native_stroke() {
        let mut tr = PenTracker::new();
        let mut log = Vec::new();
        run(&mut tr, PenPhase::Down, 1.0, true, &mut log);
        assert!(tr.is_down() && !tr.is_mouse_fallback());
        run(&mut tr, PenPhase::Move, 2.0, true, &mut log);
        run(&mut tr, PenPhase::Move, 3.0, true, &mut log);
        run(&mut tr, PenPhase::Up, 3.0, true, &mut log);
        assert!(!tr.is_down());
        assert_eq!(log, ["pen Down", "pen Move", "pen Move", "pen Up"]);
    }

    #[test]
    fn fallback_to_mouse() {
        let mut tr = PenTracker::new();
        let mut log = Vec::new();
        run(&mut tr, PenPhase::Down, 1.0, false, &mut log);
        assert!(tr.is_mouse_fallback());
        run(&mut tr, PenPhase::Move, 2.0, false, &mut log);
        run(&mut tr, PenPhase::Up, 2.0, false, &mut log);
        assert_eq!(log, ["pen Down", "mm", "md Left", "mm", "mm", "mu Left"]);
    }

    #[test]
    fn barrel_is_right_button() {
        let mut tr = PenTracker::new();
        let mut buttons = Vec::new();
        let s = PenSample::new(Point::new(0.0, 0.0)).with_barrel(true);
        for phase in [PenPhase::Down, PenPhase::Up] {
            tr.handle(phase, s, &mut |out| {
                if let PenOut::Mouse(Event::MouseDown { button, .. } | Event::MouseUp { button, .. }) = out {
                    buttons.push(*button);
                }
                EventResult::Ignored
            });
        }
        assert_eq!(buttons, [MouseButton::Right, MouseButton::Right]);
    }

    #[test]
    fn hover_and_leave() {
        let mut tr = PenTracker::new();
        let mut log = Vec::new();
        run(&mut tr, PenPhase::Hover, 1.0, false, &mut log);
        assert!(tr.is_near());
        run(&mut tr, PenPhase::Leave, 1.0, false, &mut log);
        assert!(!tr.is_near());
        assert_eq!(log, ["pen Hover", "mm", "pen Leave", "mm"]);
        // Наведение взято — мыши нет.
        let mut log = Vec::new();
        run(&mut tr, PenPhase::Hover, 1.0, true, &mut log);
        run(&mut tr, PenPhase::Leave, 1.0, true, &mut log);
        assert_eq!(log, ["pen Hover", "pen Leave"]);
    }

    #[test]
    fn leave_closes_stroke() {
        let mut tr = PenTracker::new();
        let mut log = Vec::new();
        run(&mut tr, PenPhase::Down, 1.0, true, &mut log);
        run(&mut tr, PenPhase::Leave, 1.0, true, &mut log);
        assert_eq!(log, ["pen Down", "pen Up", "pen Leave"]);
        assert!(!tr.is_down());
    }
}
