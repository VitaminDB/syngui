use crate::input::{Key, Modifiers};

const CSI: &str = "\x1b[";
const SS3: &str = "\x1bO";

pub fn map_key(key: Key, mods: Modifiers) -> Option<Vec<u8>> {
    match key {
        Key::Enter if mods.shift => Some(b"\n".to_vec()),
        Key::Enter => Some(b"\r".to_vec()),
        Key::Backspace => {
            if mods.ctrl {
                Some(vec![0x08])
            } else {
                Some(vec![0x7F])
            }
        }
        Key::Tab => {
            if mods.shift {
                Some(format!("{CSI}Z").into_bytes())
            } else {
                Some(b"\t".to_vec())
            }
        }
        Key::Escape => Some(b"\x1b".to_vec()),

        Key::Up => Some(arrow(b'A', mods)),
        Key::Down => Some(arrow(b'B', mods)),
        Key::Right => Some(arrow(b'C', mods)),
        Key::Left => Some(arrow(b'D', mods)),

        Key::Home => Some(arrow(b'H', mods)),
        Key::End => Some(arrow(b'F', mods)),
        Key::PageUp => Some(tilde(5, mods)),
        Key::PageDown => Some(tilde(6, mods)),
        Key::Insert => Some(tilde(2, mods)),
        Key::Delete => Some(tilde(3, mods)),

        Key::F1 => Some(function_low(b'P', mods)),
        Key::F2 => Some(function_low(b'Q', mods)),
        Key::F3 => Some(function_low(b'R', mods)),
        Key::F4 => Some(function_low(b'S', mods)),
        Key::F5 => Some(tilde(15, mods)),
        Key::F6 => Some(tilde(17, mods)),
        Key::F7 => Some(tilde(18, mods)),
        Key::F8 => Some(tilde(19, mods)),
        Key::F9 => Some(tilde(20, mods)),
        Key::F10 => Some(tilde(21, mods)),
        Key::F11 => Some(tilde(23, mods)),
        Key::F12 => Some(tilde(24, mods)),

        Key::Space if mods.ctrl => Some(vec![0x00]),

        Key::Shift | Key::Ctrl | Key::Alt | Key::Meta => None,

        k if mods.ctrl => letter_ctrl(k, mods),

        _ => None,
    }
}

pub fn map_char(c: char, mods: Modifiers) -> Vec<u8> {
    if mods.alt && c.is_ascii() {
        let mut buf = vec![0x1b];
        buf.extend_from_slice(c.encode_utf8(&mut [0u8; 4]).as_bytes());
        buf
    } else {
        let mut buf = [0u8; 4];
        c.encode_utf8(&mut buf).as_bytes().to_vec()
    }
}

fn modifier_param(mods: Modifiers) -> u8 {
    let mut v = 0;
    if mods.shift {
        v |= 1;
    }
    if mods.alt {
        v |= 2;
    }
    if mods.ctrl {
        v |= 4;
    }
    if mods.meta {
        v |= 8;
    }
    1 + v
}

fn arrow(letter: u8, mods: Modifiers) -> Vec<u8> {
    let m = modifier_param(mods);
    if m == 1 {
        format!("{CSI}{}", letter as char).into_bytes()
    } else {
        format!("{CSI}1;{}{}", m, letter as char).into_bytes()
    }
}

fn tilde(num: u16, mods: Modifiers) -> Vec<u8> {
    let m = modifier_param(mods);
    if m == 1 {
        format!("{CSI}{num}~").into_bytes()
    } else {
        format!("{CSI}{num};{m}~").into_bytes()
    }
}

fn function_low(letter: u8, mods: Modifiers) -> Vec<u8> {
    let m = modifier_param(mods);
    if m == 1 {
        format!("{SS3}{}", letter as char).into_bytes()
    } else {
        format!("{CSI}1;{}{}", m, letter as char).into_bytes()
    }
}

fn letter_ctrl(key: Key, _mods: Modifiers) -> Option<Vec<u8>> {
    let code = match key {
        Key::A => 0x01,
        Key::B => 0x02,
        Key::C => 0x03,
        Key::D => 0x04,
        Key::E => 0x05,
        Key::F => 0x06,
        Key::G => 0x07,
        Key::H => 0x08,
        Key::I => 0x09,
        Key::J => 0x0A,
        Key::K => 0x0B,
        Key::L => 0x0C,
        Key::M => 0x0D,
        Key::N => 0x0E,
        Key::O => 0x0F,
        Key::P => 0x10,
        Key::Q => 0x11,
        Key::R => 0x12,
        Key::S => 0x13,
        Key::T => 0x14,
        Key::U => 0x15,
        Key::V => 0x16,
        Key::W => 0x17,
        Key::X => 0x18,
        Key::Y => 0x19,
        Key::Z => 0x1A,
        _ => return None,
    };
    Some(vec![code])
}
