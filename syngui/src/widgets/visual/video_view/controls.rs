use std::sync::Arc;

use crate::core::sync::Mutex;
use crate::signal::{use_signal, RwSignal};
use crate::video::VideoPlayer;
use crate::widget::{Text, Widget, WidgetExt};
use crate::widgets::{
    Column, CrossAxisAlignment, Reactive, Row, Slider, ToolButton,
};

use super::VideoView;

const MI_PLAY_ARROW: &str = "\u{E037}";
const MI_PAUSE: &str = "\u{E034}";
const MI_VOLUME_UP: &str = "\u{E050}";
const MI_VOLUME_OFF: &str = "\u{E04F}";

pub fn video_player_view(player: Arc<Mutex<VideoPlayer>>) -> impl Widget {
    let duration = player
        .lock()
        .map(|p| p.duration_sec() as f32)
        .unwrap_or(0.0);
    let starts_paused = player.lock().map(|p| p.is_paused()).unwrap_or(false);
    let starts_volume = player.lock().map(|p| p.volume()).unwrap_or(1.0);

    let pos = use_signal(0.0_f32);
    let paused = use_signal(starts_paused);
    let volume = use_signal(starts_volume);

    let video = VideoView::new(player.clone())
        .position_signal(pos)
        .class("ffmpeg-canvas");

    let play_btn = {
        let player = player.clone();
        Reactive::new(move || {
            let is_paused = paused.get();
            let icon = if is_paused { MI_PLAY_ARROW } else { MI_PAUSE };
            let player = player.clone();
            vec![Box::new(
                ToolButton::new(icon)
                    .tooltip(if is_paused { crate::i18n::builtin("video.play", "Play") } else { crate::i18n::builtin("video.pause", "Pause") })
                    .on_click(move || {
                        if let Ok(mut p) = player.lock() {
                            if p.is_paused() {
                                p.play();
                            } else {
                                p.pause();
                            }
                            paused.set(p.is_paused());
                        }
                    })
                    .class("ffmpeg-play"),
            ) as Box<dyn Widget>]
        })
    };

    let seek = {
        let player = player.clone();
        Reactive::new(move || {
            let v = pos.get();
            let player = player.clone();
            vec![Box::new(
                Slider::new()
                    .value(v)
                    .range(0.0, duration.max(0.1))
                    .step(0.1)
                    .on_change(move |t| {
                        if let Ok(mut p) = player.lock() {
                            let _ = p.seek(t as f64);
                            pos.set(t);
                        }
                    })
                    .class("ffmpeg-seek"),
            ) as Box<dyn Widget>]
        })
    };

    let time_label = Reactive::new(move || {
        let cur = pos.get();
        vec![Box::new(
            Text::new(format_time(cur, duration)).class("ffmpeg-time"),
        ) as Box<dyn Widget>]
    });

    let mute_btn = {
        let player = player.clone();
        Reactive::new(move || {
            let v = volume.get();
            let icon = if v <= 0.0 { MI_VOLUME_OFF } else { MI_VOLUME_UP };
            let player = player.clone();
            vec![Box::new(
                ToolButton::new(icon)
                    .tooltip(if v <= 0.0 { crate::i18n::builtin("video.unmute", "Unmute") } else { crate::i18n::builtin("video.mute", "Mute") })
                    .on_click(move || {
                        let new_v = if volume.get_untracked() <= 0.0 { 1.0 } else { 0.0 };
                        if let Ok(p) = player.lock() {
                            p.set_volume(new_v);
                        }
                        volume.set(new_v);
                    })
                    .class("ffmpeg-mute"),
            ) as Box<dyn Widget>]
        })
    };

    let volume_slider = {
        let player = player.clone();
        Reactive::new(move || {
            let v = volume.get();
            let player = player.clone();
            vec![Box::new(
                Slider::new()
                    .value(v)
                    .range(0.0, 1.0)
                    .step(0.01)
                    .width(80.0)
                    .on_change(move |nv| {
                        if let Ok(p) = player.lock() {
                            p.set_volume(nv);
                        }
                        volume.set(nv);
                    })
                    .class("ffmpeg-volume"),
            ) as Box<dyn Widget>]
        })
    };

    let bar = Row::new()
        .gap(8.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(play_btn)
        .child(seek)
        .child(time_label)
        .child(mute_btn)
        .child(volume_slider)
        .class("ffmpeg-controls");

    Column::new()
        .child(video)
        .child(bar)
        .class("ffmpeg-player")
}

fn format_time(cur: f32, total: f32) -> String {
    let cur = cur.max(0.0);
    let total = total.max(0.0);
    let cm = (cur / 60.0) as u32;
    let cs = (cur as u32) % 60;
    let tm = (total / 60.0) as u32;
    let ts = (total as u32) % 60;
    format!("{cm:02}:{cs:02} / {tm:02}:{ts:02}")
}

#[allow(dead_code)]
fn touch_signal_types(_: RwSignal<f32>) {}
