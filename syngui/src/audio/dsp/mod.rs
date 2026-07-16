use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug)]
pub struct LinearGain {
    pub linear: f32,
}

impl LinearGain {
    pub const UNITY: Self = Self { linear: 1.0 };

    pub fn new(linear: f32) -> Self {
        Self { linear }
    }

    pub fn from_db(db: f32) -> Self {
        Self {
            linear: 10.0_f32.powf(db / 20.0),
        }
    }

    #[inline]
    pub fn process(&self, x: f32) -> f32 {
        x * self.linear
    }

    pub fn process_slice(&self, samples: &mut [f32]) {
        for s in samples {
            *s *= self.linear;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BiquadMode {
    LowPass,
    HighPass,
    BandPass,
    Peaking,
    LowShelf,
    HighShelf,
}

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    cached: BiquadParams,
}

#[derive(Clone, Copy, Debug)]
struct BiquadParams {
    mode: BiquadMode,
    sample_rate: u32,
    cutoff_hz: f32,
    q: f32,
    gain_db: f32,
}

impl PartialEq for BiquadParams {
    fn eq(&self, other: &Self) -> bool {
        self.mode == other.mode
            && self.sample_rate == other.sample_rate
            && self.cutoff_hz.to_bits() == other.cutoff_hz.to_bits()
            && self.q.to_bits() == other.q.to_bits()
            && self.gain_db.to_bits() == other.gain_db.to_bits()
    }
}

impl Biquad {
    pub fn new() -> Self {
        let mut b = Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            cached: BiquadParams {
                mode: BiquadMode::LowPass,
                sample_rate: 0,
                cutoff_hz: 0.0,
                q: 0.0,
                gain_db: 0.0,
            },
        };
        b.update_coeffs(BiquadMode::LowPass, 48_000, 1_000.0, 0.707, 0.0);
        b
    }

    pub fn update_coeffs(
        &mut self,
        mode: BiquadMode,
        sample_rate: u32,
        cutoff_hz: f32,
        q: f32,
        gain_db: f32,
    ) {
        let new = BiquadParams { mode, sample_rate, cutoff_hz, q, gain_db };
        if new == self.cached {
            return;
        }
        self.cached = new;
        let sr = sample_rate.max(1) as f32;
        let f0 = cutoff_hz.clamp(10.0, sr * 0.5 - 10.0);
        let q = q.max(0.05);

        let omega = TAU * f0 / sr;
        let (sn, cs) = (omega.sin(), omega.cos());
        let alpha = sn / (2.0 * q);
        let a_amp = 10.0_f32.powf(gain_db / 40.0);

        let (b0, b1, b2, a0, a1, a2) = match mode {
            BiquadMode::LowPass => (
                (1.0 - cs) * 0.5,
                1.0 - cs,
                (1.0 - cs) * 0.5,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            BiquadMode::HighPass => (
                (1.0 + cs) * 0.5,
                -(1.0 + cs),
                (1.0 + cs) * 0.5,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            BiquadMode::BandPass => (
                sn * 0.5,
                0.0,
                -sn * 0.5,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            BiquadMode::Peaking => (
                1.0 + alpha * a_amp,
                -2.0 * cs,
                1.0 - alpha * a_amp,
                1.0 + alpha / a_amp,
                -2.0 * cs,
                1.0 - alpha / a_amp,
            ),
            BiquadMode::LowShelf => {
                let beta = 2.0 * a_amp.sqrt() * alpha;
                (
                    a_amp * ((a_amp + 1.0) - (a_amp - 1.0) * cs + beta),
                    2.0 * a_amp * ((a_amp - 1.0) - (a_amp + 1.0) * cs),
                    a_amp * ((a_amp + 1.0) - (a_amp - 1.0) * cs - beta),
                    (a_amp + 1.0) + (a_amp - 1.0) * cs + beta,
                    -2.0 * ((a_amp - 1.0) + (a_amp + 1.0) * cs),
                    (a_amp + 1.0) + (a_amp - 1.0) * cs - beta,
                )
            }
            BiquadMode::HighShelf => {
                let beta = 2.0 * a_amp.sqrt() * alpha;
                (
                    a_amp * ((a_amp + 1.0) + (a_amp - 1.0) * cs + beta),
                    -2.0 * a_amp * ((a_amp - 1.0) + (a_amp + 1.0) * cs),
                    a_amp * ((a_amp + 1.0) + (a_amp - 1.0) * cs - beta),
                    (a_amp + 1.0) - (a_amp - 1.0) * cs + beta,
                    2.0 * ((a_amp - 1.0) - (a_amp + 1.0) * cs),
                    (a_amp + 1.0) - (a_amp - 1.0) * cs - beta,
                )
            }
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    pub fn process_slice(&mut self, samples: &mut [f32]) {
        for s in samples {
            *s = self.process(*s);
        }
    }
}

impl Default for Biquad {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SchroederReverb {
    combs: [Comb; 4],
    allpasses: [Allpass; 2],
    mix: f32,
    room: f32,
    sample_rate: u32,
}

impl SchroederReverb {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(1);
        const COMB_LENS_44K1: [usize; 4] = [1116, 1188, 1277, 1356];
        const ALLPASS_LENS_44K1: [usize; 2] = [225, 556];
        let scale = sr as f32 / 44_100.0;
        let combs = [
            Comb::new(((COMB_LENS_44K1[0] as f32) * scale) as usize),
            Comb::new(((COMB_LENS_44K1[1] as f32) * scale) as usize),
            Comb::new(((COMB_LENS_44K1[2] as f32) * scale) as usize),
            Comb::new(((COMB_LENS_44K1[3] as f32) * scale) as usize),
        ];
        let allpasses = [
            Allpass::new(((ALLPASS_LENS_44K1[0] as f32) * scale) as usize),
            Allpass::new(((ALLPASS_LENS_44K1[1] as f32) * scale) as usize),
        ];
        let mut r = Self {
            combs,
            allpasses,
            mix: 0.0,
            room: 0.5,
            sample_rate: sr,
        };
        r.set_room(0.5);
        r
    }

    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    pub fn set_room(&mut self, room: f32) {
        let r = room.clamp(0.1, 0.95);
        self.room = r;
        let comb_feedback = r * 0.28 + 0.70;
        for c in &mut self.combs {
            c.set_feedback(comb_feedback);
        }
        for ap in &mut self.allpasses {
            ap.set_feedback(0.5);
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn reset(&mut self) {
        for c in &mut self.combs {
            c.reset();
        }
        for ap in &mut self.allpasses {
            ap.reset();
        }
    }

    #[inline]
    pub fn process(&mut self, dry: f32) -> f32 {
        let mut wet = 0.0;
        for c in &mut self.combs {
            wet += c.process(dry);
        }
        for ap in &mut self.allpasses {
            wet = ap.process(wet);
        }
        wet *= 0.25;
        dry * (1.0 - self.mix) + wet * self.mix
    }

    pub fn process_slice(&mut self, samples: &mut [f32]) {
        for s in samples {
            *s = self.process(*s);
        }
    }
}

struct Comb {
    buf: Vec<f32>,
    idx: usize,
    feedback: f32,
    damp1: f32,
    damp2: f32,
    filter_state: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        let l = len.max(1);
        Self {
            buf: vec![0.0; l],
            idx: 0,
            feedback: 0.84,
            damp1: 0.2,
            damp2: 0.8,
            filter_state: 0.0,
        }
    }
    fn set_feedback(&mut self, f: f32) {
        self.feedback = f.clamp(0.0, 0.99);
    }
    fn reset(&mut self) {
        for v in &mut self.buf {
            *v = 0.0;
        }
        self.filter_state = 0.0;
        self.idx = 0;
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let out = self.buf[self.idx];
        self.filter_state = out * self.damp2 + self.filter_state * self.damp1;
        self.buf[self.idx] = x + self.filter_state * self.feedback;
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    idx: usize,
    feedback: f32,
}

impl Allpass {
    fn new(len: usize) -> Self {
        let l = len.max(1);
        Self {
            buf: vec![0.0; l],
            idx: 0,
            feedback: 0.5,
        }
    }
    fn set_feedback(&mut self, f: f32) {
        self.feedback = f.clamp(0.0, 0.99);
    }
    fn reset(&mut self) {
        for v in &mut self.buf {
            *v = 0.0;
        }
        self.idx = 0;
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let buf_out = self.buf[self.idx];
        let out = -x + buf_out;
        self.buf[self.idx] = x + buf_out * self.feedback;
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let s: f32 = samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32;
        s.sqrt()
    }

    fn sine(freq: f32, sr: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (TAU * freq * i as f32 / sr as f32).sin())
            .collect()
    }

    #[test]
    fn gain_unity_passthrough() {
        let g = LinearGain::new(1.0);
        let mut buf = vec![0.1, -0.2, 0.3, -0.4];
        let orig = buf.clone();
        g.process_slice(&mut buf);
        assert_eq!(buf, orig);
    }

    #[test]
    fn gain_from_db_zero_is_unity() {
        let g = LinearGain::from_db(0.0);
        assert!((g.linear - 1.0).abs() < 1e-6);
    }

    #[test]
    fn gain_from_db_minus_six_halves_amplitude() {
        let g = LinearGain::from_db(-6.0);
        assert!((g.linear - 0.5012).abs() < 0.01, "got {}", g.linear);
    }

    #[test]
    fn biquad_lowpass_attenuates_above_cutoff() {
        let sr = 48_000u32;
        let cutoff = 1_000.0_f32;
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::LowPass, sr, cutoff, 0.707, 0.0);

        let mut sig = sine(10_000.0, sr, sr as usize);
        let rms_in = rms(&sig);
        bq.process_slice(&mut sig);
        let rms_out = rms(&sig[sr as usize / 2..]);
        let ratio = rms_out / rms_in.max(1e-9);
        assert!(
            ratio < 0.05,
            "10kHz через LP@1kHz: ratio={ratio:.4} (ожидаем <0.05)"
        );
    }

    #[test]
    fn biquad_lowpass_passes_below_cutoff() {
        let sr = 48_000u32;
        let cutoff = 5_000.0_f32;
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::LowPass, sr, cutoff, 0.707, 0.0);

        let mut sig = sine(200.0, sr, sr as usize);
        let rms_in = rms(&sig);
        bq.process_slice(&mut sig);
        let rms_out = rms(&sig[sr as usize / 2..]);
        let ratio = rms_out / rms_in.max(1e-9);
        assert!(ratio > 0.9, "200Hz через LP@5kHz: ratio={ratio:.4}");
    }

    #[test]
    fn biquad_highpass_attenuates_below_cutoff() {
        let sr = 48_000u32;
        let cutoff = 5_000.0_f32;
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::HighPass, sr, cutoff, 0.707, 0.0);

        let mut sig = sine(200.0, sr, sr as usize);
        let rms_in = rms(&sig);
        bq.process_slice(&mut sig);
        let rms_out = rms(&sig[sr as usize / 2..]);
        let ratio = rms_out / rms_in.max(1e-9);
        assert!(
            ratio < 0.05,
            "200Hz через HP@5kHz: ratio={ratio:.4} (ожидаем <0.05)"
        );
    }

    #[test]
    fn biquad_update_coeffs_idempotent_when_unchanged() {
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::LowPass, 48_000, 1_000.0, 0.707, 0.0);
        let b0_before = bq.b0;
        bq.update_coeffs(BiquadMode::LowPass, 48_000, 1_000.0, 0.707, 0.0);
        assert_eq!(bq.b0, b0_before);
    }

    #[test]
    fn biquad_peaking_unity_at_zero_gain() {
        let sr = 48_000u32;
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::Peaking, sr, 1_000.0, 1.0, 0.0);
        let mut sig = sine(440.0, sr, sr as usize);
        let rms_in = rms(&sig);
        bq.process_slice(&mut sig);
        let rms_out = rms(&sig[sr as usize / 2..]);
        let ratio = rms_out / rms_in.max(1e-9);
        assert!(
            (ratio - 1.0).abs() < 0.05,
            "Peaking @1kHz 0dB должен быть identity: ratio={ratio:.4}"
        );
    }

    #[test]
    fn biquad_peaking_boosts_at_center() {
        let sr = 48_000u32;
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::Peaking, sr, 1_000.0, 1.0, 12.0);
        let mut sig = sine(1_000.0, sr, sr as usize);
        let rms_in = rms(&sig);
        bq.process_slice(&mut sig);
        let rms_out = rms(&sig[sr as usize / 2..]);
        let ratio = rms_out / rms_in.max(1e-9);
        assert!(
            ratio > 2.0,
            "Peaking @1kHz +12dB должен поднимать центр: ratio={ratio:.4}"
        );
    }

    #[test]
    fn biquad_peaking_cuts_at_center() {
        let sr = 48_000u32;
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::Peaking, sr, 1_000.0, 1.0, -12.0);
        let mut sig = sine(1_000.0, sr, sr as usize);
        let rms_in = rms(&sig);
        bq.process_slice(&mut sig);
        let rms_out = rms(&sig[sr as usize / 2..]);
        let ratio = rms_out / rms_in.max(1e-9);
        assert!(
            ratio < 0.5,
            "Peaking @1kHz -12dB должен резать центр: ratio={ratio:.4}"
        );
    }

