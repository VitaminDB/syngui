//! Demo-страница FFmpeg видео-плеера. Показывает path/URL → Open → playback
//! controls + meta-блок. Все элементы стилизованы через `styles/pages/ffmpeg_video.mss`.

use std::sync::Arc;

use syngui::core::sync::Mutex;
use syngui::prelude::*;
use syngui::video::{HwAccel, VideoPlayer};
use syngui::widgets::{
    Button, Column, CrossAxisAlignment, DecoratedBox, Dropdown, Icon, MainAxisAlignment, Reactive,
    Row, TextField,
};

use super::{section_card, section_title};

/// Material Icon: movie (placeholder, когда плеер не открыт)
const MI_MOVIE: &str = "\u{E02C}";

/// Обёртка с PartialEq, чтобы держать `RwSignal<Option<PlayerSlot>>`:
/// `Mutex<VideoPlayer>` сам по себе не сравнивается, а `RwSignal::set`
/// требует `PartialEq` для skip-на-equal оптимизации. Любая смена
/// плеера должна приводить к ребилду UI, поэтому `eq` всегда `false`.
#[derive(Clone)]
struct PlayerSlot(Arc<Mutex<VideoPlayer>>);

impl PartialEq for PlayerSlot {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

/// Соответствие лейбла из Dropdown'а — конкретному enum-варианту.
/// "auto"/"none" работают на всех ОС, остальные — стрелочные cfg-gated
/// внутри HwContext::try_init (если устройство отсутствует → sw-fallback).
fn parse_hwaccel(s: &str) -> HwAccel {
    match s {
        "none" => HwAccel::None,
        "vaapi" => HwAccel::Vaapi,
        "nvdec" => HwAccel::Nvdec,
        "videotoolbox" => HwAccel::VideoToolbox,
        "d3d11va" => HwAccel::D3D11Va,
        "dxva2" => HwAccel::Dxva2,
        "vulkan" => HwAccel::Vulkan,
        _ => HwAccel::Auto,
    }
}

pub fn build_ffmpeg_video_section() -> impl Widget {
    use syngui::widgets::DropdownItem;

    let path_signal = use_signal(String::new());
    let player_signal: RwSignal<Option<PlayerSlot>> = use_signal(None);
    let error_signal = use_signal(String::new());
    let meta_text = use_signal(String::new());
    let hwaccel_signal = use_signal::<String>("auto".to_string());

    let open_btn = {
        let path_signal = path_signal;
        Button::new("Открыть")
            .on_click(move || {
                let p = path_signal.get_untracked();
                if p.trim().is_empty() {
                    error_signal.set("Укажите путь к файлу или URL".to_string());
                    return;
                }
                error_signal.set(String::new());
                let accel = parse_hwaccel(&hwaccel_signal.get_untracked());
                match VideoPlayer::open_with_hwaccel(&p, accel) {
                    Ok(player) => {
                        let m = player.meta().clone();
                        let info = format!(
                            "{}×{}, {:.1} с, {:.1} fps, {}",
                            m.width,
                            m.height,
                            m.duration_sec,
                            m.fps_estimate,
                            if m.has_audio {
                                format!("audio {} Hz × {}", m.audio_sample_rate, m.audio_channels)
                            } else {
                                "без аудио".to_string()
                            }
                        );
                        meta_text.set(info);
                        player_signal.set(Some(PlayerSlot(Arc::new(Mutex::new(player)))));
                    }
                    Err(e) => {
                        error_signal.set(format!("{e}"));
                        player_signal.set(None);
                        meta_text.set(String::new());
                    }
                }
            })
            .class("ffmpeg-open-btn")
    };

    let hwaccel_dropdown = Dropdown::new()
        .items(vec![
            DropdownItem::new("auto", "HW: Auto"),
            DropdownItem::new("none", "HW: Off (sw)"),
            DropdownItem::new("vaapi", "HW: VAAPI"),
            DropdownItem::new("nvdec", "HW: NVDEC (CUDA)"),
            DropdownItem::new("videotoolbox", "HW: VideoToolbox"),
            DropdownItem::new("d3d11va", "HW: D3D11VA"),
            DropdownItem::new("dxva2", "HW: DXVA2"),
            DropdownItem::new("vulkan", "HW: Vulkan"),
        ])
        .selected("auto")
        .width(180.0)
        .on_change(move |v| hwaccel_signal.set(v.to_string()));

    let source_row = Row::new()
        .gap(8.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .child(
            TextField::with_text(path_signal.get_untracked())
                .placeholder("/path/to/video.mp4 или https://… или rtsp://…")
                .on_change(move |v: &str| path_signal.set(v.to_string()))
                .style("flex", 1.0_f32),
        )
        .child(hwaccel_dropdown)
        .child(open_btn)
        .class("ffmpeg-source-row");

    let error_view = Reactive::new(move || {
        let err = error_signal.get();
        if err.is_empty() {
            vec![Box::new(DecoratedBox::new()) as Box<dyn Widget>]
        } else {
            vec![Box::new(Text::new(err).class("ffmpeg-error")) as Box<dyn Widget>]
        }
    });

    let player_view = Reactive::new(move || {
        let p = player_signal.get();
        match p {
            Some(slot) => {
                let view = syngui::widgets::video_player_view(slot.0.clone());
                vec![Box::new(view) as Box<dyn Widget>]
            }
            None => {
                let placeholder = Column::new()
                    .gap(8.0)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .main_axis_alignment(MainAxisAlignment::Center)
                    .child(Icon::new(MI_MOVIE).class("ffmpeg-placeholder-icon"))
                    .child(Text::new("видео не загружено").class("ffmpeg-placeholder-text"));
                vec![
                    Box::new(
                        DecoratedBox::new()
                            .child(placeholder)
                            .class("ffmpeg-placeholder"),
                    ) as Box<dyn Widget>,
                ]
            }
        }
    });

    let meta_view = Reactive::new(move || {
        let info = meta_text.get();
        if info.is_empty() {
            vec![Box::new(DecoratedBox::new()) as Box<dyn Widget>]
        } else {
            vec![Box::new(Text::new(info).class("ffmpeg-meta")) as Box<dyn Widget>]
        }
    });

    section_card(
        Column::new()
            .gap(16.0)
            .child(section_title("FFmpeg Video Player"))
            .child(Text::new("Декодирование через ffmpeg-next 8.x. Поддерживает локальные файлы и сетевые URL (http/rtsp/rtmp).").class("section-desc"))
            .child(source_row)
            .child(error_view)
            .child(player_view)
            .child(meta_view),
    )
}
