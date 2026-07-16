use crate::signal::RwSignal;
use crate::widgets::buttons::ToolButton;
use crate::widgets::containers::Row;
use crate::widget::Widget;

use super::widget::EditorMode;

const ICON_EDIT: &str = "\u{E22B}";
const ICON_PREVIEW: &str = "\u{E8F4}";
const ICON_SPLIT: &str = "\u{E8E6}";

pub fn build_toolbar(mode: RwSignal<EditorMode>) -> Box<dyn Widget> {
    let current = mode.get_untracked();

    let mode_edit = mode;
    let mode_preview = mode;
    let mode_split = mode;

    Box::new(
        Row::new()
            .gap(4.0)
            .class("toolbar")
            .child(
                ToolButton::new(ICON_EDIT)
                    .tooltip("Edit")
                    .active(current == EditorMode::Edit)
                    .on_click(move || mode_edit.set(EditorMode::Edit)),
            )
            .child(
                ToolButton::new(ICON_PREVIEW)
                    .tooltip("Preview")
                    .active(current == EditorMode::Preview)
                    .on_click(move || mode_preview.set(EditorMode::Preview)),
            )
            .child(
                ToolButton::new(ICON_SPLIT)
                    .tooltip("Split")
                    .active(current == EditorMode::Split)
                    .on_click(move || mode_split.set(EditorMode::Split)),
            ),
    )
}
