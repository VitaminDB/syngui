//! Контекстное меню текстовых полей — «Вырезать / Копировать / Вставить /
//! Выделить всё», общее для [`TextField`](super::text_field::TextField) и
//! [`MultilineTextEdit`](super::MultilineTextEdit).
//!
//! Пункт выбирается колбэком `PopupMenu`, у которого нет доступа к элементу
//! поля, поэтому выбор кладётся в сигнал и разбирается полем в `animate`.
//! Пока меню открыто (или действие ещё не разобрано), поле обязано заявлять
//! [`Element::wants_animate_tick`](crate::widget::Element::wants_animate_tick) —
//! иначе точечный реестр анимаций его не обойдёт и пункт не сработает.

use crate::core::Point;
use crate::signal::RwSignal;
use crate::widgets::overlay::menu::{MenuItem, PopupMenu};

const ICON_CONTENT_CUT: &str = "\u{E14E}";
const ICON_CONTENT_COPY: &str = "\u{E14D}";
const ICON_CONTENT_PASTE: &str = "\u{E14F}";
const ICON_SELECT_ALL: &str = "\u{E162}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMenuAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// Меню для поля ввода. У поля только для чтения остаются «Копировать» и
/// «Выделить всё» — вырезать и вставлять в него нечего.
pub(crate) fn edit_context_menu(
    open: RwSignal<bool>,
    pos: RwSignal<Point>,
    action: RwSignal<Option<EditMenuAction>>,
    read_only: bool,
    has_selection: bool,
) -> PopupMenu {
    let mut items = Vec::with_capacity(4);
    if !read_only {
        items.push(
            MenuItem::new("cut", crate::i18n::builtin("text_edit.cut", "Cut"))
                .icon(ICON_CONTENT_CUT)
                .shortcut("Ctrl+X")
                .disabled(!has_selection),
        );
    }
    items.push(
        MenuItem::new("copy", crate::i18n::builtin("text_edit.copy", "Copy"))
            .icon(ICON_CONTENT_COPY)
            .shortcut("Ctrl+C")
            .disabled(!has_selection),
    );
    if !read_only {
        items.push(
            MenuItem::new("paste", crate::i18n::builtin("text_edit.paste", "Paste"))
                .icon(ICON_CONTENT_PASTE)
                .shortcut("Ctrl+V"),
        );
    }
    items.push(MenuItem::separator());
    items.push(
        MenuItem::new(
            "select_all",
            crate::i18n::builtin("text_edit.select_all", "Select all"),
        )
        .icon(ICON_SELECT_ALL)
        .shortcut("Ctrl+A"),
    );

    PopupMenu::new()
        .items(items)
        .is_open(open)
        .position(pos)
        .on_select(move |id| {
            let picked = match id {
                "cut" => Some(EditMenuAction::Cut),
                "copy" => Some(EditMenuAction::Copy),
                "paste" => Some(EditMenuAction::Paste),
                "select_all" => Some(EditMenuAction::SelectAll),
                _ => None,
            };
            if let Some(a) = picked {
                action.set(Some(a));
            }
        })
}
