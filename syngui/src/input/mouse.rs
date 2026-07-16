use crate::core::Point;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollDelta {
    Lines { x: f32, y: f32 },
    Pixels { x: f32, y: f32 },
}

#[derive(Clone, Debug)]
pub enum MouseEvent {
    Moved { position: Point, delta: Point },
    Down { button: MouseButton, position: Point },
    Up { button: MouseButton, position: Point },
    Wheel { delta: ScrollDelta, position: Point },
    Entered,
    Left,
}
