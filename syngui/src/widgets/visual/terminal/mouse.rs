use crate::input::{Modifiers, MouseButton};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MouseMode {
    #[default]
    Off,
    X10,
    Normal,
    ButtonEvent,
    AnyEvent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MouseEncoding {
    #[default]
    Default,
    Utf8,
    Sgr,
    Urxvt,
}

#[derive(Clone, Copy, Debug)]
pub enum MouseAction {
    Press(MouseButton),
    Release(MouseButton),
    Motion {
        button: Option<MouseButton>,
    },
    Wheel(i8),
}

pub fn should_report(mode: MouseMode, action: MouseAction, button_held: bool) -> bool {
    match (mode, action) {
        (MouseMode::Off, _) => false,
        (MouseMode::X10, MouseAction::Press(_)) => true,
        (MouseMode::X10, _) => false,
        (_, MouseAction::Press(_) | MouseAction::Release(_) | MouseAction::Wheel(_)) => true,
        (MouseMode::ButtonEvent, MouseAction::Motion { .. }) => button_held,
        (MouseMode::AnyEvent, MouseAction::Motion { .. }) => true,
        (MouseMode::Normal, MouseAction::Motion { .. }) => false,
    }
}

fn base_button_code(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        _ => 2,
    }
}

fn encode_button_raw(action: MouseAction, mods: Modifiers, encoding: MouseEncoding) -> u8 {
    let mut cb: u8 = match action {
        MouseAction::Press(b) => base_button_code(b),
        MouseAction::Release(b) => {
            if matches!(encoding, MouseEncoding::Sgr) {
                base_button_code(b)
            } else {
                3
            }
        }
        MouseAction::Motion { button } => {
            let base = button.map(base_button_code).unwrap_or(3);
            base | 0b10_0000
        }
        MouseAction::Wheel(dir) => {
            if dir > 0 { 64 } else { 65 }
        }
    };

    if mods.shift {
        cb |= 0b0000_0100;
    }
    if mods.alt || mods.meta {
        cb |= 0b0000_1000;
    }
    if mods.ctrl {
        cb |= 0b0001_0000;
    }
    cb
}

pub fn encode_event(
    encoding: MouseEncoding,
    action: MouseAction,
    col: u16,
    row: u16,
    mods: Modifiers,
) -> Option<Vec<u8>> {
    let col = col.max(1);
    let row = row.max(1);
    let cb = encode_button_raw(action, mods, encoding);

    match encoding {
        MouseEncoding::Default => encode_x10(cb, col, row),
        MouseEncoding::Utf8 => encode_utf8(cb, col, row),
        MouseEncoding::Sgr => Some(encode_sgr(cb, col, row, action)),
        MouseEncoding::Urxvt => Some(encode_urxvt(cb, col, row)),
    }
}

fn encode_x10(cb: u8, col: u16, row: u16) -> Option<Vec<u8>> {
    if col > 223 || row > 223 {
        return None;
    }
    let cb = cb.saturating_add(32);
    let cx = (col as u8).saturating_add(32);
    let cy = (row as u8).saturating_add(32);
    Some(vec![0x1b, b'[', b'M', cb, cx, cy])
}

fn encode_utf8(cb: u8, col: u16, row: u16) -> Option<Vec<u8>> {
    let cb = cb.saturating_add(32);
    let mut out = vec![0x1b, b'[', b'M', cb];
    push_utf8_coord(&mut out, col)?;
    push_utf8_coord(&mut out, row)?;
    Some(out)
}

fn push_utf8_coord(out: &mut Vec<u8>, v: u16) -> Option<()> {
    let codepoint = (v as u32).saturating_add(32);
    let ch = char::from_u32(codepoint)?;
    let mut buf = [0u8; 4];
    let s = ch.encode_utf8(&mut buf);
    out.extend_from_slice(s.as_bytes());
    Some(())
}

fn encode_sgr(cb: u8, col: u16, row: u16, action: MouseAction) -> Vec<u8> {
    let final_byte = if matches!(action, MouseAction::Release(_)) {
        b'm'
    } else {
        b'M'
    };
    let s = format!("\x1b[<{};{};{}{}", cb, col, row, final_byte as char);
    s.into_bytes()
}

