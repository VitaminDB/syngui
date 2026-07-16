//! Player window — transport controls, display, progress, volume

use syngui::prelude::*;
use syngui::mgui;
use crate::{LinAmpCtx, TrackInfo};

pub fn build_player() -> impl Widget {
    let ctx = use_context::<LinAmpCtx>();
    let tracks = use_context::<RwSignal<Vec<TrackInfo>>>();

    mgui! {
        Column::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch) => [
            // Header
            DecoratedBox::new().class("header") => [
                Row::new().gap(4.0).cross_axis_alignment(CrossAxisAlignment::Center) => [
                    Text::new("LINAMP").class("title"),
                    DecoratedBox::new().class("grow"),
                    Text::new("v1.0").class("dim"),
                ],
            ],
            // Display area
            DecoratedBox::new().class("panel") => [
                Column::new().gap(2.0) => [
                    // Track name (reactive)
                    move || {
                        let idx = ctx.current_track.get();
                        let track_list = tracks.get();
                        let name = if idx < track_list.len() {
                            format!("{}. {} - {}", idx + 1, track_list[idx].artist, track_list[idx].title)
                        } else {
                            "No track".to_string()
                        };
                        Text::new(&name).class("display")
                    },
                    // Time + volume
                    Row::new().gap(8.0) => [
                        move || {
                            let prog = ctx.progress.get();
                            let elapsed = (prog * 225.0) as u32;
                            Text::new(&format!("{}:{:02}", elapsed / 60, elapsed % 60)).class("time")
                        },
                        DecoratedBox::new().class("grow"),
                        move || {
                            let vol = ctx.volume.get();
                            Text::new(&format!("VOL: {}%", (vol * 100.0) as u32)).class("dim")
                        },
                    ],
                    // Progress slider
                    Slider::new().value(ctx.progress.get_untracked() * 100.0).range(0.0, 100.0).step(1.0)
                        .on_change(move |v| ctx.progress.set(v / 100.0))
                        .class("progress"),
                ],
            ],
            // Divider
            DecoratedBox::new().class("divider"),
            // Transport + volume
            DecoratedBox::new().class("panel") => [
                Row::new().gap(4.0).cross_axis_alignment(CrossAxisAlignment::Center) => [
                    // Prev
                    Button::new("\u{e045}").on_click(move || {
                        let idx = ctx.current_track.get_untracked();
                        let tl = tracks.get_untracked();
                        if idx > 0 { ctx.current_track.set(idx - 1); }
                        else if !tl.is_empty() { ctx.current_track.set(tl.len() - 1); }
                        ctx.progress.set(0.0);
                    }).class("transport"),
                    // Play/Pause
                    move || {
                        let playing = ctx.playing.get();
                        let icon = if playing { "\u{e034}" } else { "\u{e037}" };
                        Button::new(icon).on_click(move || {
                            ctx.playing.set(!ctx.playing.get_untracked());
                        }).class("transport")
                    },
                    // Stop
                    Button::new("\u{e047}").on_click(move || {
                        ctx.playing.set(false);
                        ctx.progress.set(0.0);
                    }).class("transport"),
                    // Next
                    Button::new("\u{e044}").on_click(move || {
                        let idx = ctx.current_track.get_untracked();
                        let tl = tracks.get_untracked();
                        if !tl.is_empty() { ctx.current_track.set((idx + 1) % tl.len()); }
                        ctx.progress.set(0.0);
                    }).class("transport"),
                    DecoratedBox::new().class("grow"),
                    // Volume icon + slider
                    Text::new("\u{e050}").class("dim"),
                    Slider::new().value(ctx.volume.get_untracked() * 100.0).range(0.0, 100.0).step(1.0)
                        .on_change(move |v| ctx.volume.set(v / 100.0))
                        .class("volume"),
                ],
            ],
        ]
    }
}
