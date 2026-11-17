use syngui::prelude::*;

use super::{label, section_card, section_title};

pub fn build_dragdrop_section() -> impl Widget {
    let drop_result = use_signal(Vec::<String>::new());
    let drop_result2 = use_signal(Vec::<String>::new());

    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Drag & Drop"))
            // Draggable items
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Draggable Items"))
                    .child(
                        Row::new()
                            .gap(8.0)
                            .child(
                                Draggable::new("item", "Apple 🍎").child(
                                    DecoratedBox::new()
                                        .child(Text::new("🍎 Apple").class("drag-item-text"))
                                        .class("drag-item"),
                                ),
                            )
                            .child(
                                Draggable::new("item", "Banana 🍌").child(
                                    DecoratedBox::new()
                                        .child(Text::new("🍌 Banana").class("drag-item-text"))
                                        .class("drag-item"),
                                ),
                            )
                            .child(
                                Draggable::new("item", "Cherry 🍒").child(
                                    DecoratedBox::new()
                                        .child(Text::new("🍒 Cherry").class("drag-item-text"))
                                        .class("drag-item"),
                                ),
                            )
                            .child(
                                Draggable::new("item", "Grape 🍇").child(
                                    DecoratedBox::new()
                                        .child(Text::new("🍇 Grape").class("drag-item-text"))
                                        .class("drag-item"),
                                ),
                            ),
                    ),
            )
            // Drop areas
            .child(
                Column::new().gap(8.0).child(label("Drop Areas")).child(
                    Row::new()
                        .gap(16.0)
                        .child(
                            DropArea::new()
                                .accept_types(vec!["item".to_string()])
                                .placeholder("Drop fruits here")
                                .on_drop(move |data| {
                                    drop_result.update(|v| v.push(data.payload.clone()));
                                }),
                        )
                        .child(
                            DropArea::new()
                                .accept_types(vec!["item".to_string()])
                                .placeholder("Or drop here")
                                .on_drop(move |data| {
                                    drop_result2.update(|v| v.push(data.payload.clone()));
                                }),
                        ),
                ),
            )
            // How it works
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("How Drag & Drop Works"))
                    .child(
                        Text::new("1. Wrap items in Draggable::new(type, payload)").class("label"),
                    )
                    .child(
                        Text::new("2. Create DropArea::new().accept_types([...])").class("label"),
                    )
                    .child(
                        Text::new("3. Use on_drop callback to handle dropped data").class("label"),
                    )
                    .child(
                        Text::new("4. Visual feedback: border highlights on drag over")
                            .class("label"),
                    ),
            ),
    )
}
