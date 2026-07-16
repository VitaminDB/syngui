use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use web_time::Duration as WebDuration;

use super::decoder::{VideoDecoder, VideoFrame, VideoMeta};
use super::error::VideoError;
use super::hwaccel::HwAccel;
use super::stream::VideoStream;
use crate::audio::{AudioPlayer, AudioStream};

const EARLY_TOLERANCE_SEC: f64 = 0.005;

const LATE_PEEK_SEC: f64 = 0.100;

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
}

impl VideoPlayer {
    pub fn open(input: &str) -> Result<Self, VideoError> {
        Self::open_with_hwaccel(input, HwAccel::None)
    }

    pub fn open_with_hwaccel(input: &str, accel: HwAccel) -> Result<Self, VideoError> {
        let mut decoder = VideoDecoder::open_with_hwaccel(input, accel)?;
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
        self.shared.volume_bits.store(v.to_bits(), Ordering::Relaxed);
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
        self.wall_start = Some(Instant::now());
        self.paused_accum = WebDuration::ZERO;
        if self.is_paused() {
            self.paused_at = Some(Instant::now());
        } else {
            self.paused_at = None;
        }
        Ok(())
    }

    pub fn poll_frame(&mut self) -> Option<VideoFrame> {
        if self.is_paused() {
            return None;
        }
        let clock = self.master_clock_sec();

        let mut candidate = self
            .pending_frame
            .take()
            .or_else(|| self.decoder.try_recv_video().ok())?;

        if candidate.pts_sec > clock + LATE_PEEK_SEC {
            self.pending_frame = Some(candidate);
            return None;
        }

        loop {
            match self.decoder.try_recv_video() {
                Ok(next) => {
                    if next.pts_sec <= clock + EARLY_TOLERANCE_SEC {
                        candidate = next;
                    } else {
                        self.pending_frame = Some(next);
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        Some(candidate)
    }

    fn master_clock_sec(&self) -> f64 {
        if let Some(audio) = self.audio.as_ref() {
            let played = audio.samples_played() as f64;
            let sr = audio.sample_rate() as f64;
            if sr > 0.0 {
                return played / sr + self.shared.seek_offset_sec();
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
        Some(AudioStream::from_channel(rx, sr, meta.audio_channels.max(1)))
    }

    pub fn uninstall_tees(&self) {
        self.decoder.uninstall_tees();
    }
}
