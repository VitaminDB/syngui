//! Smoke-тесты feature `ffmpeg`. Использует pre-generated `tests/fixtures/sample.mp4`
//! (testsrc 160×120 @ 24 fps + sine 440 Hz, 2 с).
//!
//! Запуск: `cargo test -p syngui --features ffmpeg --test video_smoke`.

#![cfg(feature = "ffmpeg")]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use syngui::video::{VideoDecoder, VideoPlayer};

fn fixture() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sample.mp4");
    assert!(p.exists(), "fixture mp4 не найден: {p:?}");
    p
}

#[test]
fn decoder_open_meta() {
    let path = fixture();
    let decoder = VideoDecoder::open(path.to_str().unwrap()).expect("open");
    let meta = decoder.meta().clone();
    assert_eq!(meta.width, 160);
    assert_eq!(meta.height, 120);
    assert!(meta.has_audio, "sample.mp4 содержит аудио-дорожку");
    // testsrc даёт ровно 2 секунды.
    assert!(
        (meta.duration_sec - 2.0).abs() < 0.2,
        "duration={} ожидали ~2.0",
        meta.duration_sec
    );
    assert!(
        (meta.fps_estimate - 24.0).abs() < 1.0,
        "fps={} ожидали ~24",
        meta.fps_estimate
    );
}

#[test]
fn decoder_emits_frames() {
    let path = fixture();
    let decoder = VideoDecoder::open(path.to_str().unwrap()).expect("open");

    let deadline = Instant::now() + Duration::from_millis(2000);
    let mut got_frame = None;
    while Instant::now() < deadline {
        match decoder.try_recv_video() {
            Ok(f) => {
                got_frame = Some(f);
                break;
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let frame = got_frame.expect("декодер должен выдать хотя бы один кадр за 2 с");
    assert_eq!(frame.width, 160);
    assert_eq!(frame.height, 120);
    assert_eq!(frame.rgba.len(), 160 * 120 * 4);
    // Alpha 255 во всех пикселях.
    assert!(frame.rgba.chunks_exact(4).all(|p| p[3] == 255));
    // Хоть один цветной пиксель (testsrc — цветные полосы).
    assert!(
        frame
            .rgba
            .chunks_exact(4)
            .any(|p| p[0] > 30 || p[1] > 30 || p[2] > 30),
        "ожидаем что-то ярче чёрного фона"
    );
}

#[test]
fn player_open_returns_meta() {
    let path = fixture();
    let player = VideoPlayer::open(path.to_str().unwrap()).expect("open");
    let meta = player.meta();
    assert_eq!(meta.width, 160);
    assert_eq!(meta.height, 120);
}

#[test]
fn player_polls_frames_after_play() {
    let path = fixture();
    let mut player = VideoPlayer::open(path.to_str().unwrap()).expect("open");
    // Дать декодеру время заполнить очередь и audio_player запуститься.
    std::thread::sleep(Duration::from_millis(300));
    let mut frame_count = 0;
    let deadline = Instant::now() + Duration::from_millis(800);
    while Instant::now() < deadline {
        if player.poll_frame().is_some() {
            frame_count += 1;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        frame_count > 0,
        "за ~0.8 с должны увидеть хоть один кадр (got {frame_count})"
    );
    assert!(
        player.position_sec() > 0.05,
        "position_sec={} ожидаем >0.05 после 1 с воспроизведения",
        player.position_sec()
    );
}
