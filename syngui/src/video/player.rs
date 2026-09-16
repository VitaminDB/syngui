use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use web_time::Duration as WebDuration;

use super::decoder::{VideoDecoder, VideoFrame, VideoMeta};
use super::error::VideoError;
use super::hwaccel::HwAccel;
use super::stream::VideoStream;
use crate::audio::{AudioPlayer, AudioStream};

/// Границы допуска «время кадра наступило»: половина периода опроса
/// (кадра UI), зажатая в эти пределы. Кадр, чей pts попадает между двумя
/// опросами, показывается на ближайшем к нему — иначе каденция гуляет на
/// целый кадр UI.
const MIN_TOLERANCE_SEC: f64 = 0.002;
const MAX_TOLERANCE_SEC: f64 = 0.020;

struct PlayerShared {
    paused: AtomicBool,
    duration_sec: f64,
    seek_offset_micros: AtomicU64,
    volume_bits: AtomicU32,
}

impl PlayerShared {
    fn seek_offset_sec(&self) -> f64 {
        self.seek_offset_micros.load(Ordering::Relaxed) as i64 as f64 / 1_000_000.0
    }

    fn set_seek_offset_sec(&self, sec: f64) {
        let micros = (sec * 1_000_000.0) as i64 as u64;
        self.seek_offset_micros.store(micros, Ordering::Relaxed);
    }
}

pub struct VideoPlayer {
    decoder: VideoDecoder,
    audio: Option<AudioPlayer>,
    shared: Arc<PlayerShared>,
    pending_frame: Option<VideoFrame>,
    wall_start: Option<Instant>,
    paused_accum: WebDuration,
    paused_at: Option<Instant>,
    input_path: String,
    has_audio: bool,
    /// Когда пришёл последний кадр — для индикатора буферизации.
    last_frame_at: Instant,
    /// Период опроса `poll_frame` (кадр UI), сглаженный — по нему считается
    /// допуск показа кадра.
    poll_period_sec: f64,
    last_poll_at: Option<Instant>,
    stats: FrameStats,
}

impl VideoPlayer {
    pub fn open(input: &str) -> Result<Self, VideoError> {
        Self::open_with_hwaccel(input, HwAccel::None)
    }

    pub fn open_with_hwaccel(input: &str, accel: HwAccel) -> Result<Self, VideoError> {
        Self::open_with_options(input, accel, &[])
    }

    /// См. [`VideoDecoder::open_with_options`]: опции libavformat
    /// (`user_agent`, `headers`, таймауты) для сетевых источников.
    pub fn open_with_options(
        input: &str,
        accel: HwAccel,
        options: &[(&str, &str)],
    ) -> Result<Self, VideoError> {
        let mut decoder = VideoDecoder::open_with_options(input, accel, options)?;
        let has_audio = decoder.meta().has_audio;
        let audio = if has_audio {
            match decoder.take_audio_rx() {
                Some(rx) => match AudioPlayer::start_streaming(rx, decoder.audio_output_sr()) {
                    Ok(p) => Some(p),
                    Err(e) => {
                        eprintln!("[syngui/video] audio init failed, продолжаем без звука: {e}");
                        None
                    }
                },
                None => None,
            }
        } else {
            None
        };

        let shared = Arc::new(PlayerShared {
            paused: AtomicBool::new(false),
            duration_sec: decoder.meta().duration_sec,
            seek_offset_micros: AtomicU64::new(0),
            volume_bits: AtomicU32::new(1.0_f32.to_bits()),
        });

        Ok(Self {
            decoder,
            audio,
            shared,
            pending_frame: None,
            wall_start: Some(Instant::now()),
            paused_accum: WebDuration::ZERO,
            paused_at: None,
            input_path: input.to_string(),
            has_audio,
            last_frame_at: Instant::now(),
            poll_period_sec: 0.0,
            last_poll_at: None,
            stats: FrameStats::new(),
        })
    }

    pub fn meta(&self) -> &VideoMeta {
        self.decoder.meta()
    }

    pub fn is_paused(&self) -> bool {
        self.shared.paused.load(Ordering::Relaxed)
    }

    pub fn play(&mut self) {
        if !self.is_paused() {
            return;
        }
        self.shared.paused.store(false, Ordering::Relaxed);
        if let Some(at) = self.paused_at.take() {
            self.paused_accum += at.elapsed();
        }
        self.last_frame_at = Instant::now();
        self.decoder.resume();
    }

