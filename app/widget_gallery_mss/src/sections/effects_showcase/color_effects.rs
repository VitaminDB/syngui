use syngui::mgui;
use syngui::prelude::*;

use super::filter_card;
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Color Effects").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("Цветовые эффекты: tint (наложение цвета), градиенты, blend modes")
                .class("label"),

            // Color Tint
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Color Tint"),
                    label("color-tint: rgba(...) — наложение полупрозрачного сплошного цвета"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "", "Original", "без tint"),
                        filter_card("gradient-ocean", "fx-tint-red", "Red Tint", "color-tint:\nrgba(255,60,0,.35)"),
                        filter_card("gradient-ocean", "fx-tint-blue", "Blue Tint", "color-tint:\nrgba(0,120,255,.35)"),
                        filter_card("gradient-ocean", "fx-tint-green", "Green Tint", "color-tint:\nrgba(0,200,100,.3)"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-tint-purple", "Purple Tint", "color-tint:\nrgba(139,92,246,.35)"),
                        filter_card("gradient-sunset", "fx-tint-amber", "Amber Tint", "color-tint:\nrgba(245,158,11,.3)"),
                    ],
                ]
            }),

            // Gradient Fill
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Gradient Fill"),
                    label("background: linear-gradient / radial-gradient — заполнение градиентом"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-horizontal", "", "Horizontal", "linear-gradient(90deg,\n#3b82f6, #8b5cf6)"),
                        filter_card("gradient-vertical", "", "Vertical", "linear-gradient(180deg,\n#f43f5e, #fb923c)"),
                        filter_card("gradient-diagonal", "", "Diagonal", "linear-gradient(135deg,\n#667eea, #764ba2)"),
                        filter_card("gradient-rainbow", "", "Rainbow", "linear-gradient(90deg,\n6 color stops)"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Sunset", "linear-gradient(135deg,\n#ff6b6b, #feca57, #ff9ff3)"),
                        filter_card("gradient-ocean", "", "Ocean", "linear-gradient(180deg,\n#0077b6, #00b4d8, #90e0ef)"),
                        filter_card("gradient-radial", "", "Radial", "radial-gradient(circle,\ncenter → #3b82f6)"),
                    ],
                ]
            }),

            // Tint + gradient combinations
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Комбинации: Gradient + Tint"),
                    label("Градиент с наложенным цветовым фильтром"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-tint-blue", "Sunset + Blue", "gradient + color-tint blue"),
                        filter_card("gradient-ocean", "fx-tint-red", "Ocean + Red", "gradient + color-tint red"),
                        filter_card("gradient-diagonal", "fx-tint-amber", "Diagonal + Amber", "gradient + color-tint amber"),
                    ],
                ]
            }),

            // Gradient Map
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Gradient Map"),
                    label("filter: gradient-map(#dark, #light) — ремаппинг яркости в двухцветный градиент"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", "без gradient-map"),
                        filter_card("gradient-sunset", "fx-gradient-map-bw", "B&W", "gradient-map:\n#000, #fff"),
                        filter_card("gradient-sunset", "fx-gradient-map-warm", "Warm", "gradient-map:\n#2d1b00, #ffe0b2"),
                        filter_card("gradient-sunset", "fx-gradient-map-cool", "Cool", "gradient-map:\n#001b2d, #b2e0ff"),
                    ],
                ]
            }),

            // Duotone
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Duotone"),
                    label("filter: duotone(#shadow, #highlight) — стилизация в два тона"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-duotone-cyan-pink", "Cyan+Pink", "duotone:\n#0ff, #f0a"),
                        filter_card("gradient-sunset", "fx-duotone-purple-gold", "Purple+Gold", "duotone:\n#7c3aed, #fbbf24"),
                        filter_card("gradient-sunset", "fx-duotone-green-blue", "Green+Blue", "duotone:\n#10b981, #3b82f6"),
                    ],
                ]
            }),

            // Color Grading
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Color Grading"),
                    label("filter: color-grade(lift, gamma, gain) — кинематографическая цветокоррекция"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "fx-grade-warm", "Warm", "color-grade:\nlift warm"),
                        filter_card("gradient-ocean", "fx-grade-cool", "Cool", "color-grade:\nlift cool"),
                        filter_card("gradient-ocean", "fx-grade-cinematic", "Cinematic", "color-grade:\ncinematic LUT"),
                    ],
                ]
            }),
        ]
    }
}
