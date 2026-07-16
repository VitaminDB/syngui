use syngui::mgui;
use syngui::prelude::*;

use super::filter_card;
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Blur & Glassmorphism").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("Размытие элемента (filter: blur) и содержимого под ним (backdrop-filter: blur)")
                .class("label"),

            // Gaussian blur
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Gaussian Blur (filter: blur)"),
                    label("Равномерное размытие самого элемента по ядру Гаусса"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", "без фильтра"),
                        filter_card("gradient-sunset", "fx-blur-4", "Blur 4px", "filter: blur(4px)"),
                        filter_card("gradient-sunset", "fx-blur-8", "Blur 8px", "filter: blur(8px)"),
                    ],
                ]
            }),

            // Directional blur
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Directional Blur"),
                    label("Размытие в заданном направлении — имитация движения или фокуса"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-dir-blur-h", "Horizontal 0°", "filter: directional-blur(0deg, 4px)"),
                        filter_card("gradient-sunset", "fx-dir-blur-d", "Diagonal 45°", "filter: directional-blur(45deg, 6px)"),
                        filter_card("gradient-sunset", "fx-dir-blur-v", "Vertical 90°", "filter: directional-blur(90deg, 4px)"),
                    ],
                ]
            }),

            // Motion blur
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Motion Blur"),
                    label("Размытие движения — эффект скорости и динамики"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "fx-motion-blur-light", "Light 4px", "filter: motion-blur(0deg, 4px)"),
                        filter_card("gradient-ocean", "fx-motion-blur-medium", "Medium 8px", "filter: motion-blur(0deg, 8px)"),
                        filter_card("gradient-ocean", "fx-motion-blur-heavy", "Diagonal 12px", "filter: motion-blur(45deg, 12px)"),
                    ],
                ]
            }),

            // Radial / Zoom blur
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Radial / Zoom Blur"),
                    label("Радиальное размытие от центра — эффект зума или взрыва"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", ""),
                        filter_card("gradient-sunset", "fx-radial-blur-light", "Light 0.2", "filter: radial-blur(0.2)"),
                        filter_card("gradient-sunset", "fx-radial-blur-medium", "Medium 0.5", "filter: radial-blur(0.5)"),
                        filter_card("gradient-sunset", "fx-radial-blur-heavy", "Heavy 0.8", "filter: radial-blur(0.8)"),
                    ],
                ]
            }),

            // Blur transition
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Blur Transition"),
                    label("transition: filter — плавное появление размытия при hover"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-trans-blur", "Hover \u{2192} Blur", "transition: filter 400ms\nhover: blur(6px)"),
                    ],
                ]
            }),

            // Animated blur
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Animated Blur (@keyframes breathe)"),
                    label("Пульсирующее размытие — декоративный фоновый эффект"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "fx-anim-breathe", "Breathe", "@keyframes breathe\nblur(0)→blur(4)→blur(0)"),
                    ],
                ]
            }),

            // Glass patterns
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Glassmorphism Patterns"),
                    label("backdrop-filter: blur + полупрозрачный фон + border — эффект стекла"),

                    // Gradient scene with glass cards on top
                    DecoratedBox::new().class("fx-glass-scene").child(mgui! {
                        Row::new().gap(16.0) => [
                            Column::new().gap(6.0) => [
                                DecoratedBox::new().class("fx-glass-card").child(mgui! {
                                    Column::new().gap(4.0) => [
                                        Text::new("Light Glass").bold().class("fx-glass-subtitle"),
                                        Text::new("blur 12px").class("fx-glass-subtitle"),
                                    ]
                                }),
                                Text::new("rgba(255,255,255,0.15)\nblur(12px)").class("fx-code"),
                            ],
                            Column::new().gap(6.0) => [
                                DecoratedBox::new().class("fx-glass-card-dark").child(mgui! {
                                    Column::new().gap(4.0) => [
                                        Text::new("Dark Glass").bold().class("fx-glass-subtitle"),
                                        Text::new("blur 8px").class("fx-glass-subtitle"),
                                    ]
                                }),
                                Text::new("rgba(0,0,0,0.2)\nblur(8px)").class("fx-code"),
                            ],
                            Column::new().gap(6.0) => [
                                DecoratedBox::new().class("fx-glass-card-strong").child(mgui! {
                                    Column::new().gap(4.0) => [
                                        Text::new("Strong Glass").bold().class("fx-glass-subtitle"),
                                        Text::new("blur 20px").class("fx-glass-subtitle"),
                                    ]
                                }),
                                Text::new("rgba(255,255,255,0.25)\nblur(20px)").class("fx-code"),
                            ],
                        ]
                    }),
                ]
            }),
        ]
    }
}
