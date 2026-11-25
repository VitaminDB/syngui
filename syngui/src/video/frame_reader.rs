//! Кадры файла по времени — для монтажа, экспорта, превью и анализа: без
//! потоков и звука, кадр в RGBA нужного размера (с учётом поворота камеры).
//! Запросы по возрастанию времени декодируются подряд; шаг назад или далеко
//! вперёд — перемотка к ключевому кадру.
//!
//! ```ignore
//! let mut r = FrameReader::open("a.mp4", 1920, 1080)?;
//! let f = r.frame_at(3.5)?; // RGBA f.width × f.height
//! ```

use std::sync::Arc;

use ffmpeg_next::format::Pixel;
use ffmpeg_next::media::Type as MediaType;
use ffmpeg_next::software::scaling::{Context as SwsContext, Flags};
use ffmpeg_next::{ffi, frame};

use super::decoder::display_rotation;
use super::error::VideoError;

/// Кадр RGBA (строки без выравнивания) и его время в файле, с.
#[derive(Clone, Debug)]
pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    pub pts: f64,
    pub rgba: Arc<Vec<u8>>,
}

/// Дальше этого вперёд — перемотка, а не декодирование подряд, с.
const SEEK_AHEAD: f64 = 3.0;

pub struct FrameReader {
    ictx: ffmpeg_next::format::context::Input,
    idx: usize,
    dec: ffmpeg_next::decoder::Video,
    tb: f64,
    start: f64,
    rotation: u32,
    max: (u32, u32),
    sws: Option<(SwsContext, Pixel, u32, u32, u32, u32)>,
    /// Последний выданный кадр и следующий за ним (уже декодированный).
    cur: Option<RgbaFrame>,
    next: Option<RgbaFrame>,
    eof: bool,
    pub duration: f64,
    pub fps: f64,
}

