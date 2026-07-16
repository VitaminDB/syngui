//! LinAmp — Winamp-style multi-window audio player demo

use syngui::prelude::*;
use syngui::text::icon_fonts::material;
use syngui::app::WindowConfig;

mod player;
mod playlist;
mod equalizer;

const STYLES: &str = include_str!("../styles/linamp.mss");

#[derive(Clone, Copy)]
pub struct LinAmpCtx {
    pub current_track: RwSignal<usize>,
    pub playing: RwSignal<bool>,
    pub progress: RwSignal<f32>,
    pub volume: RwSignal<f32>,
    pub eq_enabled: RwSignal<bool>,
    pub eq_bands: RwSignal<[f32; 10]>,
}

#[derive(Clone, Debug)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub duration: String,
}

impl LinAmpCtx {
    fn new() -> Self {
        Self {
            current_track: use_signal(0usize),
            playing: use_signal(false),
            progress: use_signal(0.35f32),
            volume: use_signal(0.75f32),
            eq_enabled: use_signal(true),
            eq_bands: use_signal([0.0f32; 10]),
        }
    }
}

pub fn demo_tracks() -> Vec<TrackInfo> {
    vec![
        TrackInfo { title: "Sandstorm".into(), artist: "Darude".into(), duration: "3:45".into() },
        TrackInfo { title: "Blue (Da Ba Dee)".into(), artist: "Eiffel 65".into(), duration: "3:28".into() },
        TrackInfo { title: "Better Off Alone".into(), artist: "Alice Deejay".into(), duration: "3:32".into() },
        TrackInfo { title: "Kernkraft 400".into(), artist: "Zombie Nation".into(), duration: "3:33".into() },
        TrackInfo { title: "Children".into(), artist: "Robert Miles".into(), duration: "4:54".into() },
        TrackInfo { title: "Insomnia".into(), artist: "Faithless".into(), duration: "6:28".into() },
        TrackInfo { title: "Firestarter".into(), artist: "The Prodigy".into(), duration: "4:42".into() },
        TrackInfo { title: "Around the World".into(), artist: "Daft Punk".into(), duration: "7:09".into() },
    ]
}

pub fn run_desktop() {
    let ctx = LinAmpCtx::new();
    provide_context(ctx);

    let tracks = demo_tracks();
    let tracks_signal = use_signal(tracks);
    provide_context(tracks_signal);

    App::new()
        .title("LinAmp")
        .size(280, 150)
        .with_styles_str(STYLES)
        .background(Color::from_hex("#232323"))
        .with_icon_font(material::FONT_DATA)
        .sticky_windows(10.0)
        .add_window(
            WindowConfig::new("playlist")
                .title("LinAmp - Playlist")
                .size(280, 250)
                .offset_from_main(0, 155),
            |_| Box::new(playlist::build_playlist()),
        )
        .add_window(
            WindowConfig::new("equalizer")
                .title("LinAmp - Equalizer")
                .size(280, 140)
                .offset_from_main(0, 410),
            |_| Box::new(equalizer::build_equalizer()),
        )
        .run(|_| Box::new(player::build_player()))
}
