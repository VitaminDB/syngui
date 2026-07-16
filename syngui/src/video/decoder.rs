use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

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
    pub rgba: Arc<[u8]>,
    pub pts_sec: f64,
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
}

const AUDIO_OUTPUT_SR: u32 = 48_000;

const VIDEO_QUEUE_CAP: usize = 8;
const AUDIO_QUEUE_CAP: usize = 64;

const VIDEO_TEE_QUEUE_CAP: usize = 4;
const AUDIO_TEE_QUEUE_CAP: usize = 32;

const AV_TIME_BASE_F64: f64 = 1_000_000.0;

impl VideoDecoder {
    pub fn open(input: &str) -> Result<Self, VideoError> {
        Self::open_with_hwaccel(input, HwAccel::None)
    }

    pub fn open_with_hwaccel(input: &str, accel: HwAccel) -> Result<Self, VideoError> {
        ffmpeg_next::init().ok();

        let ictx = ffmpeg_next::format::input(&input.to_string())
            .map_err(|e| VideoError::Open(format!("{input}: {e}")))?;

        let meta = read_meta(&ictx)?;

        let (video_tx, video_rx) = mpsc::sync_channel::<VideoFrame>(VIDEO_QUEUE_CAP);
        let (audio_tx_opt, audio_rx_opt) = if meta.has_audio {
            let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(AUDIO_QUEUE_CAP);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let (cmd_tx, cmd_rx) = mpsc::channel::<DecoderCmd>();

        let meta_thread = meta.clone();
        let join = thread::Builder::new()
            .name("syngui-video-decoder".into())
            .spawn(move || {
                run_decoder_thread(ictx, meta_thread, accel, video_tx, audio_tx_opt, cmd_rx)
            })
            .map_err(|e| VideoError::Other(format!("spawn decoder: {e}")))?;

        Ok(Self {
            meta,
            video_rx,
            audio_rx: audio_rx_opt,
            cmd_tx,
            join: Some(join),
        })
    }

    pub fn meta(&self) -> &VideoMeta {
        &self.meta
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
        self.cmd_tx.send(DecoderCmd::InstallVideoTee(Some(tx))).ok()?;
        Some(rx)
    }

    pub fn install_audio_tee(&self) -> Option<Receiver<Vec<f32>>> {
        if !self.meta.has_audio {
            return None;
        }
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(AUDIO_TEE_QUEUE_CAP);
        self.cmd_tx.send(DecoderCmd::InstallAudioTee(Some(tx))).ok()?;
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
            while self.video_rx.try_recv().is_ok() {}
            if let Some(rx) = &self.audio_rx {
                while rx.try_recv().is_ok() {}
            }
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
    mut ictx: Input,
    _meta: VideoMeta,
    accel: HwAccel,
    video_tx: SyncSender<VideoFrame>,
    mut audio_tx: Option<SyncSender<Vec<f32>>>,
    cmd_rx: Receiver<DecoderCmd>,
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

    let hw: Option<HwContext> = if matches!(accel, HwAccel::None) {
        None
    } else {
        match HwContext::try_init(accel) {
            Ok(h) => {
                // SAFETY: codec_ctx ещё не открыт (decoder().video() ниже),
                unsafe { h.attach_to(codec_ctx.as_mut_ptr()) };
                Some(h)
            }
            Err(e) => {
                log::warn!(
                    "hwaccel: init {} упал, fallback на sw: {e}",
                    accel.label()
                );
                None
            }
        }
    };

    let mut v_dec = if let Some(name) = accel.nvdec_codec_name(codec_id.into()) {
        match ffmpeg_next::codec::decoder::find_by_name(name) {
            Some(cuvid) => {
                log::info!("hwaccel: открываю NVDEC-декодер «{name}»");
                codec_ctx
                    .decoder()
                    .open_as(cuvid)
                    .map_err(|e| VideoError::DecoderInit(format!("{name}: {e}")))?
                    .video()
                    .map_err(|e| VideoError::DecoderInit(format!("video decoder: {e}")))?
            }
            None => {
                log::warn!(
                    "hwaccel: NVDEC-декодер «{name}» отсутствует в libavcodec — fallback на sw"
                );
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

    let scaler_in_fmt = if hw.is_some() {
        ffmpeg_next::format::Pixel::NV12
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
        let resampler = Resampler::new(
            a_dec.format(),
            in_layout,
            a_dec.rate(),
            AUDIO_OUTPUT_SR,
        )?;
        Some(AudioState {
            stream_idx: idx,
            decoder: a_dec,
            resampler,
        })
    } else {
        None
    };

    let mut paused = false;
    let mut logged_first_format = false;
    let mut tee_video: Option<SyncSender<Arc<VideoFrame>>> = None;
    let mut tee_audio: Option<SyncSender<Vec<f32>>> = None;

    'main: loop {
        if !paused {
            match cmd_rx.try_recv() {
                Ok(DecoderCmd::Pause) => paused = true,
                Ok(DecoderCmd::Resume) => {}
                Ok(DecoderCmd::SeekSec(t)) => {
                    perform_seek(&mut ictx, &mut v_dec, audio.as_mut(), t)?;
                }
                Ok(DecoderCmd::ReAttachAudio(new_tx)) => {
                    audio_tx = Some(new_tx);
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
                    perform_seek(&mut ictx, &mut v_dec, audio.as_mut(), t)?;
                    paused = false;
                }
                Ok(DecoderCmd::ReAttachAudio(new_tx)) => {
                    audio_tx = Some(new_tx);
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

        let mut packet = ffmpeg_next::Packet::empty();
        match packet.read(&mut ictx) {
            Ok(()) => {}
            Err(ffmpeg_next::Error::Eof) => {
                let _ = v_dec.send_eof();
                drain_video(
                    &mut v_dec,
                    &mut scaler,
                    hw.as_ref(),
                    &video_tx,
                    tee_video.as_ref(),
                    v_tb_f64,
                    &mut logged_first_format,
                );
                if let Some(a) = audio.as_mut() {
                    let _ = a.decoder.send_eof();
                    drain_audio(a, audio_tx.as_ref(), tee_audio.as_ref());
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
                        perform_seek(&mut ictx, &mut v_dec, audio.as_mut(), t)?;
                    }
                    Ok(DecoderCmd::ReAttachAudio(new_tx)) => {
                        audio_tx = Some(new_tx);
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
            Err(_e) => {
                continue;
            }
        }

        let pkt_idx = packet.stream();

        if pkt_idx == v_idx {
            match v_dec.send_packet(&packet) {
                Ok(()) => {
                    drain_video(
                        &mut v_dec,
                        &mut scaler,
                        hw.as_ref(),
                        &video_tx,
                        tee_video.as_ref(),
                        v_tb_f64,
                        &mut logged_first_format,
                    );
                }
                Err(e) => {
                    log::warn!("video: send_packet вернул ошибку: {e}");
                }
            }
        } else if let Some(a) = audio.as_mut() {
            if pkt_idx == a.stream_idx && a.decoder.send_packet(&packet).is_ok() {
                drain_audio(a, audio_tx.as_ref(), tee_audio.as_ref());
            }
        }
    }

    Ok(())
}

struct AudioState {
    stream_idx: usize,
    decoder: ffmpeg_next::decoder::Audio,
    resampler: Resampler,
}

fn drain_video(
    dec: &mut ffmpeg_next::decoder::Video,
    scaler: &mut Scaler,
    hw: Option<&HwContext>,
    tx: &SyncSender<VideoFrame>,
    tee_tx: Option<&SyncSender<Arc<VideoFrame>>>,
    tb_sec: f64,
    logged_first_format: &mut bool,
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
                    log::info!("hwaccel: первый кадр в {:?} — sw-decode", fmt);
                }
            }
        }
        let pts = decoded.pts().unwrap_or(0);
        let pts_sec = pts as f64 * tb_sec;
        let (w, h) = scaler.out_size();

        let owned_sw_frame;
        let frame_for_scaler: &frame::Video = match hw {
            Some(h) if decoded.format() == ffmpeg_next::format::Pixel::from(h.hw_pix_fmt()) => match h.transfer_to_cpu(&decoded) {
                Ok(sw) => {
                    owned_sw_frame = sw;
                    &owned_sw_frame
                }
                Err(e) => {
                    eprintln!("[syngui/video] hwframe transfer: {e}");
                    continue;
                }
            },
            _ => &decoded,
        };

        match scaler.convert(frame_for_scaler) {
            Ok(rgba) => {
                let frame = VideoFrame {
                    width: w,
                    height: h,
                    rgba,
                    pts_sec,
                };
                if let Some(t) = tee_tx {
                    let _ = t.try_send(Arc::new(frame.clone()));
                }
                if tx.send(frame).is_err() {
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
) {
    let mut decoded = frame::Audio::empty();
    while state.decoder.receive_frame(&mut decoded).is_ok() {
        match state.resampler.convert(&decoded) {
            Ok(chunk) if !chunk.is_empty() => {
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
    ictx: &mut Input,
    v_dec: &mut ffmpeg_next::decoder::Video,
    audio: Option<&mut AudioState>,
    target_sec: f64,
) -> Result<(), VideoError> {
    let target_ts = (target_sec * AV_TIME_BASE_F64) as i64;
    if let Err(e) = ictx.seek(target_ts, ..target_ts) {
        return Err(VideoError::Seek(format!(
            "ictx.seek({target_sec:.3}s): {e}"
        )));
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
