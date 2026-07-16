use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::ring::VisRing;
use super::stream::AudioStream;

const VIS_RING_CAP: usize = 8192;
const LEVEL_WINDOW: usize = 4800;
const INIT_TIMEOUT: Duration = Duration::from_secs(3);
const WARMUP_DURATION_MS: u32 = 1500;

const WARMUP_CALM_PEAK: f32 = 0.12;

const WARMUP_CALM_RUN: u32 = 3;

#[derive(Debug, Clone)]
pub enum AudioError {
    NoDevice,
    Permission,
    Cpal(String),
    Wav(String),
    NoFrames,
    SeekNotSupported,
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::NoDevice => f.write_str("Микрофон не найден"),
            AudioError::Permission => f.write_str("Нет доступа к микрофону"),
            AudioError::Cpal(s) => write!(f, "Ошибка аудио-устройства: {s}"),
            AudioError::Wav(s) => write!(f, "Ошибка кодирования WAV: {s}"),
            AudioError::NoFrames => f.write_str("Не удалось захватить ни одного кадра"),
            AudioError::SeekNotSupported => f.write_str("Перемотка не поддерживается в этом режиме"),
        }
    }
}

impl std::error::Error for AudioError {}

pub(super) struct RecorderState {
    sample_rate: AtomicU32,
    channels: AtomicU16,
    samples: Mutex<Vec<f32>>,
    vis: Mutex<VisRing>,
    started: AtomicBool,
    paused: AtomicBool,
    stream_tx: Mutex<Option<mpsc::Sender<Vec<f32>>>>,
    warmup_frames_left: AtomicU32,
    warmup_calm_streak: AtomicU32,
}

impl RecorderState {
    fn new() -> Self {
        Self {
            sample_rate: AtomicU32::new(0),
            channels: AtomicU16::new(0),
            samples: Mutex::new(Vec::with_capacity(48_000 * 30)),
            vis: Mutex::new(VisRing::new(VIS_RING_CAP)),
            started: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            stream_tx: Mutex::new(None),
            warmup_frames_left: AtomicU32::new(0),
            warmup_calm_streak: AtomicU32::new(0),
        }
    }

    fn push_frames(&self, data: &[f32], channels: u16) {
        if data.is_empty() {
            return;
        }
        self.started.store(true, Ordering::Release);
        if self.paused.load(Ordering::Acquire) {
            return;
        }
        let ch = channels.max(1) as usize;
        let frame_count = data.len() / ch;
        let warmup = self.warmup_frames_left.load(Ordering::Acquire);
        if warmup > 0 {
            let mut peak = 0.0_f32;
            for &s in data {
                let a = s.abs();
                if a > peak {
                    peak = a;
                }
            }
            if peak < WARMUP_CALM_PEAK {
                let streak = self.warmup_calm_streak.load(Ordering::Acquire) + 1;
                if streak >= WARMUP_CALM_RUN {
                    self.warmup_frames_left.store(0, Ordering::Release);
                    self.warmup_calm_streak.store(0, Ordering::Release);
                } else {
                    self.warmup_calm_streak.store(streak, Ordering::Release);
                    return;
                }
            } else {
                self.warmup_calm_streak.store(0, Ordering::Release);
                let consumed = (frame_count as u32).min(warmup);
                self.warmup_frames_left
                    .store(warmup - consumed, Ordering::Release);
                return;
            }
        }
        let mut mono: Vec<f32> = Vec::with_capacity(frame_count);
        if ch == 1 {
            mono.extend_from_slice(data);
        } else {
            for i in 0..frame_count {
                let mut sum = 0.0f32;
                for c in 0..ch {
                    sum += data[i * ch + c];
                }
                mono.push(sum / ch as f32);
            }
        }
        if let Ok(mut s) = self.samples.lock() {
            s.extend_from_slice(&mono);
        }
        if let Ok(mut v) = self.vis.lock() {
            v.push(&mono);
        }
        if let Ok(g) = self.stream_tx.lock() {
            if let Some(tx) = g.as_ref() {
                let _ = tx.send(mono.clone());
            }
        }
    }

    #[cfg(test)]
    pub(super) fn push_test_frames(&self, mono: &[f32]) {
        if let Ok(mut s) = self.samples.lock() {
            s.extend_from_slice(mono);
        }
        if let Ok(g) = self.stream_tx.lock() {
            if let Some(tx) = g.as_ref() {
                let _ = tx.send(mono.to_vec());
            }
        }
        self.started.store(true, Ordering::Release);
    }
}

#[derive(Clone)]
pub struct VisHandle(Arc<RecorderState>);

impl PartialEq for VisHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for VisHandle {}