    pub fn pause(&mut self) {
        if self.is_paused() {
            return;
        }
        self.shared.paused.store(true, Ordering::Relaxed);
        self.paused_at = Some(Instant::now());
        self.decoder.pause();
    }

    pub fn duration_sec(&self) -> f64 {
        self.shared.duration_sec
    }

    pub fn position_sec(&self) -> f64 {
        self.master_clock_sec()
    }

    pub fn set_volume(&self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        self.shared
            .volume_bits
            .store(v.to_bits(), Ordering::Relaxed);
        if let Some(p) = &self.audio {
            p.set_volume(v);
        }
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.shared.volume_bits.load(Ordering::Relaxed))
    }

    pub fn seek(&mut self, sec: f64) -> Result<(), VideoError> {
        let target = sec.clamp(0.0, self.shared.duration_sec.max(0.0));
        self.pending_frame = None;
        while self.decoder.try_recv_video().is_ok() {}

        self.decoder.seek(target);

        if self.decoder.meta().has_audio {
            if let Some(p) = self.audio.take() {
                drop(p);
            }
            if let Some(rx) = self.decoder.re_attach_audio() {
                match AudioPlayer::start_streaming(rx, self.decoder.audio_output_sr()) {
                    Ok(p) => {
                        let vol = self.volume();
                        p.set_volume(vol);
                        if self.is_paused() {
                            p.pause();
                        }
                        self.audio = Some(p);
                        self.has_audio = true;
                    }
                    Err(e) => {
                        eprintln!(
                            "[syngui/video] audio re-attach after seek failed, продолжаем без звука: {e}"
                        );
                        self.has_audio = false;
                    }
                }
            } else {
                self.has_audio = false;
            }
        }

        self.shared.set_seek_offset_sec(target);
        self.last_frame_at = Instant::now();
        self.wall_start = Some(Instant::now());
        self.paused_accum = WebDuration::ZERO;
        if self.is_paused() {
            self.paused_at = Some(Instant::now());
        } else {
            self.paused_at = None;
        }
        Ok(())
    }

    /// Кадры не приходят дольше 0,7 с при воспроизведении и не в конце —
    /// сеть/декодер не успевают, UI может показать «буферизация».
    pub fn is_buffering(&self) -> bool {
        if self.is_paused() {
            return false;
        }
        let dur = self.shared.duration_sec;
        if dur > 0.0 && self.position_sec() >= dur - 0.5 {
            return false;
        }
        self.last_frame_at.elapsed() > WebDuration::from_millis(700)
    }

    pub fn poll_frame(&mut self) -> Option<VideoFrame> {
        if self.is_paused() {
            return None;
        }
        let clock = self.master_clock_sec();
        self.stats.on_poll(clock);
        let tolerance = self.poll_tolerance_sec();

        let mut candidate = self
            .pending_frame
            .take()
            .or_else(|| self.decoder.try_recv_video().ok())?;

        if candidate.surface.is_some() {
            return self.schedule_surface_frames(candidate, clock);
        }

        // Время кадра ещё не пришло — придерживаем его до следующего опроса.
        if candidate.pts_sec > clock + tolerance {
            self.pending_frame = Some(candidate);
            return None;
        }

        loop {
            match self.decoder.try_recv_video() {
                Ok(next) => {
                    if next.pts_sec <= clock + tolerance {
                        self.stats.on_dropped();
                        candidate = next;
                    } else {
                        self.pending_frame = Some(next);
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        self.last_frame_at = Instant::now();
        self.stats.on_shown(candidate.pts_sec, clock);
        // Кадр в буфере кодека показывается сейчас, в момент выбора.
        if let Some(surface) = candidate.surface.as_ref() {
            surface.render();
        }
        Some(candidate)
    }

    /// Кадры на Surface (MediaCodec): показ планируется кодеку на
    /// `SURFACE_LOOKAHEAD_SEC` вперёд по мастер-часам — UI-поток не обязан
    /// тикать на каждый кадр. Опоздавшие кадры возвращаются кодеку без
    /// показа. Возвращает последний запланированный кадр (без пикселей) —
    /// по нему `VideoView` узнаёт о режиме Surface.
    fn schedule_surface_frames(&mut self, first: VideoFrame, clock: f64) -> Option<VideoFrame> {
        const SURFACE_LOOKAHEAD_SEC: f64 = 0.4;
        const SURFACE_LATE_SEC: f64 = 0.08;
        #[cfg(target_os = "android")]
        let now_ns = crate::video::android::monotonic_ns();
        #[cfg(not(target_os = "android"))]
        let now_ns = 0i64;
        let mut last: Option<VideoFrame> = None;
        // Самый свежий опоздавший кадр: если вовремя не успел ни один
        // (после перемотки декодер отстаёт от аудио-часов), показываем его
        // сразу — иначе все кадры отбрасывались бы бесконечно и на экране
        // висела бы «буферизация» при идущем звуке.
        let mut late: Option<VideoFrame> = None;
        let mut frame = first;
        loop {
            if frame.pts_sec > clock + SURFACE_LOOKAHEAD_SEC {
                self.pending_frame = Some(frame);
                break;
            }
            if frame.pts_sec >= clock - SURFACE_LATE_SEC {
                if let Some(s) = frame.surface.as_ref() {
                    let delay_ns = ((frame.pts_sec - clock).max(0.0) * 1e9) as i64;
                    s.render_at(now_ns + delay_ns);
                }
                last = Some(frame);
            } else {
                // Предыдущий опоздавший — drop вернёт буфер кодеку без показа.
                late = Some(frame);
            }
            match self.decoder.try_recv_video() {
                Ok(next) => frame = next,
                Err(_) => break,
            }
        }
        if last.is_none() {
            if let Some(frame) = late {
                if let Some(s) = frame.surface.as_ref() {
                    s.render();
                }
                last = Some(frame);
            }
        }
        if let Some(f) = last.as_ref() {
            self.last_frame_at = Instant::now();
            self.stats.on_shown(f.pts_sec, clock);
        }
        last
    }

    /// Допуск «пора показывать»: половина периода опроса. Опрос идёт из
    /// `animate` виджета, то есть раз в кадр UI; период меряем сами, чтобы
    /// не зависеть от частоты экрана.
    fn poll_tolerance_sec(&mut self) -> f64 {
        let now = Instant::now();
        if let Some(prev) = self.last_poll_at {
            let dt = now.duration_since(prev).as_secs_f64();
            // Промежутки длиннее четверти секунды — не кадр UI, а пауза или
            // переключение экрана: в оценку периода они не идут.
            if dt > 0.0 && dt < 0.25 {
                self.poll_period_sec = if self.poll_period_sec > 0.0 {
                    self.poll_period_sec * 0.9 + dt * 0.1
                } else {
                    dt
                };
            }
        }
        self.last_poll_at = Some(now);
        (self.poll_period_sec * 0.5).clamp(MIN_TOLERANCE_SEC, MAX_TOLERANCE_SEC)
    }

    fn master_clock_sec(&self) -> f64 {
        if let Some(audio) = self.audio.as_ref() {
            if let Some(sec) = audio.playback_clock_sec() {
                return sec + self.shared.seek_offset_sec();
            }
        }
        let elapsed = match (self.wall_start, self.paused_at) {
            (Some(start), Some(at)) => {
                let raw = at.duration_since(start);
                raw.checked_sub(self.paused_accum).unwrap_or_default()
            }
            (Some(start), None) => start
                .elapsed()
                .checked_sub(self.paused_accum)
                .unwrap_or_default(),
            _ => WebDuration::ZERO,
        };
        elapsed.as_secs_f64() + self.shared.seek_offset_sec()
    }

    pub fn input_path(&self) -> &str {
        &self.input_path
    }

    pub fn install_video_tee(&self) -> Option<Arc<VideoStream>> {
        let rx = self.decoder.install_video_tee()?;
        let meta = self.decoder.meta();
        Some(VideoStream::from_channel(
            rx,
            meta.width,
            meta.height,
            meta.fps_estimate,
            meta.duration_sec,
        ))
    }

    pub fn install_audio_tee(&self) -> Option<Arc<AudioStream>> {
        let rx = self.decoder.install_audio_tee()?;
        let meta = self.decoder.meta();
        let sr = self.decoder.audio_output_sr();
        Some(AudioStream::from_channel(
            rx,
            sr,
            meta.audio_channels.max(1),
        ))
    }

    pub fn uninstall_tees(&self) {
        self.decoder.uninstall_tees();
    }
}

/// Диагностика равномерности показа кадров: `SYNGUI_VIDEO_STATS=1` — раз в
/// секунду в лог уходит сводка по интервалам между показанными кадрами,
/// опережению кадра относительно мастер-часов и шагу самих часов. Рывки
/// видео при ровном UI видно именно здесь: интервалы min/max вместо
/// одинаковых и «ступеньки» часов больше периода кадра.
struct FrameStats {
    enabled: bool,
    window_start: Instant,
    shown: u32,
    dropped: u32,
    polls: u32,
    clock_stalls: u32,
    last_shown_at: Option<Instant>,
    interval_ms: MinMaxSum,
    lead_ms: MinMaxSum,
    clock_step_ms: MinMaxSum,
    last_clock: f64,
}

#[derive(Default, Clone, Copy)]
struct MinMaxSum {
    min: f64,
    max: f64,
    sum: f64,
    n: u32,
}

impl MinMaxSum {
    fn add(&mut self, v: f64) {
        if self.n == 0 {
            self.min = v;
            self.max = v;
        } else {
            self.min = self.min.min(v);
            self.max = self.max.max(v);
        }
        self.sum += v;
        self.n += 1;
    }

    fn avg(&self) -> f64 {
        if self.n == 0 {
            0.0
        } else {
            self.sum / self.n as f64
        }
    }
}

impl FrameStats {
    fn new() -> Self {
        Self {
            enabled: std::env::var("SYNGUI_VIDEO_STATS").is_ok(),
            window_start: Instant::now(),
            shown: 0,
            dropped: 0,
            polls: 0,
            clock_stalls: 0,
            last_shown_at: None,
            interval_ms: MinMaxSum::default(),
            lead_ms: MinMaxSum::default(),
            clock_step_ms: MinMaxSum::default(),
            last_clock: f64::NAN,
        }
    }

    fn on_poll(&mut self, clock: f64) {
        if !self.enabled {
            return;
        }
        self.polls += 1;
        if self.last_clock.is_finite() {
            let step = (clock - self.last_clock) * 1000.0;
            if step.abs() < 0.000_5 {
                self.clock_stalls += 1;
            } else {
                self.clock_step_ms.add(step);
            }
        }
        self.last_clock = clock;
        if self.window_start.elapsed() >= WebDuration::from_secs(1) {
            self.report();
        }
    }

    fn on_shown(&mut self, pts_sec: f64, clock: f64) {
        if !self.enabled {
            return;
        }
        self.shown += 1;
        let now = Instant::now();
        if let Some(prev) = self.last_shown_at {
            self.interval_ms.add(now.duration_since(prev).as_secs_f64() * 1000.0);
        }
        self.last_shown_at = Some(now);
        self.lead_ms.add((pts_sec - clock) * 1000.0);
    }

    fn on_dropped(&mut self) {
        if self.enabled {
            self.dropped += 1;
        }
    }

    fn report(&mut self) {
        let secs = self.window_start.elapsed().as_secs_f64().max(0.001);
        log::info!(
            "[VIDEO STATS {secs:.1}s] показано={} ({:.1}/с) отброшено={} опросов={} часы стояли={}% \
             интервал(мс avg/min/max)={:.1}/{:.1}/{:.1} опережение(мс avg/min/max)={:.0}/{:.0}/{:.0} \
             шаг часов(мс avg/max)={:.1}/{:.1}",
            self.shown,
            self.shown as f64 / secs,
            self.dropped,
            self.polls,
            if self.polls > 0 {
                self.clock_stalls * 100 / self.polls
            } else {
                0
            },
            self.interval_ms.avg(),
            self.interval_ms.min,
            self.interval_ms.max,
            self.lead_ms.avg(),
            self.lead_ms.min,
            self.lead_ms.max,
            self.clock_step_ms.avg(),
            self.clock_step_ms.max,
        );
        self.window_start = Instant::now();
        self.shown = 0;
        self.dropped = 0;
        self.polls = 0;
        self.clock_stalls = 0;
        self.interval_ms = MinMaxSum::default();
        self.lead_ms = MinMaxSum::default();
        self.clock_step_ms = MinMaxSum::default();
    }
}