fn encode_urxvt(cb: u8, col: u16, row: u16) -> Vec<u8> {
    let cb = cb.saturating_add(32);
    let s = format!("\x1b[{};{};{}M", cb, col, row);
    s.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_mods() -> Modifiers {
        Modifiers::default()
    }

    #[test]
    fn x10_left_click_origin() {
        let bytes = encode_event(
            MouseEncoding::Default,
            MouseAction::Press(MouseButton::Left),
            1,
            1,
            no_mods(),
        )
        .unwrap();
        assert_eq!(bytes, b"\x1b[M !!");
    }

    #[test]
    fn x10_overflow_returns_none() {
        let bytes = encode_event(
            MouseEncoding::Default,
            MouseAction::Press(MouseButton::Left),
            300,
            10,
            no_mods(),
        );
        assert!(bytes.is_none());
    }

    #[test]
    fn sgr_left_click() {
        let bytes = encode_event(
            MouseEncoding::Sgr,
            MouseAction::Press(MouseButton::Left),
            10,
            5,
            no_mods(),
        )
        .unwrap();
        assert_eq!(bytes, b"\x1b[<0;10;5M");
    }

    #[test]
    fn sgr_left_release() {
        let bytes = encode_event(
            MouseEncoding::Sgr,
            MouseAction::Release(MouseButton::Left),
            10,
            5,
            no_mods(),
        )
        .unwrap();
        assert_eq!(bytes, b"\x1b[<0;10;5m");
    }

    #[test]
    fn sgr_right_with_shift() {
        let mods = Modifiers { shift: true, ..Default::default() };
        let bytes = encode_event(
            MouseEncoding::Sgr,
            MouseAction::Press(MouseButton::Right),
            5,
            5,
            mods,
        )
        .unwrap();
        assert_eq!(bytes, b"\x1b[<6;5;5M");
    }

    #[test]
    fn sgr_drag_left_button() {
        let bytes = encode_event(
            MouseEncoding::Sgr,
            MouseAction::Motion { button: Some(MouseButton::Left) },
            5,
            5,
            no_mods(),
        )
        .unwrap();
        assert_eq!(bytes, b"\x1b[<32;5;5M");
    }

    #[test]
    fn sgr_wheel_up() {
        let bytes = encode_event(
            MouseEncoding::Sgr,
            MouseAction::Wheel(1),
            5,
            5,
            no_mods(),
        )
        .unwrap();
        assert_eq!(bytes, b"\x1b[<64;5;5M");
    }

    #[test]
    fn sgr_wheel_down() {
        let bytes = encode_event(
            MouseEncoding::Sgr,
            MouseAction::Wheel(-1),
            5,
            5,
            no_mods(),
        )
        .unwrap();
        assert_eq!(bytes, b"\x1b[<65;5;5M");
    }

    #[test]
    fn urxvt_left_click() {
        let bytes = encode_event(
            MouseEncoding::Urxvt,
            MouseAction::Press(MouseButton::Left),
            10,
            5,
            no_mods(),
        )
        .unwrap();
        assert_eq!(bytes, b"\x1b[32;10;5M");
    }

    #[test]
    fn utf8_coord_above_95_uses_two_bytes() {
        let bytes = encode_event(
            MouseEncoding::Utf8,
            MouseAction::Press(MouseButton::Left),
            200,
            5,
            no_mods(),
        )
        .unwrap();
        let expected = vec![0x1b, b'[', b'M', 32, 0xC3, 0xA8, b'%'];
        assert_eq!(bytes, expected);
    }

    #[test]
    fn x10_mode_only_reports_press() {
        assert!(should_report(
            MouseMode::X10,
            MouseAction::Press(MouseButton::Left),
            false
        ));
        assert!(!should_report(
            MouseMode::X10,
            MouseAction::Release(MouseButton::Left),
            false
        ));
        assert!(!should_report(
            MouseMode::X10,
            MouseAction::Motion { button: Some(MouseButton::Left) },
            true
        ));
    }

    #[test]
    fn button_event_reports_drag_only() {
        assert!(should_report(
            MouseMode::ButtonEvent,
            MouseAction::Motion { button: Some(MouseButton::Left) },
            true
        ));
        assert!(!should_report(
            MouseMode::ButtonEvent,
            MouseAction::Motion { button: None },
            false
        ));
    }

    #[test]
    fn any_event_reports_naked_motion() {
        assert!(should_report(
            MouseMode::AnyEvent,
            MouseAction::Motion { button: None },
            false
        ));
    }

    #[test]
    fn off_mode_reports_nothing() {
        assert!(!should_report(
            MouseMode::Off,
            MouseAction::Press(MouseButton::Left),
            false
        ));
    }
}
