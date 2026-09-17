use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ffmpeg_next::ffi;
use ffmpeg_next::format::context::Input;
use ffmpeg_next::format::sample::Sample as SampleFmt;
use ffmpeg_next::media::Type as MediaType;
use ffmpeg_next::{frame, ChannelLayout};

use super::error::VideoError;
use super::hwaccel::{HwAccel, HwContext};
use super::resampler::Resampler;
use super::scaler::Scaler;

#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    /// RGBA8; пусто у кадров, которые показывает сам кодек (`surface`).
    pub rgba: Arc<[u8]>,
    pub pts_sec: f64,
    /// Номер перемотки, после которой декодирован кадр. Кадр старого номера,
    /// проскочивший в очередь между её очисткой и перемоткой в потоке
    /// декодера, плеер выбрасывает: после перемотки назад его pts «в будущем»,
    /// и картинка ждала бы его секундами.
    pub seek_generation: u64,
    /// Кадр в выходном буфере MediaCodec (`HwAccel::MediaCodecSurface`):
    /// показывается вызовом [`SurfaceBuffer::render`] в момент показа,
    /// пикселей в `rgba` нет.
    pub surface: Option<Arc<SurfaceBuffer>>,
}

/// Выходной буфер аппаратного декодера, который показывает сам кодек
/// (Android MediaCodec → Surface). Держит ссылку на AVFrame: пока она жива,
/// буфер не возвращён кодеку; `render()` отдаёт его на экран, а drop без
/// render — возвращает кодеку без показа (пропущенный кадр).
pub struct SurfaceBuffer {
    frame: std::sync::Mutex<Option<ffmpeg_next::frame::Video>>,
    rendered: std::sync::atomic::AtomicBool,
}

