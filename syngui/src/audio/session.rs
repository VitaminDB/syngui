use std::io::Cursor;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::core::sync::Mutex;
use crate::signal::{use_signal, RwSignal};

use super::{AudioBuffer, AudioError, AudioRecorder, AudioStream, VisHandle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Paused,
    Completed,
    Failed,
}

#[derive(Clone)]
pub struct RecordingResult {
    pub wav_bytes: Arc<[u8]>,
    pub sample_rate: u32,
    pub duration: Duration,
    pub audio_buffer: Option<Arc<AudioBuffer>>,
}

/// `start_with_device`. Это упрощает thread-safety: фоновому таймеру нужен
#[derive(Clone, Debug)]
pub struct RecordingOptions {
    pub decode_on_stop: bool,
    pub open_stream_on_start: bool,
    pub elapsed_tick_interval: Duration,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            decode_on_stop: false,
            open_stream_on_start: false,
            elapsed_tick_interval: Duration::from_millis(50),
        }
    }
}

#[derive(Clone)]
pub struct RecordingSession {
    inner: Arc<SessionInner>,
}

struct SessionInner {
    state: RwSignal<RecordingState>,
    vis_handle: RwSignal<Option<VisHandle>>,
    audio_stream: RwSignal<Option<Arc<AudioStream>>>,
    elapsed_secs: RwSignal<f64>,
    error: RwSignal<Option<String>>,
    last_result: RwSignal<Option<RecordingResult>>,
    actual_device_name: RwSignal<Option<String>>,

    recorder: Mutex<Option<AudioRecorder>>,
    elapsed_stop: Mutex<Option<Sender<()>>>,
    elapsed_join: Mutex<Option<JoinHandle<()>>>,
    options: RecordingOptions,
}

impl RecordingSession {
    pub fn new(options: RecordingOptions) -> Self {
        let inner = Arc::new(SessionInner {
            state: use_signal(RecordingState::Idle),
            vis_handle: use_signal(None),
            audio_stream: use_signal(None),
            elapsed_secs: use_signal(0.0_f64),
            error: use_signal(None),
            last_result: use_signal(None),
            actual_device_name: use_signal(None),
            recorder: Mutex::new(None),
            elapsed_stop: Mutex::new(None),
            elapsed_join: Mutex::new(None),
            options,
        });
        Self { inner }
    }

    pub fn idle() -> Self {
        Self::new(RecordingOptions::default())
    }

