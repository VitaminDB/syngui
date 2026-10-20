//! Короткие звуки интерфейса (щелчки, переходы, уведомления) с малой
//! задержкой: файлы (WAV, OGG Vorbis, FLAC) декодируются один раз в память,
//! воспроизведение — микшером в общем потоке вывода. Поток открывается при
//! первом звуке и закрывается через несколько секунд тишины, чтобы не
//! держать звуковое устройство (и телефон) включённым.
//!
//! ```ignore
//! let s = syngui::audio::sfx::load("/usr/share/sounds/freedesktop/stereo/bell.oga")?;
//! syngui::audio::sfx::play(&s, 0.6);
//! ```

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};

use super::recorder::AudioError;

/// Звук в памяти: стерео, f32, чередующиеся каналы.
#[derive(Clone, Debug)]
pub struct Sound {
    pcm: Arc<[f32]>,
    sample_rate: u32,
}

impl Sound {
    /// Из готовых сэмплов (`channels` — 1 или 2, чередующиеся).
    pub fn from_pcm(samples: &[f32], sample_rate: u32, channels: u16) -> Sound {
        let pcm: Vec<f32> = match channels {
            1 => samples.iter().flat_map(|s| [*s, *s]).collect(),
            2 => samples.to_vec(),
            n => samples.chunks(n as usize).flat_map(|c| [c[0], c[1.min(c.len() - 1)]]).collect(),
        };
        Sound { pcm: pcm.into(), sample_rate: sample_rate.max(1) }
    }

    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.pcm.len() as f64 / 2.0 / self.sample_rate as f64)
    }

    /// Переложить на частоту вывода (линейная интерполяция — для коротких
    /// звуков интерфейса её достаточно).
    fn resampled(&self, rate: u32) -> Arc<[f32]> {
        if rate == self.sample_rate {
            return self.pcm.clone();
        }
        let frames = self.pcm.len() / 2;
        let out_frames = ((frames as u64 * rate as u64) / self.sample_rate as u64) as usize;
        let step = self.sample_rate as f64 / rate as f64;
        let mut out = Vec::with_capacity(out_frames * 2);
        for i in 0..out_frames {
            let p = i as f64 * step;
            let j = p as usize;
            let t = (p - j as f64) as f32;
            for c in 0..2 {
                let a = self.pcm.get(j * 2 + c).copied().unwrap_or(0.0);
                let b = self.pcm.get((j + 1) * 2 + c).copied().unwrap_or(a);
                out.push(a + (b - a) * t);
            }
        }
        out.into()
    }
}

/// Декодировать файл (WAV, OGG/OGA Vorbis, FLAC, MP3).
pub fn load(path: impl AsRef<Path>) -> Result<Sound, AudioError> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let path = path.as_ref();
    let err = |e: &dyn std::fmt::Display| AudioError::Cpal(format!("{}: {e}", path.display()));
    let file = std::fs::File::open(path).map_err(|e| err(&e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(if ext == "oga" { "ogg" } else { ext });
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| err(&e))?;
    let mut format = probed.format;
    let track = format.default_track().ok_or_else(|| err(&"нет звуковой дорожки"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| err(&e))?;
    let mut rate = track.codec_params.sample_rate.unwrap_or(48_000);
    let mut channels = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);
    let mut samples: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = *buf.spec();
                rate = spec.rate;
                channels = spec.channels.count() as u16;
                let mut sb = SampleBuffer::<f32>::new(buf.capacity() as u64, spec);
                sb.copy_interleaved_ref(buf);
                samples.extend_from_slice(sb.samples());
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(_) => break,
        }
    }
    if samples.is_empty() {
        return Err(err(&"пустой звук"));
    }
    Ok(Sound::from_pcm(&samples, rate, channels.max(1)))
}

struct Voice {
    pcm: Arc<[f32]>,
    pos: usize,
    volume: f32,
}

struct Mixer {
    voices: Vec<Voice>,
    /// Частота открытого потока; 0 — поток закрыт.
    rate: u32,
    last_active: Instant,
}

const MAX_VOICES: usize = 8;
/// Тишина, после которой поток вывода закрывается.
const IDLE_CLOSE: Duration = Duration::from_secs(4);

fn mixer() -> &'static Arc<Mutex<Mixer>> {
    static M: OnceLock<Arc<Mutex<Mixer>>> = OnceLock::new();
    M.get_or_init(|| Arc::new(Mutex::new(Mixer { voices: Vec::new(), rate: 0, last_active: Instant::now() })))
}

