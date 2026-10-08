//! Аппаратное декодирование через V4L2 stateful-декодер (Linux, `HwAccel::V4l2`).
//!
//! Декодеры систем на кристалле (Qualcomm `msm_vidc`/venus, MediaTek, Rockchip
//! hantro в stateful-режиме…) — устройства V4L2 mem2mem: в OUTPUT-очередь
//! подаётся сжатый поток (H.264/HEVC — Annex B), из CAPTURE приходят кадры NV12.
//! Обёртки ffmpeg `*_v4l2m2m` умеют только буферы MMAP, а downstream-драйверы
//! Android-ядер (`msm_vidc`) дают лишь DMABUF — поэтому здесь свой декодер:
//! буферы берутся из dma-heap (`/dev/dma_heap/system`) и отображаются в память.
//!
//! Снаружи — как декодер libavcodec: [`V4l2Decoder::send_packet`] /
//! [`V4l2Decoder::receive_frame`] (кадры NV12 в CPU-памяти, дальше — общий
//! конвейер с `Scaler`), `send_eof`, `flush`. Перемотка пересоздаёт декодер.

use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::ptr;
use std::time::{Duration, Instant};

use ffmpeg_next::ffi;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame;
use ffmpeg_next::packet::Ref as _;

use super::error::VideoError;

// ─── videodev2.h (LP64) ─────────────────────────────────────────────────────

const BUF_TYPE_CAPTURE_MPLANE: u32 = 9;
const BUF_TYPE_OUTPUT_MPLANE: u32 = 10;
const MEMORY_DMABUF: u32 = 4;
const CAP_VIDEO_CAPTURE_MPLANE: u32 = 0x0000_1000;
const CAP_VIDEO_OUTPUT_MPLANE: u32 = 0x0000_2000;
const CAP_VIDEO_M2M_MPLANE: u32 = 0x0000_4000;
const CAP_DEVICE_CAPS: u32 = 0x8000_0000;
const EVENT_EOS: u32 = 2;
const EVENT_SOURCE_CHANGE: u32 = 5;
const DEC_CMD_STOP: u32 = 1;
const BUF_FLAG_LAST: u32 = 0x0010_0000;
const BUF_FLAG_ERROR: u32 = 0x0000_0040;
const CID_MIN_BUFFERS_FOR_CAPTURE: u32 = 0x0098_0900 + 39;

const fn fourcc(s: &[u8; 4]) -> u32 {
    s[0] as u32 | (s[1] as u32) << 8 | (s[2] as u32) << 16 | (s[3] as u32) << 24
}
const PIX_NV12: u32 = fourcc(b"NV12");

#[repr(C)]
struct Capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

