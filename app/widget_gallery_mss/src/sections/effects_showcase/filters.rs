use syngui::mgui;
use syngui::prelude::*;

use super::filter_card;
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("CSS Filters").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("GPU-ускоренные фильтры: grayscale, sepia, invert, brightness, contrast, saturate, hue-rotate")
                .class("label"),

            // Grayscale & Sepia
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Grayscale & Sepia"),
                    label("Обесцвечивание и стилизация под старину — disabled-состояния, ретро"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", "без фильтра"),
                        filter_card("gradient-sunset", "fx-grayscale", "Grayscale", "filter: grayscale(100%)"),
                        filter_card("gradient-sunset", "fx-sepia", "Sepia 80%", "filter: sepia(80%)"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "", "Original", "без фильтра"),
                        filter_card("gradient-ocean", "fx-grayscale", "Grayscale", "filter: grayscale(100%)"),
                        filter_card("gradient-ocean", "fx-sepia", "Sepia 80%", "filter: sepia(80%)"),
                    ],
                ]
            }),

            // Invert
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Invert"),
                    label("Инверсия RGB-каналов — высокий контраст, специальные возможности"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-invert", "Invert 100%", "filter: invert(100%)"),
                        filter_card("gradient-ocean", "fx-invert", "Invert 100%", "filter: invert(100%)"),
                        filter_card("gradient-diagonal", "fx-invert", "Invert 100%", "filter: invert(100%)"),
                    ],
                ]
            }),

            // Brightness & Contrast
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Brightness & Contrast"),
                    label("Яркость и контрастность — адаптивные темы, фокус на элементе"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-brightness-dark", "Dark 0.5", "filter: brightness(0.5)"),
                        filter_card("gradient-sunset", "", "Normal 1.0", "filter: brightness(1.0)"),
                        filter_card("gradient-sunset", "fx-brightness-light", "Light 1.5", "filter: brightness(1.5)"),
                        filter_card("gradient-sunset", "fx-brightness-high", "High 2.0", "filter: brightness(2.0)"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-ocean", "fx-contrast", "Contrast 2.0", "filter: contrast(2.0)"),
                    ],
                ]
            }),

            // HSB Adjustment (Hue + Saturate)
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("HSB Adjustment (Hue Rotate + Saturate)"),
                    label("Коррекция оттенка и насыщенности — адаптивные темы, фильтрация, доступность"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "", "Original", "hue-rotate(0)"),
                        filter_card("gradient-sunset", "fx-hue-shift-90", "Hue +90°", "filter: hue-rotate(90deg)"),
                        filter_card("gradient-sunset", "fx-hue-shift-180", "Hue +180°", "filter: hue-rotate(180deg)"),
                        filter_card("gradient-sunset", "fx-hue-shift-270", "Hue +270°", "filter: hue-rotate(270deg)"),
                    ],

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-saturate-low", "Saturate 0.3", "filter: saturate(0.3)"),
                        filter_card("gradient-sunset", "", "Normal 1.0", "filter: saturate(1.0)"),
                        filter_card("gradient-sunset", "fx-saturate-high", "Saturate 2.0", "filter: saturate(2.0)"),
                    ],
                ]
            }),

            // Animated hue rotate
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Animated Hue Rotate"),
                    label("@keyframes hue-rotate — непрерывное смещение оттенка"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-anim-hue", "Hue Rotate Loop", "@keyframes hue-rotate\n0→180→360deg"),
                    ],
                ]
            }),
        ]
    }
}