/// Сыграть звук; `volume` — 0..1 (больше 1 — усиление).
pub fn play(sound: &Sound, volume: f32) {
    if volume <= 0.0 {
        return;
    }
    let m = mixer();
    let rate = {
        let mut g = match m.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        g.last_active = Instant::now();
        g.rate
    };
    let rate = if rate == 0 {
        // Два звука разом не должны открыть два потока.
        static OPENING: Mutex<()> = Mutex::new(());
        let _g = OPENING.lock();
        match m.lock().map(|g| g.rate).unwrap_or(0) {
            0 => match open_stream() {
                Some(r) => r,
                None => return,
            },
            r => r,
        }
    } else {
        rate
    };
    let pcm = sound.resampled(rate);
    if let Ok(mut g) = m.lock() {
        if g.voices.len() >= MAX_VOICES {
            g.voices.remove(0);
        }
        g.voices.push(Voice { pcm, pos: 0, volume });
        g.last_active = Instant::now();
    }
}

/// Открыть поток вывода в своём потоке (cpal-поток не `Send`); вернуть его
/// частоту, когда он готов.
fn open_stream() -> Option<u32> {
    let (tx, rx) = std::sync::mpsc::channel::<Option<u32>>();
    let spawned = std::thread::Builder::new().name("syngui-sfx".into()).spawn(move || {
        let host = cpal::default_host();
        let Ok((device, supported)) = super::player::pick_output_device(&host) else {
            let _ = tx.send(None);
            return;
        };
        let rate = supported.sample_rate().0;
        let out_ch = supported.channels() as usize;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let m = mixer().clone();
        let fill = move |out: &mut [f32]| {
            out.iter_mut().for_each(|s| *s = 0.0);
            let Ok(mut g) = m.try_lock() else { return };
            let frames = out.len() / out_ch.max(1);
            for v in g.voices.iter_mut() {
                for f in 0..frames {
                    let i = v.pos + f * 2;
                    if i + 1 >= v.pcm.len() {
                        break;
                    }
                    let (l, r) = (v.pcm[i] * v.volume, v.pcm[i + 1] * v.volume);
                    match out_ch {
                        1 => out[f] += (l + r) * 0.5,
                        _ => {
                            out[f * out_ch] += l;
                            out[f * out_ch + 1] += r;
                        }
                    }
                }
                v.pos += frames * 2;
            }
            g.voices.retain(|v| v.pos < v.pcm.len());
            if !g.voices.is_empty() {
                g.last_active = Instant::now();
            }
            out.iter_mut().for_each(|s| *s = s.clamp(-1.0, 1.0));
        };
        let errf = |e| log::warn!("звуки интерфейса: {e}");
        let stream = match format {
            cpal::SampleFormat::F32 => device.build_output_stream(&config, move |d: &mut [f32], _| fill(d), errf, None),
            cpal::SampleFormat::I16 => {
                let mut tmp = Vec::new();
                device.build_output_stream(
                    &config,
                    move |d: &mut [i16], _| {
                        tmp.resize(d.len(), 0.0);
                        fill(&mut tmp);
                        for (o, s) in d.iter_mut().zip(&tmp) {
                            *o = (s * 32767.0) as i16;
                        }
                    },
                    errf,
                    None,
                )
            }
            other => {
                log::warn!("звуки интерфейса: формат {other:?} не поддержан");
                let _ = tx.send(None);
                return;
            }
        };
        let Ok(stream) = stream else {
            let _ = tx.send(None);
            return;
        };
        if stream.play().is_err() {
            let _ = tx.send(None);
            return;
        }
        if let Ok(mut g) = mixer().lock() {
            g.rate = rate;
            g.last_active = Instant::now();
        }
        let _ = tx.send(Some(rate));
        // Держать поток, пока звучит или недавно звучало.
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let idle = mixer().lock().map(|g| g.voices.is_empty() && g.last_active.elapsed() > IDLE_CLOSE).unwrap_or(true);
            if idle {
                if let Ok(mut g) = mixer().lock() {
                    if g.voices.is_empty() {
                        g.rate = 0;
                        break;
                    }
                }
            }
        }
        drop(stream);
    });
    if spawned.is_err() {
        return None;
    }
    rx.recv_timeout(Duration::from_secs(3)).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_to_stereo_and_resample() {
        let s = Sound::from_pcm(&[0.0, 1.0, 0.0, -1.0], 24_000, 1);
        assert_eq!(s.pcm.len(), 8);
        let r = s.resampled(48_000);
        assert_eq!(r.len(), 16);
        assert!((r[2] - 0.5).abs() < 1e-6, "{:?}", &r[..6]);
    }

    #[test]
    fn decodes_wav() {
        let dir = std::env::temp_dir().join("syngui-sfx-test.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&dir, spec).unwrap();
        for i in 0..1600 {
            w.write_sample(((i as f32 * 0.1).sin() * 10000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        let s = load(&dir).unwrap();
        assert_eq!(s.sample_rate, 16_000);
        assert!((s.duration().as_secs_f32() - 0.1).abs() < 0.01);
        let _ = std::fs::remove_file(dir);
    }
}
