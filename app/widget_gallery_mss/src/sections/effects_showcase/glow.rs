use syngui::mgui;
use syngui::prelude::*;

use super::{shadow_card, surface_card};
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Glow & Bloom").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("glow — аддитивное свечение (additive blend): свет накапливается, создавая эффект неона")
                .class("label"),

            // Basic glow colors
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Basic Glow"),
                    label("glow: offset-x offset-y blur-radius color — аддитивный blend"),

                    Row::new().gap(16.0) => [
                        shadow_card("fx-glow-blue", "Blue Glow", "glow: 0 0 24\nrgba(99,102,241,.8)"),
                        shadow_card("fx-glow-cyan", "Cyan Glow", "glow: 0 0 20\nrgba(34,211,238,.7)"),
                        shadow_card("fx-glow-pink", "Pink Glow", "glow: 0 0 28\nrgba(236,72,153,.75)"),
                        shadow_card("fx-glow-green", "Green Glow", "glow: 0 0 22\nrgba(34,197,94,.7)"),
                    ],
                ]
            }),

            // Multi-layer glow
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Multi-layer Glow"),
                    label("Несколько слоёв glow через запятую для объёмного свечения"),

                    Row::new().gap(16.0) => [
                        shadow_card("fx-glow-multi", "Dual Glow", "glow: ... indigo,\n... pink"),
                        shadow_card("fx-glow-neon", "Neon Triple", "glow: 3 layers\n+ border glow"),
                    ],
                ]
            }),

            // Glow transitions
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Glow Transitions"),
                    label("transition: glow — плавное появление свечения при hover"),

                    Row::new().gap(16.0) => [
                        surface_card("fx-trans-glow", "Hover → Glow", "transition: glow\n400ms ease", "Наведите мышь"),
                    ],
                ]
            }),

            // Animated glow
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Animated Glow (@keyframes)"),
                    label("Пульсирующее свечение через @keyframes — идеально для индикаторов активности"),

                    Row::new().gap(16.0) => [
                        surface_card("fx-anim-glow-pulse", "Pulsing Glow", "@keyframes glow-pulse\nglow: 12→32→12", "Свечение пульсирует"),
                    ],
                ]
            }),

            // Glow vs Shadow comparison
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Glow vs Shadow — сравнение"),
                    label("Glow использует additive blend (свет + свет = ярче), Shadow — обычный alpha blend"),

                    Row::new().gap(24.0) => [
                        Column::new().gap(8.0) => [
                            shadow_card("fx-glow", "box-shadow (alpha)", "box-shadow: 0 0 20\nrgba(99,102,241,.65)"),
                        ],
                        Column::new().gap(8.0) => [
                            shadow_card("fx-glow-blue", "glow (additive)", "glow: 0 0 24\nrgba(99,102,241,.8)"),
                        ],
                    ],
                ]
            }),
        ]
    }
}