#[repr(C)]
struct FmtDesc {
    index: u32,
    type_: u32,
    flags: u32,
    description: [u8; 32],
    pixelformat: u32,
    mbus_code: u32,
    reserved: [u32; 3],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct PlanePixFormat {
    sizeimage: u32,
    bytesperline: u32,
    reserved: [u16; 6],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct PixFormatMplane {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    colorspace: u32,
    plane_fmt: [PlanePixFormat; 8],
    num_planes: u8,
    flags: u8,
    ycbcr_enc: u8,
    quantization: u8,
    xfer_func: u8,
    reserved: [u8; 7],
}

#[repr(C)]
union FormatUnion {
    pix_mp: PixFormatMplane,
    raw: [u8; 200],
    _align: [u64; 25],
}

#[repr(C)]
struct Format {
    type_: u32,
    fmt: FormatUnion,
}

#[repr(C)]
struct RequestBuffers {
    count: u32,
    type_: u32,
    memory: u32,
    capabilities: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Plane {
    bytesused: u32,
    length: u32,
    m: u64, // fd в младших 32 битах
    data_offset: u32,
    reserved: [u32; 11],
}

#[repr(C)]
struct Timecode {
    type_: u32,
    flags: u32,
    frames: u8,
    seconds: u8,
    minutes: u8,
    hours: u8,
    userbits: [u8; 4],
}

#[repr(C)]
struct Buffer {
    index: u32,
    type_: u32,
    bytesused: u32,
    flags: u32,
    field: u32,
    timestamp: libc::timeval,
    timecode: Timecode,
    sequence: u32,
    memory: u32,
    m: u64, // *mut Plane для mplane
    length: u32,
    reserved2: u32,
    request_fd: i32,
}

#[repr(C)]
struct EventSubscription {
    type_: u32,
    id: u32,
    flags: u32,
    reserved: [u32; 5],
}

#[repr(C)]
struct Event {
    type_: u32,
    u: [u64; 8],
    pending: u32,
    sequence: u32,
    timestamp: libc::timespec,
    id: u32,
    reserved: [u32; 8],
}

#[repr(C)]
struct DecoderCmd {
    cmd: u32,
    flags: u32,
    raw: [u32; 16],
}

#[repr(C)]
struct Control {
    id: u32,
    value: i32,
}

#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

#[repr(C)]
struct Crop {
    type_: u32,
    c: Rect,
}

#[repr(C)]
struct HeapAlloc {
    len: u64,
    fd: u32,
    fd_flags: u32,
    heap_flags: u64,
}

const fn ioc(dir: u32, ty: u8, nr: u8, size: usize) -> libc::c_ulong {
    ((dir << 30) | ((size as u32) << 16) | ((ty as u32) << 8) | nr as u32) as libc::c_ulong
}
const W: u32 = 1;
const R: u32 = 2;
const RW: u32 = 3;
const QUERYCAP: libc::c_ulong = ioc(R, b'V', 0, std::mem::size_of::<Capability>());
const ENUM_FMT: libc::c_ulong = ioc(RW, b'V', 2, std::mem::size_of::<FmtDesc>());
const G_FMT: libc::c_ulong = ioc(RW, b'V', 4, std::mem::size_of::<Format>());
const S_FMT: libc::c_ulong = ioc(RW, b'V', 5, std::mem::size_of::<Format>());
const REQBUFS: libc::c_ulong = ioc(RW, b'V', 8, std::mem::size_of::<RequestBuffers>());
const QBUF: libc::c_ulong = ioc(RW, b'V', 15, std::mem::size_of::<Buffer>());
const DQBUF: libc::c_ulong = ioc(RW, b'V', 17, std::mem::size_of::<Buffer>());
const STREAMON: libc::c_ulong = ioc(W, b'V', 18, 4);
const STREAMOFF: libc::c_ulong = ioc(W, b'V', 19, 4);
const G_CTRL: libc::c_ulong = ioc(RW, b'V', 27, std::mem::size_of::<Control>());
const G_CROP: libc::c_ulong = ioc(RW, b'V', 59, std::mem::size_of::<Crop>());
const DQEVENT: libc::c_ulong = ioc(R, b'V', 89, std::mem::size_of::<Event>());
const SUBSCRIBE_EVENT: libc::c_ulong = ioc(W, b'V', 90, std::mem::size_of::<EventSubscription>());
const DECODER_CMD: libc::c_ulong = ioc(RW, b'V', 96, std::mem::size_of::<DecoderCmd>());
const DMA_BUF_SYNC: libc::c_ulong = ioc(W, b'b', 0, 8);
const SYNC_READ: u64 = 1;
const SYNC_WRITE: u64 = 2;
const SYNC_END: u64 = 4;
const HEAP_ALLOC: libc::c_ulong = ioc(RW, b'H', 0, std::mem::size_of::<HeapAlloc>());

const _: () = assert!(std::mem::size_of::<Format>() == 208);
const _: () = assert!(std::mem::size_of::<Buffer>() == 88);
const _: () = assert!(std::mem::size_of::<Plane>() == 64);
const _: () = assert!(std::mem::size_of::<Event>() == 136);
const _: () = assert!(std::mem::size_of::<PixFormatMplane>() == 192);

fn xioctl<T>(fd: RawFd, req: libc::c_ulong, arg: &mut T) -> std::io::Result<()> {
    loop {
        // SAFETY: arg — структура той раскладки, которую ждёт ioctl `req`.
        let r = unsafe { libc::ioctl(fd, req as _, arg as *mut T) };
        if r >= 0 {
            return Ok(());
        }
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EINTR) {
            return Err(e);
        }
    }
}

fn zeroed<T>() -> T {
    // SAFETY: только POD-структуры ядра, у которых нулевое значение допустимо.
    unsafe { std::mem::zeroed() }
}

// ─── dma-heap ───────────────────────────────────────────────────────────────

/// Буфер из dma-heap, отображённый в память.
struct DmaBuf {
    fd: OwnedFd,
    map: *mut u8,
    len: usize,
}

unsafe impl Send for DmaBuf {}
// SAFETY: буфер выхода только читается (кадр отдаётся GPU или копируется); пишет в него устройство.
unsafe impl Sync for DmaBuf {}

impl DmaBuf {
    fn alloc(len: usize) -> Result<Self, VideoError> {
        let heap = ["/dev/dma_heap/system", "/dev/dma_heap/qcom,system"]
            .iter()
            .find_map(|p| File::options().read(true).write(true).open(p).ok())
            .ok_or_else(|| VideoError::DecoderInit("v4l2: нет /dev/dma_heap/system".into()))?;
        let mut a = HeapAlloc { len: len as u64, fd: 0, fd_flags: (libc::O_RDWR | libc::O_CLOEXEC) as u32, heap_flags: 0 };
        xioctl(heap.as_raw_fd(), HEAP_ALLOC, &mut a).map_err(|e| VideoError::DecoderInit(format!("v4l2: dma-heap {len} байт: {e}")))?;
        // SAFETY: fd только что выдан ядром и больше никем не владеет.
        let fd = unsafe { OwnedFd::from_raw_fd(a.fd as RawFd) };
        // SAFETY: отображаем выделенный буфер целиком.
        let map = unsafe { libc::mmap(ptr::null_mut(), len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd.as_raw_fd(), 0) };
        if map == libc::MAP_FAILED {
            return Err(VideoError::DecoderInit(format!("v4l2: mmap dma-buf: {}", std::io::Error::last_os_error())));
        }
        Ok(Self { fd, map: map as *mut u8, len })
    }

    /// Согласование кэша CPU с устройством (начало/конец доступа процессора).
    fn sync(&self, flags: u64) {
        let mut f = flags;
        let _ = xioctl(self.fd.as_raw_fd(), DMA_BUF_SYNC, &mut f);
    }

    fn slice(&self) -> &[u8] {
        // SAFETY: отображение живо, пока жив DmaBuf.
        unsafe { std::slice::from_raw_parts(self.map, self.len) }
    }

    fn slice_mut(&mut self) -> &mut [u8] {
        // SAFETY: как выше; буфер наш, пока не поставлен в очередь.
        unsafe { std::slice::from_raw_parts_mut(self.map, self.len) }
    }
}

impl Drop for DmaBuf {
    fn drop(&mut self) {
        // SAFETY: отображение создано в alloc().
        unsafe { libc::munmap(self.map as *mut _, self.len) };
    }
}

// ─── поиск устройства ───────────────────────────────────────────────────────

fn codec_fourcc(id: ffi::AVCodecID) -> Option<(u32, Option<&'static str>)> {
    // (формат V4L2, фильтр потока libavcodec)
    Some(match id {
        ffi::AVCodecID::AV_CODEC_ID_H264 => (fourcc(b"H264"), Some("h264_mp4toannexb")),
        ffi::AVCodecID::AV_CODEC_ID_HEVC => (fourcc(b"HEVC"), Some("hevc_mp4toannexb")),
        ffi::AVCodecID::AV_CODEC_ID_VP9 => (fourcc(b"VP90"), Some("vp9_superframe_split")),
        ffi::AVCodecID::AV_CODEC_ID_VP8 => (fourcc(b"VP80"), None),
        ffi::AVCodecID::AV_CODEC_ID_MPEG2VIDEO => (fourcc(b"MPG2"), None),
        _ => return None,
    })
}

fn is_decoder_for(fd: RawFd, want: u32) -> bool {
    let mut cap: Capability = zeroed();
    if xioctl(fd, QUERYCAP, &mut cap).is_err() {
        return false;
    }
    let caps = if cap.capabilities & CAP_DEVICE_CAPS != 0 { cap.device_caps } else { cap.capabilities };
    let m2m = caps & CAP_VIDEO_M2M_MPLANE != 0
        || (caps & CAP_VIDEO_CAPTURE_MPLANE != 0 && caps & CAP_VIDEO_OUTPUT_MPLANE != 0);
    if !m2m {
        return false;
    }
    (0..64).any(|i| {
        let mut d: FmtDesc = zeroed();
        d.index = i;
        d.type_ = BUF_TYPE_OUTPUT_MPLANE;
        xioctl(fd, ENUM_FMT, &mut d).is_ok() && d.pixelformat == want
    }) && (0..64).any(|i| {
        let mut d: FmtDesc = zeroed();
        d.index = i;
        d.type_ = BUF_TYPE_CAPTURE_MPLANE;
        xioctl(fd, ENUM_FMT, &mut d).is_ok() && d.pixelformat == PIX_NV12
    })
}

/// Есть ли в системе V4L2-декодер для этого кодека (выбор `HwAccel::Auto`).
pub fn decoder_available(codec_id: ffi::AVCodecID) -> bool {
    codec_fourcc(codec_id).is_some_and(|(f, _)| find_device(f).is_some())
}

fn find_device(want: u32) -> Option<File> {
    let mut paths: Vec<_> = std::fs::read_dir("/dev")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("video")))
        .collect();
    paths.sort();
    paths.into_iter().find_map(|p| {
        let f = File::options().read(true).write(true).custom_flags(libc::O_NONBLOCK).open(&p).ok()?;
        is_decoder_for(f.as_raw_fd(), want).then(|| {
            log::info!("v4l2: декодер {}", p.display());
            f
        })
    })
}

