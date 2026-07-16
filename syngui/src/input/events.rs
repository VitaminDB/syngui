use crate::core::Point;
use crate::input::{Key, MouseButton};
use std::time::Duration;

/// Fallback-дефолт интервала двойного клика, когда системную настройку
/// прочитать не удалось. В рантайме порог берётся из настройки ОС/DE через
/// [`crate::input::resolve_double_click_interval`]; это значение — лишь
/// запасное. 500 мс — самый распространённый OS-дефолт (Windows
/// `GetDoubleClickTime`); прежние 250 мс были слишком коротки и роняли
/// нормальные по скорости двойные клики в два одиночных.
pub const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Debug)]
pub enum Event {
    Resized { width: u32, height: u32 },
    CloseRequested,
    MouseMove(Point),
    MouseDown {
        button: MouseButton,
        position: Point,
    },
    MouseUp {
        button: MouseButton,
        position: Point,
    },
    MouseWheel {
        delta: f32,
        delta_x: f32,
        position: Point,
    },
    KeyDown(Key),
    KeyUp(Key),
    CharInput(char),
    TouchStart { id: u64, position: Point },
    TouchMove { id: u64, position: Point },
    TouchEnd { id: u64, position: Point },
    DoubleClick {
        button: MouseButton,
        position: Point,
    },
    FocusGained,
    FocusLost,
    BackPressed,
    Custom(String),
    DragStart { position: Point, data: DragData },
    DragMove { position: Point, data: DragData },
    DragEnter { position: Point, data: DragData },
    DragLeave,
    Drop { position: Point, data: DragData },
    DragEnd { cancelled: bool },
    ImePreedit { text: String, cursor: Option<(usize, usize)> },
    ImeCommit(String),
    ImeEnabled,
    ImeDisabled,
    ImeReplace(String),
}

impl Event {
    pub fn with_offset(&self, dx: f32, dy: f32) -> Event {
        match self {
            Event::MouseMove(pos) => Event::MouseMove(Point::new(pos.x + dx, pos.y + dy)),
            Event::MouseDown { button, position } => Event::MouseDown {
                button: *button,
                position: Point::new(position.x + dx, position.y + dy),
            },
            Event::MouseUp { button, position } => Event::MouseUp {
                button: *button,
                position: Point::new(position.x + dx, position.y + dy),
            },
            Event::DoubleClick { button, position } => Event::DoubleClick {
                button: *button,
                position: Point::new(position.x + dx, position.y + dy),
            },
            Event::MouseWheel {
                delta,
                delta_x,
                position,
            } => Event::MouseWheel {
                delta: *delta,
                delta_x: *delta_x,
                position: Point::new(position.x + dx, position.y + dy),
            },
            Event::TouchStart { id, position } => Event::TouchStart {
                id: *id,
                position: Point::new(position.x + dx, position.y + dy),
            },
            Event::TouchMove { id, position } => Event::TouchMove {
                id: *id,
                position: Point::new(position.x + dx, position.y + dy),
            },
            Event::TouchEnd { id, position } => Event::TouchEnd {
                id: *id,
                position: Point::new(position.x + dx, position.y + dy),
            },
            Event::DragStart { position, data } => Event::DragStart {
                position: Point::new(position.x + dx, position.y + dy),
                data: data.clone(),
            },
            Event::DragMove { position, data } => Event::DragMove {
                position: Point::new(position.x + dx, position.y + dy),
                data: data.clone(),
            },
            Event::DragEnter { position, data } => Event::DragEnter {
                position: Point::new(position.x + dx, position.y + dy),
                data: data.clone(),
            },
            Event::Drop { position, data } => Event::Drop {
                position: Point::new(position.x + dx, position.y + dy),
                data: data.clone(),
            },
            other => other.clone(),
        }
    }

