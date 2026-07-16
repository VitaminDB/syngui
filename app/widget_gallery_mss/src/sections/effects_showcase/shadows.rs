use syngui::mgui;
use syngui::prelude::*;

use super::{shadow_card, surface_card};
use crate::sections::{section_card, section_title, label};

pub fn build() -> impl Widget {
    mgui! {
        Column::new().gap(28.0) => [
            Text::new("Drop Shadow & Inner Shadow").bold().class("text-primary").style("font-size", 24.0_f32),
            Text::new("box-shadow — внешние и внутренние тени с blur-радиусом, смещением и цветом")
                .class("label"),

            // Drop shadows
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Drop Shadow (External)"),
                    label("Внешняя тень: box-shadow: offset-x offset-y blur-radius color"),

                    Row::new().gap(16.0) => [
                        shadow_card("fx-drop-shadow-sm", "Small", "box-shadow: 0 2 8\nrgba(0,0,0,.12)"),
                        shadow_card("fx-drop-shadow-md", "Medium", "box-shadow: 0 4 16\nrgba(0,0,0,.18)"),
                        shadow_card("fx-drop-shadow-lg", "Large", "box-shadow: 0 8 32\nrgba(0,0,0,.22)"),
                        shadow_card("fx-drop-shadow-colored", "Colored", "box-shadow: 0 4 20\nrgba(99,102,241,.5)"),
                    ],
                ]
            }),

            // Inner shadows
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Inner Shadow (Inset)"),
                    label("Внутренняя тень: box-shadow: inset offset-x offset-y blur-radius color"),

                    Row::new().gap(16.0) => [
                        shadow_card("fx-inner-shadow", "Inner", "box-shadow: inset\n0 2 8 rgba(0,0,0,.2)"),
                        shadow_card("fx-inner-shadow-deep", "Deep", "box-shadow: inset\n0 4 16 rgba(0,0,0,.3)"),
                        shadow_card("fx-inner-shadow-top", "Top Light", "box-shadow: inset\n0 -4 12 rgba(0,0,0,.25)"),
                    ],
                ]
            }),

            // Shadow transitions
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Shadow Transitions"),
                    label("transition: box-shadow — плавное изменение тени при hover"),

                    Row::new().gap(16.0) => [
                        surface_card("fx-trans-shadow", "Hover Shadow", "transition: box-shadow\n400ms ease", "Наведите для эффекта"),
                    ],
                ]
            }),

            // Animated shadows
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Animated Shadows (@keyframes)"),
                    label("Пульсирующие тени через @keyframes анимацию"),

                    Row::new().gap(16.0) => [
                        surface_card("fx-anim-shadow-breathe", "Breathing Shadow", "@keyframes shadow-breathe\nbox-shadow: 0 2→32", "Тень пульсирует"),
                    ],
                ]
            }),

            // Practical examples
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Практические примеры"),
                    label("Тени для карточек, кнопок, выпадающих меню"),

                    Row::new().gap(16.0) => [
                        DecoratedBox::new()
                            .class("section-card")
                            .child(
                                Column::new().gap(4.0)
                                    .child(Text::new("Card Shadow").bold().class("text-primary"))
                                    .child(Text::new("Стандартная тень карточки").class("label").style("font-size", 12.0_f32))
                            ),
                        Button::new("Button Shadow").class("fx-drop-shadow-md"),
                    ],
                ]
            }),

            // Text Shadow (gaussian blur через шейдер)
            section_card(mgui! {
                Column::new().gap(16.0) => [
                    section_title("Text Shadow (Gaussian Blur)"),
                    label("text-shadow: offset-x offset-y blur-radius color — \
                           blur_radius применяется в шейдере многотаповым gaussian'ом"),

                    Row::new().gap(28.0) => [
                        Column::new().gap(6.0) => [
                            Text::new("Sharp").class("fx-text-shadow-sharp"),
                            Text::new("blur = 0 (legacy path)").class("label"),
                        ],
                        Column::new().gap(6.0) => [
                            Text::new("Soft").class("fx-text-shadow-soft"),
                            Text::new("blur = 4").class("label"),
                        ],
                        Column::new().gap(6.0) => [
                            Text::new("Deep").class("fx-text-shadow-deep"),
                            Text::new("blur = 6").class("label"),
                        ],
                        Column::new().gap(6.0) => [
                            Text::new("Glow").class("fx-text-shadow-glow"),
                            Text::new("blur = 8, cyan").class("label"),
                        ],
                    ],
                ]
            }),
        ]
    }
}