use std::os::unix::fs::OpenOptionsExt;

// ─── фильтр потока ──────────────────────────────────────────────────────────

/// `*_mp4toannexb` и т.п.: пакеты MP4/MKV (длины NAL + avcC) → Annex B.
struct Bsf(*mut ffi::AVBSFContext);

unsafe impl Send for Bsf {}

impl Bsf {
    fn new(name: &str, par: *const ffi::AVCodecParameters, tb: ffi::AVRational) -> Result<Self, VideoError> {
        let cname = CString::new(name).unwrap();
        // SAFETY: обычная последовательность av_bsf_*; ошибки проверяются.
        unsafe {
            let f = ffi::av_bsf_get_by_name(cname.as_ptr());
            if f.is_null() {
                return Err(VideoError::DecoderInit(format!("v4l2: нет фильтра {name}")));
            }
            let mut ctx = ptr::null_mut();
            if ffi::av_bsf_alloc(f, &mut ctx) < 0 {
                return Err(VideoError::DecoderInit("v4l2: av_bsf_alloc".into()));
            }
            ffi::avcodec_parameters_copy((*ctx).par_in, par);
            (*ctx).time_base_in = tb;
            if ffi::av_bsf_init(ctx) < 0 {
                ffi::av_bsf_free(&mut ctx);
                return Err(VideoError::DecoderInit(format!("v4l2: av_bsf_init {name}")));
            }
            Ok(Self(ctx))
        }
    }
}

impl Drop for Bsf {
    fn drop(&mut self) {
        // SAFETY: контекст создан в new().
        unsafe { ffi::av_bsf_free(&mut self.0) };
    }
}

// ─── декодер ────────────────────────────────────────────────────────────────

const N_INPUT: usize = 8;
const INPUT_SIZE: u32 = 8 << 20;

struct Capture {
    bufs: Vec<std::sync::Arc<DmaBuf>>,
    /// Свой номер у каждого буфера на всё время работы процесса (кэш импорта на GPU).
    ids: Vec<u64>,
    /// Поколение буферов: возвращённые кадрами буферы прошлых поколений в очередь не идут.
    gen: u64,
    /// Кадры отдаются прямо в dma-buf (GPU импортирует их без копии); буфер
    /// возвращается в очередь, когда кадр больше не нужен.
    zero_copy: bool,
    stride: u32,
    /// Строк в плоскости Y (дальше — UV): высота формата, выровненная драйвером
    /// (у `msm_vidc` для 4K — 2176), а не видимая.
    y_lines: u32,
    visible_w: u32,
    visible_h: u32,
}

/// Кадр декодера: AVFrame NV12 или (в упакованном режиме) плоскости подряд
/// для перевода цвета на GPU — одним копированием из dma-buf, без AVFrame.
pub enum V4l2Frame {
    Av(frame::Video),
    Packed(crate::gpu::YuvFrame, Option<i64>),
}

/// Сколько упакованных буферов держать в обороте (очередь плеера + показ).
const PACKED_POOL: usize = 12;
/// Сколько пакетов может ждать входного буфера, прежде чем `send_packet` начнёт ждать декодер.
const PENDING_MAX: usize = 16;
/// Сверх минимума драйвера при кадрах без копии: очередь кадров плеера
/// (`VIDEO_QUEUE_CAP` декодера) + показанные (держит GPU-кэш) + запас.
const ZERO_COPY_EXTRA: i32 = 8 + 3 + 2;

/// V4L2 stateful-декодер с кадрами NV12 в CPU-памяти.
pub struct V4l2Decoder {
    fd: File,
    v4l2_fmt: u32,
    bsf: Option<Bsf>,
    inputs: Vec<DmaBuf>,
    input_free: Vec<bool>,
    capture: Option<Capture>,
    ready: VecDeque<V4l2Frame>,
    /// Кадры сразу упакованными плоскостями для GPU ([`Self::set_packed`]).
    packed: bool,
    /// Оборот буферов упакованных кадров.
    pool: Vec<std::sync::Arc<[u8]>>,
    /// Метка входного буфера → pts пакета (драйвер копирует метку в кадр).
    pts_by_tag: HashMap<u64, Option<i64>>,
    next_tag: u64,
    eos_sent: bool,
    eos_done: bool,
    /// Пакеты, ждущие свободного входного буфера: `send_packet` не ждёт —
    /// иначе при кадрах без копии он ждал бы входа, пока все буферы выхода
    /// держат готовые кадры, которые разбирают только после его возврата.
    pending_in: VecDeque<(Vec<u8>, Option<i64>)>,
    /// Конец потока — после того, как все ждущие пакеты уйдут в декодер.
    stop_pending: bool,
    /// Буферы выхода, освобождённые кадрами (поколение, индекс), — в очередь снова.
    returned: std::sync::Arc<std::sync::Mutex<Vec<(u64, usize)>>>,
    capture_gen: u64,
    // для пересоздания
    codec_id: ffi::AVCodecID,
    params: *mut ffi::AVCodecParameters,
    time_base: ffi::AVRational,
}

