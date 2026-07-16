//! Playlist window — track list with selection

use syngui::prelude::*;
use syngui::widgets::*;
use syngui::mgui;
use crate::{LinAmpCtx, TrackInfo};

pub fn build_playlist() -> impl Widget {
    let ctx = use_context::<LinAmpCtx>();
    let tracks = use_context::<RwSignal<Vec<TrackInfo>>>();

    mgui! {
        Column::new().gap(0.0).cross_axis_alignment(CrossAxisAlignment::Stretch) => [
            // Header
            DecoratedBox::new().class("header") => [
                Row::new().gap(4.0).cross_axis_alignment(CrossAxisAlignment::Center) => [
                    Text::new("PLAYLIST").class("title"),
                    DecoratedBox::new().class("grow"),
                    move || {
                        let t = tracks.get();
                        Text::new(&format!("{} tracks", t.len())).class("dim")
                    },
                ],
            ],
            DecoratedBox::new().class("divider"),
            // Track list
            DecoratedBox::new().class("grow") => [
                ScrollView::new().vertical() => [
                    move || {
                        let track_list = tracks.get();
                        let current = ctx.current_track.get();
                        let mut col = Column::new().gap(0.0);
                        for (i, track) in track_list.iter().enumerate() {
                            let is_current = i == current;
                            let idx = i;
                            let cls = if is_current { "track current" } else { "track" };
                            col = col.child(
                                GestureDetector::new()
                                    .on_click(move || {
                                        ctx.current_track.set(idx);
                                        ctx.progress.set(0.0);
                                        ctx.playing.set(true);
                                    })
                                    .child(
                                        DecoratedBox::new().class(cls).child(
                                            Row::new().gap(8.0).cross_axis_alignment(CrossAxisAlignment::Center)
                                                .child(Text::new(&format!("{}.", i + 1)).class("dim"))
                                                .child(
                                                    Column::new().gap(1.0)
                                                        .child(Text::new(&track.title))
                                                        .child(Text::new(&track.artist).class("dim"))
                                                )
                                                .child(DecoratedBox::new().class("grow"))
                                                .child(Text::new(&track.duration).class("dim"))
                                        )
                                    )
                            );
                        }
                        col
                    },
                ],
            ],
        ]
    }
}
