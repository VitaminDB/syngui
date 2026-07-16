use syngui::mgui;
use syngui::prelude::*;

use super::filter_card;
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Filter Chains").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("Комбинации фильтров через пробел: filter: blur(2px) grayscale(50%) noise(0.1)")
                .class("label"),

            // Basic chains
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Basic Chains"),
                    label("Два фильтра: основной эффект + модификатор"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-chain-1", "Blur + Grayscale", "filter: blur(2px)\ngrayscale(70%)"),
                        filter_card("gradient-ocean", "fx-chain-2", "Sepia + Vignette", "filter: sepia(60%)\nvignette(0.5)"),
                        filter_card("gradient-sunset", "fx-chain-3", "Brightness + Noise", "filter: brightness(1.2)\nnoise(0.2)"),
                        filter_card("gradient-ocean", "fx-chain-4", "Invert + Chroma", "filter: invert(100%)\nchromatic-aberration(2px)"),
                    ],
                ]
            }),

            // Named presets
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Named Presets"),
                    label("Готовые пресеты из нескольких фильтров"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-chain-vintage", "Vintage", "sepia(40%) noise(0.15)\nvignette(0.4)"),
                        filter_card("gradient-ocean", "fx-chain-dreamy", "Dreamy", "blur(1px) brightness(1.2)\nsaturate(1.3)"),
                        filter_card("gradient-sunset", "fx-chain-dystopia", "Dystopia", "grayscale(60%)\ncontrast(1.4) noise(0.2)"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "fx-chain-neon", "Neon", "brightness(1.3)\nsaturate(1.8)\nchromatic-aberration(1px)"),
                        filter_card("gradient-sunset", "fx-chain-retro", "Retro", "sepia(30%) crt(0.3)\nnoise(0.1)"),
                        filter_card("gradient-ocean", "fx-chain-frost", "Frost", "blur(1px)\nbrightness(1.1)\ngrayscale(20%)"),
                    ],
                ]
            }),

            // Advanced presets
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Advanced Presets"),
                    label("Продвинутые пресеты с комбинацией эффектов"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-chain-cyberpunk", "Cyberpunk", "glitch(0.3)\nchromatic-aberration(2px)\ncrt(0.3)"),
                        filter_card("gradient-ocean", "fx-chain-underwater", "Underwater", "duotone(#001a33, #00b4d8)\nheat-haze(2px, 0.5)"),
                        filter_card("gradient-sunset", "fx-chain-hologram-mix", "Hologram Mix", "hologram(#22d3ee, 0.5)\nnoise(0.1)"),
                    ],
                ]
            }),

            // Cross-gradient comparison
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Cross-Gradient Comparison"),
                    label("Один и тот же chain на разных градиентах"),

                    Text::new("Vintage preset:").bold().class("fx-label"),
                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-chain-vintage", "Sunset", "sepia(40%) noise(0.15)\nvignette(0.4)"),
                        filter_card("gradient-ocean", "fx-chain-vintage", "Ocean", "sepia(40%) noise(0.15)\nvignette(0.4)"),
                        filter_card("gradient-diagonal", "fx-chain-vintage", "Diagonal", "sepia(40%) noise(0.15)\nvignette(0.4)"),
                        filter_card("gradient-rainbow", "fx-chain-vintage", "Rainbow", "sepia(40%) noise(0.15)\nvignette(0.4)"),
                    ],
                ]
            }),
        ]
    }
}
