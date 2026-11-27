use vte::{Params, Perform};

use super::grid::{Attr, CellColor, CellFlags, Charset, Grid};
use super::mouse::{MouseEncoding, MouseMode};

pub struct Performer<'a> {
    pub grid: &'a mut Grid,
    pub title: &'a mut Option<String>,
    replies: Vec<Vec<u8>>,
}

impl<'a> Performer<'a> {
    pub fn new(grid: &'a mut Grid, title: &'a mut Option<String>) -> Self {
        Self {
            grid,
            title,
            replies: Vec::new(),
        }
    }

    pub fn take_replies(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.replies)
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

/// В `vte 0.13` при отсутствии параметров (`\x1b[s`, `\x1b[u`) `CsiDispatch`
/// всегда пушит `0`, поэтому `params.is_empty()` ложно. Проверяем оба вида.
fn no_explicit_params(params: &Params) -> bool {
    params.is_empty() || (params.len() == 1 && params.iter().next() == Some(&[0]))
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
            0x0E => self.grid.set_active_charset(1), // SO (Shift Out → G1)
            0x0F => self.grid.set_active_charset(0), // SI (Shift In → G0)
            0x07 => {}
            _ => {}
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
                let uri = params
                    .get(2)
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .unwrap_or("");
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

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let priv_marker = intermediates.first().copied();
        match action {
            'A' if intermediates.is_empty() => {
                self.grid.move_relative(-(nth(params, 0, 1) as i32), 0)
            }
            'B' | 'e' if intermediates.is_empty() => {
                self.grid.move_relative(nth(params, 0, 1) as i32, 0)
            }
            'C' | 'a' if intermediates.is_empty() => {
                self.grid.move_relative(0, nth(params, 0, 1) as i32)
            }
            'D' if intermediates.is_empty() => {
                self.grid.move_relative(0, -(nth(params, 0, 1) as i32))
            }
            'E' if intermediates.is_empty() => {
                let n = nth(params, 0, 1) as i32;
                self.grid.move_relative(n, 0);
                self.grid.cr();
            }
            'F' if intermediates.is_empty() => {
                let n = nth(params, 0, 1) as i32;
                self.grid.move_relative(-n, 0);
                self.grid.cr();
            }
            'G' | '`' if intermediates.is_empty() => {
                let col = nth(params, 0, 1).saturating_sub(1) as usize;
                let row = self.grid.cursor().row;
                self.grid.move_to(row, col);
            }
            'd' if intermediates.is_empty() => {
                let row = nth(params, 0, 1).saturating_sub(1) as usize;
                let col = self.grid.cursor().col;
                self.grid.move_to_cup(row, col);
            }
            'H' | 'f' if intermediates.is_empty() => {
                let row = nth(params, 0, 1).saturating_sub(1) as usize;
                let col = nth(params, 1, 1).saturating_sub(1) as usize;
                self.grid.move_to_cup(row, col);
            }
            'J' if intermediates.is_empty() => {
                let mode = nth_raw(params, 0, 0);
                self.grid.erase_display(mode);
            }
            'K' if intermediates.is_empty() => {
                let mode = nth_raw(params, 0, 0);
                self.grid.erase_line(mode);
            }
            'L' if intermediates.is_empty() => {
                self.grid.insert_lines(nth(params, 0, 1) as usize);
            }
            'M' if intermediates.is_empty() => {
                self.grid.delete_lines(nth(params, 0, 1) as usize);
            }
            '@' if intermediates.is_empty() => {
                self.grid.insert_chars(nth(params, 0, 1) as usize);
            }
            'P' if intermediates.is_empty() => {
                self.grid.delete_chars(nth(params, 0, 1) as usize);
            }
            'X' if intermediates.is_empty() => {
                self.grid.erase_chars(nth(params, 0, 1) as usize);
            }
            'b' if intermediates.is_empty() => {
                self.grid.repeat_char(nth(params, 0, 1) as usize);
            }
            'I' if intermediates.is_empty() => {
                self.grid.tab_forward(nth(params, 0, 1) as usize);
            }
            'Z' if intermediates.is_empty() => {
                self.grid.tab_backward(nth(params, 0, 1) as usize);
            }
            'g' if intermediates.is_empty() => {
                self.grid.clear_tab_stop(nth_raw(params, 0, 0));
            }
            'W' if intermediates == b"?" && nth_raw(params, 0, 0) == 5 => {
                self.grid.reset_tab_stops();
            }
            'S' if intermediates.is_empty() => {
                self.grid.scroll_up_region(nth(params, 0, 1) as usize)
            }
            'T' if intermediates.is_empty() && params.len() <= 1 => {
                self.grid.scroll_down_region(nth(params, 0, 1) as usize)
            }
            'r' if intermediates.is_empty() => {
                let top = nth(params, 0, 1).saturating_sub(1) as usize;
                let bottom = nth(params, 1, self.grid.rows() as u16).saturating_sub(1) as usize;
                self.grid.set_scroll_region(top, bottom);
            }
            's' if intermediates.is_empty() && no_explicit_params(params) => {
                self.grid.save_cursor()
            }
            'u' if intermediates.is_empty() && no_explicit_params(params) => {
                self.grid.restore_cursor()
            }
            'm' if intermediates.is_empty() => apply_sgr(self.grid, params),
            'n' if intermediates.is_empty() => match nth_raw(params, 0, 0) {
                5 => self.replies.push(b"\x1b[0n".to_vec()),
                6 => {
                    let c = self.grid.cursor();
                    self.replies
                        .push(format!("\x1b[{};{}R", c.row + 1, c.col + 1).into_bytes());
                }
                _ => {}
            },
            'c' if intermediates.is_empty() && nth_raw(params, 0, 0) == 0 => {
                // VT220-совместимый ответ на Primary DA.
                self.replies.push(b"\x1b[?62;c".to_vec());
            }
            'c' if intermediates == b">" && nth_raw(params, 0, 0) == 0 => {
                // Secondary DA (DA2).
                self.replies.push(b"\x1b[>0;0;0c".to_vec());
            }
            'q' if intermediates == b">" && nth_raw(params, 0, 0) == 0 => {
                // XTVERSION: мгновенный ответ без ожидания таймаута в TUI.
                self.replies.push(b"\x1bP>|syngui\x1b\\".to_vec());
            }
            'p' if intermediates == b"?$" => {
                // DECRQM (CSI ? Ps $ p): ответ DECRPM (CSI ? Ps ; Pm $ y).
                // Pm: 0 = не распознан, 1 = включён, 2 = выключен.
                let mode = nth_raw(params, 0, 0);
                let status = query_dec_mode(self.grid, mode);
                self.replies
                    .push(format!("\x1b[?{mode};{status}$y").into_bytes());
            }
            'h' | 'l' => {
                let on = action == 'h';
                if priv_marker == Some(b'?') {
                    for p in params.iter() {
                        let mode = p.first().copied().unwrap_or(0);
                        apply_dec_mode(self.grid, mode, on);
                    }
                } else if intermediates.is_empty() {
                    for p in params.iter() {
                        let mode = p.first().copied().unwrap_or(0);
                        if mode == 4 {
                            self.grid.set_insert_mode(on);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match intermediates {
            [b'('] => self.grid.configure_charset(0, parse_charset(byte)),
            [b')'] => self.grid.configure_charset(1, parse_charset(byte)),
            [b'*'] => self.grid.configure_charset(2, parse_charset(byte)),
            [b'+'] => self.grid.configure_charset(3, parse_charset(byte)),
            [] => match byte {
                b'7' => self.grid.save_cursor(),
                b'8' => self.grid.restore_cursor(),
                b'H' => self.grid.set_tab_stop(),
                b'M' => self.grid.reverse_index(),
                b'D' => self.grid.lf(),
                b'E' => {
                    self.grid.cr();
                    self.grid.lf();
                }
                b'n' => self.grid.set_active_charset(2), // LS2 (G2 → GL)
                b'o' => self.grid.set_active_charset(3), // LS3 (G3 → GL)
                b'N' => self.grid.set_single_shift(2),   // SS2 (next char from G2)
                b'O' => self.grid.set_single_shift(3),   // SS3 (next char from G3)
                b'c' => self.grid.reset(),
                _ => {}
            },
            _ => {}
        }
    }
}

fn parse_charset(byte: u8) -> Charset {
    match byte {
        b'0' => Charset::DecSpecialGraphics,
        b'A' => Charset::Uk,
        _ => Charset::Ascii,
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
        run(
            &mut g,
            b"\x1b]8;;https://example.com\x1b\\TEXT\x1b]8;;\x1b\\",
        );
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
        for ch in "ABC".chars() {
            g.print(ch);
        }
        let main_cursor = g.cursor();
        run(&mut g, b"\x1b[?1049h");
        assert!(g.on_alt());
        assert_eq!(g.line(0)[0].ch, ' ');
        assert_eq!(g.cursor().col, 0);
        for ch in "X".chars() {
            g.print(ch);
        }
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
        for ch in "ABC".chars() {
            g.print(ch);
        }
        run(&mut g, b"\x1b[?1047h");
        assert!(g.on_alt());
        for ch in "X".chars() {
            g.print(ch);
        }
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
        for ch in "ABCDEF".chars() {
            g.print(ch);
        }
        let sb_main = g.scrollback_len();
        run(&mut g, b"\x1b[?1049h");
        for ch in "XYZ123456".chars() {
            g.print(ch);
        }
        assert_eq!(g.scrollback_len(), sb_main);
        run(&mut g, b"\x1b[?1049l");
        assert_eq!(g.scrollback_len(), sb_main);
    }

    fn row_str(g: &Grid, r: usize) -> String {
        g.line(r)
            .iter()
            .filter(|c| !c.flags.contains(CellFlags::WIDE_SPACER))
            .map(|c| c.ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    /// Точная последовательность, которую `nano 9.3` шлёт при нажатии Enter
    /// в середине строки: `ESC 7`, `CSI 3;22 r`, `ESC 8`, `\r`, `CSI 3 d`,
    /// `ESC M` (Reverse Index на `scroll_top`), `CSI 1;24 r`, `CSI 2;7 H`,
    /// `CSI K`, `\r`, `CSI 3 d`, `"world"`.
    #[test]
    fn nano_enter_splits_line_with_reverse_index_in_scroll_region() {
        let mut g = Grid::new(40, 24);
        run(&mut g, b"\x1b[?1049h\x1b[1;24r");
        run(&mut g, b"\x1b[1;1HTITLE");
        run(&mut g, b"\x1b[2;1Hhello world");
        run(&mut g, b"\x1b[3;1Hsecond line");
        run(&mut g, b"\x1b[2;7H");
        // Реальная выдача nano при Enter между "hello " и "world":
        run(
            &mut g,
            b"\x1b7\x1b[3;22r\x1b8\r\x1b[3d\x1bM\x1b[1;24r\x1b[2;7H\x1b[K\r\x1b[3dworld\r",
        );
        assert_eq!(row_str(&g, 0), "TITLE");
        assert_eq!(row_str(&g, 1), "hello");
        assert_eq!(row_str(&g, 2), "world");
        assert_eq!(row_str(&g, 3), "second line");
    }

    #[test]
    fn csi_insert_and_delete_lines_respect_scroll_region() {
        let mut g = Grid::new(20, 6);
        run(&mut g, b"R0\r\nR1\r\nR2\r\nR3\r\nR4\r\nR5");
        run(&mut g, b"\x1b[2;5r"); // rows 1..=4, курсор уходит в (0,0)
        assert_eq!((g.cursor().row, g.cursor().col), (0, 0));
        run(&mut g, b"\x1b[3;2H\x1b[1L"); // insert 1 line at row 2
        assert_eq!(row_str(&g, 0), "R0");
        assert_eq!(row_str(&g, 1), "R1");
        assert_eq!(row_str(&g, 2), "");
        assert_eq!(row_str(&g, 3), "R2");
        assert_eq!(row_str(&g, 4), "R3");
        assert_eq!(row_str(&g, 5), "R5");
        assert_eq!(g.cursor().col, 0);

        run(&mut g, b"\x1b[3;1H\x1b[1M"); // delete 1 line at row 2
        assert_eq!(row_str(&g, 2), "R2");
        assert_eq!(row_str(&g, 3), "R3");
        assert_eq!(row_str(&g, 4), "");
        assert_eq!(row_str(&g, 5), "R5");
    }

    #[test]
    fn csi_ich_dch_ech_rep_and_irm() {
        let mut g = Grid::new(20, 3);
        run(&mut g, b"abcdef");
        // DCH: удалить 2 символа начиная с col 2 ('c','d')
        run(&mut g, b"\x1b[1;3H\x1b[2P");
        assert_eq!(row_str(&g, 0), "abef");
        // ICH: вставить 2 пробела в col 2
        run(&mut g, b"\x1b[1;3H\x1b[2@");
        assert_eq!(row_str(&g, 0), "ab  ef");
        // ECH: затереть 3 символа начиная с col 1 без сдвига курсора
        run(&mut g, b"\x1b[1;2H\x1b[3X");
        assert_eq!(row_str(&g, 0), "a   ef");
        assert_eq!(g.cursor().col, 1);
        // REP: повторить '-' 4 раза (1 + 3)
        run(&mut g, b"\x1b[2;1H-\x1b[3b");
        assert_eq!(row_str(&g, 1), "----");
        // IRM (CSI 4 h/l): вставка в середину сдвигает хвост вправо
        run(&mut g, b"\x1b[3;1Hac\x1b[3;2H\x1b[4hb\x1b[4l");
        assert_eq!(row_str(&g, 2), "abc");
    }

    #[test]
    fn csi_3j_clears_only_scrollback_not_viewport() {
        let mut g = Grid::new(10, 2);
        run(&mut g, b"line1\r\nline2\r\nline3");
        assert_eq!(g.scrollback_len(), 1);
        run(&mut g, b"\x1b[3J");
        assert_eq!(g.scrollback_len(), 0);
        assert_eq!(row_str(&g, 0), "line2");
        assert_eq!(row_str(&g, 1), "line3");
    }

    #[test]
    fn wide_emoji_and_variation_selector_align_columns() {
        let mut g = Grid::new(20, 2);
        // 📎 (width 2) + VS16 (\u{FE0F}, width 0) + "1 media"
        run(&mut g, "\u{1F4CE}\u{FE0F}1 media".as_bytes());
        assert_eq!(g.line(0)[0].ch, '\u{1F4CE}');
        assert!(g.line(0)[0].flags.contains(CellFlags::WIDE));
        assert!(g.line(0)[1].flags.contains(CellFlags::WIDE_SPACER));
        assert_eq!(g.line(0)[2].ch, '1');
        assert_eq!(row_str(&g, 0), "\u{1F4CE}1 media");

        // Перезапись правой половины широкого глифа очищает левую половину.
        run(&mut g, b"\x1b[1;2HX");
        assert_eq!(g.line(0)[0].ch, ' ');
        assert!(!g.line(0)[0].flags.contains(CellFlags::WIDE));
        assert_eq!(g.line(0)[1].ch, 'X');
    }

    #[test]
    fn dec_modes_and_cpr_reply_and_modify_other_keys_ignored() {
        let mut g = Grid::new(20, 5);
        let mut parser = Parser::new();
        let mut title = None;
        let mut perf = Performer::new(&mut g, &mut title);
        // CSI > 4 ; 2 m (modifyOtherKeys) не должен включать UNDERLINE/FAINT
        for b in b"\x1b[?1h\x1b[?2026h\x1b[>4;2m\x1b[3;5H\x1b[6nX\x1b[?2026l" {
            parser.advance(&mut perf, *b);
        }
        assert_eq!(perf.take_replies(), vec![b"\x1b[3;5R".to_vec()]);
        assert!(g.app_cursor());
        assert!(!g.sync_update());
        assert!(g.line(2)[4].flags.is_empty());
        assert_eq!(g.line(2)[4].ch, 'X');
    }

    #[test]
    fn csi_cht_cbt_tab_stops_and_scosc_scorc() {
        let mut g = Grid::new(40, 5);
        // CHT: 3 таба вперёд → col 24; CBT: 2 таба назад от col 26 → col 16.
        run(&mut g, b"\x1b[3Iab\x1b[2ZXY");
        assert_eq!(g.line(0)[24].ch, 'a');
        assert_eq!(g.line(0)[25].ch, 'b');
        assert_eq!(g.line(0)[16].ch, 'X');
        assert_eq!(g.line(0)[17].ch, 'Y');

        // Пользовательский стоп табуляции (TBC 3 + HTS на col 5), затем DECST8C.
        run(&mut g, b"\x1b[2;1H\x1b[3g\x1b[6G\x1bH\r\tA");
        assert_eq!(g.line(1)[5].ch, 'A');
        run(&mut g, b"\x1b[?5W\r\tB");
        assert_eq!(g.line(1)[8].ch, 'B');

        // CSI s / CSI u (в vte params не пуст — содержит [[0]]).
        run(&mut g, b"\x1b[3;5H\x1b[s\x1b[4;20H\x1b[uZ");
        assert_eq!(g.line(2)[4].ch, 'Z');
    }

    #[test]
    fn decrqm_and_xtversion_and_da2_replies() {
        let mut g = Grid::new(80, 24);
        let mut parser = Parser::new();
        let mut title = None;
        let mut perf = Performer::new(&mut g, &mut title);
        for b in b"\x1b[>q\x1b[>c\x1b[?2026$p\x1b[?2026h\x1b[?2026$p\x1b[?2027$p" {
            parser.advance(&mut perf, *b);
        }
        assert_eq!(
            perf.take_replies(),
            vec![
                b"\x1bP>|syngui\x1b\\".to_vec(),
                b"\x1b[>0;0;0c".to_vec(),
                b"\x1b[?2026;2$y".to_vec(),
                b"\x1b[?2026;1$y".to_vec(),
                b"\x1b[?2027;0$y".to_vec(),
            ]
        );
    }

    #[test]
    fn dec_special_graphics_smacs_rmacs_and_nvtop_box() {
        let mut g = Grid::new(40, 4);
        // xterm-256color: smacs = \E(0, rmacs = \E(B (как шлёт ncurses / nvtop)
        run(&mut g, b"87\x1b(0f\x1b(BC\r\n");
        run(&mut g, b"\x1b(0lqqqk\x1b(B\r\n");
        run(&mut g, b"\x1b(0x\x1b(Babc\x1b(0x\x1b(B\r\n");
        run(&mut g, b"\x1b(0m\x1b(B0\x1b(0sj\x1b(B");
        assert_eq!(row_str(&g, 0), "87°C");
        assert_eq!(row_str(&g, 1), "┌───┐");
        assert_eq!(row_str(&g, 2), "│abc│");
        assert_eq!(row_str(&g, 3), "└0⎽┘");
    }

    #[test]
    fn charset_g1_shift_out_in_and_single_shift_and_decsc() {
        let mut g = Grid::new(40, 4);
        // G1 = DEC Special Graphics, SO (0x0E) / SI (0x0F)
        run(&mut g, b"\x1b)0A\x0elqk\x0fB\r\n");
        assert_eq!(row_str(&g, 0), "A┌─┐B");

        // G2 = DEC Special Graphics, SS2 (ESC N) действует ровно на один символ
        run(&mut g, b"\x1b*0\x1bNqq\r\n");
        assert_eq!(row_str(&g, 1), "─q");

        // DECSC/DECRC сохраняют и восстанавливают таблицы; REP повторяет уже переведённый символ
        run(&mut g, b"\x1b(0\x1b7\x1b(Bq");
        assert_eq!(row_str(&g, 2), "q");
        run(&mut g, b"\x1b8q\x1b[2b\r\n");
        assert_eq!(row_str(&g, 2), "───");

        // RIS (ESC c) сбрасывает таблицу в ASCII
        run(&mut g, b"\x1bcq");
        assert_eq!(row_str(&g, 0), "q");
    }
}

fn query_dec_mode(grid: &Grid, mode: u16) -> u8 {
    let flag = match mode {
        1 => Some(grid.app_cursor()),
        6 => Some(grid.origin_mode()),
        7 => Some(grid.auto_wrap()),
        25 => Some(grid.cursor_visible()),
        1000 => Some(grid.mouse_mode() == MouseMode::Normal),
        1002 => Some(grid.mouse_mode() == MouseMode::ButtonEvent),
        1003 => Some(grid.mouse_mode() == MouseMode::AnyEvent),
        1004 => Some(grid.focus_events()),
        1006 => Some(grid.mouse_encoding() == MouseEncoding::Sgr),
        1007 => Some(grid.alt_scroll()),
        1047 | 1049 => Some(grid.on_alt()),
        2004 => Some(grid.bracketed_paste()),
        2026 => Some(grid.sync_update()),
        // 2027 (grapheme clustering) не поддерживается → 0 (не распознан),
        // чтобы TUI считали ширины по стандартному wcwidth.
        _ => None,
    };
    match flag {
        Some(true) => 1,
        Some(false) => 2,
        None => 0,
    }
}

fn apply_dec_mode(grid: &mut Grid, mode: u16, on: bool) {
    match mode {
        1 => grid.set_app_cursor(on),
        6 => grid.set_origin_mode(on),
        7 => grid.set_auto_wrap(on),
        25 => grid.set_cursor_visible(on),

        9 => {
            grid.set_mouse_mode(if on { MouseMode::X10 } else { MouseMode::Off });
        }
        1000 => {
            grid.set_mouse_mode(if on {
                MouseMode::Normal
            } else {
                MouseMode::Off
            });
        }
        1002 => {
            grid.set_mouse_mode(if on {
                MouseMode::ButtonEvent
            } else {
                MouseMode::Off
            });
        }
        1003 => {
            grid.set_mouse_mode(if on {
                MouseMode::AnyEvent
            } else {
                MouseMode::Off
            });
        }

        1004 => grid.set_focus_events(on),

        1005 => {
            grid.set_mouse_encoding(if on {
                MouseEncoding::Utf8
            } else {
                MouseEncoding::Default
            });
        }
        1006 => {
            grid.set_mouse_encoding(if on {
                MouseEncoding::Sgr
            } else {
                MouseEncoding::Default
            });
        }
        1015 => {
            grid.set_mouse_encoding(if on {
                MouseEncoding::Urxvt
            } else {
                MouseEncoding::Default
            });
        }

        2004 => grid.set_bracketed_paste(on),
        2026 => grid.set_sync_update(on),

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

fn read_extended<'a>(mode: u16, iter: &mut std::iter::Peekable<vte::ParamsIter<'a>>) -> CellColor {
    match mode {
        5 => {
            let idx = iter.next().and_then(|p| p.first().copied()).unwrap_or(0);
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