impl SurfaceBuffer {
    fn new(frame: ffmpeg_next::frame::Video) -> Self {
        Self {
            frame: std::sync::Mutex::new(Some(frame)),
            rendered: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Показать кадр (один раз; повторные вызовы — no-op).
    pub fn render(&self) {
        self.render_impl(None);
    }

    /// Запланировать показ на момент `time_ns` (часы `CLOCK_MONOTONIC`,
    /// см. `video::android::monotonic_ns`): кодек выведет кадр сам на
    /// ближайшем vsync, UI-поток может не тикать на каждый кадр.
    pub fn render_at(&self, time_ns: i64) {
        self.render_impl(Some(time_ns));
    }

    fn render_impl(&self, at_ns: Option<i64>) {
        if self
            .rendered
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            return;
        }
        let Ok(mut slot) = self.frame.lock() else {
            return;
        };
        if let Some(frame) = slot.take() {
            #[cfg(target_os = "android")]
            unsafe {
                let buffer = (*frame.as_ptr()).data[3] as *mut std::ffi::c_void;
                match at_ns {
                    Some(t) => super::android::render_mediacodec_buffer_at(buffer, t),
                    None => super::android::release_mediacodec_buffer(buffer, true),
                }
            }
            #[cfg(not(target_os = "android"))]
            let _ = at_ns;
            drop(frame);
        }
    }
}

impl std::fmt::Debug for SurfaceBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceBuffer")
            .field(
                "rendered",
                &self.rendered.load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct VideoMeta {
    pub width: u32,
    pub height: u32,
    pub duration_sec: f64,
    pub fps_estimate: f32,
    pub video_time_base: (i32, i32),
    pub has_audio: bool,
    pub audio_sample_rate: u32,
    pub audio_channels: u16,
}

#[derive(Debug)]
pub(crate) enum DecoderCmd {
    Pause,
    Resume,
    SeekSec(f64),
    Stop,
    ReAttachAudio(SyncSender<Vec<f32>>),
    InstallVideoTee(Option<SyncSender<Arc<VideoFrame>>>),
    InstallAudioTee(Option<SyncSender<Vec<f32>>>),
}

pub struct VideoDecoder {
    meta: VideoMeta,
    video_rx: Receiver<VideoFrame>,
    audio_rx: Option<Receiver<Vec<f32>>>,
    cmd_tx: Sender<DecoderCmd>,
    join: Option<JoinHandle<Result<(), VideoError>>>,
    audio_base_pts: Arc<AtomicI64>,
}

/// «pts ещё неизвестен» в [`VideoDecoder::audio_base_pts_sec`].
const PTS_UNKNOWN: i64 = i64::MIN;

const AUDIO_OUTPUT_SR: u32 = 48_000;

const VIDEO_QUEUE_CAP: usize = 8;
/// Столько ошибок send_packet подряд — поток декодера завершается.
const MAX_CONSECUTIVE_VIDEO_ERRORS: u32 = 200;
/// Очередь кадров в буферах MediaCodec: показ планируется на ~0,4 с
/// вперёд (`VideoPlayer::poll_frame`), больше держать незачем, а кодек с
/// ~8–16 выходными буферами не должен остаться без них.
const SURFACE_QUEUE_CAP: usize = 6;
const AUDIO_QUEUE_CAP: usize = 64;

const VIDEO_TEE_QUEUE_CAP: usize = 4;
const AUDIO_TEE_QUEUE_CAP: usize = 32;

const AV_TIME_BASE_F64: f64 = 1_000_000.0;

/// Упреждающее чтение: отдельный поток тянет пакеты из демуксера в очередь,
/// пока она не заполнится. Для HLS с CDN это буфер ~20–30 с сжатого видео
/// (пакет ≈ кадр или аудиофрейм, ~70 шт./с; 1080p ≈ 15 МБ на 2000 пакетов):
/// зависший сегмент или мёртвый edge-хост не останавливают картинку.
const READAHEAD_PACKETS: usize = 2000;

enum ReaderCmd {
    /// Перемотка: после `ictx.seek` ридер шлёт `Flushed(gen)`, и декодер
    /// выбрасывает всё, что пришло до этого маркера.
    Seek(f64, u64),
    Stop,
}

enum ReaderItem {
    Packet(ffmpeg_next::Packet),
    Flushed(u64, Option<String>),
    Eof,
}

/// Поток чтения пакетов (владеет `Input`). На EOF ждёт команду (seek/stop).
fn run_reader_thread(
    mut ictx: Input,
    item_tx: SyncSender<ReaderItem>,
    cmd_rx: Receiver<ReaderCmd>,
) {
    let mut at_eof = false;
    loop {
        let cmd = if at_eof {
            match cmd_rx.recv() {
                Ok(c) => Some(c),
                Err(_) => return,
            }
        } else {
            cmd_rx.try_recv().ok()
        };
        match cmd {
            Some(ReaderCmd::Stop) => return,
            Some(ReaderCmd::Seek(sec, gen)) => {
                let target_ts = (sec * AV_TIME_BASE_F64) as i64;
                let err = ictx
                    .seek(target_ts, ..target_ts)
                    .err()
                    .map(|e| format!("ictx.seek({sec:.3}s): {e}"));
                at_eof = false;
                if item_tx.send(ReaderItem::Flushed(gen, err)).is_err() {
                    return;
                }
                continue;
            }
            None => {}
        }
        let mut packet = ffmpeg_next::Packet::empty();
        match packet.read(&mut ictx) {
            Ok(()) => {
                if item_tx.send(ReaderItem::Packet(packet)).is_err() {
                    return;
                }
            }
            Err(ffmpeg_next::Error::Eof) => {
                at_eof = true;
                if item_tx.send(ReaderItem::Eof).is_err() {
                    return;
                }
            }
            Err(e) => {
                log::debug!("video: read packet: {e}");
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

impl VideoDecoder {
    pub fn open(input: &str) -> Result<Self, VideoError> {
        Self::open_with_hwaccel(input, HwAccel::None)
    }

    pub fn open_with_hwaccel(input: &str, accel: HwAccel) -> Result<Self, VideoError> {
        Self::open_with_options(input, accel, &[])
    }

    /// Открыть с опциями демуксера/протокола libavformat: `user_agent`,
    /// `headers` («Referer: …\r\n»), `timeout`, `rw_timeout`,
    /// `reconnect`… — то, что принимает `avformat_open_input` через
    /// AVDictionary. Нужны для HLS с CDN, требующих Referer/UA.
    pub fn open_with_options(
        input: &str,
        accel: HwAccel,
        options: &[(&str, &str)],
    ) -> Result<Self, VideoError> {
        ffmpeg_next::init().ok();

        let ictx = if options.is_empty() {
            ffmpeg_next::format::input(&input.to_string())
        } else {
            let mut dict = ffmpeg_next::Dictionary::new();
            for (k, v) in options {
                dict.set(k, v);
            }
            ffmpeg_next::format::input_with_dictionary(&input.to_string(), dict)
        }
        .map_err(|e| VideoError::Open(format!("{input}: {e}")))?;

        let meta = read_meta(&ictx)?;

        // Кадры на Surface держат выходные буферы MediaCodec (их немного):
        // очередь короче, иначе кодек остаётся без буферов и встаёт.
        let video_cap = if accel == HwAccel::MediaCodecSurface {
            SURFACE_QUEUE_CAP
        } else {
            VIDEO_QUEUE_CAP
        };
        let (video_tx, video_rx) = mpsc::sync_channel::<VideoFrame>(video_cap);
        let (audio_tx_opt, audio_rx_opt) = if meta.has_audio {
            let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(AUDIO_QUEUE_CAP);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let (cmd_tx, cmd_rx) = mpsc::channel::<DecoderCmd>();

        let meta_thread = meta.clone();
        let audio_base_pts = Arc::new(AtomicI64::new(PTS_UNKNOWN));
        let audio_base_thread = audio_base_pts.clone();
        let join = thread::Builder::new()
            .name("syngui-video-decoder".into())
            .spawn(move || {
                let r = run_decoder_thread(
                    ictx,
                    meta_thread,
                    accel,
                    video_tx,
                    audio_tx_opt,
                    cmd_rx,
                    &audio_base_thread,
                );
                if let Err(e) = &r {
                    log::error!("video: поток декодера завершился с ошибкой: {e}");
                }
                r
            })
            .map_err(|e| VideoError::Other(format!("spawn decoder: {e}")))?;

        Ok(Self {
            meta,
            video_rx,
            audio_rx: audio_rx_opt,
            cmd_tx,
            join: Some(join),
            audio_base_pts,
        })
    }

    pub fn meta(&self) -> &VideoMeta {
        &self.meta
    }

    /// pts (в секундах, шкала видео) первого сэмпла, ушедшего в текущий
    /// аудио-канал — после открытия или `re_attach_audio`. Проигранные
    /// сэмплы отсчитываются от него: не от нуля и не от цели перемотки,
    /// потому что поток начинается не с нуля, а перемотка встаёт на ключевой
    /// кадр раньше цели. `None` — звук в новый канал ещё не пошёл.
    pub fn audio_base_pts_sec(&self) -> Option<f64> {
        match self.audio_base_pts.load(Ordering::Acquire) {
            PTS_UNKNOWN => None,
            micros => Some(micros as f64 / 1_000_000.0),
        }
    }

    pub fn try_recv_video(&self) -> Result<VideoFrame, TryRecvError> {
        self.video_rx.try_recv()
    }

    pub fn take_audio_rx(&mut self) -> Option<Receiver<Vec<f32>>> {
        self.audio_rx.take()
    }

    pub fn audio_output_sr(&self) -> u32 {
        AUDIO_OUTPUT_SR
    }

    pub fn pause(&self) {
        let _ = self.cmd_tx.send(DecoderCmd::Pause);
    }

    pub fn resume(&self) {
        let _ = self.cmd_tx.send(DecoderCmd::Resume);
    }

    pub fn seek(&self, sec: f64) {
        let _ = self.cmd_tx.send(DecoderCmd::SeekSec(sec));
    }

    pub fn re_attach_audio(&self) -> Option<Receiver<Vec<f32>>> {
        if !self.meta.has_audio {
            return None;
        }
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(AUDIO_QUEUE_CAP);
        self.cmd_tx.send(DecoderCmd::ReAttachAudio(tx)).ok()?;
        Some(rx)
    }

    pub fn install_video_tee(&self) -> Option<Receiver<Arc<VideoFrame>>> {
        let (tx, rx) = mpsc::sync_channel::<Arc<VideoFrame>>(VIDEO_TEE_QUEUE_CAP);
        self.cmd_tx
            .send(DecoderCmd::InstallVideoTee(Some(tx)))
            .ok()?;
        Some(rx)
    }

    pub fn install_audio_tee(&self) -> Option<Receiver<Vec<f32>>> {
        if !self.meta.has_audio {
            return None;
        }
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(AUDIO_TEE_QUEUE_CAP);
        self.cmd_tx
            .send(DecoderCmd::InstallAudioTee(Some(tx)))
            .ok()?;
        Some(rx)
    }

    pub fn uninstall_tees(&self) {
        let _ = self.cmd_tx.send(DecoderCmd::InstallVideoTee(None));
        let _ = self.cmd_tx.send(DecoderCmd::InstallAudioTee(None));
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(DecoderCmd::Stop);
        if let Some(j) = self.join.take() {
            // Отпускаем приёмники: поток декодера, стоящий в `send` на полной
            // очереди, получит ошибку и выйдет — иначе join ждал бы вечно.
            let (_dummy_tx, dummy_rx) = mpsc::sync_channel::<VideoFrame>(1);
            drop(std::mem::replace(&mut self.video_rx, dummy_rx));
            drop(self.audio_rx.take());
            let _ = j.join();
        }
    }
}

fn read_meta(ictx: &Input) -> Result<VideoMeta, VideoError> {
    let v = ictx
        .streams()
        .best(MediaType::Video)
        .ok_or(VideoError::NoVideoStream)?;
    let v_params = v.parameters();
    let v_dec = ffmpeg_next::codec::context::Context::from_parameters(v_params)
        .map_err(|e| VideoError::DecoderInit(format!("video params: {e}")))?
        .decoder()
        .video()
        .map_err(|e| VideoError::DecoderInit(format!("video decoder: {e}")))?;

    let tb = v.time_base();
    let avg_fr = v.avg_frame_rate();
    let fps = if avg_fr.denominator() != 0 {
        avg_fr.numerator() as f32 / avg_fr.denominator() as f32
    } else {
        0.0
    };
    let duration_sec = if v.duration() > 0 {
        v.duration() as f64 * tb.numerator() as f64 / tb.denominator() as f64
    } else if ictx.duration() > 0 {
        ictx.duration() as f64 / AV_TIME_BASE_F64
    } else {
        0.0
    };

    let (has_audio, audio_sr, audio_ch) = match ictx.streams().best(MediaType::Audio) {
        Some(a) => {
            let a_dec = ffmpeg_next::codec::context::Context::from_parameters(a.parameters())
                .ok()
                .and_then(|c| c.decoder().audio().ok());
            match a_dec {
                Some(dec) => (true, dec.rate(), dec.channels()),
                None => (false, 0, 0),
            }
        }
        None => (false, 0, 0),
    };

    Ok(VideoMeta {
        width: v_dec.width(),
        height: v_dec.height(),
        duration_sec,
        fps_estimate: fps,
        video_time_base: (tb.numerator(), tb.denominator()),
        has_audio,
        audio_sample_rate: audio_sr,
        audio_channels: audio_ch,
    })
}

fn run_decoder_thread(
    ictx: Input,
    _meta: VideoMeta,
    accel: HwAccel,
    video_tx: SyncSender<VideoFrame>,
    mut audio_tx: Option<SyncSender<Vec<f32>>>,
    cmd_rx: Receiver<DecoderCmd>,
    audio_base: &AtomicI64,
) -> Result<(), VideoError> {
    let v_idx = ictx
        .streams()
        .best(MediaType::Video)
        .ok_or(VideoError::NoVideoStream)?
        .index();
    let a_idx = ictx.streams().best(MediaType::Audio).map(|s| s.index());

    let v_params = ictx
        .stream(v_idx)
        .ok_or(VideoError::NoVideoStream)?
        .parameters();
    let codec_id = v_params.id();
    let v_stream = ictx.stream(v_idx).ok_or(VideoError::NoVideoStream)?;
    let v_tb_av = v_stream.time_base();

    let mut codec_ctx = ffmpeg_next::codec::context::Context::from_parameters(v_params)
        .map_err(|e| VideoError::DecoderInit(format!("video params: {e}")))?;

    unsafe {
        (*codec_ctx.as_mut_ptr()).pkt_timebase = ffi::AVRational {
            num: v_tb_av.numerator(),
            den: v_tb_av.denominator(),
        };
    }

    let hw: Option<HwContext> = if !accel.uses_hw_device() {
        None
    } else {
        match HwContext::try_init(accel) {
            Ok(h) => {
                // SAFETY: codec_ctx ещё не открыт (decoder().video() ниже),
                unsafe { h.attach_to(codec_ctx.as_mut_ptr()) };
                Some(h)
            }
            Err(e) => {
                log::warn!("hwaccel: init {} упал, fallback на sw: {e}", accel.label());
                None
            }
        }
    };

    // MediaCodec → Surface: hw-device-контекст с Surface подвешивается на
    // кодек до открытия; без Surface декодер работает как обычный MediaCodec.
    #[cfg(target_os = "android")]
    let mut surface_output = false;
    #[cfg(target_os = "android")]
    if accel == HwAccel::MediaCodecSurface {
        match super::android::video_surface() {
            Some(surface) => {
                // SAFETY: codec_ctx ещё не открыт.
                match unsafe {
                    super::android::attach_surface_device(codec_ctx.as_mut_ptr(), surface)
                } {
                    Ok(()) => {
                        surface_output = true;
                        log::info!(
                            "hwaccel: MediaCodec выводит в Surface (без копирования кадров)"
                        );
                    }
                    Err(e) => log::warn!(
                        "hwaccel: mediacodec surface device: {e} — кадры пойдут через CPU"
                    ),
                }
            }
            None => log::warn!("hwaccel: video Surface недоступен — кадры пойдут через CPU"),
        }
    }
    #[cfg(not(target_os = "android"))]
    let surface_output = false;
    let _ = surface_output;

    // Отдельные hw-кодеки (NVDEC, MediaCodec): если кодек есть, но открыть
    // не удалось (нет JavaVM, устройство не поддерживает профиль) — не
    // роняем плеер, а пересоздаём контекст и открываем sw-декодер.
    let mut hw_codec_active = false;
    let hw_label = accel.label();
    let mut v_dec = if let Some(name) = accel.hw_codec_name(codec_id.into()) {
        match ffmpeg_next::codec::decoder::find_by_name(name) {
            Some(hw_codec) => {
                log::info!("hwaccel: открываю {}-декодер «{name}»", accel.label());
                match codec_ctx.decoder().open_as(hw_codec) {
                    Ok(opened) => {
                        hw_codec_active = true;
                        opened
                            .video()
                            .map_err(|e| VideoError::DecoderInit(format!("video decoder: {e}")))?
                    }
                    Err(e) => {
                        log::warn!("hwaccel: «{name}» не открылся ({e}) — fallback на sw");
                        let v_params = ictx
                            .stream(v_idx)
                            .ok_or(VideoError::NoVideoStream)?
                            .parameters();
                        let mut sw_ctx =
                            ffmpeg_next::codec::context::Context::from_parameters(v_params)
                                .map_err(|e| {
                                    VideoError::DecoderInit(format!("video params: {e}"))
                                })?;
                        unsafe {
                            (*sw_ctx.as_mut_ptr()).pkt_timebase = ffi::AVRational {
                                num: v_tb_av.numerator(),
                                den: v_tb_av.denominator(),
                            };
                        }
                        sw_ctx
                            .decoder()
                            .video()
                            .map_err(|e| VideoError::DecoderInit(format!("video decoder: {e}")))?
                    }
                }
            }
            None => {
                log::warn!("hwaccel: декодер «{name}» отсутствует в libavcodec — fallback на sw");
                codec_ctx
                    .decoder()
                    .video()
                    .map_err(|e| VideoError::DecoderInit(format!("video decoder: {e}")))?
            }
        }
    } else {
        codec_ctx
            .decoder()
            .video()
            .map_err(|e| VideoError::DecoderInit(format!("video decoder: {e}")))?
    };

    let v_tb = ictx.stream(v_idx).unwrap().time_base();
    let v_tb_f64 = v_tb.numerator() as f64 / v_tb.denominator() as f64;

    // До первого кадра формат может быть неизвестен (MediaCodec выставляет
    // pix_fmt только после старта кодека): sws_getContext с `None` падает,
    // поэтому стартуем с YUV420P, а `Scaler::convert` пересоздаст контекст
    // по фактическому формату кадра.
    let scaler_in_fmt = if hw.is_some() {
        ffmpeg_next::format::Pixel::NV12
    } else if matches!(
        v_dec.format(),
        ffmpeg_next::format::Pixel::None | ffmpeg_next::format::Pixel::MEDIACODEC
    ) {
        // MEDIACODEC — кадры на Surface, скейлер им не нужен; заглушка,
        // чтобы sws_getContext не падал на аппаратном формате.
        ffmpeg_next::format::Pixel::YUV420P
    } else {
        v_dec.format()
    };
    let mut scaler = Scaler::new(
        scaler_in_fmt,
        v_dec.width(),
        v_dec.height(),
        v_dec.width(),
        v_dec.height(),
    )?;

    let mut audio: Option<AudioState> = if let (Some(idx), Some(_)) = (a_idx, audio_tx.as_ref()) {
        let a_params = ictx.stream(idx).unwrap().parameters();
        let a_dec = ffmpeg_next::codec::context::Context::from_parameters(a_params)
            .map_err(|e| VideoError::DecoderInit(format!("audio params: {e}")))?
            .decoder()
            .audio()
            .map_err(|e| VideoError::DecoderInit(format!("audio decoder: {e}")))?;

        let in_layout = if a_dec.channel_layout().is_empty() {
            ChannelLayout::default(a_dec.channels() as i32)
        } else {
            a_dec.channel_layout()
        };
        let resampler = Resampler::new(a_dec.format(), in_layout, a_dec.rate(), AUDIO_OUTPUT_SR)?;
        let tb = ictx.stream(idx).unwrap().time_base();
        Some(AudioState {
            stream_idx: idx,
            decoder: a_dec,
            resampler,
            tb_sec: tb.numerator() as f64 / tb.denominator().max(1) as f64,
            base_pending: true,
            skip_before: None,
            timeline: AudioTimeline::new(),
        })
    } else {
        None
    };

    let mut paused = false;
    let mut logged_first_format = false;
    let mut tee_video: Option<SyncSender<Arc<VideoFrame>>> = None;
    let mut tee_audio: Option<SyncSender<Vec<f32>>> = None;

    // Демуксер уезжает в поток чтения; декодеру остаётся очередь пакетов.
    let (item_tx, item_rx) = mpsc::sync_channel::<ReaderItem>(READAHEAD_PACKETS);
    let (reader_tx, reader_rx) = mpsc::channel::<ReaderCmd>();
    let reader_join = thread::Builder::new()
        .name("syngui-video-reader".into())
        .spawn(move || run_reader_thread(ictx, item_tx, reader_rx))
        .map_err(|e| VideoError::Other(format!("spawn reader: {e}")))?;
    let mut seek_gen: u64 = 0;
    let mut reader = ReaderLink {
        cmd_tx: reader_tx,
        item_rx,
        join: Some(reader_join),
    };

    let mut video_errors: u32 = 0;
    let mut stages = StageStats::new();
    // Цель последней перемотки, пока до неё не дошли кадры.
    let mut video_skip_before: Option<f64> = None;
    'main: loop {
        if !paused {
            match cmd_rx.try_recv() {
                Ok(DecoderCmd::Pause) => paused = true,
                Ok(DecoderCmd::Resume) => {}
                Ok(DecoderCmd::SeekSec(t)) => {
                    perform_seek(&mut reader, &mut seek_gen, &mut v_dec, audio.as_mut(), t)?;
                    reset_audio_base(audio.as_mut(), audio_base);
                    stages.first_frame_pending = true;
                    video_skip_before = Some(t);
                    if let Some(a) = audio.as_mut() {
                        a.skip_before = Some(t);
                    }
                }
                Ok(DecoderCmd::ReAttachAudio(new_tx)) => {
                    audio_tx = Some(new_tx);
                    reset_audio_base(audio.as_mut(), audio_base);
                }
                Ok(DecoderCmd::InstallVideoTee(tx)) => {
                    tee_video = tx;
                }
                Ok(DecoderCmd::InstallAudioTee(tx)) => {
                    tee_audio = tx;
                }
                Ok(DecoderCmd::Stop) => break 'main,
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => break 'main,
            }
        }

        if paused {
            match cmd_rx.recv() {
                Ok(DecoderCmd::Resume) => paused = false,
                Ok(DecoderCmd::Pause) => {}
                Ok(DecoderCmd::SeekSec(t)) => {
                    perform_seek(&mut reader, &mut seek_gen, &mut v_dec, audio.as_mut(), t)?;
                    reset_audio_base(audio.as_mut(), audio_base);
                    stages.first_frame_pending = true;
                    video_skip_before = Some(t);
                    if let Some(a) = audio.as_mut() {
                        a.skip_before = Some(t);
                    }
                    paused = false;
                }
                Ok(DecoderCmd::ReAttachAudio(new_tx)) => {
                    audio_tx = Some(new_tx);
                    reset_audio_base(audio.as_mut(), audio_base);
                }
                Ok(DecoderCmd::InstallVideoTee(tx)) => {
                    tee_video = tx;
                }
                Ok(DecoderCmd::InstallAudioTee(tx)) => {
                    tee_audio = tx;
                }
                Ok(DecoderCmd::Stop) => break 'main,
                Err(_) => break 'main,
            }
            continue;
        }

        let packet = match reader.item_rx.recv() {
            Ok(ReaderItem::Packet(p)) => p,
            // Маркер устаревшей перемотки — пропускаем.
            Ok(ReaderItem::Flushed(..)) => continue,
            Err(_) => break 'main,
            Ok(ReaderItem::Eof) => {
                let _ = v_dec.send_eof();
                drain_video(
                    &mut v_dec,
                    &mut scaler,
                    hw.as_ref(),
                    &video_tx,
                    tee_video.as_ref(),
                    v_tb_f64,
                    &mut logged_first_format,
                    hw_codec_active.then_some(hw_label),
                    &mut stages,
                    &mut video_skip_before,
                    seek_gen,
                );
                if let Some(a) = audio.as_mut() {
                    let _ = a.decoder.send_eof();
                    drain_audio(a, audio_tx.as_ref(), tee_audio.as_ref(), audio_base);
                    if let Ok(tail) = a.resampler.flush() {
                        if !tail.is_empty() {
                            if let Some(tx) = tee_audio.as_ref() {
                                let _ = tx.try_send(tail.clone());
                            }
                            if let Some(tx) = audio_tx.as_ref() {
                                let _ = tx.send(tail);
                            }
                        }
                    }
                }
                match cmd_rx.recv() {
                    Ok(DecoderCmd::SeekSec(t)) => {
                        perform_seek(&mut reader, &mut seek_gen, &mut v_dec, audio.as_mut(), t)?;
                        reset_audio_base(audio.as_mut(), audio_base);
                        stages.first_frame_pending = true;
                        video_skip_before = Some(t);
                        if let Some(a) = audio.as_mut() {
                            a.skip_before = Some(t);
                        }
                    }
                    Ok(DecoderCmd::ReAttachAudio(new_tx)) => {
                        audio_tx = Some(new_tx);
                        reset_audio_base(audio.as_mut(), audio_base);
                    }
                    Ok(DecoderCmd::InstallVideoTee(tx)) => {
                        tee_video = tx;
                    }
                    Ok(DecoderCmd::InstallAudioTee(tx)) => {
                        tee_audio = tx;
                    }
                    Ok(DecoderCmd::Stop) | Err(_) => break 'main,
                    _ => {}
                }
                continue;
            }
        };

        let pkt_idx = packet.stream();

        if pkt_idx == v_idx {
            match v_dec.send_packet(&packet) {
                Ok(()) => {
                    video_errors = 0;
                    drain_video(
                        &mut v_dec,
                        &mut scaler,
                        hw.as_ref(),
                        &video_tx,
                        tee_video.as_ref(),
                        v_tb_f64,
                        &mut logged_first_format,
                        hw_codec_active.then_some(hw_label),
                        &mut stages,
                        &mut video_skip_before,
                        seek_gen,
                    );
                }
                Err(e) => {
                    video_errors += 1;
                    if video_errors == 1 || video_errors % 100 == 0 {
                        log::warn!("video: send_packet вернул ошибку ({video_errors} подряд): {e}");
                    }
                    // Кодек мёртв (например, у MediaCodec отняли Surface):
                    // молотить сеть и аудио на полной скорости бессмысленно.
                    if video_errors >= MAX_CONSECUTIVE_VIDEO_ERRORS {
                        log::error!("video: декодер не восстанавливается — останавливаю поток");
                        break 'main;
                    }
                }
            }
        } else if let Some(a) = audio.as_mut() {
            if pkt_idx == a.stream_idx && a.decoder.send_packet(&packet).is_ok() {
                drain_audio(a, audio_tx.as_ref(), tee_audio.as_ref(), audio_base);
            }
        }
    }

    reader.stop();
    Ok(())
}

/// Связь декодера с потоком чтения.
struct ReaderLink {
    cmd_tx: Sender<ReaderCmd>,
    item_rx: Receiver<ReaderItem>,
    join: Option<JoinHandle<()>>,
}

impl ReaderLink {
    fn stop(&mut self) {
        let _ = self.cmd_tx.send(ReaderCmd::Stop);
        // Ридер может стоять на send в полную очередь — освобождаем её.
        while self.item_rx.try_recv().is_ok() {}
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for ReaderLink {
    fn drop(&mut self) {
        self.stop();
    }
}

struct AudioState {
    stream_idx: usize,
    decoder: ffmpeg_next::decoder::Audio,
    resampler: Resampler,
    tb_sec: f64,
    /// Следующий отправленный чанк — первый в новом канале: его pts станет
    /// базой часов (`audio_base`).
    base_pending: bool,
    /// Цель точной перемотки: звук до неё не отправляется, кадр на границе
    /// обрезается по сэмплам.
    skip_before: Option<f64>,
    timeline: AudioTimeline,
}

/// Новый аудио-канал (открытие, перемотка): база часов неизвестна до
/// первого отправленного в него чанка.
fn reset_audio_base(audio: Option<&mut AudioState>, audio_base: &AtomicI64) {
    audio_base.store(PTS_UNKNOWN, Ordering::Release);
    if let Some(a) = audio {
        a.base_pending = true;
    }
}

/// Звуковая дорожка текущего канала: pts первого сэмпла и сколько сэмплов
/// отправлено с тех пор. Сэмплы обязаны идти вровень с pts: часы плеера
/// считают время по ним. Дыра в звуке (потерянный сегмент, ошибка
/// декодирования) иначе сдвигает звук относительно картинки, а когда звук
/// кончается раньше кадров — видео-очередь полна, часы стоят, декодер ждёт
/// места в очереди, и воспроизведение встаёт насовсем.
struct AudioTimeline {
    first_pts: f64,
    samples: u64,
    stats: bool,
    window_start: Instant,
}

/// Расхождение pts и сэмплов, которое выравниваем. Меньше — дрожание pts
/// контейнера; больше `AUDIO_GAP_MAX_SEC` — разрыв шкалы времени, а не дыра,
/// его тишиной не заполнить.
const AUDIO_GAP_MIN_SEC: f64 = 0.040;
const AUDIO_GAP_MAX_SEC: f64 = 2.0;

impl AudioTimeline {
    fn new() -> Self {
        Self {
            first_pts: 0.0,
            samples: 0,
            stats: std::env::var("SYNGUI_VIDEO_STATS").is_ok(),
            window_start: Instant::now(),
        }
    }

    fn reset(&mut self, pts: f64) {
        self.first_pts = pts;
        self.samples = 0;
        if self.stats {
            log::info!("[AUDIO SYNC] первый звук в канале: pts={pts:.3}s");
        }
    }

    /// Где должен начаться следующий чанк, если звук идёт без дыр.
    fn expected_pts(&self) -> f64 {
        self.first_pts + self.samples as f64 / AUDIO_OUTPUT_SR as f64
    }

    /// Подогнать чанк с этим `pts` к дорожке: дыру перед ним заполнить
    /// тишиной, перекрытие с уже отправленным — обрезать.
    fn align(&mut self, pts: f64, chunk: &mut Vec<f32>) {
        let gap = pts - self.expected_pts();
        if gap.abs() < AUDIO_GAP_MIN_SEC || gap.abs() > AUDIO_GAP_MAX_SEC {
            if gap.abs() > AUDIO_GAP_MAX_SEC {
                log::warn!("audio: разрыв шкалы pts {gap:+.3}s — не выравниваю");
            }
            return;
        }
        let n = (gap.abs() * AUDIO_OUTPUT_SR as f64).round() as usize;
        if gap > 0.0 {
            log::debug!("audio: дыра {:.0} мс — вставляю тишину", gap * 1000.0);
            chunk.splice(0..0, std::iter::repeat(0.0).take(n));
        } else {
            log::debug!("audio: перекрытие {:.0} мс — обрезаю", -gap * 1000.0);
            chunk.drain(..n.min(chunk.len()));
        }
    }

    fn on_sent(&mut self, samples: usize) {
        self.samples += samples as u64;
        if self.stats && self.window_start.elapsed() >= Duration::from_secs(1) {
            log::info!(
                "[AUDIO SYNC] отправлено звука {:.3}s с pts {:.3}s",
                self.samples as f64 / AUDIO_OUTPUT_SR as f64,
                self.first_pts
            );
            self.window_start = Instant::now();
        }
    }
}

/// Профиль конвейера кадра (`SYNGUI_VIDEO_STATS=1`): сколько времени уходит
/// на выгрузку кадра из GPU и на конверсию в RGBA. Оба этапа — на CPU, и на
/// 4K они и есть потолок частоты кадров.
struct StageStats {
    enabled: bool,
    window_start: Instant,
    frames: u32,
    transfer_us: u64,
    convert_us: u64,
    send_block_us: u64,
    /// Залогировать pts первого кадра после открытия/перемотки.
    first_frame_pending: bool,
}

impl StageStats {
    fn new() -> Self {
        Self {
            enabled: std::env::var("SYNGUI_VIDEO_STATS").is_ok(),
            window_start: Instant::now(),
            frames: 0,
            transfer_us: 0,
            convert_us: 0,
            send_block_us: 0,
            first_frame_pending: true,
        }
    }

    fn report_if_due(&mut self) {
        if !self.enabled || self.window_start.elapsed() < Duration::from_secs(1) {
            return;
        }
        let n = self.frames.max(1) as u64;
        log::info!(
            "[VIDEO PIPE 1s] кадров={} выгрузка из GPU={}us/кадр конверсия RGBA={}us/кадр ожидание очереди={}us/кадр",
            self.frames,
            self.transfer_us / n,
            self.convert_us / n,
            self.send_block_us / n,
        );
        self.window_start = Instant::now();
        self.frames = 0;
        self.transfer_us = 0;
        self.convert_us = 0;
        self.send_block_us = 0;
    }
}

fn drain_video(
    dec: &mut ffmpeg_next::decoder::Video,
    scaler: &mut Scaler,
    hw: Option<&HwContext>,
    tx: &SyncSender<VideoFrame>,
    tee_tx: Option<&SyncSender<Arc<VideoFrame>>>,
    tb_sec: f64,
    logged_first_format: &mut bool,
    hw_codec: Option<&'static str>,
    stages: &mut StageStats,
    skip_before: &mut Option<f64>,
    seek_generation: u64,
) {
    let mut decoded = frame::Video::empty();
    loop {
        match dec.receive_frame(&mut decoded) {
            Ok(()) => {}
            Err(e) => {
                let s = format!("{e:?}");
                let is_eagain = s.contains("11:") || s.contains("EAGAIN");
                let is_eof = s.contains("Eof");
                if !is_eagain && !is_eof {
                    log::warn!("video: receive_frame: {s}");
                }
                break;
            }
        }
        if !*logged_first_format {
            *logged_first_format = true;
            let fmt = decoded.format();
            match hw {
                Some(h) => {
                    let expected = ffmpeg_next::format::Pixel::from(h.hw_pix_fmt());
                    if fmt == expected {
                        log::info!(
                            "hwaccel: первый кадр пришёл в {:?} — HW-decode ({}) активен",
                            fmt,
                            h.label()
                        );
                    } else {
                        log::warn!(
                            "hwaccel: HW-устройство {} инициализировано, но декодер выдал {:?} (ожидался {:?}) — кодек не умеет этот hwaccel, идёт sw-decode",
                            h.label(),
                            fmt,
                            expected
                        );
                    }
                }
                None => {
                    if let Some(label) = hw_codec {
                        log::info!(
                            "hwaccel: первый кадр в {:?} — hw-кодек {label} (кадры в CPU-памяти)",
                            fmt
                        );
                    } else {
                        log::info!("hwaccel: первый кадр в {:?} — sw-decode", fmt);
                    }
                }
            }
        }
        let pts = decoded.pts().or_else(|| decoded.timestamp()).unwrap_or(0);
        let pts_sec = pts as f64 * tb_sec;
        // Точная перемотка: демуксер встаёт на ключевой кадр до цели (в HLS —
        // на начало сегмента, до 10 с раньше). Кадры до цели декодируются —
        // без них не собрать следующие, — но не выгружаются и не показываются.
        if let Some(target) = *skip_before {
            if decoded.pts().or_else(|| decoded.timestamp()).is_some() && pts_sec < target {
                continue;
            }
            *skip_before = None;
        }
        if stages.first_frame_pending {
            stages.first_frame_pending = false;
            if stages.enabled {
                log::info!("[VIDEO PIPE] первый кадр в очереди: pts={pts_sec:.3}s");
            }
        }

        // Кадр в буфере MediaCodec: показывает сам кодек, пикселей не берём.
        if decoded.format() == ffmpeg_next::format::Pixel::MEDIACODEC {
            // SAFETY: av_frame_clone добавляет ссылку на буфер кадра; кадр
            // живёт в SurfaceBuffer до показа или сброса.
            let cloned = unsafe {
                let ptr = ffi::av_frame_clone(decoded.as_ptr());
                if ptr.is_null() {
                    continue;
                }
                frame::Video::wrap(ptr)
            };
            let frame = VideoFrame {
                width: decoded.width(),
                height: decoded.height(),
                rgba: Arc::from(Vec::new().into_boxed_slice()),
                pts_sec,
                seek_generation,
                surface: Some(Arc::new(SurfaceBuffer::new(cloned))),
            };
            if let Some(t) = tee_tx {
                let _ = t.try_send(Arc::new(frame.clone()));
            }
            if tx.send(frame).is_err() {
                return;
            }
            continue;
        }

        let (w, h) = scaler.out_size();

        let owned_sw_frame;
        let frame_for_scaler: &frame::Video = match hw {
            Some(h) if decoded.format() == ffmpeg_next::format::Pixel::from(h.hw_pix_fmt()) => {
                let t = Instant::now();
                let r = h.transfer_to_cpu(&decoded);
                stages.transfer_us += t.elapsed().as_micros() as u64;
                match r {
                    Ok(sw) => {
                        owned_sw_frame = sw;
                        &owned_sw_frame
                    }
                    Err(e) => {
                        eprintln!("[syngui/video] hwframe transfer: {e}");
                        continue;
                    }
                }
            }
            _ => &decoded,
        };

        let t_convert = Instant::now();
        let converted = scaler.convert(frame_for_scaler);
        stages.convert_us += t_convert.elapsed().as_micros() as u64;
        stages.frames += 1;
        match converted {
            Ok(rgba) => {
                let frame = VideoFrame {
                    width: w,
                    height: h,
                    rgba,
                    pts_sec,
                    seek_generation,
                    surface: None,
                };
                if let Some(t) = tee_tx {
                    let _ = t.try_send(Arc::new(frame.clone()));
                }
                let t_send = Instant::now();
                let sent = tx.send(frame);
                stages.send_block_us += t_send.elapsed().as_micros() as u64;
                stages.report_if_due();
                if sent.is_err() {
                    return;
                }
            }
            Err(e) => {
                eprintln!("[syngui/video] scaler convert: {e}");
            }
        }
    }
}

fn drain_audio(
    state: &mut AudioState,
    tx: Option<&SyncSender<Vec<f32>>>,
    tee_tx: Option<&SyncSender<Vec<f32>>>,
    audio_base: &AtomicI64,
) {
    let mut decoded = frame::Audio::empty();
    while state.decoder.receive_frame(&mut decoded).is_ok() {
        let pts_sec = decoded
            .pts()
            .or_else(|| decoded.timestamp())
            .map(|p| p as f64 * state.tb_sec);
        match state.resampler.convert(&decoded) {
            Ok(mut chunk) if !chunk.is_empty() => {
                let mut pts_sec = pts_sec;
                if let (Some(target), Some(pts)) = (state.skip_before, pts_sec) {
                    let skip = ((target - pts) * AUDIO_OUTPUT_SR as f64).round();
                    if skip >= chunk.len() as f64 {
                        continue;
                    }
                    if skip > 0.0 {
                        chunk.drain(..skip as usize);
                        pts_sec = Some(target);
                    }
                    state.skip_before = None;
                }
                if let Some(pts) = pts_sec {
                    if state.base_pending && tx.is_some() {
                        state.base_pending = false;
                        audio_base.store((pts * 1_000_000.0) as i64, Ordering::Release);
                        state.timeline.reset(pts);
                    } else {
                        state.timeline.align(pts, &mut chunk);
                    }
                }
                if chunk.is_empty() {
                    continue;
                }
                state.timeline.on_sent(chunk.len());
                if let Some(t) = tee_tx {
                    let _ = t.try_send(chunk.clone());
                }
                if let Some(tx) = tx {
                    if tx.send(chunk).is_err() {
                        return;
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("[syngui/video] resampler convert: {e}");
            }
        }
    }
}

fn perform_seek(
    reader: &mut ReaderLink,
    seek_gen: &mut u64,
    v_dec: &mut ffmpeg_next::decoder::Video,
    audio: Option<&mut AudioState>,
    target_sec: f64,
) -> Result<(), VideoError> {
    *seek_gen += 1;
    let gen = *seek_gen;
    reader
        .cmd_tx
        .send(ReaderCmd::Seek(target_sec, gen))
        .map_err(|_| VideoError::Seek("поток чтения завершился".into()))?;
    // Выбрасываем упреждающий буфер до маркера нашей перемотки.
    loop {
        match reader.item_rx.recv() {
            Ok(ReaderItem::Flushed(g, err)) if g == gen => {
                if let Some(e) = err {
                    return Err(VideoError::Seek(e));
                }
                break;
            }
            Ok(_) => continue,
            Err(_) => return Err(VideoError::Seek("поток чтения завершился".into())),
        }
    }
    v_dec.flush();
    if let Some(a) = audio {
        a.decoder.flush();
    }
    Ok(())
}

#[allow(dead_code)]
fn brief_yield() {
    thread::sleep(Duration::from_millis(1));
}

#[allow(dead_code)]
fn _sample_fmt_doc(_s: SampleFmt) {}
