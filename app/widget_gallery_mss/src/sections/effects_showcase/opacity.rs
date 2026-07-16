use syngui::mgui;
use syngui::prelude::*;

use super::filter_card;
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Opacity & Transparency").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("Базовый контроль альфа-канала: статическая прозрачность, transitions, анимации")
                .class("label"),

            // Static opacity levels
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Static Opacity"),
                    label("opacity: 0.0–1.0 — базовый контроль прозрачности элемента"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-opacity-100", "100%", "opacity: 1.0"),
                        filter_card("gradient-sunset", "fx-opacity-80", "80%", "opacity: 0.8"),
                        filter_card("gradient-sunset", "fx-opacity-60", "60%", "opacity: 0.6"),
                        filter_card("gradient-sunset", "fx-opacity-40", "40%", "opacity: 0.4"),
                        filter_card("gradient-sunset", "fx-opacity-20", "20%", "opacity: 0.2"),
                    ],
                ]
            }),

            // Opacity transition
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Opacity Transition"),
                    label("transition: opacity — плавное изменение прозрачности при hover"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-trans-opacity", "Hover → Fade", "transition: opacity\n400ms ease\nhover: opacity 0.3"),
                        filter_card("gradient-ocean", "fx-trans-opacity", "Hover → Fade", "transition: opacity\n400ms ease\nhover: opacity 0.3"),
                    ],
                ]
            }),

            // Pulse animation
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Pulse / Flicker (@keyframes)"),
                    label("Периодическое изменение прозрачности — индикаторы активности, уведомления"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-anim-pulse", "Pulse", "@keyframes pulse\nopacity: 1→0.3→1"),
                        filter_card("gradient-ocean", "fx-anim-pulse", "Pulse", "@keyframes pulse\nopacity: 1→0.3→1"),
                    ],
                ]
            }),

            // Practical examples
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Практические примеры"),
                    label("Disabled-состояния, оверлеи, тултипы"),

                    Row::new().gap(16.0) => [
                        Column::new().gap(4.0) => [
                            Button::new("Normal"),
                            Text::new("opacity: 1.0").class("fx-code"),
                        ],
                        Column::new().gap(4.0) => [
                            Button::new("Disabled").disabled(true),
                            Text::new("disabled state").class("fx-code"),
                        ],
                    ],
                ]
            }),
        ]
    }
}
