use syngui::prelude::*;
use syngui::signal::use_signal;

use super::{label, section_card, section_title};

pub fn build_dialogs_section() -> impl Widget {
    let alert_open = use_signal(false);
    let confirm_open = use_signal(false);
    let custom_open = use_signal(false);
    let window_open = use_signal(false);
    let window_pos = use_signal(Point::new(200.0, 150.0));
    let confirm_result = use_signal(String::from("(no result)"));
    let portal_open = use_signal(false);
    let portal_nonmodal_open = use_signal(false);

    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Dialogs & Windows"))
            // Alert Dialog
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Alert Dialog"))
                    .child(
                        Row::new().gap(8.0).child({
                            Button::new("Show Alert").class("primary").on_click(move || {
                                alert_open.set(true);
                            })
                        }),
                    )
                    .child(AlertDialog::new(
                        "Alert",
                        "This is an informational alert dialog. Click OK or outside to dismiss.",
                        alert_open,
                    )),
            )
            // Confirm Dialog
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Confirm Dialog"))
                    .child(
                        Row::new()
                            .gap(8.0)
                            .child({
                                Button::new("Show Confirm").class("primary").on_click(move || {
                                    confirm_open.set(true);
                                })
                            })
                            .child(move || {
                                let result = confirm_result.get();
                                Text::new(format!("Result: {}", result)).class("label")
                            }),
                    )
                    .child(ConfirmDialog::new(
                        "Confirm Action",
                        "Are you sure you want to proceed? This action may have consequences.",
                        confirm_open,
                        move |confirmed| {
                            confirm_result.set(if confirmed {
                                "Confirmed!".to_string()
                            } else {
                                "Cancelled".to_string()
                            });
                        },
                    )),
            )
            // Custom Dialog
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Custom Dialog"))
                    .child({
                        Button::new("Show Custom Dialog")
                            .class("primary")
                            .on_click(move || {
                                custom_open.set(true);
                            })
                    })
                    .child({
                        Dialog::new("Custom Dialog")
                            .body(
                                "This dialog has custom action buttons with different styles.",
                            )
                            .is_open(custom_open)
                            .width(450.0)
                            .action(DialogAction::new("Delete", move || {
                                custom_open.set(false);
                            }))
                            .action(
                                DialogAction::new("Save", move || {
                                    custom_open.set(false);
                                })
                                ,
                            )
                    }),
            )
            // Floating Window
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Floating Window (draggable)"))
                    .child({
                        Button::new("Open Floating Window")
                            .class("primary")
                            .on_click(move || {
                                window_open.set(true);
                            })
                    })
                    .child(
                        FloatingWindow::new("Floating Window")
                            .is_open(window_open)
                            .position(window_pos)
                            .size(Size::new(320.0, 180.0))
                            .child(
                                Column::new()
                                    .gap(8.0)
                                    .child(Text::new("This window can be dragged by its title bar.").class("label"))
                                    .child(Text::new("Click the X to close it.").class("label"))
                                    .child(Text::new("Now it supports arbitrary child widgets!").class("label"))
                            ),
                    ),
            )
            // Portal (modal, with custom child widgets)
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Portal (modal, compositional)"))
                    .child({
                        Button::new("Open Portal Dialog")
                            .class("primary")
                            .on_click(move || {
                                portal_open.set(true);
                            })
                    })
                    .child({
                        Portal::new()
                            .is_open(portal_open)
                            .modal(true)
                            .child(
                                Card::new().child(
                                    Padding::all(24.0).child(
                                        Column::new()
                                            .gap(16.0)
                                            .child(Text::new("Portal Dialog").class("h2"))
                                            .child(Text::new(
                                                "This dialog is built with Portal — arbitrary child widgets \
                                                 rendered in the overlay layer. Unlike Dialog, you can use \
                                                 any widget as content: inputs, lists, custom layouts, etc."
                                            ).class("label"))
                                            .child(TextField::new().placeholder("Type something..."))
                                            .child(
                                                Row::new()
                                                    .gap(8.0)
                                                    .child(DecoratedBox::new().class("grow"))
                                                    .child(Button::new("Close").class("secondary").on_click(
                                                        move || { portal_open.set(false); }
                                                    ))
                                                    .child(Button::new("Submit").class("primary").on_click(
                                                        move || { portal_open.set(false); }
                                                    ))
                                            )
                                    )
                                )
                            )
                    }),
            )
            // Portal (non-modal)
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Portal (non-modal)"))
                    .child({
                        Button::new("Open Non-Modal Portal")
                            .class("primary")
                            .on_click(move || {
                                portal_nonmodal_open.set(true);
                            })
                    })
                    .child({
                        Portal::new()
                            .is_open(portal_nonmodal_open)
                            .modal(false)
                            .backdrop(false)
                            .child(
                                Card::new().child(
                                    Padding::all(16.0).child(
                                        Column::new()
                                            .gap(12.0)
                                            .child(Text::new("Non-Modal Portal").class("h3"))
                                            .child(Text::new(
                                                "This portal does not block interaction with the rest of the UI."
                                            ).class("label"))
                                            .child(Button::new("Dismiss").class("secondary").on_click(
                                                move || { portal_nonmodal_open.set(false); }
                                            ))
                                    )
                                )
                            )
                    }),
            ),
    )
}
