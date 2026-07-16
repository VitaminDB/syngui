use crate::core::Color;

use super::grid::CellColor;

const TANGO_16: [(u8, u8, u8); 16] = [
    (0x2E, 0x34, 0x36),
    (0xCC, 0x00, 0x00),
    (0x4E, 0x9A, 0x06),
    (0xC4, 0xA0, 0x00),
    (0x34, 0x65, 0xA4),
    (0x75, 0x50, 0x7B),
    (0x06, 0x98, 0x9A),
    (0xD3, 0xD7, 0xCF),
    (0x55, 0x57, 0x53),
    (0xEF, 0x29, 0x29),
    (0x8A, 0xE2, 0x34),
    (0xFC, 0xE9, 0x4F),
    (0x72, 0x9F, 0xCF),
    (0xAD, 0x7F, 0xA8),
    (0x34, 0xE2, 0xE2),
    (0xEE, 0xEE, 0xEC),
];

const CUBE_RAMP: [u8; 6] = [0, 95, 135, 175, 215, 255];

fn srgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_srgb(r, g, b, 1.0)
}

fn indexed_color(idx: u8) -> Color {
    if (idx as usize) < TANGO_16.len() {
        let (r, g, b) = TANGO_16[idx as usize];
        return srgb(r, g, b);
    }
    if (16..=231).contains(&idx) {
        let i = idx - 16;
        let r = CUBE_RAMP[(i / 36) as usize];
        let g = CUBE_RAMP[((i % 36) / 6) as usize];
        let b = CUBE_RAMP[(i % 6) as usize];
        return srgb(r, g, b);
    }
    let v = 8 + (idx - 232) as u32 * 10;
    let v = v.min(255) as u8;
    srgb(v, v, v)
}

pub fn resolve(c: CellColor, default_fg: Color, default_bg: Color, is_fg: bool) -> Color {
    match c {
        CellColor::Default => {
            if is_fg {
                default_fg
            } else {
                default_bg
            }
        }
        CellColor::Indexed(idx) => indexed_color(idx),
        CellColor::Rgb(r, g, b) => srgb(r, g, b),
    }
}
