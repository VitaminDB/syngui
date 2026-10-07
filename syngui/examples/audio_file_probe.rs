//! Проверка `AudioFilePlayer`: `cargo run --example audio_file_probe --features ffmpeg -- ФАЙЛ`
fn main() {
    let path = std::env::args().nth(1).expect("файл");
    let meta = syngui::video::probe_audio(&path).expect("probe");
    println!("meta: title={:?} artist={:?} album={:?} dur={:.2} cover={:?}", meta.title, meta.artist, meta.album, meta.duration_sec, meta.cover.as_ref().map(|c| c.len()));
    let mut p = syngui::video::AudioFilePlayer::open(&path).expect("open");
    p.set_volume(0.05);
    std::thread::sleep(std::time::Duration::from_millis(1200));
    println!("pos after 1.2s: {:.2}", p.position_sec());
    p.seek(meta.duration_sec - 1.0).expect("seek");
    std::thread::sleep(std::time::Duration::from_millis(300));
    println!("pos after seek to end-1: {:.2}", p.position_sec());
    for _ in 0..40 {
        if p.is_ended() { println!("ended at {:.2}", p.position_sec()); return; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    println!("NOT ended, pos {:.2}, err {:?}", p.position_sec(), p.error());
}
