//! Smoke-тесты feature `ffmpeg`. Использует pre-generated `tests/fixtures/sample.mp4`
//! (testsrc 160×120 @ 24 fps + sine 440 Hz, 2 с).
//!
//! Запуск: `cargo test -p syngui --features ffmpeg --test video_smoke`.

#![cfg(feature = "ffmpeg")]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use syngui::video::{VideoDecoder, VideoPlayer};

fn fixture() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.mp4");
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

/// Перемотка на паузе показывает кадр новой позиции, не дожидаясь Play:
/// раньше `poll_frame` на паузе всегда отдавал `None`, и картинка под
/// ползунком застывала на старом месте.
#[test]
fn seek_while_paused_yields_one_preview_frame() {
    let path = fixture();
    let mut player = VideoPlayer::open(path.to_str().unwrap()).expect("open");
    player.set_volume(0.0);
    player.pause();
    player.seek(1.0).expect("seek");
    assert!(player.wants_seek_preview());

    let deadline = Instant::now() + Duration::from_millis(2000);
    let mut frame = None;
    while Instant::now() < deadline {
        if let Some(f) = player.poll_frame() {
            frame = Some(f);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let frame = frame.expect("кадр после перемотки на паузе");
    assert!(
        frame.pts_sec >= 0.9,
        "кадр новой позиции, pts={}",
        frame.pts_sec
    );
    assert!(player.is_paused(), "превью не снимает паузу");
    assert!(!player.wants_seek_preview(), "превью — один кадр");
    std::thread::sleep(Duration::from_millis(100));
    assert!(player.poll_frame().is_none(), "дальше на паузе кадров нет");
}

/// Пауза в конце ролика не вешает остановку декодера. На EOF поток чтения
/// ждёт команду и пакетов больше не шлёт, а декодер после Pause возвращался
/// к `item_rx.recv()` и вставал навсегда: Stop из `Drop` он не видел, и
/// `join` вешал поток, отпускавший плеер, — UI при закрытии просмотра
/// (плеер synthos ставит паузу в конце ролика). Теперь на EOF декодер ждёт
/// только перемотку или остановку; проверка — `drop` быстрее своего
/// фолбэка (`DROP_JOIN_WAIT`, 300 мс), то есть поток действительно вышел.
#[test]
fn drop_after_pause_at_eof_is_quick() {
    let path = fixture();
    let mut player = VideoPlayer::open(path.to_str().unwrap()).expect("open");
    player.set_volume(0.0);
    // Остаток в 0,2 с влезает в очередь кадров: декодер дочитывает до EOF и
    // ждёт команду, даже если кадры никто не забирает.
    player.seek(1.8).expect("seek");
    std::thread::sleep(Duration::from_millis(500));
    player.pause();
    std::thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    drop(player);
    let took = started.elapsed();
    assert!(
        took < Duration::from_millis(250),
        "drop плеера после паузы на EOF занял {took:?}"
    );
}
