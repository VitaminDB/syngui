use syngui::mgui;
use syngui::prelude::*;

use super::{shadow_card, filter_card, surface_card};
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Outline & Stroke").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("SDF-обводка и edge detection для границ, фокус-колец, hover-выделений")
                .class("label"),

            // Outline / Focus Ring
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Outline (SDF обводка)"),
                    label("outline-width + outline-color + outline-offset — чёткое кольцо с поддержкой border-radius"),

                    Row::new().gap(24.0) => [
                        shadow_card("fx-outline-default", "Default 2px", "outline-width: 2px\noutline-color: indigo"),
                        shadow_card("fx-outline-wide", "Wide 4px", "outline-width: 4px\noutline-color: green"),
                        shadow_card("fx-outline-offset", "Offset 4px", "outline-width: 2px\noutline-offset: 4px"),
                        shadow_card("fx-outline-rounded", "Rounded 16px", "border-radius: 16px\noutline-width: 2px"),
                    ],
                ]
            }),

            // Outline transition
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Outline Transition"),
                    label("transition: outline-width — плавное появление обводки при hover"),

                    Row::new().gap(16.0) => [
                        surface_card("fx-trans-outline", "Hover → Outline", "transition: outline-width\n300ms ease", "Наведите мышь"),
                    ],
                ]
            }),

            // Animated border glow
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Animated Border"),
                    label("@keyframes border-glow — пульсирующая граница"),

                    Row::new().gap(16.0) => [
                        surface_card("fx-anim-border-glow", "Border Pulse", "@keyframes border-glow\nborder-color rgba → 1.0 → rgba", "Граница пульсирует"),
                    ],
                ]
            }),

            // Edge Detection
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Edge Detection"),
                    label("filter: edge-detect() — выделение границ через градиент яркости"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-edge-soft", "Soft 0.2", "filter: edge-detect(0.2)"),
                        filter_card("gradient-sunset", "fx-edge-medium", "Medium 0.5", "filter: edge-detect(0.5)"),
                        filter_card("gradient-sunset", "fx-edge-hard", "Hard 0.8", "filter: edge-detect(0.8)"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "fx-edge-soft", "Soft 0.2", "filter: edge-detect(0.2)"),
                        filter_card("gradient-ocean", "fx-edge-medium", "Medium 0.5", "filter: edge-detect(0.5)"),
                        filter_card("gradient-ocean", "fx-edge-hard", "Hard 0.8", "filter: edge-detect(0.8)"),
                    ],
                ]
            }),

            // Silhouette
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Silhouette"),
                    label("filter: silhouette(#color) — заливка формы сплошным цветом по альфа-контуру"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", ""),
                        filter_card("gradient-sunset", "fx-silhouette-black", "Black", "filter: silhouette(#000000)"),
                        filter_card("gradient-sunset", "fx-silhouette-indigo", "Indigo", "filter: silhouette(#6366f1)"),
                        filter_card("gradient-sunset", "fx-silhouette-white", "White", "filter: silhouette(#ffffff)"),
                    ],
                ]
            }),
        ]
    }
}
