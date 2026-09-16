//! Потоковый вывод звука на Android — oboe напрямую, минуя cpal.
//!
//! Зачем свой путь: мастер-часы видео идут по числу проигранных сэмплов
//! (`AudioPlayer::playback_clock_sec`), а cpal умеет сообщить только, сколько
//! сэмплов он *отдал* в буфер устройства. На телефоне разница в десяток
//! миллисекунд незаметна, но на ТВ тракт (микшер AudioFlinger + HDMI +
//! панель) держит сотни миллисекунд — и картинка уезжает вперёд звука.
//!
//! oboe знает задержку своего стрима (`calculate_latency_millis`), поэтому
//! здесь курсор — это «что слышно сейчас»: отданные сэмплы минус то, что
//! ещё висит в тракте. Заодно частоту конвертирует сам Android, а не
//! rubato на armv7 (sinc 256 в аудио-потоке ТВ не бесплатен).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use oboe::{
    AudioOutputCallback, AudioOutputStreamSafe, AudioStream, AudioStreamAsync, AudioStreamBuilder,
    ContentType, DataCallbackResult, Mono, Output, PerformanceMode, SampleRateConversionQuality,
    Usage,
};

use super::player::{monotonic_nanos, AudioPlayer, PlayerState};
use super::recorder::AudioError;

/// Предел очереди вывода (~10 с при 48 кГц моно) — как в cpal-пути.
const MAX_QUEUED_SAMPLES: usize = 48_000 * 10;
/// Как часто спрашивать у стрима задержку: раз в столько колбэков
/// (при ~10 мс на колбэк — примерно раз в четверть секунды).
const LATENCY_EVERY: u32 = 24;
/// Потолок доверия к задержке: больше секунды — заведомо мусор (часы бы
/// дёрнуло назад, и видео встало бы).
const LATENCY_MAX_SEC: f64 = 1.0;
/// Сглаживание замеров: часы должны ползти, а не прыгать на каждом опросе.
const LATENCY_SMOOTH: f64 = 0.15;

/// Запустить вывод: `rx` — моно-чанки f32 частоты `sample_rate` от декодера.
pub(super) fn start_streaming(
    rx: Receiver<Vec<f32>>,
    sample_rate: u32,
) -> Result<AudioPlayer, AudioError> {
    if sample_rate == 0 {
        return Err(AudioError::Cpal("нулевой sample rate".into()));
    }

    let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
    let total_written = Arc::new(AtomicUsize::new(0));
    let total_played = Arc::new(AtomicUsize::new(0));
    // Источник закрыт: только после этого пустая очередь означает конец
    // потока, а не буферизацию.
    let input_done = Arc::new(AtomicBool::new(false));

    let state = Arc::new(PlayerState::new_pending());
    state.set_audio_channels(1);
    state.set_sample_rate(sample_rate);
    state.set_ready();
    state.set_streaming();

    spawn_drainer(rx, queue.clone(), total_written.clone(), input_done.clone())?;

    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), AudioError>>(1);
    let join = thread::Builder::new()
        .name("syngui-audio-oboe".into())
        .spawn({
            let state = state.clone();
            let queue = queue.clone();
            let written = total_written.clone();
            let played = total_played.clone();
            let input_done = input_done.clone();
            move || {
                run_stream_thread(
                    state,
                    queue,
                    written,
                    played,
                    input_done,
                    sample_rate,
                    init_tx,
                    stop_rx,
                )
            }
        })
        .map_err(|e| AudioError::Cpal(format!("spawn oboe thread: {e}")))?;

    match init_rx.recv_timeout(Duration::from_secs(3)) {
        Ok(Ok(())) => Ok(AudioPlayer::from_parts(state, stop_tx, join)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(AudioError::Cpal("timeout инициализации oboe output".into())),
    }
}

/// Чтение чанков декодера в очередь вывода. Обратное давление: очередь не
/// растёт без предела, декодер упирается в свой канал.
fn spawn_drainer(
    rx: Receiver<Vec<f32>>,
    queue: Arc<Mutex<VecDeque<f32>>>,
    written: Arc<AtomicUsize>,
    input_done: Arc<AtomicBool>,
) -> Result<(), AudioError> {
    thread::Builder::new()
        .name("syngui-audio-oboe-drainer".into())
        .spawn(move || {
            struct DoneGuard(Arc<AtomicBool>);
            impl Drop for DoneGuard {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Release);
                }
            }
            let _guard = DoneGuard(input_done);
            while let Ok(chunk) = rx.recv() {
                while queue
                    .lock()
                    .map(|q| q.len() > MAX_QUEUED_SAMPLES)
                    .unwrap_or(false)
                {
                    thread::sleep(Duration::from_millis(10));
                }
                written.fetch_add(chunk.len(), Ordering::AcqRel);
                if let Ok(mut q) = queue.lock() {
                    q.extend(chunk.into_iter());
                }
            }
        })
        .map(|_| ())
        .map_err(|e| AudioError::Cpal(format!("spawn oboe drainer: {e}")))
}

