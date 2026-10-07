//! Проигрыватель аудиофайлов на ffmpeg: любой формат, который знает libavformat/libavcodec (MP3, FLAC, Ogg
//! Vorbis/Opus, AAC/M4A, WAV, WMA, APE, …), потоком — без загрузки трека в память, в стерео.
//!
//! Декодер работает в своём потоке: демуксер → кодек → swresample (f32 interleaved, стерео, 48 кГц) → канал →
//! [`AudioPlayer::start_streaming_channels`]. Перемотка перезапускает поток декодера с нужной позиции (старый
//! вывод останавливается сразу, без хвоста буфера). Пауза и громкость — на выводе.
//!
//! [`probe_audio`] читает теги (название, исполнитель, альбом), длительность и обложку (вложенная картинка —
//! байты JPEG/PNG как есть) без воспроизведения.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use ffmpeg_next::ffi;
use ffmpeg_next::format::sample::{Sample, Type};
use ffmpeg_next::media::Type as MediaType;
use ffmpeg_next::software::resampling::Context as ResampleCtx;
use ffmpeg_next::{frame, ChannelLayout};

use super::error::VideoError;
use crate::audio::AudioPlayer;

const OUT_RATE: u32 = 48_000;
const AV_TIME_BASE: f64 = 1_000_000.0;

/// Сведения о треке.
#[derive(Clone, Debug, Default)]
pub struct AudioTrackMeta {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Номер дорожки на альбоме («3» или «3/12» — как в теге).
    pub track: Option<String>,
    pub duration_sec: f64,
    /// Обложка — содержимое вложенной картинки (обычно JPEG или PNG).
    pub cover: Option<Vec<u8>>,
}

fn tag(ictx: &ffmpeg_next::format::context::Input, keys: &[&str]) -> Option<String> {
    let find = |d: ffmpeg_next::DictionaryRef| {
        for (k, v) in d.iter() {
            if keys.iter().any(|w| k.eq_ignore_ascii_case(w)) && !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
        None
    };
    // теги контейнера, иначе — аудиопотока (Ogg/Opus хранят их в потоке)
    find(ictx.metadata()).or_else(|| ictx.streams().best(MediaType::Audio).and_then(|s| find(s.metadata())))
}

fn read_meta(ictx: &ffmpeg_next::format::context::Input) -> AudioTrackMeta {
    let duration_sec = if ictx.duration() > 0 { ictx.duration() as f64 / AV_TIME_BASE } else { 0.0 };
    let mut cover = None;
    for s in ictx.streams() {
        // SAFETY: поток жив вместе с ictx; attached_pic — пакет внутри AVStream
        unsafe {
            let st = s.as_ptr();
            if (*st).disposition & ffi::AV_DISPOSITION_ATTACHED_PIC as i32 != 0 {
                let pkt = &(*st).attached_pic;
                if !pkt.data.is_null() && pkt.size > 0 {
                    cover = Some(std::slice::from_raw_parts(pkt.data, pkt.size as usize).to_vec());
                    break;
                }
            }
        }
    }
    AudioTrackMeta {
        title: tag(ictx, &["title"]),
        artist: tag(ictx, &["artist", "album_artist", "performer"]),
        album: tag(ictx, &["album"]),
        track: tag(ictx, &["track", "tracknumber"]),
        duration_sec,
        cover,
    }
}

/// Теги, длительность и обложка файла (без воспроизведения).
pub fn probe_audio(path: &str) -> Result<AudioTrackMeta, VideoError> {
    ffmpeg_next::init().ok();
    let ictx = ffmpeg_next::format::input(&path.to_string()).map_err(|e| VideoError::Open(format!("{path}: {e}")))?;
    if ictx.streams().best(MediaType::Audio).is_none() {
        return Err(VideoError::Other(format!("{path}: нет аудиопотока")));
    }
    Ok(read_meta(&ictx))
}

/// Поток декодера: файл с позиции `start_sec` → стерео-чанки в `tx`, пока жив приёмник и не поднят `stop`.
fn decode_thread(path: String, start_sec: f64, tx: mpsc::SyncSender<Vec<f32>>, stop: Arc<AtomicBool>) -> Result<(), VideoError> {
    let mut ictx = ffmpeg_next::format::input(&path).map_err(|e| VideoError::Open(format!("{path}: {e}")))?;
    let stream = ictx.streams().best(MediaType::Audio).ok_or_else(|| VideoError::Other("нет аудиопотока".into()))?;
    let idx = stream.index();
    let tb = stream.time_base();
    let tb_sec = tb.numerator() as f64 / tb.denominator().max(1) as f64;
    let mut dec = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| VideoError::DecoderInit(format!("audio params: {e}")))?
        .decoder()
        .audio()
        .map_err(|e| VideoError::DecoderInit(format!("audio decoder: {e}")))?;
    if start_sec > 0.0 {
        let ts = (start_sec * AV_TIME_BASE) as i64;
        ictx.seek(ts, ..ts).map_err(|e| VideoError::Seek(format!("{start_sec:.2} с: {e}")))?;
    }
    let mut swr: Option<ResampleCtx> = None;
    let mut decoded = frame::Audio::empty();
    // после перемотки на ключевой пакет — отбросить сэмплы раньше цели (точная позиция)
    let mut skip_until = if start_sec > 0.0 { Some(start_sec) } else { None };
    let send = |swr: &mut Option<ResampleCtx>, f: &frame::Audio, skip: &mut Option<f64>| -> bool {
        if swr.is_none() {
            let layout = if f.channel_layout().is_empty() { ChannelLayout::default(f.channels() as i32) } else { f.channel_layout() };
            match ResampleCtx::get(f.format(), layout, f.rate(), Sample::F32(Type::Packed), ChannelLayout::STEREO, OUT_RATE) {
                Ok(c) => *swr = Some(c),
                Err(e) => {
                    log::warn!("audio: swr init: {e}");
                    return false;
                }
            }
        }
        let ctx = swr.as_mut().unwrap();
        // SAFETY: контекст инициализирован, функция только считает
        let cap = unsafe { ffi::swr_get_out_samples(ctx.as_mut_ptr(), f.samples() as i32) }.max(f.samples() as i32).max(1);
        let mut out = frame::Audio::new(Sample::F32(Type::Packed), cap as usize, ChannelLayout::STEREO);
        if ctx.run(f, &mut out).is_err() || out.samples() == 0 {
            return true;
        }
        let n = out.samples() * 2;
        // SAFETY: packed f32 стерео — плоскость 0 длиной samples*2
        let data: &[f32] = unsafe { std::slice::from_raw_parts(out.data(0).as_ptr() as *const f32, n) };
        let mut data = data;
        if let Some(target) = *skip {
            let pts = f.pts().map(|p| p as f64 * tb_sec);
            match pts {
                Some(t) if t + f.samples() as f64 / f.rate().max(1) as f64 <= target => return true,
                Some(t) if t < target => {
                    let drop_frames = (((target - t) * OUT_RATE as f64) as usize).min(n / 2);
                    data = &data[drop_frames * 2..];
                }
                _ => {}
            }
            *skip = None;
        }
        !data.is_empty() && tx.send(data.to_vec()).is_ok() || data.is_empty()
    };
    let mut packet = ffmpeg_next::Packet::empty();
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        match packet.read(&mut ictx) {
            Ok(()) => {
                if packet.stream() != idx {
                    continue;
                }
                if dec.send_packet(&packet).is_err() {
                    continue;
                }
                while dec.receive_frame(&mut decoded).is_ok() {
                    if !send(&mut swr, &decoded, &mut skip_until) {
                        return Ok(());
                    }
                }
            }
            Err(ffmpeg_next::Error::Eof) => break,
            Err(ffmpeg_next::Error::Other { errno: libc_eagain }) if libc_eagain == 11 => continue,
            Err(e) => {
                log::warn!("audio: read: {e}");
                break;
            }
        }
    }
    let _ = dec.send_eof();
    while dec.receive_frame(&mut decoded).is_ok() {
        if !send(&mut swr, &decoded, &mut skip_until) {
            return Ok(());
        }
    }
    Ok(())
}

