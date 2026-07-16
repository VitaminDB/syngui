use vte::{Params, Perform};

use super::grid::{Attr, CellColor, CellFlags, Grid};
use super::mouse::{MouseEncoding, MouseMode};

pub struct Performer<'a> {
    pub grid: &'a mut Grid,
    pub title: &'a mut Option<String>,
}

impl<'a> Performer<'a> {
    pub fn new(grid: &'a mut Grid, title: &'a mut Option<String>) -> Self {
        Self { grid, title }
    }
}

fn nth(params: &Params, idx: usize, default: u16) -> u16 {
    params
        .iter()
        .nth(idx)
        .and_then(|p| p.first().copied())
        .filter(|v| *v != 0)
        .unwrap_or(default)
}

fn nth_raw(params: &Params, idx: usize, default: u16) -> u16 {
    params
        .iter()
        .nth(idx)
        .and_then(|p| p.first().copied())
        .unwrap_or(default)
}

impl<'a> Perform for Performer<'a> {
    fn print(&mut self, c: char) {
        self.grid.print(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => self.grid.bs(),
            0x09 => self.grid.tab(),
            0x0A | 0x0B | 0x0C => {
                self.grid.lf();
            }
            0x0D => self.grid.cr(),
            0x07 => {
            }
            _ => {
            }
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.is_empty() {
            return;
        }
        let cmd = std::str::from_utf8(params[0]).unwrap_or("");
        match cmd {
            "0" | "1" | "2" => {
                if params.len() >= 2 {
                    if let Ok(text) = std::str::from_utf8(params[1]) {
                        *self.title = Some(text.to_string());
                    }
                }
            }
            "8" => {
                let uri = params.get(2).and_then(|b| std::str::from_utf8(b).ok()).unwrap_or("");
                if uri.is_empty() {
                    self.grid.set_current_link(None);
                } else {
                    let id = self.grid.intern_link(uri);
                    self.grid.set_current_link(Some(id));
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let priv_marker = intermediates.first().copied();
        match action {
            'A' => self.grid.move_relative(-(nth(params, 0, 1) as i32), 0),
            'B' | 'e' => self.grid.move_relative(nth(params, 0, 1) as i32, 0),
            'C' | 'a' => self.grid.move_relative(0, nth(params, 0, 1) as i32),
            'D' => self.grid.move_relative(0, -(nth(params, 0, 1) as i32)),
            'E' => {
                let n = nth(params, 0, 1) as i32;
                self.grid.move_relative(n, 0);
                self.grid.cr();
            }
            'F' => {
                let n = nth(params, 0, 1) as i32;
                self.grid.move_relative(-n, 0);
                self.grid.cr();
            }
            'G' | '`' => {
                let col = nth(params, 0, 1).saturating_sub(1) as usize;
                let row = self.grid.cursor().row;
                self.grid.move_to(row, col);
            }
            'd' => {
                let row = nth(params, 0, 1).saturating_sub(1) as usize;
                let col = self.grid.cursor().col;
                self.grid.move_to(row, col);
            }
            'H' | 'f' => {
                let row = nth(params, 0, 1).saturating_sub(1) as usize;
                let col = nth(params, 1, 1).saturating_sub(1) as usize;
                self.grid.move_to(row, col);
            }
            'J' => {
                let mode = nth_raw(params, 0, 0);
                self.grid.erase_display(mode);
            }
            'K' => {
                let mode = nth_raw(params, 0, 0);
                self.grid.erase_line(mode);
            }
            'S' => self.grid.scroll_up_region(nth(params, 0, 1) as usize),
            'T' => self.grid.scroll_down_region(nth(params, 0, 1) as usize),
            'r' => {
                let top = nth(params, 0, 1).saturating_sub(1) as usize;
                let bottom = nth(params, 1, self.grid.rows() as u16).saturating_sub(1) as usize;
                self.grid.set_scroll_region(top, bottom);
            }
            's' => self.grid.save_cursor(),
            'u' => self.grid.restore_cursor(),
            'm' => apply_sgr(self.grid, params),
            'h' | 'l' => {
                if priv_marker == Some(b'?') {
                    let on = action == 'h';
                    for p in params.iter() {
                        let mode = p.first().copied().unwrap_or(0);
                        apply_dec_mode(self.grid, mode, on);
                    }
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        if !intermediates.is_empty() {
            return;
        }
        match byte {
            b'7' => self.grid.save_cursor(),
            b'8' => self.grid.restore_cursor(),
            b'M' => {
                if self.grid.cursor().row == 0 {
                    self.grid.scroll_down_region(1);
                } else {
                    self.grid.move_relative(-1, 0);
                }
            }
            b'D' => {
                self.grid.lf();
            }
            b'c' => {
                self.grid.set_attr(Attr::default());
                self.grid.move_to(0, 0);
                self.grid.erase_display(2);
            }
            _ => {}
        }
    }
}

fn apply_sgr(grid: &mut Grid, params: &Params) {
    let mut attr = grid.current_attr();
    let mut iter = params.iter().peekable();

    if params.is_empty() {
        attr = Attr::default();
        grid.set_attr(attr);
        return;
    }

    while let Some(p) = iter.next() {
        let code = p.first().copied().unwrap_or(0);
        match code {
            0 => attr = Attr::default(),
            1 => attr.flags.insert(CellFlags::BOLD),
            2 => attr.flags.insert(CellFlags::FAINT),
            3 => attr.flags.insert(CellFlags::ITALIC),
            4 => attr.flags.insert(CellFlags::UNDERLINE),
            7 => attr.flags.insert(CellFlags::REVERSE),
            9 => attr.flags.insert(CellFlags::STRIKE),
            22 => attr.flags.remove(CellFlags::BOLD | CellFlags::FAINT),
            23 => attr.flags.remove(CellFlags::ITALIC),
            24 => attr.flags.remove(CellFlags::UNDERLINE),
            27 => attr.flags.remove(CellFlags::REVERSE),
            29 => attr.flags.remove(CellFlags::STRIKE),
            30..=37 => attr.fg = CellColor::Indexed((code - 30) as u8),
            38 => {
                if p.len() >= 2 {
                    attr.fg = parse_extended_color(&p[1..]);
                } else if let Some(next) = iter.next() {
                    let mode = next.first().copied().unwrap_or(0);
                    attr.fg = read_extended(mode, &mut iter);
                }
            }
            39 => attr.fg = CellColor::Default,
            40..=47 => attr.bg = CellColor::Indexed((code - 40) as u8),
            48 => {
                if p.len() >= 2 {
                    attr.bg = parse_extended_color(&p[1..]);
                } else if let Some(next) = iter.next() {
                    let mode = next.first().copied().unwrap_or(0);
                    attr.bg = read_extended(mode, &mut iter);
                }
            }
            49 => attr.bg = CellColor::Default,
            90..=97 => attr.fg = CellColor::Indexed((code - 90 + 8) as u8),
            100..=107 => attr.bg = CellColor::Indexed((code - 100 + 8) as u8),
            _ => {}
        }
    }
    grid.set_attr(attr);
}

fn parse_extended_color(rest: &[u16]) -> CellColor {
    match rest.first().copied().unwrap_or(0) {
        5 => {
            let idx = rest.get(1).copied().unwrap_or(0);
            CellColor::Indexed(idx.min(255) as u8)
        }
        2 => {
            let (r_idx, g_idx, b_idx) = if rest.len() >= 5 {
                (2, 3, 4)
            } else {
                (1, 2, 3)
            };
            let r = rest.get(r_idx).copied().unwrap_or(0).min(255) as u8;
            let g = rest.get(g_idx).copied().unwrap_or(0).min(255) as u8;
            let b = rest.get(b_idx).copied().unwrap_or(0).min(255) as u8;
            CellColor::Rgb(r, g, b)
        }
        _ => CellColor::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vte::Parser;

    fn run(grid: &mut Grid, bytes: &[u8]) {
        let mut parser = Parser::new();
        let mut title = None;
        let mut performer = Performer::new(grid, &mut title);
        for b in bytes {
            parser.advance(&mut performer, *b);
        }
    }

    #[test]
    fn dec_25_toggles_cursor_visibility() {
        let mut g = Grid::new(40, 5);
        assert!(g.cursor_visible());
        run(&mut g, b"\x1b[?25l");
        assert!(!g.cursor_visible());
        run(&mut g, b"\x1b[?25h");
        assert!(g.cursor_visible());
    }

    #[test]
    fn dec_1000_enables_normal_mouse_mode() {
        let mut g = Grid::new(40, 5);
        run(&mut g, b"\x1b[?1000h");
        assert_eq!(g.mouse_mode(), MouseMode::Normal);
        run(&mut g, b"\x1b[?1000l");
        assert_eq!(g.mouse_mode(), MouseMode::Off);
    }

    #[test]
    fn dec_1002_button_event_mode() {
        let mut g = Grid::new(40, 5);
        run(&mut g, b"\x1b[?1002h");
        assert_eq!(g.mouse_mode(), MouseMode::ButtonEvent);
    }

    #[test]
    fn dec_1006_enables_sgr_encoding() {
        let mut g = Grid::new(40, 5);
        run(&mut g, b"\x1b[?1006h");
        assert_eq!(g.mouse_encoding(), MouseEncoding::Sgr);
        run(&mut g, b"\x1b[?1006l");
        assert_eq!(g.mouse_encoding(), MouseEncoding::Default);
    }

    #[test]
    fn dec_1015_enables_urxvt_encoding() {
        let mut g = Grid::new(40, 5);
        run(&mut g, b"\x1b[?1015h");
        assert_eq!(g.mouse_encoding(), MouseEncoding::Urxvt);
    }

    #[test]
    fn dec_2004_toggles_bracketed_paste() {
        let mut g = Grid::new(40, 5);
        assert!(!g.bracketed_paste());
        run(&mut g, b"\x1b[?2004h");
        assert!(g.bracketed_paste());
        run(&mut g, b"\x1b[?2004l");
        assert!(!g.bracketed_paste());
    }

    #[test]
    fn dec_1004_focus_events() {
        let mut g = Grid::new(40, 5);
        run(&mut g, b"\x1b[?1004h");
        assert!(g.focus_events());
    }

    #[test]
    fn osc_8_sets_link_then_clears() {
        let mut g = Grid::new(40, 5);
        run(&mut g, b"\x1b]8;;https://example.com\x1b\\TEXT\x1b]8;;\x1b\\");
        let row = g.line(0);
        let id = row[0].link_id.expect("first cell should have link");
        assert_eq!(g.link(id), Some("https://example.com"));
        assert_eq!(row[1].link_id, Some(id));
        assert_eq!(row[2].link_id, Some(id));
        assert_eq!(row[3].link_id, Some(id));
    }

    #[test]
    fn sgr_truecolor_legacy_form() {
        let mut g = Grid::new(40, 5);
        run(&mut g, b"\x1b[38;2;255;128;0mX");
        assert_eq!(g.line(0)[0].fg, CellColor::Rgb(255, 128, 0));
    }

    #[test]
    fn sgr_truecolor_colon_with_empty_cs() {
        let mut g = Grid::new(40, 5);
        run(&mut g, b"\x1b[38:2::255:128:0mX");
        assert_eq!(g.line(0)[0].fg, CellColor::Rgb(255, 128, 0));
    }

    #[test]
    fn sgr_truecolor_colon_with_cs() {
        let mut g = Grid::new(40, 5);
        run(&mut g, b"\x1b[38:2:0:255:128:0mX");
        assert_eq!(g.line(0)[0].fg, CellColor::Rgb(255, 128, 0));
    }

    #[test]
    fn sgr_indexed_colon_form() {
        let mut g = Grid::new(40, 5);
        run(&mut g, b"\x1b[38:5:202mX");
        assert_eq!(g.line(0)[0].fg, CellColor::Indexed(202));
    }

    #[test]
    fn alt_screen_1049_save_switch_clear() {
        let mut g = Grid::new(10, 3);
        for ch in "ABC".chars() { g.print(ch); }
        let main_cursor = g.cursor();
        run(&mut g, b"\x1b[?1049h");
        assert!(g.on_alt());
        assert_eq!(g.line(0)[0].ch, ' ');
        assert_eq!(g.cursor().col, 0);
        for ch in "X".chars() { g.print(ch); }
        run(&mut g, b"\x1b[?1049l");
        assert!(!g.on_alt());
        assert_eq!(g.line(0)[0].ch, 'A');
        assert_eq!(g.line(0)[1].ch, 'B');
        assert_eq!(g.line(0)[2].ch, 'C');
        assert_eq!(g.cursor().col, main_cursor.col);
    }

    #[test]
    fn alt_screen_1047_no_save() {
        let mut g = Grid::new(10, 3);
        for ch in "ABC".chars() { g.print(ch); }
        run(&mut g, b"\x1b[?1047h");
        assert!(g.on_alt());
        for ch in "X".chars() { g.print(ch); }
        run(&mut g, b"\x1b[?1047l");
        assert!(!g.on_alt());
        assert_eq!(g.line(0)[0].ch, 'A');
    }

    #[test]
    fn dec_1007_toggles_alt_scroll() {
        let mut g = Grid::new(10, 3);
        assert!(g.alt_scroll());
        run(&mut g, b"\x1b[?1007l");
        assert!(!g.alt_scroll());
        run(&mut g, b"\x1b[?1007h");
        assert!(g.alt_scroll());
    }

    #[test]
    fn alt_scrollback_frozen_on_alt() {
        let mut g = Grid::new(3, 2);
        for ch in "ABCDEF".chars() { g.print(ch); }
        let sb_main = g.scrollback_len();
        run(&mut g, b"\x1b[?1049h");
        for ch in "XYZ123456".chars() { g.print(ch); }
        assert_eq!(g.scrollback_len(), sb_main);
        run(&mut g, b"\x1b[?1049l");
        assert_eq!(g.scrollback_len(), sb_main);
    }
}

fn apply_dec_mode(grid: &mut Grid, mode: u16, on: bool) {
    match mode {
        25 => grid.set_cursor_visible(on),

        9 => {
            grid.set_mouse_mode(if on { MouseMode::X10 } else { MouseMode::Off });
        }
        1000 => {
            grid.set_mouse_mode(if on { MouseMode::Normal } else { MouseMode::Off });
        }
        1002 => {
            grid.set_mouse_mode(if on { MouseMode::ButtonEvent } else { MouseMode::Off });
        }
        1003 => {
            grid.set_mouse_mode(if on { MouseMode::AnyEvent } else { MouseMode::Off });
        }

        1004 => grid.set_focus_events(on),

        1005 => {
            grid.set_mouse_encoding(if on { MouseEncoding::Utf8 } else { MouseEncoding::Default });
        }
        1006 => {
            grid.set_mouse_encoding(if on { MouseEncoding::Sgr } else { MouseEncoding::Default });
        }
        1015 => {
            grid.set_mouse_encoding(if on { MouseEncoding::Urxvt } else { MouseEncoding::Default });
        }

        2004 => grid.set_bracketed_paste(on),

        1007 => grid.set_alt_scroll(on),

        47 | 1047 => {
            if on {
                grid.enter_alt_screen(true);
            } else {
                grid.exit_alt_screen();
            }
        }
        1048 => {
            if on {
                grid.save_cursor();
            } else {
                grid.restore_cursor();
            }
        }
        1049 => {
            if on {
                grid.save_cursor();
                grid.enter_alt_screen(true);
            } else {
                grid.exit_alt_screen();
                grid.restore_cursor();
            }
        }

        _ => {}
    }
}

fn read_extended<'a>(
    mode: u16,
    iter: &mut std::iter::Peekable<vte::ParamsIter<'a>>,
) -> CellColor {
    match mode {
        5 => {
            let idx = iter
                .next()
                .and_then(|p| p.first().copied())
                .unwrap_or(0);
            CellColor::Indexed(idx.min(255) as u8)
        }
        2 => {
            let r = iter
                .next()
                .and_then(|p| p.first().copied())
                .unwrap_or(0)
                .min(255) as u8;
            let g = iter
                .next()
                .and_then(|p| p.first().copied())
                .unwrap_or(0)
                .min(255) as u8;
            let b = iter
                .next()
                .and_then(|p| p.first().copied())
                .unwrap_or(0)
                .min(255) as u8;
            CellColor::Rgb(r, g, b)
        }
        _ => CellColor::Default,
    }
}