unsafe impl Send for V4l2Decoder {}

impl V4l2Decoder {
    /// Декодер для потока с этими параметрами; `None`/ошибка — кодека или устройства нет.
    pub fn new(par: *const ffi::AVCodecParameters, time_base: ffi::AVRational) -> Result<Self, VideoError> {
        // SAFETY: par — параметры потока демуксера, живы на время вызова; копируем их себе.
        let (codec_id, w, h, params) = unsafe {
            let copy = ffi::avcodec_parameters_alloc();
            ffi::avcodec_parameters_copy(copy, par);
            ((*par).codec_id, (*par).width.max(16) as u32, (*par).height.max(16) as u32, copy)
        };
        let (v4l2_fmt, bsf_name) =
            codec_fourcc(codec_id).ok_or_else(|| VideoError::DecoderInit(format!("v4l2: кодек {codec_id:?} не поддержан")))?;
        let fd = find_device(v4l2_fmt).ok_or_else(|| VideoError::DecoderInit("v4l2: нет декодера для кодека".into()))?;
        let raw = fd.as_raw_fd();

        let mut of: Format = zeroed();
        of.type_ = BUF_TYPE_OUTPUT_MPLANE;
        // SAFETY: запись в поле pix_mp union'а.
        unsafe {
            of.fmt.pix_mp.width = w;
            of.fmt.pix_mp.height = h;
            of.fmt.pix_mp.pixelformat = v4l2_fmt;
            of.fmt.pix_mp.num_planes = 1;
            of.fmt.pix_mp.plane_fmt[0].sizeimage = INPUT_SIZE;
        }
        xioctl(raw, S_FMT, &mut of).map_err(|e| VideoError::DecoderInit(format!("v4l2: S_FMT вход: {e}")))?;
        // Драйвер может потребовать буфер больше (msm_vidc, HEVC 4K: 8,4 МБ).
        let in_size = unsafe { of.fmt.pix_mp.plane_fmt[0].sizeimage }.max(1 << 20) as usize;

        // NV12 на выходе — до потока, иначе драйвер может выбрать свой формат (у Qualcomm — UBWC).
        let mut cf: Format = zeroed();
        cf.type_ = BUF_TYPE_CAPTURE_MPLANE;
        let _ = xioctl(raw, G_FMT, &mut cf);
        cf.fmt.pix_mp.pixelformat = PIX_NV12;
        let _ = xioctl(raw, S_FMT, &mut cf);

        for t in [EVENT_SOURCE_CHANGE, EVENT_EOS] {
            let mut s: EventSubscription = zeroed();
            s.type_ = t;
            let _ = xioctl(raw, SUBSCRIBE_EVENT, &mut s);
        }

        let mut rb = RequestBuffers { count: N_INPUT as u32, type_: BUF_TYPE_OUTPUT_MPLANE, memory: MEMORY_DMABUF, capabilities: 0, flags: 0, reserved: [0; 3] };
        xioctl(raw, REQBUFS, &mut rb).map_err(|e| VideoError::DecoderInit(format!("v4l2: REQBUFS вход: {e}")))?;
        let inputs = (0..rb.count).map(|_| DmaBuf::alloc(in_size)).collect::<Result<Vec<_>, _>>()?;
        let mut t = BUF_TYPE_OUTPUT_MPLANE as i32;
        xioctl(raw, STREAMON, &mut t).map_err(|e| VideoError::DecoderInit(format!("v4l2: STREAMON вход: {e}")))?;

        let bsf = match bsf_name {
            Some(n) => Some(Bsf::new(n, params, time_base)?),
            None => None,
        };
        let n = inputs.len();
        Ok(Self {
            fd,
            v4l2_fmt,
            bsf,
            inputs,
            input_free: vec![true; n],
            capture: None,
            ready: VecDeque::new(),
            packed: false,
            pool: Vec::new(),
            pts_by_tag: HashMap::new(),
            next_tag: 1,
            eos_sent: false,
            eos_done: false,
            pending_in: VecDeque::new(),
            stop_pending: false,
            returned: Default::default(),
            capture_gen: 0,
            codec_id,
            params,
            time_base,
        })
    }

    pub fn codec_id(&self) -> ffi::AVCodecID {
        self.codec_id
    }

    pub fn width(&self) -> u32 {
        self.capture.as_ref().map(|c| c.visible_w).unwrap_or_else(|| unsafe { (*self.params).width as u32 })
    }

    pub fn height(&self) -> u32 {
        self.capture.as_ref().map(|c| c.visible_h).unwrap_or_else(|| unsafe { (*self.params).height as u32 })
    }

    /// Подать пакет демуксера. Блокирует, пока не освободится входной буфер
    /// (декодированные за это время кадры копятся для `receive_frame`).
    pub fn send_packet(&mut self, pkt: &ffmpeg_next::Packet) -> Result<(), VideoError> {
        let mut units: Vec<(Vec<u8>, Option<i64>)> = Vec::new();
        match &self.bsf {
            Some(b) => {
                // SAFETY: av_bsf_send_packet забирает ссылку на копию пакета.
                unsafe {
                    let p = ffi::av_packet_clone(pkt.as_ptr());
                    if ffi::av_bsf_send_packet(b.0, p) < 0 {
                        ffi::av_packet_free(&mut (p as *mut _));
                        return Err(VideoError::Other("v4l2: av_bsf_send_packet".into()));
                    }
                    let mut p = p;
                    ffi::av_packet_free(&mut p);
                    let mut out = ffi::av_packet_alloc();
                    while ffi::av_bsf_receive_packet(b.0, out) == 0 {
                        let data = std::slice::from_raw_parts((*out).data, (*out).size as usize).to_vec();
                        let pts = if (*out).pts == ffi::AV_NOPTS_VALUE { None } else { Some((*out).pts) };
                        units.push((data, pts));
                        ffi::av_packet_unref(out);
                    }
                    ffi::av_packet_free(&mut out);
                }
            }
            None => units.push((pkt.data().unwrap_or(&[]).to_vec(), pkt.pts())),
        }
        for (data, pts) in units {
            if !data.is_empty() {
                self.queue_input(&data, pts)?;
            }
        }
        self.service(false)?;
        Ok(())
    }