    pub fn state(&self) -> RwSignal<RecordingState> {
        self.inner.state
    }
    pub fn vis_handle(&self) -> RwSignal<Option<VisHandle>> {
        self.inner.vis_handle
    }
    pub fn audio_stream(&self) -> RwSignal<Option<Arc<AudioStream>>> {
        self.inner.audio_stream
    }
    pub fn elapsed_secs(&self) -> RwSignal<f64> {
        self.inner.elapsed_secs
    }
    pub fn error(&self) -> RwSignal<Option<String>> {
        self.inner.error
    }
    pub fn last_result(&self) -> RwSignal<Option<RecordingResult>> {
        self.inner.last_result
    }
    pub fn actual_device_name(&self) -> RwSignal<Option<String>> {
        self.inner.actual_device_name
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.inner.state.get_untracked(),
            RecordingState::Recording | RecordingState::Paused
        )
    }
    pub fn is_recording(&self) -> bool {
        self.inner.state.get_untracked() == RecordingState::Recording
    }
    pub fn is_paused(&self) -> bool {
        self.inner.state.get_untracked() == RecordingState::Paused
    }

    pub fn start(&self) -> Result<(), AudioError> {
        self.start_with_device(None)
    }

    pub fn start_with_device(&self, preferred: Option<&str>) -> Result<(), AudioError> {
        if self.is_active() {
            return Err(AudioError::Cpal(
                "сессия уже активна — вызовите stop() перед повторным start()".into(),
            ));
        }

        self.inner.error.set(None);
        self.inner.last_result.set_always(None);
        self.inner.elapsed_secs.set(0.0);

        let recorder = match AudioRecorder::start_with_device(preferred) {
            Ok(r) => r,
            Err(e) => {
                self.inner.error.set(Some(format!("{e}")));
                self.inner.state.set(RecordingState::Failed);
                return Err(e);
            }
        };

        let vis = recorder.vis_handle();
        self.inner.vis_handle.set(Some(vis));
        self.inner
            .actual_device_name
            .set(Some(recorder.actual_device_name().to_string()));

        if self.inner.options.open_stream_on_start {
            if let Some(stream) = recorder.open_stream() {
                self.inner.audio_stream.set_always(Some(stream));
            }
        }

        if let Ok(mut g) = self.inner.recorder.lock() {
            *g = Some(recorder);
        }

        self.inner.state.set(RecordingState::Recording);
        self.spawn_elapsed_ticker();
        Ok(())
    }

    pub fn pause(&self) {
        let Ok(g) = self.inner.recorder.lock() else {
            return;
        };
        if let Some(r) = g.as_ref() {
            r.pause();
            self.inner.state.set(RecordingState::Paused);
        }
    }

    pub fn resume(&self) {
        let Ok(g) = self.inner.recorder.lock() else {
            return;
        };
        if let Some(r) = g.as_ref() {
            r.resume();
            self.inner.state.set(RecordingState::Recording);
        }
    }

    pub fn stop(&self) -> Result<RecordingResult, AudioError> {
        if !self.is_active() {
            return Err(AudioError::Cpal(
                "сессия не активна — нечего останавливать".into(),
            ));
        }
        self.stop_elapsed_ticker();

        let recorder = self
            .inner
            .recorder
            .lock()
            .ok()
            .and_then(|mut g| g.take());
        let Some(recorder) = recorder else {
            self.inner.error.set(Some("recorder уже отдан".into()));
            self.inner.state.set(RecordingState::Failed);
            return Err(AudioError::Cpal("recorder уже отдан".into()));
        };

        let sample_rate = recorder.sample_rate();
        let duration = Duration::from_secs_f64(self.inner.elapsed_secs.get_untracked());

        let wav = match recorder.stop_and_encode_wav() {
            Ok(b) => b,
            Err(e) => {
                self.inner.error.set(Some(format!("{e}")));
                self.inner.state.set(RecordingState::Failed);
                self.inner.audio_stream.set_always(None);
                self.inner.vis_handle.set(None);
                return Err(e);
            }
        };
        let wav_bytes: Arc<[u8]> = Arc::from(wav.into_boxed_slice());

        let audio_buffer = if self.inner.options.decode_on_stop {
            match decode_pcm_wav(&wav_bytes) {
                Ok(buf) => Some(Arc::new(buf)),
                Err(e) => {
                    eprintln!("[syngui/audio] decode_on_stop: {e}");
                    None
                }
            }
        } else {
            None
        };

        self.inner.audio_stream.set_always(None);
        self.inner.vis_handle.set(None);

        let result = RecordingResult {
            wav_bytes,
            sample_rate,
            duration,
            audio_buffer,
        };
        self.inner.last_result.set_always(Some(result.clone()));
        self.inner.state.set(RecordingState::Completed);
        Ok(result)
    }

    pub fn reset(&self) {
        if self.is_active() {
            let _ = self.stop();
        }
        self.inner.state.set(RecordingState::Idle);
        self.inner.vis_handle.set(None);
        self.inner.audio_stream.set_always(None);
        self.inner.elapsed_secs.set(0.0);
        self.inner.error.set(None);
        self.inner.last_result.set_always(None);
        self.inner.actual_device_name.set(None);
    }

    fn spawn_elapsed_ticker(&self) {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let interval = self.inner.options.elapsed_tick_interval;
        let weak = Arc::downgrade(&self.inner);
        let elapsed = self.inner.elapsed_secs;

        let join = thread::Builder::new()
            .name("syngui-recording-session-tick".into())
            .spawn(move || loop {
                match stop_rx.recv_timeout(interval) {
                    Ok(_) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                let Some(inner) = weak.upgrade() else { break; };
                let secs = inner
                    .recorder
                    .lock()
                    .ok()
                    .and_then(|g| g.as_ref().map(|r| r.elapsed().as_secs_f64()))
                    .unwrap_or(0.0);
                elapsed.set(secs);
            })
            .ok();
        if let Ok(mut g) = self.inner.elapsed_stop.lock() {
            *g = Some(stop_tx);
        }
        if let Ok(mut g) = self.inner.elapsed_join.lock() {
            *g = join;
        }
    }

    fn stop_elapsed_ticker(&self) {
        let stop_tx = self
            .inner
            .elapsed_stop
            .lock()
            .ok()
            .and_then(|mut g| g.take());
        if let Some(tx) = stop_tx {
            let _ = tx.send(());
        }
        let join = self
            .inner
            .elapsed_join
            .lock()
            .ok()
            .and_then(|mut g| g.take());
        if let Some(j) = join {
            let _ = j.join();
        }
    }
}

