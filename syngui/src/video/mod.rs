#[cfg(target_os = "android")]
pub mod android;
pub mod decoder;
pub mod error;
pub mod hwaccel;
pub mod player;
pub mod resampler;
pub mod scaler;
pub mod stream;

pub use decoder::{VideoDecoder, VideoFrame, VideoMeta};
pub use error::VideoError;
pub use hwaccel::HwAccel;
pub use player::VideoPlayer;
pub use stream::VideoStream;

/// Уровень подробности логов libav* (уходят в stderr / logcat как
/// `RustStdoutStderr`). По умолчанию FFmpeg пишет `Info`; для разбора
/// сетевых и TLS-проблем полезен `Verbose`/`Debug`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FfmpegLogLevel {
    Quiet,
    Error,
    Warning,
    Info,
    Verbose,
    Debug,
}

/// Задать уровень логов FFmpeg для всего процесса.
pub fn set_ffmpeg_log_level(level: FfmpegLogLevel) {
    use ffmpeg_next::log::{set_level, Level};
    set_level(match level {
        FfmpegLogLevel::Quiet => Level::Quiet,
        FfmpegLogLevel::Error => Level::Error,
        FfmpegLogLevel::Warning => Level::Warning,
        FfmpegLogLevel::Info => Level::Info,
        FfmpegLogLevel::Verbose => Level::Verbose,
        FfmpegLogLevel::Debug => Level::Debug,
    });
}