    fn queue_input(&mut self, data: &[u8], pts: Option<i64>) -> Result<(), VideoError> {
        self.pending_in.push_back((data.to_vec(), pts));
        self.service(false)?;
        // Ждать декодер — только если пакетов накопилось много, а готовых кадров
        // нет (их разберут после возврата, и это освободит буферы).
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.pending_in.len() > PENDING_MAX && self.ready.is_empty() {
            if Instant::now() > deadline {
                return Err(VideoError::Other("v4l2: декодер не принимает поток".into()));
            }
            self.wait(50);
            self.service(false)?;
        }
        Ok(())
    }

    /// Ждущие пакеты — в свободные входные буферы; затем, если просили, конец потока.
    fn feed(&mut self) -> Result<(), VideoError> {
        while !self.pending_in.is_empty() {
            let Some(idx) = self.input_free.iter().position(|f| *f) else { break };
            let (data, pts) = self.pending_in.pop_front().expect("не пусто");
            self.submit_input(idx, &data, pts)?;
        }
        if self.stop_pending && self.pending_in.is_empty() {
            self.stop_pending = false;
            let mut c: DecoderCmd = zeroed();
            c.cmd = DEC_CMD_STOP;
            if let Err(e) = xioctl(self.fd.as_raw_fd(), DECODER_CMD, &mut c) {
                log::warn!("v4l2: DEC_CMD_STOP: {e}");
                self.eos_done = true;
            }
        }
        Ok(())
    }

    fn submit_input(&mut self, idx: usize, data: &[u8], pts: Option<i64>) -> Result<(), VideoError> {
        let buf = &mut self.inputs[idx];
        if data.len() > buf.len {
            return Err(VideoError::Other(format!("v4l2: пакет {} байт больше буфера", data.len())));
        }
        buf.sync(SYNC_WRITE);
        buf.slice_mut()[..data.len()].copy_from_slice(data);
        buf.sync(SYNC_WRITE | SYNC_END);
        let tag = self.next_tag;
        self.next_tag += 1;
        self.pts_by_tag.insert(tag, pts);
        let mut pl: Plane = zeroed();
        pl.m = buf.fd.as_raw_fd() as u64;
        pl.length = buf.len as u32;
        pl.bytesused = data.len() as u32;
        let mut b: Buffer = zeroed();
        b.index = idx as u32;
        b.type_ = BUF_TYPE_OUTPUT_MPLANE;
        b.memory = MEMORY_DMABUF;
        b.length = 1;
        b.m = &mut pl as *mut Plane as u64;
        b.timestamp = libc::timeval { tv_sec: tag as _, tv_usec: 0 };
        xioctl(self.fd.as_raw_fd(), QBUF, &mut b).map_err(|e| VideoError::Other(format!("v4l2: QBUF вход: {e}")))?;
        self.input_free[idx] = false;
        Ok(())
    }

    fn wait(&self, ms: i32) {
        let mut p = libc::pollfd { fd: self.fd.as_raw_fd(), events: libc::POLLIN | libc::POLLOUT | libc::POLLPRI, revents: 0 };
        // SAFETY: один pollfd.
        unsafe { libc::poll(&mut p, 1, ms) };
    }

    /// События, освободившиеся входы, готовые кадры — без ожидания.
    fn service(&mut self, _block: bool) -> Result<(), VideoError> {
        let raw = self.fd.as_raw_fd();
        loop {
            let mut ev: Event = zeroed();
            if xioctl(raw, DQEVENT, &mut ev).is_err() {
                break;
            }
            if ev.type_ == EVENT_SOURCE_CHANGE {
                self.setup_capture()?;
            }
        }
        loop {
            let mut pl: Plane = zeroed();
            let mut b: Buffer = zeroed();
            b.type_ = BUF_TYPE_OUTPUT_MPLANE;
            b.memory = MEMORY_DMABUF;
            b.length = 1;
            b.m = &mut pl as *mut Plane as u64;
            if xioctl(raw, DQBUF, &mut b).is_err() {
                break;
            }
            if let Some(f) = self.input_free.get_mut(b.index as usize) {
                *f = true;
            }
        }
        self.feed()?;
        // Буферы, освобождённые кадрами без копии, — снова декодеру.
        let back: Vec<(u64, usize)> = std::mem::take(&mut *self.returned.lock().unwrap_or_else(|e| e.into_inner()));
        if let Some(gen) = self.capture.as_ref().map(|c| c.gen) {
            for (g, idx) in back {
                if g == gen {
                    self.requeue_capture(idx)?;
                }
            }
        }
        while self.capture.is_some() {
            let mut pl: Plane = zeroed();
            let mut b: Buffer = zeroed();
            b.type_ = BUF_TYPE_CAPTURE_MPLANE;
            b.memory = MEMORY_DMABUF;
            b.length = 1;
            b.m = &mut pl as *mut Plane as u64;
            if xioctl(raw, DQBUF, &mut b).is_err() {
                break;
            }
            let last = b.flags & BUF_FLAG_LAST != 0;
            if pl.bytesused > 0 && b.flags & BUF_FLAG_ERROR == 0 {
                let tag = b.timestamp.tv_sec as u64;
                let pts = self.pts_by_tag.remove(&tag).flatten();
                let zero_copy = self.capture.as_ref().is_some_and(|c| c.zero_copy);
                let f = if self.packed && zero_copy {
                    V4l2Frame::Packed(self.dmabuf_frame(b.index as usize), pts)
                } else if self.packed {
                    V4l2Frame::Packed(self.copy_packed(b.index as usize), pts)
                } else {
                    V4l2Frame::Av(self.copy_frame(b.index as usize, pts))
                };
                self.ready.push_back(f);
                if last {
                    self.eos_done = true;
                    break;
                }
                if self.packed && zero_copy {
                    // буфер вернёт кадр (CaptureHold), когда станет не нужен
                    continue;
                }
            }
            if last {
                self.eos_done = true;
                break;
            }
            self.requeue_capture(b.index as usize)?;
        }
        Ok(())
    }

