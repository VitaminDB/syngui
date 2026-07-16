use syngui::mgui;
use syngui::prelude::*;

use super::filter_card;
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Overlay Effects").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("Наложения: noise (зернистость), vignette (затемнение краёв)")
                .class("label"),

            // Noise / Grain
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Noise / Grain"),
                    label("noise: 0.0–1.0 — процедурная зернистость для текстурирования и винтажного стиля"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", "без noise"),
                        filter_card("gradient-sunset", "fx-noise-light", "Light 0.15", "noise: 0.15"),
                        filter_card("gradient-sunset", "fx-noise-medium", "Medium 0.35", "noise: 0.35"),
                        filter_card("gradient-sunset", "fx-noise-heavy", "Heavy 0.6", "noise: 0.6"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "", "Original", "без noise"),
                        filter_card("gradient-ocean", "fx-noise-light", "Light 0.15", "noise: 0.15"),
                        filter_card("gradient-ocean", "fx-noise-medium", "Medium 0.35", "noise: 0.35"),
                        filter_card("gradient-ocean", "fx-noise-heavy", "Heavy 0.6", "noise: 0.6"),
                    ],
                ]
            }),

            // Vignette
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Vignette"),
                    label("vignette: 0.0–1.0 — затемнение краёв элемента, фокус на центре"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", "без vignette"),
                        filter_card("gradient-sunset", "fx-vignette-light", "Light 0.3", "vignette: 0.3"),
                        filter_card("gradient-sunset", "fx-vignette-medium", "Medium 0.6", "vignette: 0.6"),
                        filter_card("gradient-sunset", "fx-vignette-heavy", "Heavy 0.9", "vignette: 0.9"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "", "Original", "без vignette"),
                        filter_card("gradient-ocean", "fx-vignette-light", "Light 0.3", "vignette: 0.3"),
                        filter_card("gradient-ocean", "fx-vignette-medium", "Medium 0.6", "vignette: 0.6"),
                        filter_card("gradient-ocean", "fx-vignette-heavy", "Heavy 0.9", "vignette: 0.9"),
                    ],
                ]
            }),

            // Hologram
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Hologram"),
                    label("filter: hologram(#color, intensity) — голографическое наложение цвета"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "", "Original", ""),
                        filter_card("gradient-ocean", "fx-hologram-cyan", "Cyan", "filter: hologram(#22d3ee, 0.3)"),
                        filter_card("gradient-ocean", "fx-hologram-cyan-strong", "Cyan Strong", "filter: hologram(#22d3ee, 0.7)"),
                        filter_card("gradient-ocean", "fx-hologram-pink", "Pink", "filter: hologram(#ec4899, 0.4)"),
                    ],
                ]
            }),

            // Lens Flare
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Lens Flare"),
                    label("filter: lens-flare(intensity) — имитация бликов объектива"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", ""),
                        filter_card("gradient-sunset", "fx-lens-flare-low", "Low", "filter: lens-flare(0.2)"),
                        filter_card("gradient-sunset", "fx-lens-flare-medium", "Medium", "filter: lens-flare(0.5)"),
                        filter_card("gradient-sunset", "fx-lens-flare-high", "High", "filter: lens-flare(0.8)"),
                    ],
                ]
            }),

            // Noise + Vignette combo
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Noise + Vignette"),
                    label("Комбинация: зернистость + затемнение краёв = кинематографический эффект"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-chain-vintage", "Vintage", "filter: sepia(40%)\nnoise(0.15) vignette(0.4)"),
                    ],
                ]
            }),
        ]
    }
}