    pub fn with_inverse_transform(&self, scroll: Point, scale: f32) -> Event {
        debug_assert!(scale > 0.0, "event_scale must be positive (got {scale})");
        let k = scale.max(f32::EPSILON);
        let map = |p: Point| Point::new((p.x + scroll.x) / k, (p.y + scroll.y) / k);
        match self {
            Event::MouseMove(pos) => Event::MouseMove(map(*pos)),
            Event::MouseDown { button, position } => Event::MouseDown {
                button: *button,
                position: map(*position),
            },
            Event::MouseUp { button, position } => Event::MouseUp {
                button: *button,
                position: map(*position),
            },
            Event::DoubleClick { button, position } => Event::DoubleClick {
                button: *button,
                position: map(*position),
            },
            Event::MouseWheel {
                delta,
                delta_x,
                position,
            } => Event::MouseWheel {
                delta: *delta,
                delta_x: *delta_x,
                position: map(*position),
            },
            Event::TouchStart { id, position } => Event::TouchStart {
                id: *id,
                position: map(*position),
            },
            Event::TouchMove { id, position } => Event::TouchMove {
                id: *id,
                position: map(*position),
            },
            Event::TouchEnd { id, position } => Event::TouchEnd {
                id: *id,
                position: map(*position),
            },
            Event::DragStart { position, data } => Event::DragStart {
                position: map(*position),
                data: data.clone(),
            },
            Event::DragMove { position, data } => Event::DragMove {
                position: map(*position),
                data: data.clone(),
            },
            Event::DragEnter { position, data } => Event::DragEnter {
                position: map(*position),
                data: data.clone(),
            },
            Event::Drop { position, data } => Event::Drop {
                position: map(*position),
                data: data.clone(),
            },
            other => other.clone(),
        }
    }