    fn copy_frame(&self, idx: usize, pts: Option<i64>) -> frame::Video {
        let c = self.capture.as_ref().unwrap();
        let (w, h) = (c.visible_w, c.visible_h);
        let mut f = frame::Video::new(Pixel::NV12, w, h);
        c.bufs[idx].sync(SYNC_READ);
        let src = c.bufs[idx].slice();
        let stride = c.stride as usize;
        let uv_off = stride * c.y_lines as usize;
        for plane in 0..2 {
            let rows = if plane == 0 { h as usize } else { (h as usize).div_ceil(2) };
            let dst_stride = f.stride(plane);
            let base = if plane == 0 { 0 } else { uv_off };
            let n = (w as usize).min(dst_stride).min(stride);
            let dst = f.data_mut(plane);
            for r in 0..rows {
                let s = base + r * stride;
                if s + n > src.len() {
                    break;
                }
                dst[r * dst_stride..r * dst_stride + n].copy_from_slice(&src[s..s + n]);
            }
        }
        c.bufs[idx].sync(SYNC_READ | SYNC_END);
        f.set_pts(pts);
        f
    }

    /// Кадр в буфере выхода как есть — GPU возьмёт плоскости из dma-buf без копии.
    fn dmabuf_frame(&mut self, idx: usize) -> crate::gpu::YuvFrame {
        use crate::gpu::{YuvDmaBuf, YuvFrame, YuvLayout};
        let c = self.capture.as_ref().unwrap();
        let (w, h) = (c.visible_w, c.visible_h);
        let buf = c.bufs[idx].clone();
        let stride = c.stride;
        let uv_offset = stride * c.y_lines;
        let hold = CaptureHold { idx, gen: c.gen, returned: self.returned.clone() };
        let copy_src = buf.clone();
        let copy = move || pack_nv12(&copy_src, stride as usize, uv_offset as usize, w, h);
        let d = YuvDmaBuf {
            id: c.ids[idx],
            fd: buf.fd.as_raw_fd(),
            y_offset: 0,
            y_pitch: stride,
            uv_offset,
            uv_pitch: stride,
            copy: Box::new(copy),
            hold: Box::new((hold, buf)),
        };
        let (matrix, full_range) = self.color();
        YuvFrame { layout: YuvLayout::Nv12, matrix, full_range, width: w, height: h, data: std::sync::Arc::from(Vec::new().into_boxed_slice()), dmabuf: Some(std::sync::Arc::new(d)) }
    }

    /// Цветовое пространство — из параметров потока (VUI); не указано — по высоте.
    fn color(&self) -> (crate::gpu::YuvMatrix, bool) {
        use crate::gpu::YuvMatrix;
        let h = self.capture.as_ref().map(|c| c.visible_h).unwrap_or(0);
        // SAFETY: params — наша копия параметров потока, жива вместе с декодером.
        let (space, range) = unsafe { ((*self.params).color_space, (*self.params).color_range) };
        let matrix = match space {
            ffi::AVColorSpace::AVCOL_SPC_BT709 => YuvMatrix::Bt709,
            ffi::AVColorSpace::AVCOL_SPC_BT2020_NCL | ffi::AVColorSpace::AVCOL_SPC_BT2020_CL => YuvMatrix::Bt2020,
            ffi::AVColorSpace::AVCOL_SPC_BT470BG | ffi::AVColorSpace::AVCOL_SPC_SMPTE170M => YuvMatrix::Bt601,
            _ if h >= 720 => YuvMatrix::Bt709,
            _ => YuvMatrix::Bt601,
        };
        (matrix, range == ffi::AVColorRange::AVCOL_RANGE_JPEG)
    }

    /// Кадры NV12 сразу упакованными плоскостями (`receive` отдаёт
    /// [`V4l2Frame::Packed`]): перевод цвета — на GPU.
    pub fn set_packed(&mut self, on: bool) {
        self.packed = on;
    }

    fn copy_packed(&mut self, idx: usize) -> crate::gpu::YuvFrame {
        use crate::gpu::{YuvFrame, YuvLayout, YuvMatrix};
        let c = self.capture.as_ref().unwrap();
        let (w, h) = (c.visible_w, c.visible_h);
        let len = YuvFrame::byte_len(YuvLayout::Nv12, w, h);
        self.pool.retain(|b| b.len() == len);
        let mut buf = match self.pool.iter().position(|b| std::sync::Arc::strong_count(b) == 1 && std::sync::Arc::weak_count(b) == 0) {
            Some(i) => self.pool.swap_remove(i),
            None => std::sync::Arc::from(vec![0u8; len].into_boxed_slice()),
        };
        let out = std::sync::Arc::get_mut(&mut buf).expect("буфер из оборота никто не держит");
        c.bufs[idx].sync(SYNC_READ);
        let src = c.bufs[idx].slice();
        let stride = c.stride as usize;
        let uv_off = stride * c.y_lines as usize;
        let row = (w as usize).min(stride);
        let (wu, ch) = (w as usize, (h as usize).div_ceil(2));
        let uv_row = (wu.div_ceil(2) * 2).min(stride);
        let mut o = 0;
        for (base, rows, n, dst_row) in [(0, h as usize, row, wu), (uv_off, ch, uv_row, wu.div_ceil(2) * 2)] {
            for r in 0..rows {
                let s = base + r * stride;
                if s + n <= src.len() {
                    out[o..o + n].copy_from_slice(&src[s..s + n]);
                }
                o += dst_row;
            }
        }
        c.bufs[idx].sync(SYNC_READ | SYNC_END);
        if self.pool.len() < PACKED_POOL {
            self.pool.push(buf.clone());
        }
        // Цветовое пространство — из параметров потока (VUI); не указано — по высоте.
        // SAFETY: params — наша копия параметров потока, жива вместе с декодером.
        let (space, range) = unsafe { ((*self.params).color_space, (*self.params).color_range) };
        let matrix = match space {
            ffi::AVColorSpace::AVCOL_SPC_BT709 => YuvMatrix::Bt709,
            ffi::AVColorSpace::AVCOL_SPC_BT2020_NCL | ffi::AVColorSpace::AVCOL_SPC_BT2020_CL => YuvMatrix::Bt2020,
            ffi::AVColorSpace::AVCOL_SPC_BT470BG | ffi::AVColorSpace::AVCOL_SPC_SMPTE170M => YuvMatrix::Bt601,
            _ if h >= 720 => YuvMatrix::Bt709,
            _ => YuvMatrix::Bt601,
        };
        YuvFrame {
            layout: YuvLayout::Nv12,
            matrix,
            full_range: range == ffi::AVColorRange::AVCOL_RANGE_JPEG,
            width: w,
            height: h,
            data: buf,
            dmabuf: None,
        }
    }

