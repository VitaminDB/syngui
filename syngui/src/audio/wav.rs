use std::fs::File;
use std::io::{BufWriter, Cursor, Write, Seek};
use std::path::{Path, PathBuf};

use hound::{SampleFormat, WavSpec, WavWriter};

use super::recorder::AudioError;

pub struct WavStreamWriter {
    inner: WriterImpl<BufWriter<File>>,
    pub sample_rate: u32,
    pub channels: u16,
    pub path: PathBuf,
}

impl WavStreamWriter {
    pub fn open<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Self, AudioError> {
        let path = path.as_ref().to_path_buf();
        let file = File::create(&path)
            .map_err(|e| AudioError::Wav(format!("create {}: {e}", path.display())))?;
        let buf = BufWriter::new(file);
        let spec = WavSpec {
            channels: channels.max(1),
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let writer =
            WavWriter::new(buf, spec).map_err(|e| AudioError::Wav(e.to_string()))?;
        Ok(Self {
            inner: WriterImpl::Open {
                writer,
                samples_written: 0,
            },
            sample_rate,
            channels: channels.max(1),
            path,
        })
    }

    pub fn write_chunk(&mut self, samples: &[f32]) -> Result<(), AudioError> {
        let WriterImpl::Open { writer, samples_written } = &mut self.inner else {
            return Err(AudioError::Wav("writer уже finalize'нут".into()));
        };
        for s in samples {
            let amp = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer
                .write_sample(amp)
                .map_err(|e| AudioError::Wav(e.to_string()))?;
        }
        *samples_written += samples.len() as u64;
        Ok(())
    }

    pub fn finalize(&mut self) -> Result<(), AudioError> {
        let prev = std::mem::replace(&mut self.inner, WriterImpl::Closed);
        match prev {
            WriterImpl::Open { writer, .. } => {
                writer.finalize().map_err(|e| AudioError::Wav(e.to_string()))
            }
            WriterImpl::Closed => Err(AudioError::Wav("writer уже finalize'нут".into())),
        }
    }

    pub fn samples_written(&self) -> u64 {
        match &self.inner {
            WriterImpl::Open { samples_written, .. } => *samples_written,
            WriterImpl::Closed => 0,
        }
    }

    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        let frames = self.samples_written() / self.channels as u64;
        frames as f64 / self.sample_rate as f64
    }

    pub fn is_open(&self) -> bool {
        matches!(self.inner, WriterImpl::Open { .. })
    }
}

impl Drop for WavStreamWriter {
    fn drop(&mut self) {
        if self.is_open() {
            let _ = self.finalize();
        }
    }
}

enum WriterImpl<W: Write + Seek> {
    Open { writer: WavWriter<W>, samples_written: u64 },
    Closed,
}

pub fn into_pcm16_bytes(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, AudioError> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut buf: Vec<u8> = Vec::with_capacity(samples.len() * 2 + 64);
    {
        let cursor = Cursor::new(&mut buf);
        let mut writer = WavWriter::new(cursor, spec).map_err(|e| AudioError::Wav(e.to_string()))?;
        for s in samples {
            let amp = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer
                .write_sample(amp)
                .map_err(|e| AudioError::Wav(e.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|e| AudioError::Wav(e.to_string()))?;
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        temp_dir().join(format!("{prefix}-{nanos}.wav"))
    }

    #[test]
    fn streaming_roundtrip_pcm16() {
        let path = unique_path("syngui-wav-stream-test");
        let chunk_a: Vec<f32> = vec![0.0, 0.25, -0.25, 0.5];
        let chunk_b: Vec<f32> = vec![-0.5, 1.0, -1.0, 0.0];

        let mut w = WavStreamWriter::open(&path, 16_000, 1).expect("open");
        w.write_chunk(&chunk_a).expect("chunk a");
        w.write_chunk(&chunk_b).expect("chunk b");
        assert_eq!(w.samples_written(), 8);
        let secs = w.duration_seconds();
        assert!((secs - (8.0 / 16_000.0)).abs() < 1e-9);
        w.finalize().expect("finalize");

        let mut reader = hound::WavReader::open(&path).expect("open reader");
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16_000);
        assert_eq!(spec.bits_per_sample, 16);

        let samples: Vec<i16> = reader
            .samples::<i16>()
            .map(|s| s.expect("sample"))
            .collect();
        assert_eq!(samples.len(), 8);

        let expected: Vec<i16> = chunk_a
            .iter()
            .chain(chunk_b.iter())
            .map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect();
        for (i, (got, exp)) in samples.iter().zip(expected.iter()).enumerate() {
            assert!(
                (*got - *exp).abs() <= 1,
                "sample {i}: got {got}, expected ~{exp}"
            );
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn finalize_after_finalize_errors() {
        let path = unique_path("syngui-wav-double-finalize");
        let mut w = WavStreamWriter::open(&path, 8_000, 1).expect("open");
        w.write_chunk(&[0.1, -0.1]).expect("chunk");
        w.finalize().expect("first finalize");
        assert!(!w.is_open());
        assert!(w.finalize().is_err(), "повторный finalize должен вернуть Err");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_after_finalize_errors() {
        let path = unique_path("syngui-wav-write-after-final");
        let mut w = WavStreamWriter::open(&path, 8_000, 1).expect("open");
        w.finalize().expect("finalize");
        assert!(w.write_chunk(&[0.0]).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn into_pcm16_bytes_matches_streaming() {
        let samples = vec![0.0, 0.25, -0.25, 0.5, -0.5, 1.0, -1.0, 0.0];
        let bytes = into_pcm16_bytes(&samples, 8_000).expect("encode");
        assert!(bytes.len() >= 44 + 16);

        let cursor = Cursor::new(bytes);
        let mut reader = hound::WavReader::new(cursor).expect("reader");
        let got: Vec<i16> = reader
            .samples::<i16>()
            .map(|s| s.expect("sample"))
            .collect();
        assert_eq!(got.len(), 8);
    }
}
