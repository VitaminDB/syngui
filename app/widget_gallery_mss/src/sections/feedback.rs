use syngui::prelude::*;
use syngui::signal::use_signal;

use super::{label, section_card, section_title};

pub fn build_feedback_section() -> impl Widget {
    let snackbar_show = use_signal(false);
    let notifications = NotificationCtx::new();

    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("Feedback"))
            // Tooltip
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Tooltip (hover over buttons)"))
                    .child(
                        Row::new()
                            .gap(12.0)
                            .child(
                                Tooltip::new(
                                    Button::new("Below"),
                                    "Tooltip appears below",
                                )
                                .position(TooltipPosition::Below),
                            )
                            .child(
                                Tooltip::new(
                                    Button::new("Above"),
                                    "Tooltip appears above",
                                )
                                .position(TooltipPosition::Above),
                            )
                            .child(
                                Tooltip::new(
                                    Button::new("Left"),
                                    "Tooltip appears left",
                                )
                                .position(TooltipPosition::Left),
                            )
                            .child(
                                Tooltip::new(
                                    Button::new("Right"),
                                    "Tooltip appears right",
                                )
                                .position(TooltipPosition::Right),
                            ),
                    ),
            )
            // Snackbar
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Snackbar"))
                    .child({
                        Button::new("Show Snackbar").on_click(move || {
                            snackbar_show.set(true);
                        })
                    })
                    .child(
                        Snackbar::new("File saved successfully!", snackbar_show)
                            .action("Undo", move || {
                                snackbar_show.set(false);
                            })
                            .duration_ms(4000)
                            .position(SnackbarPosition::BottomCenter),
                    ),
            )
            // NotificationHost
            .child(
                Column::new()
                    .gap(8.0)
                    .child(label("Notifications"))
                    .child(
                        Row::new()
                            .gap(8.0)
                            .child({
                                let n = notifications.clone();
                                Button::new("Info").on_click(move || {
                                    n.show(
                                        NotificationItem::info("Information")
                                            .message("This is an info notification")
                                            .duration_ms(3000),
                                    );
                                })
                            })
                            .child({
                                let n = notifications.clone();
                                Button::new("Success").on_click(move || {
                                    n.show(
                                        NotificationItem::success("Success!")
                                            .message("Operation completed successfully")
                                            .duration_ms(3000),
                                    );
                                })
                            })
                            .child({
                                let n = notifications.clone();
                                Button::new("Warning").on_click(move || {
                                    n.show(
                                        NotificationItem::warning("Warning")
                                            .message("Please check your settings")
                                            .duration_ms(4000),
                                    );
                                })
                            })
                            .child({
                                let n = notifications.clone();
                                Button::new("Error").on_click(move || {
                                    n.show(
                                        NotificationItem::error("Error")
                                            .message("Something went wrong")
                                            .duration_ms(5000),
                                    );
                                })
                            }),
                    )
                    .child(NotificationHost::new(notifications.clone())),
            ),
    )
}