    fn requeue_capture(&mut self, idx: usize) -> Result<(), VideoError> {
        let c = self.capture.as_ref().unwrap();
        let buf = &c.bufs[idx];
        let mut pl: Plane = zeroed();
        pl.m = buf.fd.as_raw_fd() as u64;
        pl.length = buf.len as u32;
        let mut b: Buffer = zeroed();
        b.index = idx as u32;
        b.type_ = BUF_TYPE_CAPTURE_MPLANE;
        b.memory = MEMORY_DMABUF;
        b.length = 1;
        b.m = &mut pl as *mut Plane as u64;
        xioctl(self.fd.as_raw_fd(), QBUF, &mut b).map_err(|e| VideoError::Other(format!("v4l2: QBUF выход: {e}")))
    }

    /// Смена параметров потока: выход заново (формат, размер, буферы).
    fn setup_capture(&mut self) -> Result<(), VideoError> {
        let raw = self.fd.as_raw_fd();
        if self.capture.take().is_some() {
            let mut t = BUF_TYPE_CAPTURE_MPLANE as i32;
            let _ = xioctl(raw, STREAMOFF, &mut t);
            let mut rb = RequestBuffers { count: 0, type_: BUF_TYPE_CAPTURE_MPLANE, memory: MEMORY_DMABUF, capabilities: 0, flags: 0, reserved: [0; 3] };
            let _ = xioctl(raw, REQBUFS, &mut rb);
        }
        let mut cf: Format = zeroed();
        cf.type_ = BUF_TYPE_CAPTURE_MPLANE;
        xioctl(raw, G_FMT, &mut cf).map_err(|e| VideoError::Other(format!("v4l2: G_FMT выход: {e}")))?;
        if unsafe { cf.fmt.pix_mp.pixelformat } != PIX_NV12 {
            cf.fmt.pix_mp.pixelformat = PIX_NV12;
            xioctl(raw, S_FMT, &mut cf).map_err(|e| VideoError::Other(format!("v4l2: выход NV12: {e}")))?;
        }
        let pm = unsafe { cf.fmt.pix_mp };
        let (width, height) = (pm.width, pm.height);
        let stride = pm.plane_fmt[0].bytesperline;
        let size = pm.plane_fmt[0].sizeimage as usize;
        let (mut vw, mut vh) = (width, height);
        let mut crop: Crop = zeroed();
        crop.type_ = BUF_TYPE_CAPTURE_MPLANE;
        if xioctl(raw, G_CROP, &mut crop).is_ok() && crop.c.width > 0 && crop.c.height > 0 {
            vw = crop.c.width.min(width);
            vh = crop.c.height.min(height);
        }
        // Не все драйверы отдают видимую область через G_CROP (Venus — только G_SELECTION): иначе кадр 1280×720
        // показывался 1280×736 с полосой выравнивания внизу. Размер потока из контейнера — верхняя граница.
        // SAFETY: params — наша копия параметров потока, жива вместе с декодером.
        let (pw, ph) = unsafe { ((*self.params).width, (*self.params).height) };
        if pw > 0 && ph > 0 {
            vw = vw.min(pw as u32);
            vh = vh.min(ph as u32);
        }
        let mut mb = Control { id: CID_MIN_BUFFERS_FOR_CAPTURE, value: 4 };
        let _ = xioctl(raw, G_CTRL, &mut mb);
        // Без копий кадры держат буферы, пока ждут в очереди плеера и на экране —
        // буферов нужно больше на эту очередь (иначе декодер встанет). Решение —
        // по рендереру, не по `packed`: режим YUV плеер включает уже после
        // открытия, когда буферы выделены первым пакетом.
        let zero_copy = crate::gpu::dmabuf::import_supported();
        let extra = if zero_copy { ZERO_COPY_EXTRA } else { 4 };
        let want = (mb.value.max(2) + extra) as u32;
        let mut rb = RequestBuffers { count: want, type_: BUF_TYPE_CAPTURE_MPLANE, memory: MEMORY_DMABUF, capabilities: 0, flags: 0, reserved: [0; 3] };
        xioctl(raw, REQBUFS, &mut rb).map_err(|e| VideoError::Other(format!("v4l2: REQBUFS выход: {e}")))?;
        let bufs = (0..rb.count).map(|_| DmaBuf::alloc(size).map(std::sync::Arc::new)).collect::<Result<Vec<_>, _>>()?;
        log::info!(
            "v4l2: поток {width}x{height} (видно {vw}x{vh}), NV12 stride {stride}, буферов {} ({} МБ){}",
            bufs.len(),
            bufs.len() * size >> 20,
            if zero_copy { ", кадры в GPU без копии (в режиме YUV)" } else { "" }
        );
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let ids = bufs.iter().map(|_| NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)).collect();
        self.capture_gen += 1;
        self.capture = Some(Capture { bufs, ids, gen: self.capture_gen, zero_copy, stride, y_lines: height, visible_w: vw, visible_h: vh });
        for i in 0..rb.count as usize {
            self.requeue_capture(i)?;
        }
        let mut t = BUF_TYPE_CAPTURE_MPLANE as i32;
        xioctl(raw, STREAMON, &mut t).map_err(|e| VideoError::Other(format!("v4l2: STREAMON выход: {e}")))?;
        Ok(())
    }

    /// Готовый кадр, если есть. После `send_eof` ждёт последних кадров.
    pub fn receive_frame(&mut self, out: &mut frame::Video) -> Result<(), ffmpeg_next::Error> {
        self.fill_ready();
        match self.ready.pop_front() {
            Some(V4l2Frame::Av(f)) => {
                *out = f;
                Ok(())
            }
            // Упакованный режим выключили, а кадры остались в очереди.
            Some(V4l2Frame::Packed(y, pts)) => {
                let mut f = frame::Video::new(Pixel::NV12, y.width, y.height);
                let (yp, (uv, _)) = (y.y_plane(), y.chroma_planes());
                for (plane, src, row) in [(0, yp, y.width as usize), (1, uv, y.chroma_size().0 as usize * 2)] {
                    let st = f.stride(plane);
                    let dst = f.data_mut(plane);
                    for (r, line) in src.chunks_exact(row).enumerate() {
                        dst[r * st..r * st + row].copy_from_slice(line);
                    }
                }
                f.set_pts(pts);
                *out = f;
                Ok(())
            }
            None if self.eos_sent => Err(ffmpeg_next::Error::Eof),
            None => Err(ffmpeg_next::Error::Other { errno: libc::EAGAIN }),
        }
    }

    /// Готовый кадр как есть (в упакованном режиме — [`V4l2Frame::Packed`]).
    pub fn receive(&mut self) -> Result<V4l2Frame, ffmpeg_next::Error> {
        self.fill_ready();
        match self.ready.pop_front() {
            Some(f) => Ok(f),
            None if self.eos_sent => Err(ffmpeg_next::Error::Eof),
            None => Err(ffmpeg_next::Error::Other { errno: libc::EAGAIN }),
        }
    }

    /// Забрать готовые кадры; после `send_eof` — дождаться последних.
    fn fill_ready(&mut self) {
        if self.ready.is_empty() {
            let _ = self.service(false);
        }
        if self.ready.is_empty() && self.eos_sent && !self.eos_done {
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.ready.is_empty() && !self.eos_done && Instant::now() < deadline {
                self.wait(50);
                let _ = self.service(false);
            }
        }
    }

    pub fn send_eof(&mut self) -> Result<(), VideoError> {
        if self.eos_sent {
            return Ok(());
        }
        self.eos_sent = true;
        self.stop_pending = true;
        self.feed()
    }

    /// Сброс (перемотка): декодер создаётся заново — надёжнее, чем сброс очередей
    /// у разных драйверов; первый кадр после пересоздания — десятки мс.
    pub fn flush(&mut self) -> Result<(), VideoError> {
        // Сначала закрыть старую сессию: у кодека SoC общий лимит нагрузки на все сессии (Venus SM-T295 —
        // 352800 макроблоков/с), и с открытым старым декодером новый 1080p его превышал: «HW is overloaded»,
        // сессия падала (session error 1001). Вместо дескриптора — /dev/null: старый закрывается сразу.
        let raw = self.fd.as_raw_fd();
        for t in [BUF_TYPE_OUTPUT_MPLANE, BUF_TYPE_CAPTURE_MPLANE] {
            let mut t = t as i32;
            let _ = xioctl(raw, STREAMOFF, &mut t);
        }
        self.capture = None;
        self.inputs.clear();
        if let Ok(null) = File::open("/dev/null") {
            self.fd = null;
        }
        let fresh = Self::new(self.params, self.time_base)?;
        *self = fresh;
        Ok(())
    }
}

