use syngui::mgui;
use syngui::prelude::*;

use super::{filter_card, surface_card};
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Keyframe Animations").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("@keyframes — непрерывные анимации фильтров, свечения, трансформаций")
                .class("label"),

            // Filter keyframes
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Filter Animations"),
                    label("@keyframes с filter свойствами — пульсация, дыхание, сдвиг оттенка"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-anim-pulse", "Pulse", "@keyframes pulse\nopacity: 1→0.3→1\n2s infinite"),
                        filter_card("gradient-ocean", "fx-anim-breathe", "Breathe", "@keyframes breathe\nblur: 0→4→0\n3s infinite"),
                        filter_card("gradient-sunset", "fx-anim-hue", "Hue Rotate", "@keyframes hue-rotate\nhue: 0→180→360\n4s infinite linear"),
                    ],
                ]
            }),

            // Shadow & glow keyframes
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Shadow & Glow Animations"),
                    label("Пульсирующие тени и свечение"),

                    Row::new().gap(16.0) => [
                        surface_card("fx-anim-shadow-breathe", "Shadow Breathe", "@keyframes shadow-breathe\nbox-shadow: 8→32→8\n3s infinite", "Тень дышит"),
                        surface_card("fx-anim-glow-pulse", "Glow Pulse", "@keyframes glow-pulse\nglow: 12→32→12\n2s infinite", "Свечение пульсирует"),
                        surface_card("fx-anim-border-glow", "Border Glow", "@keyframes border-glow\nborder-color: dim→bright\n2s infinite", "Граница мигает"),
                    ],
                ]
            }),

            // Color & bg keyframes
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Color Shift"),
                    label("Плавная смена цвета фона через @keyframes"),

                    Row::new().gap(16.0) => [
                        filter_card("gradient-sunset", "fx-anim-color-shift", "Color Shift", "@keyframes color-shift\nblue→purple→pink→blue\n4s infinite"),
                    ],
                ]
            }),

            // Transform keyframes
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Transform Animations"),
                    label("@keyframes с translate, rotate, scale — движение, вращение, масштабирование"),

                    Row::new().gap(16.0) => [
                        surface_card("fx-anim-float", "Float", "@keyframes float\ntranslate-y: 0→-10→0\n3s infinite", "Плавает вверх-вниз"),
                        surface_card("fx-anim-shake", "Shake", "@keyframes shake\ntranslate-x: 0→-5→5→-3→0\n0.5s infinite", "Трясётся"),
                        surface_card("fx-anim-spin-scale", "Spin + Scale", "@keyframes spin-scale\nrotate + scale\n3s infinite", "Вращается и масштабируется"),
                    ],
                ]
            }),

            // Syntax reference
            section_card(mgui! {
                Column::new().gap(12.0) => [
                    section_title("Syntax Reference"),
                    label("Все свойства для @keyframes анимаций в MSS"),

                    Text::new("animation-name: <name>              — имя @keyframes").class("fx-code"),
                    Text::new("animation-duration: 2s               — длительность").class("fx-code"),
                    Text::new("animation-iteration-count: infinite   — кол-во повторов").class("fx-code"),
                    Text::new("animation-timing-function: ease-in-out — easing").class("fx-code"),
                    Text::new("animation-direction: alternate         — направление").class("fx-code"),
                    Text::new("animation-delay: 500ms                 — задержка").class("fx-code"),
                ]
            }),
        ]
    }
}
