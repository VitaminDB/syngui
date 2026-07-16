use syngui::mgui;
use syngui::prelude::*;

use super::filter_card;
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Filter Transitions").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("transition: filter — плавная интерполяция фильтров при наведении мыши")
                .class("label"),

            // Blur transition
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Blur Transition"),
                    label("Наведите мышь: элемент плавно размывается"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-trans-blur", "→ Blur 6px", "transition: filter 400ms\nhover: blur(6px)"),
                        filter_card("gradient-ocean", "fx-trans-blur", "→ Blur 6px", "transition: filter 400ms\nhover: blur(6px)"),
                    ],
                ]
            }),

            // Color filter transitions
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Color Transitions"),
                    label("Плавный переход к цветовым фильтрам"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-trans-grayscale", "→ Grayscale", "hover: grayscale(100%)"),
                        filter_card("gradient-sunset", "fx-trans-sepia", "→ Sepia", "hover: sepia(80%)"),
                        filter_card("gradient-ocean", "fx-trans-grayscale", "→ Grayscale", "hover: grayscale(100%)"),
                        filter_card("gradient-ocean", "fx-trans-sepia", "→ Sepia", "hover: sepia(80%)"),
                    ],
                ]
            }),

            // Brightness & contrast transitions
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Brightness & Other Transitions"),
                    label("Яркость, контраст, инверсия, пикселизация"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-trans-bright", "→ Bright 1.5", "hover: brightness(1.5)"),
                        filter_card("gradient-ocean", "fx-trans-contrast", "→ Contrast 2.0", "hover: contrast(2.0)"),
                        filter_card("gradient-sunset", "fx-trans-invert", "→ Invert", "hover: invert(100%)"),
                        filter_card("gradient-ocean", "fx-trans-pixelate", "→ Pixelate", "hover: pixelate(6px)"),
                    ],
                ]
            }),

            // Hue rotate transition
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Hue Rotate Transition"),
                    label("Плавный сдвиг оттенка при наведении"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-trans-hue", "→ Hue +180°", "hover: hue-rotate(180deg)"),
                        filter_card("gradient-ocean", "fx-trans-hue", "→ Hue +180°", "hover: hue-rotate(180deg)"),
                        filter_card("gradient-rainbow", "fx-trans-hue", "→ Hue +180°", "hover: hue-rotate(180deg)"),
                    ],
                ]
            }),

            // Dissolve & Glitch transitions
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Dissolve & Glitch Transitions"),
                    label("transition: filter — dissolve и glitch эффекты при наведении"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-trans-dissolve", "→ Dissolve", "hover: dissolve(0.5)"),
                        filter_card("gradient-ocean", "fx-trans-dissolve", "→ Dissolve", "hover: dissolve(0.5)"),
                        filter_card("gradient-sunset", "fx-trans-glitch", "→ Glitch", "hover: glitch(0.5)"),
                        filter_card("gradient-ocean", "fx-trans-glitch", "→ Glitch", "hover: glitch(0.5)"),
                    ],
                ]
            }),

            // Mask Reveal transition
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Mask Reveal Transition"),
                    label("transition: filter — маска раскрытия при наведении"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-trans-mask-reveal", "→ Mask Reveal", "hover: mask-reveal(1.0)"),
                        filter_card("gradient-ocean", "fx-trans-mask-reveal", "→ Mask Reveal", "hover: mask-reveal(1.0)"),
                        filter_card("gradient-diagonal", "fx-trans-mask-reveal", "→ Mask Reveal", "hover: mask-reveal(1.0)"),
                    ],
                ]
            }),

            // All transitions summary
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Transition Timing Functions"),
                    label("transition: filter <duration> <easing>"),

                    Text::new("Поддерживаемые easing:").bold().class("fx-label"),
                    Text::new("ease, linear, ease-in, ease-out, ease-in-out, ease-out-bounce").class("fx-code"),
                    Text::new("\ntransition: filter 400ms ease — стандартный\ntransition: filter 200ms ease-out — быстрый отклик\ntransition: filter 800ms ease-in-out — плавный").class("fx-code"),
                ]
            }),
        ]
    }
}
