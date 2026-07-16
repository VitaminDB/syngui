use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::recorder::AudioError;

const INIT_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) struct PlayerState {
    cursor: AtomicUsize,
    total: AtomicUsize,
    done: AtomicBool,
    ready: AtomicBool,
    paused: AtomicBool,
    error_msg: Mutex<Option<String>>,
    sample_rate: AtomicU32,
    audio_channels: AtomicU16,
    volume_bits: AtomicU32,
    streaming: AtomicBool,
}

impl PlayerState {
    fn new_pending() -> Self {
        Self {
            cursor: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
            done: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            error_msg: Mutex::new(None),
            sample_rate: AtomicU32::new(0),
            audio_channels: AtomicU16::new(1),
            volume_bits: AtomicU32::new(1.0_f32.to_bits()),
            streaming: AtomicBool::new(false),
        }
    }

    fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    fn total(&self) -> usize {
        self.total.load(Ordering::Acquire)
    }

    fn audio_channels(&self) -> u16 {
        self.audio_channels.load(Ordering::Relaxed)
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    fn set_error(&self, e: &AudioError) {
        let msg = format!("{e}");
        if let Ok(mut g) = self.error_msg.lock() {
            *g = Some(msg);
        }
        self.done.store(true, Ordering::Release);
    }
}

pub struct AudioPlayer {
    state: Arc<PlayerState>,
    stop_tx: Option<Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl AudioPlayer {
    pub fn start(pcm: Arc<[f32]>, sample_rate: u32) -> Result<Self, AudioError> {
        if pcm.is_empty() {
            return Err(AudioError::NoFrames);
        }
        let state = Arc::new(PlayerState::new_pending());
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let state_thread = state.clone();
        let join = thread::Builder::new()
            .name("syngui-audio-prep-mono".into())
            .spawn(move || run_mono_prep_and_play(state_thread, pcm, sample_rate, stop_rx))
            .map_err(|e| AudioError::Cpal(format!("spawn prep thread: {e}")))?;
        Ok(Self {
            state,
            stop_tx: Some(stop_tx),
            join: Some(join),
        })
    }

    pub fn start_stereo(interleaved: Arc<[f32]>, sample_rate: u32) -> Result<Self, AudioError> {
        if interleaved.is_empty() {
            return Err(AudioError::NoFrames);
        }
        if interleaved.len() % 2 != 0 {
            return Err(AudioError::Cpal(format!(
                "stereo PCM длина {} не делится на 2",
                interleaved.len()
            )));
        }
        let state = Arc::new(PlayerState::new_pending());
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let state_thread = state.clone();
        let join = thread::Builder::new()
            .name("syngui-audio-prep-stereo".into())
            .spawn(move || run_stereo_prep_and_play(state_thread, interleaved, sample_rate, stop_rx))
            .map_err(|e| AudioError::Cpal(format!("spawn prep thread: {e}")))?;
        Ok(Self {
            state,
            stop_tx: Some(stop_tx),
            join: Some(join),
        })
    }

    pub fn position(&self) -> f32 {
        if !self.state.ready.load(Ordering::Acquire) {
            return 0.0;
        }
        let total = self.state.total();
        if total == 0 {
            return 1.0;
        }
        let cur = self.state.cursor.load(Ordering::Relaxed);
        (cur.min(total) as f32) / (total as f32)
    }

    pub fn is_done(&self) -> bool {
        self.state.done.load(Ordering::Acquire)
    }

    pub fn is_ready(&self) -> bool {
        self.state.ready.load(Ordering::Acquire)
    }

    pub fn error(&self) -> Option<String> {
        self.state.error_msg.lock().ok().and_then(|g| g.clone())
    }

    pub fn sample_rate(&self) -> u32 {
        self.state.sample_rate.load(Ordering::Acquire)
    }

    pub fn samples_played(&self) -> u64 {
        self.state.cursor.load(Ordering::Relaxed) as u64
    }

    pub fn stop(mut self) {
        self.stop_inner();
    }

    pub fn pause(&self) {
        self.state.paused.store(true, Ordering::Release);
    }

    pub fn resume(&self) {
        self.state.paused.store(false, Ordering::Release);
    }

    pub fn is_paused(&self) -> bool {
        self.state.is_paused()
    }

    pub fn seek_seconds(&self, t: f64) -> Result<(), AudioError> {
        if self.state.streaming.load(Ordering::Acquire) {
            return Err(AudioError::SeekNotSupported);
        }
        if !self.state.ready.load(Ordering::Acquire) {
            return Err(AudioError::SeekNotSupported);
        }
        let sr = self.state.sample_rate.load(Ordering::Acquire) as f64;
        let ch = self.state.audio_channels() as usize;
        let total = self.state.total();
        if sr <= 0.0 || ch == 0 || total == 0 {
            return Err(AudioError::SeekNotSupported);
        }
        let target_frames = (t.max(0.0) * sr).round() as usize;
        let target = (target_frames * ch).min(total);
        self.state.cursor.store(target, Ordering::Release);
        if target < total {
            self.state.done.store(false, Ordering::Release);
        }
        Ok(())
    }

    pub fn duration_seconds(&self) -> f64 {
        let sr = self.state.sample_rate.load(Ordering::Acquire) as f64;
        let ch = self.state.audio_channels() as usize;
        let total = self.state.total();
        if sr <= 0.0 || ch == 0 || total == 0 {
            return 0.0;
        }
        let frames = total / ch;
        frames as f64 / sr
    }

    pub fn position_seconds(&self) -> f64 {
        let sr = self.state.sample_rate.load(Ordering::Acquire) as f64;
        let ch = self.state.audio_channels() as usize;
        if sr <= 0.0 || ch == 0 {
            return 0.0;
        }
        let cur = self.state.cursor.load(Ordering::Relaxed);
        let frames = cur / ch;
        frames as f64 / sr
    }

    pub fn set_volume(&self, volume: f32) {
        let v = volume.max(0.0).min(8.0);
        self.state
            .volume_bits
            .store(v.to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        self.state.volume()
    }

    pub fn start_streaming(
        rx: Receiver<Vec<f32>>,
        sample_rate: u32,
    ) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let (device, supported) = pick_output_device(&host)?;
        let native_sr = supported.sample_rate().0;
        let channels = supported.channels();
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let needs_resample = native_sr != sample_rate;

        let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let total_written = Arc::new(AtomicUsize::new(0));
        let total_played = Arc::new(AtomicUsize::new(0));

        let q_drainer = queue.clone();
        let written_drainer = total_written.clone();
        let _drainer = thread::Builder::new()
            .name("syngui-audio-player-drainer".into())
            .spawn(move || {
                if !needs_resample {
                    while let Ok(chunk) = rx.recv() {
                        written_drainer.fetch_add(chunk.len(), Ordering::AcqRel);
                        if let Ok(mut q) = q_drainer.lock() {
                            q.extend(chunk.into_iter());
                        }
                    }
                    return;
                }
                use rubato::{
                    Resampler as _, SincFixedIn, SincInterpolationParameters,
                    SincInterpolationType, WindowFunction,
                };
                let params = SincInterpolationParameters {
                    sinc_len: 256,
                    f_cutoff: 0.95,
                    oversampling_factor: 256,
                    interpolation: SincInterpolationType::Linear,
                    window: WindowFunction::BlackmanHarris2,
                };
                let in_chunk: usize = 1024;
                let ratio = native_sr as f64 / sample_rate as f64;
                let mut resampler = match SincFixedIn::<f32>::new(ratio, 2.0, params, in_chunk, 1) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!(
                            "[AudioPlayer streaming] rubato init failed ({sample_rate}→{native_sr}): {e}"
                        );
                        return;
                    }
                };
                let mut accum: Vec<f32> = Vec::with_capacity(in_chunk * 4);
                let push_resampled = |q: &Arc<Mutex<VecDeque<f32>>>,
                                      written: &Arc<AtomicUsize>,
                                      out: &[f32]| {
                    written.fetch_add(out.len(), Ordering::AcqRel);
                    if let Ok(mut g) = q.lock() {
                        g.extend(out.iter().copied());
                    }
                };
                while let Ok(chunk) = rx.recv() {
                    accum.extend_from_slice(&chunk);
                    while accum.len() >= in_chunk {
                        let block: Vec<f32> = accum.drain(..in_chunk).collect();
                        match resampler.process(&[block], None) {
                            Ok(out) => push_resampled(&q_drainer, &written_drainer, &out[0]),
                            Err(e) => {
                                eprintln!("[AudioPlayer streaming] rubato process: {e}");
                                return;
                            }
                        }
                    }
                }
                if !accum.is_empty() {
                    let remaining = accum.len();
                    accum.resize(in_chunk, 0.0);
                    if let Ok(out) = resampler.process(&[accum.clone()], None) {
                        let out_chunk = out[0].len();
                        let take = ((out_chunk as f64) * (remaining as f64) / (in_chunk as f64))
                            .round() as usize;
                        push_resampled(
                            &q_drainer,
                            &written_drainer,
                            &out[0][..take.min(out_chunk)],
                        );
                    }
                }
            })
            .map_err(|e| AudioError::Cpal(format!("spawn drainer: {e}")))?;