/// Проигрыватель аудиофайла: пауза, перемотка, громкость, позиция, конец трека.
pub struct AudioFilePlayer {
    path: String,
    meta: AudioTrackMeta,
    audio: Option<AudioPlayer>,
    stop: Arc<AtomicBool>,
    /// Позиция начала текущего запуска декодера (после перемотки).
    offset_sec: f64,
    paused: bool,
    volume: f32,
}

impl AudioFilePlayer {
    /// Открыть и сразу начать играть.
    pub fn open(path: &str) -> Result<Self, VideoError> {
        let meta = probe_audio(path)?;
        let mut p = Self {
            path: path.to_string(),
            meta,
            audio: None,
            stop: Arc::new(AtomicBool::new(false)),
            offset_sec: 0.0,
            paused: false,
            volume: 1.0,
        };
        p.start(0.0)?;
        Ok(p)
    }

    fn start(&mut self, at: f64) -> Result<(), VideoError> {
        self.stop.store(true, Ordering::Release);
        if let Some(a) = self.audio.take() {
            a.stop();
        }
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = stop.clone();
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(16);
        let path = self.path.clone();
        thread::Builder::new()
            .name("syngui-audio-file".into())
            .spawn(move || {
                if let Err(e) = decode_thread(path, at, tx, stop) {
                    log::warn!("audio: {e}");
                }
            })
            .map_err(|e| VideoError::Other(format!("spawn decoder: {e}")))?;
        let a = AudioPlayer::start_streaming_channels(rx, OUT_RATE, 2)?;
        a.set_volume(self.volume);
        if self.paused {
            a.pause();
        }
        self.audio = Some(a);
        self.offset_sec = at;
        Ok(())
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn meta(&self) -> &AudioTrackMeta {
        &self.meta
    }

    pub fn duration_sec(&self) -> f64 {
        self.meta.duration_sec
    }

    pub fn position_sec(&self) -> f64 {
        let p = self.offset_sec + self.audio.as_ref().map(|a| a.position_seconds()).unwrap_or(0.0);
        if self.meta.duration_sec > 0.0 {
            p.min(self.meta.duration_sec)
        } else {
            p
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn play(&mut self) {
        self.paused = false;
        if let Some(a) = &self.audio {
            a.resume();
        }
    }

    pub fn pause(&mut self) {
        self.paused = true;
        if let Some(a) = &self.audio {
            a.pause();
        }
    }

    /// Перемотка: декодер перезапускается с позиции `sec`.
    pub fn seek(&mut self, sec: f64) -> Result<(), VideoError> {
        let max = if self.meta.duration_sec > 0.0 { self.meta.duration_sec } else { f64::MAX };
        self.start(sec.clamp(0.0, max))
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 2.0);
        if let Some(a) = &self.audio {
            a.set_volume(self.volume);
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Трек доигран (источник закрыт и всё отдано устройству).
    pub fn is_ended(&self) -> bool {
        self.audio.as_ref().is_none_or(|a| a.is_done())
    }

    /// Ошибка устройства вывода, если была.
    pub fn error(&self) -> Option<String> {
        self.audio.as_ref().and_then(|a| a.error())
    }
}

impl Drop for AudioFilePlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(a) = self.audio.take() {
            a.stop();
        }
    }
}