impl Drop for V4l2Decoder {
    fn drop(&mut self) {
        let raw = self.fd.as_raw_fd();
        for t in [BUF_TYPE_OUTPUT_MPLANE, BUF_TYPE_CAPTURE_MPLANE] {
            let mut t = t as i32;
            let _ = xioctl(raw, STREAMOFF, &mut t);
        }
        // SAFETY: параметры скопированы в new(); при flush() старый объект
        // освобождает свою копию, новый держит свою.
        unsafe { ffi::avcodec_parameters_free(&mut self.params) };
        let _ = self.v4l2_fmt;
    }
}

/// Держит буфер выхода, пока кадр без копии нужен плеру или GPU; освобождение
/// возвращает буфер декодеру (в очередь — при следующем обслуживании).
struct CaptureHold {
    idx: usize,
    gen: u64,
    returned: std::sync::Arc<std::sync::Mutex<Vec<(u64, usize)>>>,
}

impl Drop for CaptureHold {
    fn drop(&mut self) {
        if let Ok(mut r) = self.returned.lock() {
            r.push((self.gen, self.idx));
        }
    }
}

/// Плоскости NV12 буфера выхода → упакованный NV12 `w × h` (GPU без импорта dma-buf).
fn pack_nv12(buf: &DmaBuf, stride: usize, uv_off: usize, w: u32, h: u32) -> std::sync::Arc<[u8]> {
    let len = crate::gpu::YuvFrame::byte_len(crate::gpu::YuvLayout::Nv12, w, h);
    let mut out = vec![0u8; len];
    buf.sync(SYNC_READ);
    let src = buf.slice();
    let (wu, ch) = (w as usize, (h as usize).div_ceil(2));
    let row = wu.min(stride);
    let uv_row = (wu.div_ceil(2) * 2).min(stride);
    let mut o = 0;
    for (base, rows, n, dst_row) in [(0, h as usize, row, wu), (uv_off, ch, uv_row, wu.div_ceil(2) * 2)] {
        for r in 0..rows {
            let s = base + r * stride;
            if s + n <= src.len() && o + n <= out.len() {
                out[o..o + n].copy_from_slice(&src[s..s + n]);
            }
            o += dst_row;
        }
    }
    buf.sync(SYNC_READ | SYNC_END);
    std::sync::Arc::from(out.into_boxed_slice())
}