    pub fn position(&self) -> Option<Point> {
        match self {
            Event::MouseMove(pos) => Some(*pos),
            Event::MouseDown { position, .. } => Some(*position),
            Event::MouseUp { position, .. } => Some(*position),
            Event::DoubleClick { position, .. } => Some(*position),
            Event::MouseWheel { position, .. } => Some(*position),
            Event::TouchStart { position, .. } => Some(*position),
            Event::TouchMove { position, .. } => Some(*position),
            Event::TouchEnd { position, .. } => Some(*position),
            Event::DragStart { position, .. } => Some(*position),
            Event::DragMove { position, .. } => Some(*position),
            Event::DragEnter { position, .. } => Some(*position),
            Event::Drop { position, .. } => Some(*position),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DragData {
    pub drag_type: String,
    pub payload: String,
    pub source_id: u64,
    pub label: Option<String>,
}

impl DragData {
    pub const TYPE_FILE: &'static str = "file";

    pub fn new(drag_type: impl Into<String>, payload: impl Into<String>, source_id: u64) -> Self {
        Self {
            drag_type: drag_type.into(),
            payload: payload.into(),
            source_id,
            label: None,
        }
    }

    pub fn external_file(path: &std::path::Path) -> Self {
        let payload = path.to_string_lossy().into_owned();
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        Self {
            drag_type: Self::TYPE_FILE.to_string(),
            payload,
            source_id: 0,
            label,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EventResult {
    #[default]
    Ignored,
    Handled,
    Captured,
}

impl EventResult {
    pub fn is_handled(&self) -> bool {
        matches!(self, EventResult::Handled | EventResult::Captured)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CursorIcon {
    #[default]
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    Move,
    NotAllowed,
    Crosshair,
    ColResize,
    RowResize,
    NwResize,
    NeResize,
    SeResize,
    SwResize,
    NResize,
    EResize,
    SResize,
    WResize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const fn empty() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.shift && !self.ctrl && !self.alt && !self.meta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn event_result_default_is_ignored() {
        assert_eq!(EventResult::default(), EventResult::Ignored);
    }

    #[test]
    fn event_result_is_handled() {
        assert!(!EventResult::Ignored.is_handled());
        assert!(EventResult::Handled.is_handled());
        assert!(EventResult::Captured.is_handled());
    }

    #[test]
    fn modifiers_empty() {
        let m = Modifiers::empty();
        assert!(m.is_empty());
        assert!(!m.shift);
        assert!(!m.ctrl);
        assert!(!m.alt);
        assert!(!m.meta);
    }

    #[test]
    fn modifiers_default_is_empty() {
        assert!(Modifiers::default().is_empty());
    }

    #[test]
    fn modifiers_not_empty_with_shift() {
        let m = Modifiers {
            shift: true,
            ..Modifiers::empty()
        };
        assert!(!m.is_empty());
    }

    #[test]
    fn modifiers_not_empty_with_ctrl() {
        let m = Modifiers {
            ctrl: true,
            ..Modifiers::empty()
        };
        assert!(!m.is_empty());
    }

    #[test]
    fn modifiers_not_empty_with_alt() {
        let m = Modifiers {
            alt: true,
            ..Modifiers::empty()
        };
        assert!(!m.is_empty());
    }

    #[test]
    fn modifiers_not_empty_with_meta() {
        let m = Modifiers {
            meta: true,
            ..Modifiers::empty()
        };
        assert!(!m.is_empty());
    }

    #[test]
    fn drag_data_new() {
        let d = DragData::new("text", "hello", 42);
        assert_eq!(d.drag_type, "text");
        assert_eq!(d.payload, "hello");
        assert_eq!(d.source_id, 42);
    }

    #[test]
    fn drag_data_clone() {
        let d = DragData::new("file", "path.txt", 1);
        let d2 = d.clone();
        assert_eq!(d2.drag_type, "file");
        assert_eq!(d2.source_id, 1);
    }

    #[test]
    fn cursor_icon_default() {
        assert_eq!(CursorIcon::default(), CursorIcon::Default);
    }

    #[test]
    fn position_mouse_move() {
        let e = Event::MouseMove(pt(10.0, 20.0));
        assert_eq!(e.position(), Some(pt(10.0, 20.0)));
    }

    #[test]
    fn position_mouse_down() {
        let e = Event::MouseDown {
            button: MouseButton::Left,
            position: pt(5.0, 15.0),
        };
        assert_eq!(e.position(), Some(pt(5.0, 15.0)));
    }

    #[test]
    fn position_mouse_up() {
        let e = Event::MouseUp {
            button: MouseButton::Right,
            position: pt(3.0, 7.0),
        };
        assert_eq!(e.position(), Some(pt(3.0, 7.0)));
    }

    #[test]
    fn position_double_click() {
        let e = Event::DoubleClick {
            button: MouseButton::Left,
            position: pt(1.0, 2.0),
        };
        assert_eq!(e.position(), Some(pt(1.0, 2.0)));
    }

    #[test]
    fn position_mouse_wheel() {
        let e = Event::MouseWheel {
            delta: 3.0,
            delta_x: 0.0,
            position: pt(50.0, 60.0),
        };
        assert_eq!(e.position(), Some(pt(50.0, 60.0)));
    }

    #[test]
    fn position_touch_start() {
        let e = Event::TouchStart {
            id: 1,
            position: pt(100.0, 200.0),
        };
        assert_eq!(e.position(), Some(pt(100.0, 200.0)));
    }

    #[test]
    fn position_touch_move() {
        let e = Event::TouchMove {
            id: 1,
            position: pt(110.0, 210.0),
        };
        assert_eq!(e.position(), Some(pt(110.0, 210.0)));
    }

    #[test]
    fn position_touch_end() {
        let e = Event::TouchEnd {
            id: 1,
            position: pt(120.0, 220.0),
        };
        assert_eq!(e.position(), Some(pt(120.0, 220.0)));
    }

    #[test]
    fn position_drag_events() {
        let data = DragData::new("t", "p", 0);
        assert!(Event::DragStart {
            position: pt(1.0, 2.0),
            data: data.clone()
        }
        .position()
        .is_some());
        assert!(Event::DragMove {
            position: pt(1.0, 2.0),
            data: data.clone()
        }
        .position()
        .is_some());
        assert!(Event::DragEnter {
            position: pt(1.0, 2.0),
            data: data.clone()
        }
        .position()
        .is_some());
        assert!(Event::Drop {
            position: pt(1.0, 2.0),
            data: data.clone()
        }
        .position()
        .is_some());
    }

    #[test]
    fn position_non_positional_events() {
        assert!(Event::Resized {
            width: 100,
            height: 200
        }
        .position()
        .is_none());
        assert!(Event::CloseRequested.position().is_none());
        assert!(Event::KeyDown(Key::A).position().is_none());
        assert!(Event::KeyUp(Key::B).position().is_none());
        assert!(Event::CharInput('x').position().is_none());
        assert!(Event::FocusGained.position().is_none());
        assert!(Event::FocusLost.position().is_none());
        assert!(Event::BackPressed.position().is_none());
        assert!(Event::DragLeave.position().is_none());
        assert!(Event::DragEnd { cancelled: false }.position().is_none());
        assert!(Event::Custom("test".into()).position().is_none());
    }

    #[test]
    fn with_offset_mouse_move() {
        let e = Event::MouseMove(pt(10.0, 20.0)).with_offset(5.0, -3.0);
        assert_eq!(e.position(), Some(pt(15.0, 17.0)));
    }

    #[test]
    fn with_offset_mouse_down() {
        let e = Event::MouseDown {
            button: MouseButton::Left,
            position: pt(0.0, 0.0),
        }
        .with_offset(100.0, 200.0);
        assert_eq!(e.position(), Some(pt(100.0, 200.0)));
    }

    #[test]
    fn with_offset_mouse_up() {
        let e = Event::MouseUp {
            button: MouseButton::Right,
            position: pt(50.0, 50.0),
        }
        .with_offset(-10.0, -20.0);
        assert_eq!(e.position(), Some(pt(40.0, 30.0)));
    }

    #[test]
    fn with_offset_double_click() {
        let e = Event::DoubleClick {
            button: MouseButton::Left,
            position: pt(10.0, 10.0),
        }
        .with_offset(5.0, 5.0);
        assert_eq!(e.position(), Some(pt(15.0, 15.0)));
    }

    #[test]
    fn with_offset_mouse_wheel() {
        let e = Event::MouseWheel {
            delta: 1.0,
            delta_x: 0.5,
            position: pt(10.0, 20.0),
        }
        .with_offset(3.0, 4.0);
        if let Event::MouseWheel {
            delta,
            delta_x,
            position,
        } = e
        {
            assert_eq!(delta, 1.0);
            assert_eq!(delta_x, 0.5);
            assert_eq!(position, pt(13.0, 24.0));
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn with_offset_touch_events() {
        let e = Event::TouchStart {
            id: 5,
            position: pt(10.0, 10.0),
        }
        .with_offset(1.0, 2.0);
        if let Event::TouchStart { id, position } = e {
            assert_eq!(id, 5);
            assert_eq!(position, pt(11.0, 12.0));
        } else {
            panic!("wrong variant");
        }

        let e = Event::TouchMove {
            id: 5,
            position: pt(20.0, 20.0),
        }
        .with_offset(1.0, 2.0);
        assert_eq!(e.position(), Some(pt(21.0, 22.0)));

        let e = Event::TouchEnd {
            id: 5,
            position: pt(30.0, 30.0),
        }
        .with_offset(1.0, 2.0);
        assert_eq!(e.position(), Some(pt(31.0, 32.0)));
    }

    #[test]
    fn with_offset_drag_events() {
        let data = DragData::new("t", "p", 0);
        let e = Event::DragStart {
            position: pt(10.0, 10.0),
            data: data.clone(),
        }
        .with_offset(5.0, 5.0);
        assert_eq!(e.position(), Some(pt(15.0, 15.0)));

        let e = Event::DragMove {
            position: pt(10.0, 10.0),
            data: data.clone(),
        }
        .with_offset(5.0, 5.0);
        assert_eq!(e.position(), Some(pt(15.0, 15.0)));

        let e = Event::DragEnter {
            position: pt(10.0, 10.0),
            data: data.clone(),
        }
        .with_offset(5.0, 5.0);
        assert_eq!(e.position(), Some(pt(15.0, 15.0)));

        let e = Event::Drop {
            position: pt(10.0, 10.0),
            data: data.clone(),
        }
        .with_offset(5.0, 5.0);
        assert_eq!(e.position(), Some(pt(15.0, 15.0)));
    }

    #[test]
    fn with_offset_non_positional_unchanged() {
        if let Event::KeyDown(Key::A) = Event::KeyDown(Key::A).with_offset(5.0, 5.0) {
        } else {
            panic!("KeyDown should pass through");
        }

        if let Event::CharInput('z') = Event::CharInput('z').with_offset(1.0, 1.0) {
        } else {
            panic!("CharInput should pass through");
        }

        if let Event::FocusGained = Event::FocusGained.with_offset(1.0, 1.0) {
        } else {
            panic!("FocusGained should pass through");
        }
    }

    #[test]
    fn with_inverse_transform_mouse_move() {
        let e = Event::MouseMove(pt(20.0, 40.0))
            .with_inverse_transform(pt(10.0, 10.0), 2.0);
        assert_eq!(e.position(), Some(pt(15.0, 25.0)));
    }

    #[test]
    fn with_inverse_transform_mouse_down() {
        let e = Event::MouseDown {
            button: MouseButton::Left,
            position: pt(100.0, 200.0),
        }
        .with_inverse_transform(pt(0.0, 0.0), 2.0);
        assert_eq!(e.position(), Some(pt(50.0, 100.0)));
    }

    #[test]
    fn with_inverse_transform_mouse_up() {
        let e = Event::MouseUp {
            button: MouseButton::Right,
            position: pt(50.0, 50.0),
        }
        .with_inverse_transform(pt(-10.0, -20.0), 1.0);
        assert_eq!(e.position(), Some(pt(40.0, 30.0)));
    }

    #[test]
    fn with_inverse_transform_double_click() {
        let e = Event::DoubleClick {
            button: MouseButton::Left,
            position: pt(40.0, 40.0),
        }
        .with_inverse_transform(pt(0.0, 0.0), 4.0);
        assert_eq!(e.position(), Some(pt(10.0, 10.0)));
    }

    #[test]
    fn with_inverse_transform_mouse_wheel() {
        let e = Event::MouseWheel {
            delta: 1.0,
            delta_x: 0.5,
            position: pt(40.0, 80.0),
        }
        .with_inverse_transform(pt(0.0, 0.0), 2.0);
        if let Event::MouseWheel {
            delta,
            delta_x,
            position,
        } = e
        {
            assert_eq!(delta, 1.0);
            assert_eq!(delta_x, 0.5);
            assert_eq!(position, pt(20.0, 40.0));
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn with_inverse_transform_touch_events() {
        let e = Event::TouchStart {
            id: 5,
            position: pt(20.0, 20.0),
        }
        .with_inverse_transform(pt(0.0, 0.0), 2.0);
        if let Event::TouchStart { id, position } = e {
            assert_eq!(id, 5);
            assert_eq!(position, pt(10.0, 10.0));
        } else {
            panic!("wrong variant");
        }

        let e = Event::TouchMove {
            id: 5,
            position: pt(20.0, 20.0),
        }
        .with_inverse_transform(pt(0.0, 0.0), 2.0);
        assert_eq!(e.position(), Some(pt(10.0, 10.0)));

        let e = Event::TouchEnd {
            id: 5,
            position: pt(30.0, 30.0),
        }
        .with_inverse_transform(pt(0.0, 0.0), 2.0);
        assert_eq!(e.position(), Some(pt(15.0, 15.0)));
    }

    #[test]
    fn with_inverse_transform_drag_events() {
        let data = DragData::new("t", "p", 0);
        let e = Event::DragStart {
            position: pt(20.0, 20.0),
            data: data.clone(),
        }
        .with_inverse_transform(pt(0.0, 0.0), 2.0);
        assert_eq!(e.position(), Some(pt(10.0, 10.0)));

        let e = Event::DragMove {
            position: pt(20.0, 20.0),
            data: data.clone(),
        }
        .with_inverse_transform(pt(0.0, 0.0), 2.0);
        assert_eq!(e.position(), Some(pt(10.0, 10.0)));

        let e = Event::DragEnter {
            position: pt(20.0, 20.0),
            data: data.clone(),
        }
        .with_inverse_transform(pt(0.0, 0.0), 2.0);
        assert_eq!(e.position(), Some(pt(10.0, 10.0)));

        let e = Event::Drop {
            position: pt(20.0, 20.0),
            data: data.clone(),
        }
        .with_inverse_transform(pt(0.0, 0.0), 2.0);
        assert_eq!(e.position(), Some(pt(10.0, 10.0)));
    }

    #[test]
    fn with_inverse_transform_non_positional_unchanged() {
        if let Event::KeyDown(Key::A) =
            Event::KeyDown(Key::A).with_inverse_transform(pt(5.0, 5.0), 2.0)
        {
        } else {
            panic!("KeyDown should pass through");
        }
        if let Event::CharInput('z') =
            Event::CharInput('z').with_inverse_transform(pt(1.0, 1.0), 2.0)
        {
        } else {
            panic!("CharInput should pass through");
        }
        if let Event::FocusGained =
            Event::FocusGained.with_inverse_transform(pt(1.0, 1.0), 2.0)
        {
        } else {
            panic!("FocusGained should pass through");
        }
    }

    #[test]
    fn with_inverse_transform_unit_scale_equals_with_offset() {
        let scroll = pt(7.0, -3.0);
        let cases: [Event; 5] = [
            Event::MouseMove(pt(10.0, 20.0)),
            Event::MouseDown {
                button: MouseButton::Left,
                position: pt(0.0, 0.0),
            },
            Event::MouseUp {
                button: MouseButton::Right,
                position: pt(50.0, 50.0),
            },
            Event::MouseWheel {
                delta: 1.0,
                delta_x: 0.0,
                position: pt(5.0, 5.0),
            },
            Event::TouchStart {
                id: 1,
                position: pt(10.0, 10.0),
            },
        ];
        for ev in &cases {
            let a = ev.with_inverse_transform(scroll, 1.0).position().unwrap();
            let b = ev.with_offset(scroll.x, scroll.y).position().unwrap();
            assert!(
                (a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5,
                "scale=1 must equal with_offset for {:?}: inv={:?} off={:?}",
                ev,
                a,
                b
            );
        }
    }

    #[test]
    fn with_inverse_transform_zero_scale_is_safe() {
        if cfg!(debug_assertions) {
            return;
        }
        let e = Event::MouseMove(pt(10.0, 20.0)).with_inverse_transform(pt(0.0, 0.0), 0.0);
        if let Event::MouseMove(p) = e {
            assert!(p.x.is_finite() && p.y.is_finite());
        }
    }
}