    #[test]
    fn biquad_peaking_does_not_affect_far_band() {
        let sr = 48_000u32;
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::Peaking, sr, 1_000.0, 1.0, 12.0);
        let mut sig = sine(100.0, sr, sr as usize);
        let rms_in = rms(&sig);
        bq.process_slice(&mut sig);
        let rms_out = rms(&sig[sr as usize / 2..]);
        let ratio = rms_out / rms_in.max(1e-9);
        assert!(
            ratio < 1.7,
            "Peaking @1kHz +12dB не должен сильно влиять на 100Hz: ratio={ratio:.4}"
        );
    }

    #[test]
    fn biquad_low_shelf_boosts_low_freq() {
        let sr = 48_000u32;
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::LowShelf, sr, 1_000.0, 0.707, 12.0);
        let mut lo = sine(200.0, sr, sr as usize);
        let rms_in_lo = rms(&lo);
        bq.process_slice(&mut lo);
        let rms_out_lo = rms(&lo[sr as usize / 2..]);
        let ratio_lo = rms_out_lo / rms_in_lo.max(1e-9);
        assert!(ratio_lo > 2.5, "LowShelf 200Hz +12dB ratio={ratio_lo:.4}");
    }

    #[test]
    fn biquad_high_shelf_boosts_high_freq() {
        let sr = 48_000u32;
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::HighShelf, sr, 1_000.0, 0.707, 12.0);
        let mut hi = sine(8_000.0, sr, sr as usize);
        let rms_in_hi = rms(&hi);
        bq.process_slice(&mut hi);
        let rms_out_hi = rms(&hi[sr as usize / 2..]);
        let ratio_hi = rms_out_hi / rms_in_hi.max(1e-9);
        assert!(ratio_hi > 2.5, "HighShelf 8kHz +12dB ratio={ratio_hi:.4}");
    }

    #[test]
    fn biquad_peaking_cache_invalidates_on_gain_change() {
        let mut bq = Biquad::new();
        bq.update_coeffs(BiquadMode::Peaking, 48_000, 1_000.0, 1.0, 0.0);
        let b0_zero = bq.b0;
        bq.update_coeffs(BiquadMode::Peaking, 48_000, 1_000.0, 1.0, 12.0);
        assert!(
            (bq.b0 - b0_zero).abs() > 1e-3,
            "Cache должен invalidate'нуться при смене gain_db: было {b0_zero}, стало {}",
            bq.b0
        );
    }

    #[test]
    fn reverb_dry_zero_mix_is_identity() {
        let mut r = SchroederReverb::new(48_000);
        r.set_mix(0.0);
        let mut sig = sine(440.0, 48_000, 1024);
        let orig = sig.clone();
        r.process_slice(&mut sig);
        for (i, (a, b)) in sig.iter().zip(orig.iter()).enumerate() {
            assert!(
                (a - b).abs() < 1e-6,
                "sample {i}: {a} vs {b} (ожидаем identity при mix=0)"
            );
        }
    }

    #[test]
    fn reverb_tail_decays_after_impulse() {
        let mut r = SchroederReverb::new(48_000);
        r.set_mix(1.0);
        r.set_room(0.5);
        let mut sig = vec![0.0_f32; 48_000 * 4];
        sig[0] = 1.0;
        r.process_slice(&mut sig);
        let head = &sig[..9_600];
        let tail = &sig[sig.len() - 9_600..];
        let rms_head = rms(head);
        let rms_tail = rms(tail);
        assert!(
            rms_head > rms_tail * 4.0,
            "tail должен затухать: head_rms={rms_head:.4}, tail_rms={rms_tail:.4}"
        );
    }

    #[test]
    fn reverb_set_mix_clamps() {
        let mut r = SchroederReverb::new(48_000);
        r.set_mix(2.0);
        assert_eq!(r.mix, 1.0);
        r.set_mix(-0.5);
        assert_eq!(r.mix, 0.0);
    }

}