impl FrameReader {
    /// Открыть файл; кадры вписываются в `max_w × max_h` (не увеличиваются).
    pub fn open(input: &str, max_w: u32, max_h: u32) -> Result<Self, VideoError> {
        ffmpeg_next::init().ok();
        let ictx = ffmpeg_next::format::input(&input.to_string()).map_err(|e| VideoError::Open(format!("{input}: {e}")))?;
        let stream = ictx.streams().best(MediaType::Video).ok_or(VideoError::NoVideoStream)?;
        let idx = stream.index();
        let rotation = display_rotation(&stream.parameters());
        let tb = f64::from(stream.time_base());
        let rate = stream.avg_frame_rate();
        let fps = if rate.denominator() != 0 { f64::from(rate) } else { 0.0 };
        let start = if stream.start_time() != ffi::AV_NOPTS_VALUE { stream.start_time() as f64 * tb } else { 0.0 };
        let mut ctx = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters()).map_err(|e| VideoError::DecoderInit(format!("video params: {e}")))?;
        // SAFETY: контекст ещё не открыт; потоки декодера — по числу ядер.
        unsafe { (*ctx.as_mut_ptr()).thread_count = 0 };
        let dec = ctx.decoder().video().map_err(|e| VideoError::DecoderInit(format!("video decoder: {e}")))?;
        let duration = if ictx.duration() > 0 { ictx.duration() as f64 / ffi::AV_TIME_BASE as f64 } else { 0.0 };
        Ok(Self { ictx, idx, dec, tb, start, rotation, max: (max_w.max(1), max_h.max(1)), sws: None, cur: None, next: None, eof: false, duration, fps })
    }

    fn seek(&mut self, t: f64) {
        let ts = ((t.max(0.0) + self.start) * ffi::AV_TIME_BASE as f64) as i64;
        let _ = self.ictx.seek(ts, ..ts);
        self.dec.flush();
        self.cur = None;
        self.next = None;
        self.eof = false;
    }

    /// Следующий кадр файла или `None` в конце.
    fn decode_next(&mut self) -> Result<Option<RgbaFrame>, VideoError> {
        let mut decoded = frame::Video::empty();
        loop {
            if self.dec.receive_frame(&mut decoded).is_ok() {
                return self.convert(&decoded).map(Some);
            }
            if self.eof {
                return Ok(None);
            }
            match self.ictx.packets().next() {
                Some((s, packet)) => {
                    if s.index() == self.idx {
                        let _ = self.dec.send_packet(&packet);
                    }
                }
                None => {
                    let _ = self.dec.send_eof();
                    self.eof = true;
                }
            }
        }
    }

    fn convert(&mut self, f: &frame::Video) -> Result<RgbaFrame, VideoError> {
        let pts = f.timestamp().or(f.pts()).map(|p| p as f64 * self.tb - self.start).unwrap_or(0.0);
        let (sw, sh) = (f.width(), f.height());
        // размер с учётом поворота: вписываем повёрнутый кадр
        let (vw, vh) = if self.rotation % 180 == 90 { (sh, sw) } else { (sw, sh) };
        let k = (self.max.0 as f64 / vw.max(1) as f64).min(self.max.1 as f64 / vh.max(1) as f64).min(1.0);
        let (tw, th) = if self.rotation % 180 == 90 {
            (((sw as f64 * k).round() as u32).max(2) & !1, ((sh as f64 * k).round() as u32).max(2) & !1)
        } else {
            (((vw as f64 * k).round() as u32).max(2) & !1, ((vh as f64 * k).round() as u32).max(2) & !1)
        };
        let fresh = !matches!(&self.sws, Some((_, fmt, w, h, ow, oh)) if *fmt == f.format() && *w == sw && *h == sh && *ow == tw && *oh == th);
        if fresh {
            let ctx = SwsContext::get(f.format(), sw, sh, Pixel::RGBA, tw, th, Flags::BILINEAR).map_err(|e| VideoError::Scaler(e.to_string()))?;
            self.sws = Some((ctx, f.format(), sw, sh, tw, th));
        }
        let mut out = frame::Video::empty();
        self.sws.as_mut().unwrap().0.run(f, &mut out).map_err(|e| VideoError::Scaler(e.to_string()))?;
        let stride = out.stride(0);
        let data = out.data(0);
        let mut rgba = Vec::with_capacity((tw * th * 4) as usize);
        for y in 0..th as usize {
            rgba.extend_from_slice(&data[y * stride..y * stride + tw as usize * 4]);
        }
        let (width, height, rgba) = super::thumbnail::rotate(tw, th, rgba, self.rotation);
        Ok(RgbaFrame { width, height, pts, rgba: Arc::new(rgba) })
    }

    /// Кадр, видимый в момент `t` (с от начала файла): последний с `pts ≤ t`.
    pub fn frame_at(&mut self, t: f64) -> Result<RgbaFrame, VideoError> {
        let half = if self.fps > 0.0 { 0.5 / self.fps } else { 0.02 };
        self.frame_within(t, half)
    }

    /// Последний кадр не позже `t` (а не ближайший): для интерполяции между
    /// ним и [`FrameReader::peek_next`] — у ближайшего в середине между
    /// кадрами уже следующий, и промежуточный кадр не построить.
    pub fn frame_floor(&mut self, t: f64) -> Result<RgbaFrame, VideoError> {
        self.frame_within(t, 1e-6)
    }

    fn frame_within(&mut self, t: f64, half: f64) -> Result<RgbaFrame, VideoError> {
        let back = self.cur.as_ref().is_some_and(|c| t + half < c.pts);
        let far = self.cur.as_ref().map_or(t > SEEK_AHEAD, |c| t - c.pts > SEEK_AHEAD);
        if back || far {
            self.seek(t);
        }
        loop {
            if self.next.is_none() {
                self.next = self.decode_next()?;
            }
            match &self.next {
                Some(n) if n.pts <= t + half || self.cur.is_none() => {
                    self.cur = self.next.take();
                }
                _ => break,
            }
        }
        self.cur.clone().ok_or_else(|| VideoError::Other("нет кадров".into()))
    }
}

// SAFETY: контексты FFmpeg (формат, декодер, масштабирование) можно переносить
// между потоками, если ими не пользуются одновременно; `FrameReader` берут
// через `&mut self`, одновременного доступа нет.
unsafe impl Send for FrameReader {}

impl FrameReader {
    /// Кадр после последнего выданного [`FrameReader::frame_at`] (для
    /// интерполяции между соседними кадрами); `None` — конец файла.
    pub fn peek_next(&mut self) -> Option<RgbaFrame> {
        if self.next.is_none() {
            self.next = self.decode_next().ok().flatten();
        }
        self.next.clone()
    }
}