impl Drop for SessionInner {
    fn drop(&mut self) {
        if let Ok(mut g) = self.elapsed_stop.lock() {
            if let Some(tx) = g.take() {
                let _ = tx.send(());
            }
        }
        if let Ok(mut g) = self.elapsed_join.lock() {
            if let Some(j) = g.take() {
                let _ = j.join();
            }
        }
    }
}

fn decode_pcm_wav(bytes: &[u8]) -> Result<AudioBuffer, AudioError> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|e| AudioError::Wav(format!("open: {e}")))?;
    let spec = reader.spec();
    let pcm: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample.max(1).min(32);
            let max = ((1u64 << (bits.saturating_sub(1))) as f32).max(1.0);
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|x| (x as f32 / max).clamp(-1.0, 1.0))
                .collect()
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .filter_map(Result::ok)
            .collect(),
    };
    if pcm.is_empty() {
        return Err(AudioError::Wav("decode: пустой PCM".into()));
    }
    Ok(AudioBuffer::new(
        Arc::from(pcm.into_boxed_slice()),
        spec.sample_rate,
        spec.channels.max(1),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::init_main_thread;

    fn ensure_main_thread() {
        init_main_thread();
    }

    #[test]
    fn new_session_is_idle() {
        ensure_main_thread();
        let s = RecordingSession::idle();
        assert_eq!(s.state().get_untracked(), RecordingState::Idle);
        assert!(!s.is_active());
        assert!(s.last_result().get_untracked().is_none());
        assert_eq!(s.elapsed_secs().get_untracked(), 0.0);
    }

    #[test]
    fn stop_without_start_returns_error() {
        ensure_main_thread();
        let s = RecordingSession::idle();
        let result = s.stop();
        assert!(result.is_err());
        assert_eq!(s.state().get_untracked(), RecordingState::Idle);
    }

    #[test]
    fn decode_pcm_wav_roundtrip() {
        let sample_rate = 16_000u32;
        let total = (sample_rate as f32 * 0.1) as usize;
        let mut samples: Vec<f32> = Vec::with_capacity(total);
        for i in 0..total {
            let t = i as f32 / sample_rate as f32;
            samples.push((t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5);
        }
        let wav = crate::audio::wav::into_pcm16_bytes(&samples, sample_rate).expect("encode");
        let buf = decode_pcm_wav(&wav).expect("decode");
        assert_eq!(buf.sample_rate, sample_rate);
        assert_eq!(buf.channels, 1);
        assert!(buf.frames() == samples.len());
        for (orig, dec) in samples.iter().zip(buf.pcm.iter()) {
            assert!((orig - dec).abs() < 1.0 / 16_000.0, "{orig} vs {dec}");
        }
    }
}