impl VisHandle {
    pub fn snapshot_bars(&self, n_bars: usize) -> Vec<f32> {
        if n_bars == 0 {
            return Vec::new();
        }
        let mut buf: Vec<f32> = Vec::new();
        if let Ok(v) = self.0.vis.lock() {
            v.snapshot(&mut buf);
        }
        if buf.is_empty() {
            return vec![0.0; n_bars];
        }
        let bucket = (buf.len() / n_bars).max(1);
        let mut out = Vec::with_capacity(n_bars);
        for i in 0..n_bars {
            let start = i * bucket;
            let end = ((i + 1) * bucket).min(buf.len());
            if start >= end {
                out.push(0.0);
                continue;
            }
            let sum_sq: f32 = buf[start..end].iter().map(|x| x * x).sum();
            let rms = (sum_sq / (end - start) as f32).sqrt();
            out.push((rms * 4.0).min(1.0));
        }
        out
    }

    pub fn level(&self) -> f32 {
        let mut buf: Vec<f32> = Vec::new();
        if let Ok(v) = self.0.vis.lock() {
            v.snapshot(&mut buf);
        }
        if buf.is_empty() {
            return 0.0;
        }
        let n = buf.len().min(LEVEL_WINDOW);
        let tail = &buf[buf.len() - n..];
        tail.iter().map(|x| x.abs()).fold(0.0f32, f32::max)
    }

    pub fn has_started(&self) -> bool {
        self.0.started.load(Ordering::Acquire)
    }
}

pub struct AudioRecorder {
    state: Arc<RecorderState>,
    stop_tx: Option<Sender<()>>,
    join: Option<JoinHandle<Result<(), AudioError>>>,
    started_at: Instant,
    actual_device_name: String,
}

impl AudioRecorder {
    pub fn start() -> Result<Self, AudioError> {
        Self::start_with_device(None)
    }

