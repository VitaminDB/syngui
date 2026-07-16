//! Equalizer window — 10-band EQ with presets

use syngui::prelude::*;
use syngui::mgui;
use crate::LinAmpCtx;

const PRESETS: [(&str, [f32; 10]); 4] = [
    ("Flat", [0.0; 10]),
    ("Rock", [4.0, 3.0, -1.0, -3.0, -1.0, 2.0, 4.0, 5.0, 5.0, 4.0]),
    ("Pop", [-1.0, 2.0, 4.0, 4.0, 2.0, -1.0, -2.0, -1.0, 1.0, 2.0]),
    ("Jazz", [3.0, 2.0, 0.0, 2.0, -2.0, -2.0, 0.0, 2.0, 3.0, 4.0]),
];

pub fn build_equalizer() -> impl Widget {
    let ctx = use_context::<LinAmpCtx>();

    mgui! {
        Column::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch) => [
            // Header + enable toggle
            DecoratedBox::new().class("header") => [
                Row::new().gap(4.0).cross_axis_alignment(CrossAxisAlignment::Center) => [
                    Text::new("EQUALIZER").class("title"),
                    DecoratedBox::new().class("grow"),
                    move || {
                        let enabled = ctx.eq_enabled.get();
                        Button::new(if enabled { "ON" } else { "OFF" })
                            .on_click(move || ctx.eq_enabled.set(!ctx.eq_enabled.get_untracked()))
                            .class(if enabled { "preset active" } else { "preset" })
                    },
                ],
            ],
            DecoratedBox::new().class("divider"),
            // Preset buttons
            DecoratedBox::new().class("panel") => [
                Row::new().gap(4.0) => [
                    Text::new("Preset:").class("dim"),
                    Button::new("Flat").on_click(move || ctx.eq_bands.set(PRESETS[0].1)).class("preset"),
                    Button::new("Rock").on_click(move || ctx.eq_bands.set(PRESETS[1].1)).class("preset"),
                    Button::new("Pop").on_click(move || ctx.eq_bands.set(PRESETS[2].1)).class("preset"),
                    Button::new("Jazz").on_click(move || ctx.eq_bands.set(PRESETS[3].1)).class("preset"),
                ],
            ],
            // Band labels
            DecoratedBox::new().class("panel") => [
                Row::new().gap(2.0).main_axis_alignment(MainAxisAlignment::SpaceEvenly) => [
                    Text::new("60").class("dim").style("font-size", 8.0),
                    Text::new("170").class("dim").style("font-size", 8.0),
                    Text::new("310").class("dim").style("font-size", 8.0),
                    Text::new("600").class("dim").style("font-size", 8.0),
                    Text::new("1K").class("dim").style("font-size", 8.0),
                    Text::new("3K").class("dim").style("font-size", 8.0),
                    Text::new("6K").class("dim").style("font-size", 8.0),
                    Text::new("12K").class("dim").style("font-size", 8.0),
                    Text::new("14K").class("dim").style("font-size", 8.0),
                    Text::new("16K").class("dim").style("font-size", 8.0),
                ],
            ],
            // Band values display
            DecoratedBox::new().class("panel") => [
                move || {
                    let bands = ctx.eq_bands.get();
                    let mut row = Row::new().gap(2.0).main_axis_alignment(MainAxisAlignment::SpaceEvenly);
                    for &val in &bands {
                        let db = format!("{:+.0}", val);
                        row = row.child(Text::new(&db).class("dim").style("font-size", 9.0));
                    }
                    row
                },
            ],
        ]
    }
}
