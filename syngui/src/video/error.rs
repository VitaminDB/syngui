use crate::audio::AudioError;

#[derive(Debug, Clone)]
pub enum VideoError {
    Open(String),
    NoVideoStream,
    DecoderInit(String),
    Scaler(String),
    Resampler(String),
    Audio(AudioError),
    Seek(String),
    Eof,
    Other(String),
}

impl std::fmt::Display for VideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoError::Open(s) => write!(f, "не удалось открыть источник: {s}"),
            VideoError::NoVideoStream => write!(f, "в файле нет видео-потока"),
            VideoError::DecoderInit(s) => write!(f, "ошибка инициализации декодера: {s}"),
            VideoError::Scaler(s) => write!(f, "ошибка swscale: {s}"),
            VideoError::Resampler(s) => write!(f, "ошибка swresample: {s}"),
            VideoError::Audio(e) => write!(f, "ошибка аудио-устройства: {e}"),
            VideoError::Seek(s) => write!(f, "ошибка перемотки: {s}"),
            VideoError::Eof => write!(f, "конец потока"),
            VideoError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for VideoError {}

impl From<AudioError> for VideoError {
    fn from(e: AudioError) -> Self {
        VideoError::Audio(e)
    }
}

impl From<ffmpeg_next::Error> for VideoError {
    fn from(e: ffmpeg_next::Error) -> Self {
        VideoError::Other(format!("ffmpeg: {e}"))
    }
}
