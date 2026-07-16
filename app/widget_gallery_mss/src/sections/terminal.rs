//! Demo-страница виджета `Terminal` (feature `terminal`).
//!
//! Полнофункциональный VT100/ANSI-терминал поверх PTY:
//!
//! - выделение текста: drag (Simple), double-click (Word), triple-click (Line),
//!   `Alt`+drag (Block); копирование `Ctrl+Shift+C`, вставка `Ctrl+Shift+V`
//!   (с bracketed-paste, если приложение его включило);
//! - mouse reporting (`htop`, `mc`, `vim` с `:set mouse=a`, `tmux`) — DECSET
//!   1000/1002/1003 + 1006/1015 кодировок; `Shift`+drag отключает forwarding
//!   и активирует обычное выделение поверх mouse-grabbing программ;
//! - OSC 8 hyperlinks (`gh issue list`, `ls --hyperlink=auto`, `eza --hyperlink`):
//!   подчёркивание, hover-Pointer, click — открывает URL через `webbrowser`.

use syngui::prelude::*;
use syngui::widgets::Terminal;

pub fn build_terminal_section() -> impl Widget {
    Column::new()
        .gap(12.0)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .child(super::section_title("Terminal (PTY + VT100 + mouse + OSC 8)"))
        .child(
            super::section_card(
                Column::new()
                    .gap(8.0)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .child(
                        Text::new(
                            "Кликните в область — фокус. Поддерживается полный xterm-протокол: \
                             SGR (16/256/truecolor + colon-form), mouse reporting (htop, mc, vim, tmux), \
                             OSC 8 hyperlinks (попробуйте: \
                             `printf '\\e]8;;https://example.com\\e\\\\link\\e]8;;\\e\\\\\\n'`), \
                             alt-screen (vim/nano/htop/less не загрязняют scrollback), \
                             alternate-scroll (wheel в nano/less шлёт стрелки). \
                             Выделение мышью + Ctrl+Alt+C / Ctrl+Alt+V (Ctrl+Shift зарезервирован \
                             переключателем раскладки). Shift+drag поверх htop/tmux отключает их \
                             mouse-grab и выделяет поверх.",
                        )
                        .class("description"),
                    )
                    .child(
                        DecoratedBox::new()
                            .class("terminal-host")
                            .child(Terminal::new().font_size(13.0).class("gallery-terminal")),
                    ),
            ),
        )
}
