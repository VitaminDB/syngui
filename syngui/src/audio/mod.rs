pub mod dsp;
mod player;
mod recorder;
mod ring;
pub mod session;
mod stream;
pub mod wav;

pub use dsp::{Biquad, BiquadMode, LinearGain, SchroederReverb};
pub use player::{AudioPlayer, GrowingWriter};
pub use recorder::{list_input_devices, AudioError, AudioRecorder, VisHandle};
pub use session::{RecordingOptions, RecordingResult, RecordingSession, RecordingState};
pub use stream::AudioStream;
pub use wav::WavStreamWriter;

use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct AudioBuffer {
    pub pcm: Arc<[f32]>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl PartialEq for AudioBuffer {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pcm, &other.pcm)
            && self.sample_rate == other.sample_rate
            && self.channels == other.channels
    }
}

impl Eq for AudioBuffer {}

impl AudioBuffer {
    pub fn new(pcm: Arc<[f32]>, sample_rate: u32, channels: u16) -> Self {
        Self {
            pcm,
            sample_rate,
            channels: channels.max(1),
        }
    }

    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        let frames = self.pcm.len() / self.channels.max(1) as usize;
        frames as f64 / self.sample_rate as f64
    }

    pub fn frames(&self) -> usize {
        self.pcm.len() / self.channels.max(1) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.pcm.is_empty()
    }
}

pub fn compute_rms_bins(pcm: &[f32], channels: u16, bins: usize) -> Vec<f32> {
    if pcm.is_empty() || bins == 0 {
        return vec![0.0; bins.max(1)];
    }
    let ch = channels.max(1) as usize;
    let frames = pcm.len() / ch;
    if frames == 0 {
        return vec![0.0; bins];
    }
    let chunk = (frames / bins.max(1)).max(1);
    let mut out = Vec::with_capacity(bins);
    for i in 0..bins {
        let start_frame = i * chunk;
        let end_frame = (start_frame + chunk).min(frames);
        if start_frame >= end_frame {
            out.push(0.0);
            continue;
        }
        let mut sum = 0.0_f64;
        let mut count = 0_usize;
        for f in start_frame..end_frame {
            let mut s = 0.0_f64;
            for c in 0..ch {
                let idx = f * ch + c;
                s += pcm.get(idx).copied().unwrap_or(0.0) as f64;
            }
            s /= ch as f64;
            sum += s * s;
            count += 1;
        }
        let rms = (sum / count.max(1) as f64).sqrt();
        out.push(rms.clamp(0.0, 1.0) as f32);
    }
    let max = out.iter().fold(0.0_f32, |a, &b| a.max(b));
    if max > f32::EPSILON {
        for v in &mut out {
            *v /= max;
        }
    }
    out
}

#[cfg(test)]
mod buffer_tests {
    use super::*;

    #[test]
    fn rms_bins_empty_returns_zeros() {
        let bins = compute_rms_bins(&[], 1, 8);
        assert_eq!(bins, vec![0.0_f32; 8]);
    }

    #[test]
    fn rms_bins_constant_signal_normalizes_to_one() {
        let pcm: Vec<f32> = vec![0.5; 1024];
        let bins = compute_rms_bins(&pcm, 1, 16);
        assert_eq!(bins.len(), 16);
        for v in &bins {
            assert!((*v - 1.0).abs() < 1e-5, "expected ~1.0, got {v}");
        }
    }

    #[test]
    fn rms_bins_silence_zero() {
        let pcm: Vec<f32> = vec![0.0; 1024];
        let bins = compute_rms_bins(&pcm, 1, 8);
        for v in &bins {
            assert_eq!(*v, 0.0);
        }
    }

    #[test]
    fn audio_buffer_duration() {
        let pcm: Arc<[f32]> = Arc::from(vec![0.0; 48000].into_boxed_slice());
        let buf = AudioBuffer::new(pcm, 48000, 1);
        assert!((buf.duration_seconds() - 1.0).abs() < 1e-6);
    }
}
