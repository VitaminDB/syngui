use crate::input::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionGranularity {
    Char,
    Word,
    Line,
    Page,
    Document,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Move {
        granularity: MotionGranularity,
        forward: bool,
        extend_selection: bool,
    },
    MoveVertical {
        down: bool,
        page: bool,
        extend_selection: bool,
    },
    DeleteChar { forward: bool, word: bool },
    InsertNewline,
    InsertTab,
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
    Save,
    Escape,
    FindOpen,
    GoToLineOpen,
}

pub fn map_key(key: Key, m: Modifiers) -> Option<KeyAction> {
    let shift = m.shift;
    let ctrl = m.ctrl || m.meta;

    if ctrl && !m.alt {
        return match key {
            Key::A => Some(KeyAction::SelectAll),
            Key::C => Some(KeyAction::Copy),
            Key::X => Some(KeyAction::Cut),
            Key::V => Some(KeyAction::Paste),
            Key::S => Some(KeyAction::Save),
            Key::F => Some(KeyAction::FindOpen),
            Key::G => Some(KeyAction::GoToLineOpen),
            Key::Z if shift => Some(KeyAction::Redo),
            Key::Z => Some(KeyAction::Undo),
            Key::Y => Some(KeyAction::Redo),
            Key::Home => Some(KeyAction::Move {
                granularity: MotionGranularity::Document,
                forward: false,
                extend_selection: shift,
            }),
            Key::End => Some(KeyAction::Move {
                granularity: MotionGranularity::Document,
                forward: true,
                extend_selection: shift,
            }),
            Key::Left => Some(KeyAction::Move {
                granularity: MotionGranularity::Word,
                forward: false,
                extend_selection: shift,
            }),
            Key::Right => Some(KeyAction::Move {
                granularity: MotionGranularity::Word,
                forward: true,
                extend_selection: shift,
            }),
            Key::Backspace => Some(KeyAction::DeleteChar {
                forward: false,
                word: true,
            }),
            Key::Delete => Some(KeyAction::DeleteChar {
                forward: true,
                word: true,
            }),
            _ => None,
        };
    }

    match key {
        Key::Left => Some(KeyAction::Move {
            granularity: MotionGranularity::Char,
            forward: false,
            extend_selection: shift,
        }),
        Key::Right => Some(KeyAction::Move {
            granularity: MotionGranularity::Char,
            forward: true,
            extend_selection: shift,
        }),
        Key::Up => Some(KeyAction::MoveVertical {
            down: false,
            page: false,
            extend_selection: shift,
        }),
        Key::Down => Some(KeyAction::MoveVertical {
            down: true,
            page: false,
            extend_selection: shift,
        }),
        Key::PageUp => Some(KeyAction::MoveVertical {
            down: false,
            page: true,
            extend_selection: shift,
        }),
        Key::PageDown => Some(KeyAction::MoveVertical {
            down: true,
            page: true,
            extend_selection: shift,
        }),
        Key::Home => Some(KeyAction::Move {
            granularity: MotionGranularity::Line,
            forward: false,
            extend_selection: shift,
        }),
        Key::End => Some(KeyAction::Move {
            granularity: MotionGranularity::Line,
            forward: true,
            extend_selection: shift,
        }),
        Key::Backspace => Some(KeyAction::DeleteChar {
            forward: false,
            word: false,
        }),
        Key::Delete => Some(KeyAction::DeleteChar {
            forward: true,
            word: false,
        }),
        Key::Enter => Some(KeyAction::InsertNewline),
        Key::Tab => Some(KeyAction::InsertTab),
        Key::Escape => Some(KeyAction::Escape),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_mod() -> Modifiers {
        Modifiers::empty()
    }
    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Modifiers::empty()
        }
    }
    fn ctrl() -> Modifiers {
        Modifiers {
            ctrl: true,
            ..Modifiers::empty()
        }
    }
    fn ctrl_shift() -> Modifiers {
        Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::empty()
        }
    }

    #[test]
    fn arrow_left_char_motion() {
        assert_eq!(
            map_key(Key::Left, no_mod()),
            Some(KeyAction::Move {
                granularity: MotionGranularity::Char,
                forward: false,
                extend_selection: false,
            })
        );
    }

    #[test]
    fn shift_arrow_extends_selection() {
        let action = map_key(Key::Right, shift()).unwrap();
        if let KeyAction::Move {
            extend_selection, ..
        } = action
        {
            assert!(extend_selection);
        } else {
            panic!("expected Move");
        }
    }

    #[test]
    fn ctrl_arrow_word_motion() {
        assert_eq!(
            map_key(Key::Right, ctrl()),
            Some(KeyAction::Move {
                granularity: MotionGranularity::Word,
                forward: true,
                extend_selection: false,
            })
        );
    }

    #[test]
    fn ctrl_z_undo_ctrl_shift_z_redo() {
        assert_eq!(map_key(Key::Z, ctrl()), Some(KeyAction::Undo));
        assert_eq!(map_key(Key::Z, ctrl_shift()), Some(KeyAction::Redo));
        assert_eq!(map_key(Key::Y, ctrl()), Some(KeyAction::Redo));
    }

    #[test]
    fn ctrl_home_document_start() {
        assert_eq!(
            map_key(Key::Home, ctrl()),
            Some(KeyAction::Move {
                granularity: MotionGranularity::Document,
                forward: false,
                extend_selection: false,
            })
        );
    }

    #[test]
    fn enter_inserts_newline() {
        assert_eq!(map_key(Key::Enter, no_mod()), Some(KeyAction::InsertNewline));
    }

    #[test]
    fn tab_inserts_tab() {
        assert_eq!(map_key(Key::Tab, no_mod()), Some(KeyAction::InsertTab));
    }

    #[test]
    fn ctrl_s_save() {
        assert_eq!(map_key(Key::S, ctrl()), Some(KeyAction::Save));
    }

    #[test]
    fn ctrl_a_select_all() {
        assert_eq!(map_key(Key::A, ctrl()), Some(KeyAction::SelectAll));
    }

    #[test]
    fn unknown_key_returns_none() {
        assert_eq!(map_key(Key::F1, no_mod()), None);
    }
}
