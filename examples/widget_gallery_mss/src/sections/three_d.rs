//! 3D-трансформации, отражения, «рыбий глаз» и частицы.

use syngui::animation::{Animation, Easing};
use syngui::prelude::*;
use syngui::widgets::{Animated, Fisheye, GestureDetector, Icon, ParticleEmitter, ParticlePreset, RepeatMode};

use super::{label, section_card, section_title};

fn tile(class: &str, text: &str) -> impl Widget {
    DecoratedBox::new()
        .child(
            Column::new()
                .main_axis_alignment(MainAxisAlignment::Center)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(Text::new(text.to_string()).class("td-tile-text")),
        )
        .class(format!("td-tile {class}"))
}

fn burst_button(name: &'static str, preset: ParticlePreset) -> impl Widget {
    let n = use_signal(0u32);
    Column::new().gap(4.0).cross_axis_alignment(CrossAxisAlignment::Center).child(move || {
        ParticleEmitter::new().preset(preset).burst_token(n.get()).child(
            GestureDetector::new()
                .on_click(move || n.set(n.get_untracked() + 1))
                .child(DecoratedBox::new().child(Text::new(name).class("td-burst-text")).class("td-burst")),
        )
    })
}

pub fn build_three_d_section() -> impl Widget {
    let glyphs = ["\u{E88A}", "\u{E8B8}", "\u{E0BE}", "\u{E3F4}", "\u{E405}", "\u{E86F}", "\u{E80B}", "\u{E838}"];
    let mut dock = Fisheye::new().zoom(1.9).range(2.5).overflow(true).gap(6.0).class("td-dock");
    for g in glyphs {
        dock = dock.child(DecoratedBox::new().child(Icon::new(g).class("td-dock-icon")).class("td-dock-item"));
    }

    section_card(
        Column::new()
            .gap(22.0)
            .child(section_title("3D, частицы, «рыбий глаз»"))
            .child(label("MSS: rotate-x / rotate-y / translate-z / perspective — наведите на плитки"))
            .child(
                Row::new()
                    .gap(24.0)
                    .child(tile("td-flip", "rotate-y 180°"))
                    .child(tile("td-tilt", "наклон"))
                    .child(tile("td-lift", "translate-z"))
                    .child(tile("td-coin", "@keyframes"))
                    .child(tile("td-reflect", "box-reflect")),
            )
            .child(label("Animated: .rotate_y() с повтором туда-обратно"))
            .child(
                Row::new().gap(24.0).child(
                    Animated::new(tile("td-anim", "Animated"))
                        .rotate_y(Animation::tween(Easing::EaseInOutCubic).from(-50.0).to(50.0).duration_ms(1400).build())
                        .rotate_x(Animation::tween(Easing::EaseInOutSine).from(10.0).to(-10.0).duration_ms(1400).build())
                        .perspective(420.0)
                        .repeat_mode(RepeatMode::PingPong(u32::MAX)),
                ),
            )
            .child(label("Fisheye: увеличение под указателем, как в доке macOS"))
            .child(DecoratedBox::new().child(Column::new().main_axis_alignment(MainAxisAlignment::End).cross_axis_alignment(CrossAxisAlignment::Center).child(dock)).class("td-dock-area"))
            .child(label("ParticleEmitter: всплески (клик) и потоки"))
            .child(
                Flex::new()
                    .wrap()
                    .gap(12.0)
                    .child(burst_button("Звёзды", ParticlePreset::Stars))
                    .child(burst_button("Конфетти", ParticlePreset::Confetti))
                    .child(burst_button("Фейерверк", ParticlePreset::Fireworks))
                    .child(burst_button("Искры", ParticlePreset::Sparks))
                    .child(burst_button("Сердечки", ParticlePreset::Hearts))
                    .child(burst_button("Облачко", ParticlePreset::Poof)),
            )
            .child(
                Row::new()
                    .gap(16.0)
                    .child(DecoratedBox::new().child(ParticleEmitter::new().class("td-fire")).class("td-stream"))
                    .child(DecoratedBox::new().child(ParticleEmitter::new().class("td-snow")).class("td-stream"))
                    .child(DecoratedBox::new().child(ParticleEmitter::new().class("td-smoke")).class("td-stream"))
                    .child(
                        ParticleEmitter::new()
                            .class("td-hover-fx")
                            .child(DecoratedBox::new().child(Text::new("наведите").class("td-tile-text")).class("td-stream td-hover-box")),
                    ),
            ),
    )
}