#[allow(clippy::too_many_arguments)]
fn run_stream_thread(
    state: Arc<PlayerState>,
    queue: Arc<Mutex<VecDeque<f32>>>,
    written: Arc<AtomicUsize>,
    played: Arc<AtomicUsize>,
    input_done: Arc<AtomicBool>,
    sample_rate: u32,
    init_tx: mpsc::SyncSender<Result<(), AudioError>>,
    stop_rx: Receiver<()>,
) {
    let callback = OboeCallback {
        queue: queue.clone(),
        played: played.clone(),
        state: state.clone(),
        sample_rate,
        ticks: 0,
        latency_sec: None,
        logged_latency_sec: -1.0,
    };

    // Частоту конвертирует Android, если устройство не умеет 48 кГц:
    // дешевле, чем ресемплер в нашем потоке, и не ломает счёт сэмплов —
    // колбэк всё равно работает на запрошенной частоте.
    let stream = AudioStreamBuilder::default()
        .set_output()
        .set_format::<f32>()
        .set_mono()
        .set_sample_rate(sample_rate as i32)
        .set_sample_rate_conversion_quality(SampleRateConversionQuality::Medium)
        // Не LowLatency: на ТВ важнее не ловить underrun, а задержку мы и
        // так вычитаем из часов.
        .set_performance_mode(PerformanceMode::None)
        .set_usage(Usage::Media)
        .set_content_type(ContentType::Movie)
        .set_callback(callback)
        .open_stream();

    let mut stream: AudioStreamAsync<Output, OboeCallback> = match stream {
        Ok(s) => s,
        Err(e) => {
            let _ = init_tx.send(Err(AudioError::Cpal(format!("oboe open_stream: {e}"))));
            return;
        }
    };
    if let Err(e) = stream.start() {
        let _ = init_tx.send(Err(AudioError::Cpal(format!("oboe start: {e}"))));
        return;
    }
    let _ = init_tx.send(Ok(()));

    loop {
        if state.is_done() {
            break;
        }
        let played_now = played.load(Ordering::Acquire);
        if input_done.load(Ordering::Acquire) {
            let q_len = queue.lock().map(|q| q.len()).unwrap_or(0);
            if q_len == 0 && played_now >= written.load(Ordering::Acquire) {
                state.mark_done();
                break;
            }
        }
        state.set_cursor(played_now);
        match stop_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(()) => {
                state.mark_done();
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = stream.stop();
    let _ = stream.close();
}

struct OboeCallback {
    queue: Arc<Mutex<VecDeque<f32>>>,
    played: Arc<AtomicUsize>,
    state: Arc<PlayerState>,
    sample_rate: u32,
    ticks: u32,
    /// Сглаженная задержка тракта, секунды; `None` — ещё ни одного замера.
    latency_sec: Option<f64>,
    /// Последнее залогированное значение — пишем в лог только заметные
    /// изменения, а не каждый опрос.
    logged_latency_sec: f64,
}

impl AudioOutputCallback for OboeCallback {
    type FrameType = (f32, Mono);

    fn on_audio_ready(
        &mut self,
        stream: &mut dyn AudioOutputStreamSafe,
        out: &mut [f32],
    ) -> DataCallbackResult {
        let paused = self.state.is_paused();
        let volume = self.state.volume();
        let mut taken = 0usize;
        if !paused {
            if let Ok(mut q) = self.queue.lock() {
                let take = out.len().min(q.len());
                for slot in out.iter_mut().take(take) {
                    *slot = q.pop_front().unwrap_or(0.0) * volume;
                }
                taken = take;
            }
        }
        // Недостачу добиваем тишиной: пустая очередь — буферизация, курсор
        // при этом стоит, и часы видео стоят вместе с ним.
        for slot in out.iter_mut().skip(taken) {
            *slot = 0.0;
        }
        if taken > 0 {
            self.played.fetch_add(taken, Ordering::AcqRel);
        }
        // Якорь часов воспроизведения: сейчас слышно столько, сколько отдано
        // минус то, что ещё висит в тракте. Между колбэками часы идут по
        // системному времени (см. `AudioPlayer::playback_clock_sec`) — иначе
        // видео равнялось бы на ступеньку размером с буфер.
        let audible = self
            .played
            .load(Ordering::Acquire)
            .saturating_sub(self.state.latency_samples());
        self.state.set_clock_anchor(audible, monotonic_nanos());

        // Пока замера нет, спрашиваем каждый колбэк: первый ответ сдвигает
        // часы назад, и чем раньше это случится, тем короче рывок в начале.
        self.ticks = self.ticks.wrapping_add(1);
        if self.latency_sec.is_none() || self.ticks % LATENCY_EVERY == 0 {
            if let Ok(ms) = stream.calculate_latency_millis() {
                let measured = (ms / 1000.0).clamp(0.0, LATENCY_MAX_SEC);
                let smoothed = match self.latency_sec {
                    Some(prev) => prev + (measured - prev) * LATENCY_SMOOTH,
                    None => measured,
                };
                self.latency_sec = Some(smoothed);
                self.state
                    .set_latency_samples((smoothed * self.sample_rate as f64) as usize);
                if (smoothed - self.logged_latency_sec).abs() > 0.025 {
                    self.logged_latency_sec = smoothed;
                    log::info!(
                        "audio(oboe): задержка вывода {:.0} мс — на столько сдвинуты часы A/V",
                        smoothed * 1000.0
                    );
                }
            }
        }
        DataCallbackResult::Continue
    }
}