        let state = Arc::new(PlayerState::new_pending());
        state.audio_channels.store(1, Ordering::Release);
        state.sample_rate.store(native_sr, Ordering::Release);
        state.ready.store(true, Ordering::Release);
        state.streaming.store(true, Ordering::Release);

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), AudioError>>(1);

        let state_thread = state.clone();
        let queue_cb = queue.clone();
        let written_cb = total_written.clone();
        let played_cb = total_played.clone();
        let join = thread::Builder::new()
            .name("syngui-audio-player-streaming".into())
            .spawn(move || {
                let _ = run_player_thread_streaming(
                    state_thread,
                    queue_cb,
                    written_cb,
                    played_cb,
                    init_tx,
                    stop_rx,
                    &device,
                    &config,
                    sample_format,
                    channels,
                );
            })
            .map_err(|e| AudioError::Cpal(format!("spawn thread: {e}")))?;

        match init_rx.recv_timeout(INIT_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                state,
                stop_tx: Some(stop_tx),
                join: Some(join),
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AudioError::Cpal(
                "timeout инициализации streaming output stream".into(),
            )),
        }
    }

    fn stop_inner(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
        self.state.done.store(true, Ordering::Release);
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

fn pick_output_device(
    host: &cpal::Host,
) -> Result<(cpal::Device, cpal::SupportedStreamConfig), AudioError> {
    let mut errors: Vec<String> = Vec::new();

    if let Some(device) = host.default_output_device() {
        match device.default_output_config() {
            Ok(cfg) => return Ok((device, cfg)),
            Err(e) => {
                let name = device.name().unwrap_or_else(|_| "default".into());
                errors.push(format!("default[{name}]: {e}"));
            }
        }
    }

    let outputs = host
        .output_devices()
        .map_err(|e| AudioError::Cpal(format!("output_devices: {e}")))?;
    for device in outputs {
        let name = device.name().unwrap_or_else(|_| "<unnamed>".into());
        match device.default_output_config() {
            Ok(cfg) => {
                eprintln!("[syngui/audio/player] fallback на устройство '{name}'");
                return Ok((device, cfg));
            }
            Err(e1) => {
                errors.push(format!("{name} default_config: {e1}"));
                if let Ok(mut configs) = device.supported_output_configs() {
                    if let Some(range) = configs.next() {
                        let cfg = range.with_max_sample_rate();
                        eprintln!(
                            "[syngui/audio/player] fallback на '{name}' через supported_output_configs"
                        );
                        return Ok((device, cfg));
                    }
                }
            }
        }
    }

    Err(AudioError::Cpal(format!(
        "не удалось подобрать output-устройство ({} попыток): {}",
        errors.len(),
        errors.join("; ")
    )))
}

fn run_player_thread_streaming(
    state: Arc<PlayerState>,
    queue: Arc<Mutex<VecDeque<f32>>>,
    total_written: Arc<AtomicUsize>,
    total_played: Arc<AtomicUsize>,
    init_tx: mpsc::SyncSender<Result<(), AudioError>>,
    stop_rx: mpsc::Receiver<()>,
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    channels: u16,
) -> Result<(), AudioError> {
    let err_fn = |e| eprintln!("[syngui/audio/player streaming] stream error: {e}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let q = queue.clone();
            let played = total_played.clone();
            let st = state.clone();
            device.build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    write_streaming_f32(&q, &played, data, channels, st.volume(), st.is_paused());
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let q = queue.clone();
            let played = total_played.clone();
            let st = state.clone();
            device.build_output_stream(
                config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    write_streaming_i16(&q, &played, data, channels, st.volume(), st.is_paused());
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let q = queue.clone();
            let played = total_played.clone();
            let st = state.clone();
            device.build_output_stream(
                config,
                move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                    write_streaming_u16(&q, &played, data, channels, st.volume(), st.is_paused());
                },
                err_fn,
                None,
            )
        }
        other => {
            let err = AudioError::Cpal(format!(
                "streaming: неподдерживаемый sample format: {other:?}"
            ));
            let _ = init_tx.send(Err(err.clone()));
            return Err(err);
        }
    };
    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            let err = AudioError::Cpal(format!("streaming build_output_stream: {e}"));
            let _ = init_tx.send(Err(err.clone()));
            return Err(err);
        }
    };
    if let Err(e) = stream.play() {
        let err = AudioError::Cpal(format!("streaming stream.play: {e}"));
        let _ = init_tx.send(Err(err.clone()));
        return Err(err);
    }
    let _ = init_tx.send(Ok(()));

    loop {
        if state.done.load(Ordering::Acquire) {
            break;
        }
        let written = total_written.load(Ordering::Acquire);
        let played = total_played.load(Ordering::Acquire);
        let q_len = queue.lock().map(|q| q.len()).unwrap_or(0);
        if q_len == 0 && played >= written && written > 0 {
            thread::sleep(Duration::from_millis(40));
            let written2 = total_written.load(Ordering::Acquire);
            if written2 == written {
                state.done.store(true, Ordering::Release);
                break;
            }
        }
        state.cursor.store(played, Ordering::Release);
        match stop_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(()) => {
                state.done.store(true, Ordering::Release);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(stream);
    Ok(())
}

fn write_streaming_f32(
    queue: &Mutex<VecDeque<f32>>,
    played: &AtomicUsize,
    data: &mut [f32],
    channels: u16,
    volume: f32,
    paused: bool,
) {
    let ch = channels.max(1) as usize;
    if paused {
        for v in data.iter_mut() { *v = 0.0; }
        return;
    }
    let frames = data.len() / ch;
    if let Ok(mut q) = queue.lock() {
        let take = frames.min(q.len());
        for i in 0..frames {
            let s = if i < take { q.pop_front().unwrap_or(0.0) } else { 0.0 } * volume;
            for c in 0..ch {
                data[i * ch + c] = s;
            }
        }
        played.fetch_add(take, Ordering::AcqRel);
    } else {
        for v in data.iter_mut() { *v = 0.0; }
    }
}

fn write_streaming_i16(
    queue: &Mutex<VecDeque<f32>>,
    played: &AtomicUsize,
    data: &mut [i16],
    channels: u16,
    volume: f32,
    paused: bool,
) {
    let ch = channels.max(1) as usize;
    if paused {
        for v in data.iter_mut() { *v = 0; }
        return;
    }
    let frames = data.len() / ch;
    if let Ok(mut q) = queue.lock() {
        let take = frames.min(q.len());
        for i in 0..frames {
            let s = if i < take { q.pop_front().unwrap_or(0.0) } else { 0.0 } * volume;
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            for c in 0..ch {
                data[i * ch + c] = v;
            }
        }
        played.fetch_add(take, Ordering::AcqRel);
    } else {
        for v in data.iter_mut() { *v = 0; }
    }
}

fn write_streaming_u16(
    queue: &Mutex<VecDeque<f32>>,
    played: &AtomicUsize,
    data: &mut [u16],
    channels: u16,
    volume: f32,
    paused: bool,
) {
    let ch = channels.max(1) as usize;
    let mid_u16 = u16::MAX / 2;
    if paused {
        for v in data.iter_mut() { *v = mid_u16; }
        return;
    }
    let frames = data.len() / ch;
    let mid = mid_u16 as f32;
    if let Ok(mut q) = queue.lock() {
        let take = frames.min(q.len());
        for i in 0..frames {
            let s = if i < take { q.pop_front().unwrap_or(0.0) } else { 0.0 } * volume;
            let v = ((s.clamp(-1.0, 1.0) * mid) + mid) as u16;
            for c in 0..ch {
                data[i * ch + c] = v;
            }
        }
        played.fetch_add(take, Ordering::AcqRel);
    } else {
        for v in data.iter_mut() { *v = mid_u16; }
    }
}

fn run_mono_prep_and_play(
    state: Arc<PlayerState>,
    pcm: Arc<[f32]>,
    sample_rate: u32,
    stop_rx: mpsc::Receiver<()>,
) {
    let host = cpal::default_host();
    let (device, supported) = match pick_output_device(&host) {
        Ok(v) => v,
        Err(e) => {
            state.set_error(&e);
            return;
        }
    };
    let native_sr = supported.sample_rate().0;
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let pcm_native: Arc<[f32]> = if sample_rate == native_sr {
        pcm
    } else {
        match resample_mono_f32(&pcm, sample_rate, native_sr) {
            Ok(v) => Arc::from(v.into_boxed_slice()),
            Err(e) => {
                state.set_error(&e);
                return;
            }
        }
    };

    state.audio_channels.store(1, Ordering::Release);
    state.sample_rate.store(native_sr, Ordering::Release);
    state.total.store(pcm_native.len(), Ordering::Release);

    if let Err(e) = build_and_run_stream(
        &state, pcm_native, &device, &config, sample_format, channels, stop_rx,
    ) {
        state.set_error(&e);
    }
}

fn run_stereo_prep_and_play(
    state: Arc<PlayerState>,
    interleaved: Arc<[f32]>,
    sample_rate: u32,
    stop_rx: mpsc::Receiver<()>,
) {
    let host = cpal::default_host();
    let (device, supported) = match pick_output_device(&host) {
        Ok(v) => v,
        Err(e) => {
            state.set_error(&e);
            return;
        }
    };
    let native_sr = supported.sample_rate().0;
    let device_channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let target_audio_channels: u16 = if device_channels >= 2 { 2 } else { 1 };

    let frames = interleaved.len() / 2;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for i in 0..frames {
        left.push(interleaved[i * 2]);
        right.push(interleaved[i * 2 + 1]);
    }
    drop(interleaved);

    let (left_native, right_native) = if sample_rate == native_sr {
        (left, right)
    } else {
        let l = match resample_mono_f32(&left, sample_rate, native_sr) {
            Ok(v) => v,
            Err(e) => {
                state.set_error(&e);
                return;
            }
        };
        let r = match resample_mono_f32(&right, sample_rate, native_sr) {
            Ok(v) => v,
            Err(e) => {
                state.set_error(&e);
                return;
            }
        };
        (l, r)
    };

    let pcm_native: Arc<[f32]> = if target_audio_channels == 2 {
        let mut out = Vec::with_capacity(left_native.len() * 2);
        for i in 0..left_native.len() {
            out.push(left_native[i]);
            out.push(right_native[i]);
        }
        Arc::from(out.into_boxed_slice())
    } else {
        let mut out = Vec::with_capacity(left_native.len());
        for i in 0..left_native.len() {
            out.push(0.5 * (left_native[i] + right_native[i]));
        }
        Arc::from(out.into_boxed_slice())
    };

    state
        .audio_channels
        .store(target_audio_channels, Ordering::Release);
    state.sample_rate.store(native_sr, Ordering::Release);
    state.total.store(pcm_native.len(), Ordering::Release);

    if let Err(e) = build_and_run_stream(
        &state,
        pcm_native,
        &device,
        &config,
        sample_format,
        device_channels,
        stop_rx,
    ) {
        state.set_error(&e);
    }
}

fn build_and_run_stream(
    state: &Arc<PlayerState>,
    pcm: Arc<[f32]>,
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    channels: u16,
    stop_rx: mpsc::Receiver<()>,
) -> Result<(), AudioError> {
    let err_fn = |e| eprintln!("[syngui/audio/player] stream error: {e}");

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let st = state.clone();
            let pcm_cb = pcm.clone();
            device.build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    write_callback_f32(&st, &pcm_cb, data, channels);
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let st = state.clone();
            let pcm_cb = pcm.clone();
            device.build_output_stream(
                config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    write_callback_i16(&st, &pcm_cb, data, channels);
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let st = state.clone();
            let pcm_cb = pcm.clone();
            device.build_output_stream(
                config,
                move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                    write_callback_u16(&st, &pcm_cb, data, channels);
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
    };
    let stream = stream.map_err(|e| AudioError::Cpal(format!("build_output_stream: {e}")))?;
    stream
        .play()
        .map_err(|e| AudioError::Cpal(format!("stream.play: {e}")))?;
    state.ready.store(true, Ordering::Release);

    loop {
        if state.done.load(Ordering::Acquire) {
            break;
        }
        if state.cursor.load(Ordering::Relaxed) >= state.total() {
            state.done.store(true, Ordering::Release);
            break;
        }
        match stop_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(()) => {
                state.done.store(true, Ordering::Release);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(stream);
    drop(pcm);
    Ok(())
}

fn write_callback_f32(state: &PlayerState, pcm: &[f32], data: &mut [f32], channels: u16) {
    let dev_ch = channels.max(1) as usize;
    let frames = data.len() / dev_ch;
    let chunk = next_chunk_frames(state, pcm, frames);
    let gain = state.volume();
    fill_device_frames_f32(state.audio_channels() as usize, dev_ch, gain, &chunk, data);
}

fn write_callback_i16(state: &PlayerState, pcm: &[f32], data: &mut [i16], channels: u16) {
    let dev_ch = channels.max(1) as usize;
    let frames = data.len() / dev_ch;
    let chunk = next_chunk_frames(state, pcm, frames);
    let audio_ch = state.audio_channels() as usize;
    let vol = state.volume();
    for f in 0..frames {
        for c in 0..dev_ch {
            let sample = if c < audio_ch.min(2) {
                chunk[f * audio_ch + c]
            } else if audio_ch == 1 {
                chunk[f]
            } else {
                0.0
            };
            data[f * dev_ch + c] = ((sample * vol).clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        }
    }
}

fn write_callback_u16(state: &PlayerState, pcm: &[f32], data: &mut [u16], channels: u16) {
    let dev_ch = channels.max(1) as usize;
    let frames = data.len() / dev_ch;
    let chunk = next_chunk_frames(state, pcm, frames);
    let audio_ch = state.audio_channels() as usize;
    let mid = (u16::MAX / 2) as f32;
    let vol = state.volume();
    for f in 0..frames {
        for c in 0..dev_ch {
            let sample = if c < audio_ch.min(2) {
                chunk[f * audio_ch + c]
            } else if audio_ch == 1 {
                chunk[f]
            } else {
                0.0
            };
            data[f * dev_ch + c] = (((sample * vol).clamp(-1.0, 1.0) * mid) + mid) as u16;
        }
    }
}

fn fill_device_frames_f32(
    audio_ch: usize,
    dev_ch: usize,
    gain: f32,
    chunk: &[f32],
    out: &mut [f32],
) {
    let frames = out.len() / dev_ch;
    if audio_ch == 1 {
        for f in 0..frames {
            let s = chunk.get(f).copied().unwrap_or(0.0) * gain;
            for c in 0..dev_ch {
                out[f * dev_ch + c] = s;
            }
        }
    } else {
        for f in 0..frames {
            let l = chunk.get(f * 2).copied().unwrap_or(0.0) * gain;
            let r = chunk.get(f * 2 + 1).copied().unwrap_or(0.0) * gain;
            for c in 0..dev_ch {
                out[f * dev_ch + c] = match c {
                    0 => l,
                    1 => r,
                    _ => 0.0,
                };
            }
        }
    }
}

fn next_chunk_frames(state: &PlayerState, pcm: &[f32], frames: usize) -> Vec<f32> {
    let ach = state.audio_channels().max(1) as usize;
    let needed = frames * ach;
    if state.is_paused() {
        return vec![0.0; needed];
    }
    let total = state.total();
    let start = state.cursor.fetch_add(needed, Ordering::AcqRel);
    if start >= total {
        let _ = state.cursor.fetch_sub(needed, Ordering::AcqRel);
        return vec![0.0; needed];
    }
    let end = (start + needed).min(total);
    let mut out: Vec<f32> = pcm[start..end].to_vec();
    if out.len() < needed {
        out.resize(needed, 0.0);
    }
    out
}

fn resample_mono_f32(pcm: &[f32], sr_in: u32, sr_out: u32) -> Result<Vec<f32>, AudioError> {
    use rubato::{
        Resampler as _, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
        WindowFunction,
    };

    if sr_in == sr_out {
        return Ok(pcm.to_vec());
    }

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        oversampling_factor: 256,
        interpolation: SincInterpolationType::Linear,
        window: WindowFunction::BlackmanHarris2,
    };
    let in_chunk: usize = 1024;
    let ratio = sr_out as f64 / sr_in as f64;
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, in_chunk, 1)
        .map_err(|e| AudioError::Cpal(format!("rubato create: {e}")))?;

    let mut out: Vec<f32> =
        Vec::with_capacity((pcm.len() as f64 * ratio) as usize + in_chunk);

    let mut pos = 0;
    while pos + in_chunk <= pcm.len() {
        let chunk = vec![pcm[pos..pos + in_chunk].to_vec()];
        let processed = resampler
            .process(&chunk, None)
            .map_err(|e| AudioError::Cpal(format!("rubato process: {e}")))?;
        out.extend_from_slice(&processed[0]);
        pos += in_chunk;
    }
    if pos < pcm.len() {
        let remaining_in = pcm.len() - pos;
        let mut remaining = pcm[pos..].to_vec();
        remaining.resize(in_chunk, 0.0);
        let chunk = vec![remaining];
        let processed = resampler
            .process(&chunk, None)
            .map_err(|e| AudioError::Cpal(format!("rubato tail: {e}")))?;
        let take = ((remaining_in as f64) * ratio).round() as usize;
        out.extend_from_slice(&processed[0][..take.min(processed[0].len())]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(audio_channels: u16, total: usize) -> PlayerState {
        let s = PlayerState::new_pending();
        s.audio_channels.store(audio_channels, Ordering::Release);
        s.total.store(total, Ordering::Release);
        s
    }

    #[test]
    fn next_chunk_zeros_after_end() {
        let pcm: Vec<f32> = vec![1.0_f32, 2.0, 3.0, 4.0];
        let state = make_state(1, pcm.len());
        let chunk = next_chunk_frames(&state, &pcm, 6);
        assert_eq!(chunk, vec![1.0, 2.0, 3.0, 4.0, 0.0, 0.0]);
    }

    #[test]
    fn next_chunk_paused_returns_silence_without_advancing_cursor() {
        let pcm: Vec<f32> = vec![0.5_f32; 16];
        let state = make_state(1, pcm.len());
        let _ = next_chunk_frames(&state, &pcm, 4);
        let cursor_before_pause = state.cursor.load(Ordering::Acquire);
        assert_eq!(cursor_before_pause, 4);
        state.paused.store(true, Ordering::Release);
        let chunk = next_chunk_frames(&state, &pcm, 4);
        assert_eq!(chunk, vec![0.0, 0.0, 0.0, 0.0]);
        assert_eq!(state.cursor.load(Ordering::Acquire), cursor_before_pause);
        state.paused.store(false, Ordering::Release);
        let chunk = next_chunk_frames(&state, &pcm, 2);
        assert_eq!(chunk, vec![0.5, 0.5]);
        assert_eq!(state.cursor.load(Ordering::Acquire), 6);
    }

    #[test]
    fn seek_via_cursor_store_clamps_to_total() {
        let pcm: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let state = make_state(1, pcm.len());
        state.cursor.store(10, Ordering::Release);
        let chunk = next_chunk_frames(&state, &pcm, 4);
        assert_eq!(chunk, vec![10.0, 11.0, 12.0, 13.0]);
        assert_eq!(state.cursor.load(Ordering::Acquire), 14);
    }

    #[test]
    fn next_chunk_stereo_interleaved() {
        let pcm: Vec<f32> = vec![0.1_f32, -0.1, 0.2, -0.2, 0.3, -0.3];
        let state = make_state(2, pcm.len());
        let chunk = next_chunk_frames(&state, &pcm, 5);
        assert_eq!(
            chunk,
            vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3, 0.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn fill_device_frames_stereo_to_stereo() {
        let chunk = vec![0.5_f32, -0.5, 0.25, -0.25];
        let mut out = vec![0.0_f32; 4];
        fill_device_frames_f32(2, 2, 1.0, &chunk, &mut out);
        assert_eq!(out, vec![0.5, -0.5, 0.25, -0.25]);
    }

    #[test]
    fn fill_device_frames_stereo_to_mono_takes_left_only() {
        let chunk = vec![0.5_f32, -0.5, 0.25, -0.25];
        let mut out = vec![0.0_f32; 2];
        fill_device_frames_f32(2, 1, 1.0, &chunk, &mut out);
        assert_eq!(out, vec![0.5, 0.25]);
    }

    #[test]
    fn fill_device_frames_applies_gain_per_sample() {
        let chunk = vec![0.5_f32, -0.5, 1.0, -1.0];
        let mut out = vec![0.0_f32; 4];
        fill_device_frames_f32(2, 2, 0.5, &chunk, &mut out);
        assert_eq!(out, vec![0.25, -0.25, 0.5, -0.5]);
    }

    #[test]
    fn resample_passthrough_same_rate() {
        let pcm: Vec<f32> = vec![0.1, 0.2, 0.3];
        let out = resample_mono_f32(&pcm, 48000, 48000)
            .expect("same-rate resample must succeed (passthrough)");
        assert_eq!(out, pcm);
    }

    #[test]
    fn resample_24k_to_44k_does_not_panic() {
        let pcm: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.01).sin()).collect();
        let out = resample_mono_f32(&pcm, 24_000, 44_100)
            .expect("24k→44.1k resample должен работать без паник");
        let expected = (pcm.len() as f64 * 44_100.0 / 24_000.0) as usize;
        let tol = 1024;
        assert!(
            (out.len() as i64 - expected as i64).unsigned_abs() <= tol as u64,
            "выход {} сильно отличается от ожидаемого {}",
            out.len(),
            expected,
        );
    }
}
