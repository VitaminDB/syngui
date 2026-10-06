//! Миниатюра видео: один кадр около заданной доли длительности, уменьшенный
//! до `max` пикселей по большей стороне и повёрнутый по матрице отображения
//! (видео камер телефонов). Без потоков декодера и звука — для списков файлов.

use ffmpeg_next::format::Pixel;
use ffmpeg_next::media::Type as MediaType;
use ffmpeg_next::software::scaling::{Context as SwsContext, Flags};
use ffmpeg_next::{ffi, frame};

use super::decoder::display_rotation;
use super::error::VideoError;

/// Кадр-миниатюра в RGBA (строки без выравнивания) и сведения о ролике.
#[derive(Clone, Debug)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Длительность ролика, с (0 — неизвестна).
    pub duration_sec: f64,
    /// Размер кадра при показе (с учётом поворота).
    pub video_width: u32,
    pub video_height: u32,
}

/// Миниатюра кадра около `at` (доля длительности, 0..1; у коротких роликов — первый ключевой кадр).
pub fn thumbnail(input: &str, max: u32, at: f64) -> Result<Thumbnail, VideoError> {
    ffmpeg_next::init().ok();
    let mut ictx = ffmpeg_next::format::input(&input.to_string())
        .map_err(|e| VideoError::Open(format!("{input}: {e}")))?;
    let stream = ictx.streams().best(MediaType::Video).ok_or(VideoError::NoVideoStream)?;
    let idx = stream.index();
    let rotation = display_rotation(&stream.parameters());
    let mut ctx = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| VideoError::DecoderInit(format!("video params: {e}")))?;
    // SAFETY: контекст ещё не открыт; потоки декодера — по числу ядер.
    unsafe { (*ctx.as_mut_ptr()).thread_count = 0 };
    let mut dec = ctx
        .decoder()
        .video()
        .map_err(|e| VideoError::DecoderInit(format!("video decoder: {e}")))?;
    let duration_sec = if ictx.duration() > 0 { ictx.duration() as f64 / ffi::AV_TIME_BASE as f64 } else { 0.0 };
    if duration_sec > 2.0 {
        let ts = (duration_sec * at.clamp(0.0, 0.95) * ffi::AV_TIME_BASE as f64) as i64;
        // Ключевой кадр до цели: его хватает для миниатюры.
        let _ = ictx.seek(ts, ..ts);
    }

    let mut decoded = frame::Video::empty();
    let mut got = false;
    let mut packets = 0;
    for (s, packet) in ictx.packets() {
        if s.index() != idx {
            continue;
        }
        packets += 1;
        if dec.send_packet(&packet).is_ok() && dec.receive_frame(&mut decoded).is_ok() {
            got = true;
            break;
        }
        if packets > 300 {
            break;
        }
    }
    if !got {
        let _ = dec.send_eof();
        got = dec.receive_frame(&mut decoded).is_ok();
    }
    if !got {
        return Err(VideoError::Other(format!("{input}: нет ни одного кадра")));
    }

    let (sw, sh) = (decoded.width(), decoded.height());
    let k = (max as f64 / sw.max(sh).max(1) as f64).min(1.0);
    let (tw, th) = (((sw as f64 * k).round() as u32).max(1), ((sh as f64 * k).round() as u32).max(1));
    let mut sws = SwsContext::get(decoded.format(), sw, sh, Pixel::RGBA, tw, th, Flags::BILINEAR)
        .map_err(|e| VideoError::Scaler(e.to_string()))?;
    let mut out = frame::Video::empty();
    sws.run(&decoded, &mut out).map_err(|e| VideoError::Scaler(e.to_string()))?;
    let stride = out.stride(0);
    let data = out.data(0);
    let mut rgba = Vec::with_capacity((tw * th * 4) as usize);
    for y in 0..th as usize {
        rgba.extend_from_slice(&data[y * stride..y * stride + tw as usize * 4]);
    }
    let (width, height, rgba) = rotate(tw, th, rgba, rotation);
    let (video_width, video_height) = if rotation % 180 == 90 { (sh, sw) } else { (sw, sh) };
    Ok(Thumbnail { width, height, rgba, duration_sec, video_width, video_height })
}

/// Поворот RGBA по часовой на 0/90/180/270°.
fn rotate(w: u32, h: u32, src: Vec<u8>, cw: u32) -> (u32, u32, Vec<u8>) {
    if cw == 0 {
        return (w, h, src);
    }
    let (w, h) = (w as usize, h as usize);
    let (nw, nh) = if cw == 180 { (w, h) } else { (h, w) };
    let mut dst = vec![0u8; src.len()];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match cw {
                90 => (h - 1 - y, x),
                180 => (w - 1 - x, h - 1 - y),
                _ => (y, w - 1 - x),
            };
            let s = (y * w + x) * 4;
            let d = (ny * nw + nx) * 4;
            dst[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    (nw as u32, nh as u32, dst)
}

#[cfg(test)]
mod tests {
    use super::rotate;

    #[test]
    fn rotate_90_moves_top_left_to_top_right() {
        // 2×1: красный, зелёный → 1×2: красный сверху
        let src = vec![255, 0, 0, 255, 0, 255, 0, 255];
        let (w, h, d) = rotate(2, 1, src.clone(), 90);
        assert_eq!((w, h), (1, 2));
        assert_eq!(&d[0..4], &src[0..4]);
        let (_, _, d) = rotate(2, 1, src.clone(), 270);
        assert_eq!(&d[0..4], &src[4..8]);
    }
}
