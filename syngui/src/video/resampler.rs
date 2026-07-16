use ffmpeg_next::format::sample::{Sample, Type};
use ffmpeg_next::frame;
use ffmpeg_next::software::resampling::Context as ResampleCtx;
use ffmpeg_next::ChannelLayout;

use super::error::VideoError;

pub struct Resampler {
    ctx: ResampleCtx,
    out_rate: u32,
}

impl Resampler {
    pub fn new(
        in_fmt: Sample,
        in_layout: ChannelLayout,
        in_rate: u32,
        out_rate: u32,
    ) -> Result<Self, VideoError> {
        if in_rate == 0 || out_rate == 0 {
            return Err(VideoError::Resampler(format!(
                "нулевой sample rate: in {in_rate} → out {out_rate}"
            )));
        }
        let ctx = ResampleCtx::get(
            in_fmt,
            in_layout,
            in_rate,
            Sample::F32(Type::Packed),
            ChannelLayout::MONO,
            out_rate,
        )
        .map_err(|e| VideoError::Resampler(format!("swr_alloc_set_opts: {e}")))?;
        Ok(Self { ctx, out_rate })
    }

    pub fn out_rate(&self) -> u32 {
        self.out_rate
    }

    pub fn convert(&mut self, frame: &frame::Audio) -> Result<Vec<f32>, VideoError> {
        let mut out = frame::Audio::empty();
        self.ctx
            .run(frame, &mut out)
            .map_err(|e| VideoError::Resampler(format!("swr_convert: {e}")))?;
        let samples = out.samples();
        if samples == 0 {
            return Ok(Vec::new());
        }
        let plane = out.plane::<f32>(0);
        Ok(plane.to_vec())
    }

    pub fn flush(&mut self) -> Result<Vec<f32>, VideoError> {
        let mut out = frame::Audio::empty();
        self.ctx
            .flush(&mut out)
            .map_err(|e| VideoError::Resampler(format!("swr_flush: {e}")))?;
        if out.samples() == 0 {
            return Ok(Vec::new());
        }
        Ok(out.plane::<f32>(0).to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_resample_basic() {
        ffmpeg_next::init().ok();
        let in_rate = 48_000u32;
        let out_rate = 44_100u32;
        let n = 1024usize;
        let mut frame_in = frame::Audio::new(Sample::F32(Type::Packed), n, ChannelLayout::MONO);
        unsafe {
            (*frame_in.as_mut_ptr()).sample_rate = in_rate as i32;
        }
        {
            let plane = frame_in.plane_mut::<f32>(0);
            for (i, s) in plane.iter_mut().enumerate() {
                let t = i as f32 / in_rate as f32;
                *s = (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            }
        }

        let mut r = Resampler::new(
            Sample::F32(Type::Packed),
            ChannelLayout::MONO,
            in_rate,
            out_rate,
        )
        .expect("new");
        let out = r.convert(&frame_in).expect("convert");
        assert!(!out.is_empty(), "resampler не должен вернуть пустой выход");
        let expected = (n as f64 * out_rate as f64 / in_rate as f64) as usize;
        assert!(
            out.len() <= expected + n / 5 + 1,
            "длина out={} больше ожидаемой {} (+padding)",
            out.len(),
            expected
        );
        assert!(out.iter().any(|&s| s.abs() > 0.01));
    }
}
