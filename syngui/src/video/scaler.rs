use std::sync::Arc;

use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame;
use ffmpeg_next::software::scaling::{Context, Flags};

use super::error::VideoError;

pub struct Scaler {
    ctx: Context,
    in_fmt: Pixel,
    in_w: u32,
    in_h: u32,
    out_w: u32,
    out_h: u32,
}

impl Scaler {
    pub fn new(
        in_fmt: Pixel,
        in_w: u32,
        in_h: u32,
        out_w: u32,
        out_h: u32,
    ) -> Result<Self, VideoError> {
        if in_w == 0 || in_h == 0 || out_w == 0 || out_h == 0 {
            return Err(VideoError::Scaler(format!(
                "нулевой размер: in {in_w}x{in_h} → out {out_w}x{out_h}"
            )));
        }
        let ctx = Context::get(in_fmt, in_w, in_h, Pixel::RGBA, out_w, out_h, Flags::BILINEAR)
            .map_err(|e| VideoError::Scaler(format!("sws_getContext: {e}")))?;
        Ok(Self {
            ctx,
            in_fmt,
            in_w,
            in_h,
            out_w,
            out_h,
        })
    }

    fn ensure_input(
        &mut self,
        in_fmt: Pixel,
        in_w: u32,
        in_h: u32,
    ) -> Result<(), VideoError> {
        if in_fmt == self.in_fmt && in_w == self.in_w && in_h == self.in_h {
            return Ok(());
        }
        log::debug!(
            "scaler: input изменился: {:?} {}x{} → {:?} {}x{}",
            self.in_fmt,
            self.in_w,
            self.in_h,
            in_fmt,
            in_w,
            in_h
        );
        self.ctx = Context::get(
            in_fmt,
            in_w,
            in_h,
            Pixel::RGBA,
            self.out_w,
            self.out_h,
            Flags::BILINEAR,
        )
        .map_err(|e| VideoError::Scaler(format!("sws_getContext (relink): {e}")))?;
        self.in_fmt = in_fmt;
        self.in_w = in_w;
        self.in_h = in_h;
        Ok(())
    }

    pub fn convert(&mut self, frame: &frame::Video) -> Result<Arc<[u8]>, VideoError> {
        self.ensure_input(frame.format(), frame.width(), frame.height())?;
        let mut rgba = frame::Video::empty();
        self.ctx
            .run(frame, &mut rgba)
            .map_err(|e| VideoError::Scaler(format!("sws_scale: {e}")))?;

        let stride = rgba.stride(0);
        let plane = rgba.data(0);
        let row_bytes = (self.out_w as usize) * 4;
        let mut out = Vec::with_capacity(row_bytes * (self.out_h as usize));
        for y in 0..(self.out_h as usize) {
            let off = y * stride;
            out.extend_from_slice(&plane[off..off + row_bytes]);
        }
        Ok(Arc::from(out.into_boxed_slice()))
    }

    pub fn out_size(&self) -> (u32, u32) {
        (self.out_w, self.out_h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ffmpeg_next::frame;

    #[test]
    fn yuv420p_to_rgba_basic() {
        ffmpeg_next::init().ok();
        let mut yuv = frame::Video::new(Pixel::YUV420P, 4, 4);
        for plane in 0..3 {
            let stride = yuv.stride(plane);
            let data = yuv.data_mut(plane);
            let val = if plane == 0 { 128u8 } else { 128u8 };
            for byte in data.iter_mut().take(stride * if plane == 0 { 4 } else { 2 }) {
                *byte = val;
            }
        }

        let mut scaler = Scaler::new(Pixel::YUV420P, 4, 4, 4, 4).expect("scaler");
        let rgba = scaler.convert(&yuv).expect("convert");
        assert_eq!(rgba.len(), 4 * 4 * 4, "RGBA должен быть 4×4×4 байт");
        for pixel in rgba.chunks_exact(4) {
            assert_eq!(pixel[3], 255, "alpha=255 для непрозрачной RGBA-конверсии");
        }
    }
}
