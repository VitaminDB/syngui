use syngui::mgui;
use syngui::prelude::*;

use super::filter_card;
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Distortion Effects").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("Искажения: chromatic aberration, pixelate, CRT/scanlines, wave displacement")
                .class("label"),

            // Chromatic Aberration
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Chromatic Aberration"),
                    label("filter: chromatic-aberration() — смещение RGB-каналов, глитч-стиль"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", "без эффекта"),
                        filter_card("gradient-sunset", "fx-chroma-2", "2px", "filter:\nchromatic-aberration(2px)"),
                        filter_card("gradient-sunset", "fx-chroma-5", "5px", "filter:\nchromatic-aberration(5px)"),
                        filter_card("gradient-sunset", "fx-chroma-8", "8px", "filter:\nchromatic-aberration(8px)"),
                    ],
                ]
            }),

            // Pixelate / Mosaic
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Pixelate / Mosaic"),
                    label("filter: pixelate() — снижение разрешения, пиксель-арт стиль"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", "без эффекта"),
                        filter_card("gradient-sunset", "fx-pixelate-4", "4px", "filter: pixelate(4px)"),
                        filter_card("gradient-sunset", "fx-pixelate-8", "8px", "filter: pixelate(8px)"),
                        filter_card("gradient-sunset", "fx-pixelate-16", "16px", "filter: pixelate(16px)"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "", "Original", "без эффекта"),
                        filter_card("gradient-ocean", "fx-pixelate-4", "4px", "filter: pixelate(4px)"),
                        filter_card("gradient-ocean", "fx-pixelate-8", "8px", "filter: pixelate(8px)"),
                        filter_card("gradient-ocean", "fx-pixelate-16", "16px", "filter: pixelate(16px)"),
                    ],
                ]
            }),

            // CRT / Scanlines
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Scanlines / CRT"),
                    label("filter: crt() — горизонтальные полосы, эмуляция CRT монитора"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", "без эффекта"),
                        filter_card("gradient-sunset", "fx-crt-light", "Light 0.3", "filter: crt(0.3)"),
                        filter_card("gradient-sunset", "fx-crt-medium", "Medium 0.5", "filter: crt(0.5)"),
                        filter_card("gradient-sunset", "fx-crt-heavy", "Heavy 0.8", "filter: crt(0.8)"),
                    ],
                ]
            }),

            // Wave Displacement
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Displacement / Wave"),
                    label("filter: wave(amplitude, frequency) — смещение пикселей по синусоиде"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "", "Original", "без эффекта"),
                        filter_card("gradient-ocean", "fx-wave-subtle", "Subtle", "filter: wave(2px, 0.3)"),
                        filter_card("gradient-ocean", "fx-wave-medium", "Medium", "filter: wave(4px, 0.5)"),
                        filter_card("gradient-ocean", "fx-wave-heavy", "Heavy", "filter: wave(8px, 0.8)"),
                    ],
                ]
            }),

            // Glitch
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Glitch"),
                    label("filter: glitch(intensity) — цифровые помехи, RGB split"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", ""),
                        filter_card("gradient-sunset", "fx-glitch-light", "Light 0.2", "filter: glitch(0.2)"),
                        filter_card("gradient-sunset", "fx-glitch-medium", "Medium 0.5", "filter: glitch(0.5)"),
                        filter_card("gradient-sunset", "fx-glitch-heavy", "Heavy 0.8", "filter: glitch(0.8)"),
                    ],
                ]
            }),

            // Swirl
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Swirl"),
                    label("filter: swirl(angle) — спиральное закручивание изображения"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "", "Original", ""),
                        filter_card("gradient-ocean", "fx-swirl-subtle", "Subtle 0.3", "filter: swirl(0.3)"),
                        filter_card("gradient-ocean", "fx-swirl-medium", "Medium 0.6", "filter: swirl(0.6)"),
                        filter_card("gradient-ocean", "fx-swirl-heavy", "Heavy 1.0", "filter: swirl(1.0)"),
                    ],
                ]
            }),

            // Bulge & Pinch
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Bulge & Pinch"),
                    label("filter: bulge(strength) / pinch(strength) — выпуклость и вогнутость"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", ""),
                        filter_card("gradient-sunset", "fx-bulge-subtle", "Bulge 0.3", "filter: bulge(0.3)"),
                        filter_card("gradient-sunset", "fx-bulge-medium", "Bulge 0.5", "filter: bulge(0.5)"),
                        filter_card("gradient-sunset", "fx-bulge-heavy", "Bulge 0.8", "filter: bulge(0.8)"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", ""),
                        filter_card("gradient-sunset", "fx-pinch-subtle", "Pinch 0.3", "filter: pinch(0.3)"),
                        filter_card("gradient-sunset", "fx-pinch-medium", "Pinch 0.5", "filter: pinch(0.5)"),
                        filter_card("gradient-sunset", "fx-pinch-heavy", "Pinch 0.8", "filter: pinch(0.8)"),
                    ],
                ]
            }),

            // Heat Haze
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Heat Haze"),
                    label("filter: heat-haze(intensity) — эффект марева, дрожание от жара"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "", "Original", ""),
                        filter_card("gradient-ocean", "fx-heat-haze-subtle", "Subtle 0.2", "filter: heat-haze(0.2)"),
                        filter_card("gradient-ocean", "fx-heat-haze-medium", "Medium 0.5", "filter: heat-haze(0.5)"),
                        filter_card("gradient-ocean", "fx-heat-haze-heavy", "Heavy 0.8", "filter: heat-haze(0.8)"),
                    ],
                ]
            }),

            // Refraction
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Refraction"),
                    label("filter: refract(strength) — преломление света, эффект стекла/воды"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", ""),
                        filter_card("gradient-sunset", "fx-refract-subtle", "Subtle 0.2", "filter: refract(0.2)"),
                        filter_card("gradient-sunset", "fx-refract-medium", "Medium 0.5", "filter: refract(0.5)"),
                        filter_card("gradient-sunset", "fx-refract-heavy", "Heavy 0.8", "filter: refract(0.8)"),
                    ],
                ]
            }),

            // Distortion combinations
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Комбинации искажений"),
                    label("Составные эффекты для глитч-стиля и ретро"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-chain-retro", "Retro", "sepia(30%) crt(0.3)\nnoise(0.1)"),
                        filter_card("gradient-ocean", "fx-chain-4", "Glitch", "invert(100%)\nchromatic-aberration(2px)"),
                    ],
                ]
            }),
        ]
    }
}
