//! Потоковый вывод с паузами между кусками (как озвучка по фразам):
//! `stream_gap <a.wav> <b.wav> [пауза_мс]` — куски идут в плеер с паузой;
//! конец первого должен прозвучать до паузы, а не перед вторым.
use std::time::Duration;

fn read(path: &str) -> (Vec<f32>, u32) {
    let mut r = hound::WavReader::open(path).expect("wav");
    let sr = r.spec().sample_rate;
    (
        r.samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect(),
        sr,
    )
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (x, sr) = read(&a[0]);
    let (y, _) = read(&a[1]);
    let gap = a.get(2).and_then(|v| v.parse().ok()).unwrap_or(2000);
    let (tx, rx) = std::sync::mpsc::channel();
    let p = syngui::audio::AudioPlayer::start_streaming(rx, sr).expect("вывод");
    tx.send(x).unwrap();
    std::thread::sleep(Duration::from_millis(gap));
    tx.send(y).unwrap();
    drop(tx);
    while !p.is_done() {
        std::thread::sleep(Duration::from_millis(20));
    }
}