    pub fn start_with_device(preferred: Option<&str>) -> Result<Self, AudioError> {
        let state = Arc::new(RecorderState::new());
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<String, AudioError>>(1);

        let state_thread = state.clone();
        let preferred_owned = preferred.map(|s| s.to_string());
        let join = thread::Builder::new()
            .name("syngui-audio-recorder".into())
            .spawn(move || {
                run_audio_thread(state_thread, init_tx, stop_rx, preferred_owned)
            })
            .map_err(|e| AudioError::Cpal(format!("spawn thread: {e}")))?;

        match init_rx.recv_timeout(INIT_TIMEOUT) {
            Ok(Ok(name)) => Ok(Self {
                state,
                stop_tx: Some(stop_tx),
                join: Some(join),
                started_at: Instant::now(),
                actual_device_name: name,
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AudioError::Cpal("timeout инициализации стрима".into())),
        }
    }

    pub fn actual_device_name(&self) -> &str {
        &self.actual_device_name
    }

    pub fn vis_handle(&self) -> VisHandle {
        VisHandle(self.state.clone())
    }

    pub fn open_stream(&self) -> Option<Arc<AudioStream>> {
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let mut g = self.state.stream_tx.lock().ok()?;
        if g.is_some() {
            return None;
        }
        *g = Some(tx);
        let sr = self.state.sample_rate.load(Ordering::Acquire);
        let ch = self.state.channels.load(Ordering::Acquire).max(1);
        let _ = ch;
        Some(AudioStream::new(rx, sr, 1))
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn sample_rate(&self) -> u32 {
        self.state.sample_rate.load(Ordering::Acquire)
    }

    pub fn pause(&self) {
        self.state.paused.store(true, Ordering::Release);
    }

    pub fn resume(&self) {
        self.state.paused.store(false, Ordering::Release);
    }

    pub fn set_paused(&self, paused: bool) {
        self.state.paused.store(paused, Ordering::Release);
    }

    pub fn is_paused(&self) -> bool {
        self.state.paused.load(Ordering::Acquire)
    }

    pub fn stop_and_encode_wav(mut self) -> Result<Vec<u8>, AudioError> {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }

        let sample_rate = self.state.sample_rate.load(Ordering::Acquire);
        if sample_rate == 0 {
            return Err(AudioError::NoFrames);
        }
        let samples_lock = self
            .state
            .samples
            .lock()
            .map_err(|_| AudioError::Cpal("lock samples".into()))?;
        if samples_lock.is_empty() {
            return Err(AudioError::NoFrames);
        }
        super::wav::into_pcm16_bytes(&samples_lock, sample_rate)
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

#[cfg(target_os = "linux")]
const LINUX_PREFERRED_DEVICES: &[&str] = &["pipewire", "pulse", "default"];
#[cfg(not(target_os = "linux"))]
const LINUX_PREFERRED_DEVICES: &[&str] = &[];

fn run_audio_thread(
    state: Arc<RecorderState>,
    init_tx: mpsc::SyncSender<Result<String, AudioError>>,
    stop_rx: mpsc::Receiver<()>,
    preferred: Option<String>,
) -> Result<(), AudioError> {
    let host = cpal::default_host();

    let mut last_err: Option<AudioError> = None;
    let mut already_tried: Vec<String> = Vec::new();

    let result = (|| -> Option<(cpal::Stream, String)> {
        if let Some(name) = preferred.as_deref() {
            if !name.is_empty() && !name.eq_ignore_ascii_case("auto") {
                if let Some(dev) = find_input_device_by_name(&host, name) {
                    match try_build_stream(&dev, &state) {
                        Ok(s) => return Some((s, name.to_string())),
                        Err(e) => {
                            eprintln!(
                                "[syngui/audio] preferred {name:?} не подошёл: {e}"
                            );
                            last_err = Some(e);
                        }
                    }
                    already_tried.push(name.to_string());
                } else {
                    eprintln!(
                        "[syngui/audio] preferred {name:?} не найден в системе"
                    );
                }
            }
        }

        for name in LINUX_PREFERRED_DEVICES {
            if already_tried.iter().any(|n| n == name) {
                continue;
            }
            let Some(dev) = find_input_device_by_name(&host, name) else {
                continue;
            };
            match try_build_stream(&dev, &state) {
                Ok(s) => return Some((s, (*name).to_string())),
                Err(e) => {
                    eprintln!(
                        "[syngui/audio] {name:?} не подошёл: {e}"
                    );
                    last_err = Some(e);
                }
            }
            already_tried.push((*name).to_string());
        }

        if let Some(dev) = host.default_input_device() {
            let dname = dev.name().unwrap_or_else(|_| "default".into());
            if !already_tried.iter().any(|n| n == &dname) {
                match try_build_stream(&dev, &state) {
                    Ok(s) => return Some((s, dname)),
                    Err(e) => {
                        eprintln!(
                            "[syngui/audio] default device {dname:?} не подошёл: {e}"
                        );
                        last_err = Some(e);
                    }
                }
                already_tried.push(dname);
            }
        }

        let devices = match host.input_devices() {
            Ok(it) => it,
            Err(e) => {
                last_err = Some(AudioError::Cpal(format!("input_devices: {e}")));
                return None;
            }
        };
        for dev in devices {
            let name = dev.name().unwrap_or_else(|_| "<unknown>".into());
            if already_tried.iter().any(|n| n == &name) {
                continue;
            }
            match try_build_stream(&dev, &state) {
                Ok(s) => {
                    eprintln!("[syngui/audio] fallback на устройство: {name}");
                    return Some((s, name));
                }
                Err(e) => {
                    eprintln!("[syngui/audio] {name} не подошёл: {e}");
                    last_err = Some(e);
                }
            }
        }
        None
    })();

    let (stream, actual_name) = match result {
        Some(v) => v,
        None => {
            let err = last_err.unwrap_or(AudioError::NoDevice);
            let _ = init_tx.send(Err(err.clone()));
            return Err(err);
        }
    };

    let _ = init_tx.send(Ok(actual_name));

    let _ = stop_rx.recv();
    drop(stream);
    Ok(())
}

fn find_input_device_by_name(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    let devices = host.input_devices().ok()?;
    for dev in devices {
        if dev.name().ok().as_deref() == Some(name) {
            return Some(dev);
        }
    }
    None
}

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(it) => it
            .filter_map(|d| d.name().ok())
            .collect(),
        Err(e) => {
            eprintln!("[syngui/audio] list_input_devices: {e}");
            Vec::new()
        }
    }
}

fn try_build_stream(
    device: &cpal::Device,
    state: &Arc<RecorderState>,
) -> Result<cpal::Stream, AudioError> {
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::Cpal(format!("default_input_config: {e}")))?;
    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let err_fn = |e| eprintln!("[syngui/audio] cpal stream error: {e}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let st = state.clone();
            device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    st.push_frames(data, channels);
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let st = state.clone();
            device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let f: Vec<f32> = data
                        .iter()
                        .map(|&x| x as f32 / i16::MAX as f32)
                        .collect();
                    st.push_frames(&f, channels);
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let st = state.clone();
            device.build_input_stream(
                &config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let mid = u16::MAX as f32 / 2.0;
                    let f: Vec<f32> = data
                        .iter()
                        .map(|&x| (x as f32 - mid) / mid)
                        .collect();
                    st.push_frames(&f, channels);
                },
                err_fn,
                None,
            )
        }
        other => {
            return Err(AudioError::Cpal(format!(
                "неподдерживаемый sample format: {other:?}"
            )));
        }
    }
    .map_err(|e| AudioError::Cpal(format!("build_input_stream: {e}")))?;

    stream
        .play()
        .map_err(|e| AudioError::Cpal(format!("stream.play: {e}")))?;

    state.sample_rate.store(sample_rate, Ordering::Release);
    state.channels.store(channels, Ordering::Release);
    let warmup_frames =
        (sample_rate as u64 * WARMUP_DURATION_MS as u64 / 1000) as u32;
    state
        .warmup_frames_left
        .store(warmup_frames, Ordering::Release);
    state.warmup_calm_streak.store(0, Ordering::Release);
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::Duration as StdDuration;

    #[test]
    fn open_stream_pushes_to_receiver() {
        let state = Arc::new(RecorderState::new());
        state.sample_rate.store(48_000, Ordering::Release);
        state.channels.store(1, Ordering::Release);

        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        {
            let mut g = state.stream_tx.lock().expect("lock stream_tx");
            *g = Some(tx);
        }
        state.push_test_frames(&[0.1, 0.2, 0.3]);

        let chunk = rx
            .recv_timeout(StdDuration::from_millis(100))
            .expect("recv chunk");
        assert_eq!(chunk, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn stream_tx_already_set_blocks_second_open() {
        let state = Arc::new(RecorderState::new());
        let (tx1, _rx1) = mpsc::channel::<Vec<f32>>();
        {
            let mut g = state.stream_tx.lock().expect("lock");
            assert!(g.is_none(), "свежий state — стрим не открыт");
            *g = Some(tx1);
        }
        let g = state.stream_tx.lock().expect("lock");
        assert!(g.is_some(), "второй open видит уже занятый stream_tx");
    }

    #[test]
    fn paused_skips_samples_and_stream() {
        let state = Arc::new(RecorderState::new());
        state.sample_rate.store(48_000, Ordering::Release);
        state.channels.store(1, Ordering::Release);

        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        {
            let mut g = state.stream_tx.lock().expect("lock");
            *g = Some(tx);
        }

        state.push_frames(&[0.5, 0.5, 0.5, 0.5], 1);
        assert_eq!(state.samples.lock().unwrap().len(), 4);
        assert_eq!(
            rx.recv_timeout(StdDuration::from_millis(50)).expect("pre"),
            vec![0.5, 0.5, 0.5, 0.5]
        );
        assert!(state.started.load(Ordering::Acquire));

        state.paused.store(true, Ordering::Release);
        let samples_before = state.samples.lock().unwrap().len();
        state.push_frames(&[0.9, 0.9, 0.9, 0.9], 1);
        let samples_after = state.samples.lock().unwrap().len();
        assert_eq!(samples_before, samples_after, "paused — samples не растут");
        assert!(state.started.load(Ordering::Acquire));
        assert!(rx.recv_timeout(StdDuration::from_millis(50)).is_err());

        state.paused.store(false, Ordering::Release);
        state.push_frames(&[0.1, 0.2, 0.3], 1);
        assert_eq!(state.samples.lock().unwrap().len(), samples_after + 3);
        assert_eq!(
            rx.recv_timeout(StdDuration::from_millis(50)).expect("post"),
            vec![0.1, 0.2, 0.3]
        );
    }

    #[test]
    fn dropping_state_closes_stream_receiver() {
        let state = Arc::new(RecorderState::new());
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        {
            let mut g = state.stream_tx.lock().expect("lock");
            *g = Some(tx);
        }
        drop(state);
        match rx.recv() {
            Ok(_) => panic!("ожидаем RecvError после drop state'а"),
            Err(_) => {}
        }
    }

    #[test]
    fn wav_roundtrip_pcm16() {
        let sample_rate = 16_000u32;
        let total = (sample_rate as f32 * 0.1) as usize;
        let mut samples = Vec::with_capacity(total);
        for i in 0..total {
            let t = i as f32 / sample_rate as f32;
            samples.push((t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5);
        }

        let wav = super::super::wav::into_pcm16_bytes(&samples, sample_rate).expect("encode");
        let mut reader = hound::WavReader::new(Cursor::new(&wav)).expect("read");
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, sample_rate);
        assert_eq!(spec.bits_per_sample, 16);

        let decoded: Vec<i16> = reader.samples::<i16>().filter_map(Result::ok).collect();
        assert_eq!(decoded.len(), samples.len());
        let max_abs = decoded.iter().map(|x| x.unsigned_abs() as i32).max().unwrap_or(0);
        assert!(max_abs > i16::MAX as i32 / 4, "{max_abs}");
        assert!(max_abs <= i16::MAX as i32);
    }
}
