//! Субтитры потоков контейнера: декодеры ffmpeg по дорожкам, пакеты от
//! потока декодера видео. Текст приходит строками ASS, картинки — палитрой.

use std::collections::HashMap;
use std::sync::Arc;

use ffmpeg_next::ffi;
use ffmpeg_next::format::context::Input;
use ffmpeg_next::media::Type as MediaType;
use ffmpeg_next::packet::Ref as _;

use super::ass::{self, AssHeader};
use super::{SubBitmap, SubContent, SubText, SubtitleSource, SubtitleStore, SubtitleTrackInfo};

/// Формат для людей по кодеку.
pub(crate) fn codec_label(id: ffmpeg_next::codec::Id) -> (&'static str, bool) {
    use ffmpeg_next::codec::Id;
    match id {
        Id::SUBRIP | Id::SRT => ("SRT", false),
        Id::ASS | Id::SSA => ("ASS", false),
        Id::WEBVTT => ("WebVTT", false),
        Id::MOV_TEXT => ("Timed Text", false),
        Id::TEXT => ("Текст", false),
        Id::MICRODVD => ("MicroDVD", false),
        Id::HDMV_PGS_SUBTITLE => ("PGS", true),
        Id::DVD_SUBTITLE => ("VobSub", true),
        Id::DVB_SUBTITLE => ("DVB", true),
        Id::XSUB => ("XSUB", true),
        Id::HDMV_TEXT_SUBTITLE => ("HDMV Text", false),
        Id::EIA_608 => ("CC", false),
        _ => ("?", false),
    }
}

/// Дорожки субтитров контейнера (декодеры, которых нет у ffmpeg, — мимо).
pub(crate) fn tracks_of(ictx: &Input) -> Vec<SubtitleTrackInfo> {
    use ffmpeg_next::format::stream::Disposition;
    ictx.streams()
        .filter(|s| s.parameters().medium() == MediaType::Subtitle)
        .filter_map(|s| {
            let par = s.parameters();
            let (codec, bitmap) = codec_label(par.id());
            if codec == "?" || ffmpeg_next::decoder::find(par.id()).is_none() {
                return None;
            }
            let tags = s.metadata();
            let tag = |k: &str| tags.get(k).map(str::trim).filter(|v| !v.is_empty() && *v != "und").map(String::from);
            let title = tag("title");
            let d = s.disposition();
            let lower = title.as_deref().unwrap_or("").to_lowercase();
            Some(SubtitleTrackInfo {
                source: SubtitleSource::Embedded(s.index()),
                language: tag("language"),
                forced: d.contains(Disposition::FORCED) || lower.contains("forced") || lower.contains("надпис"),
                hearing_impaired: d.contains(Disposition::HEARING_IMPAIRED) || lower.contains("sdh"),
                title,
                codec: codec.into(),
                bitmap,
                default: d.contains(Disposition::DEFAULT),
            })
        })
        .collect()
}

struct Track {
    dec: ffmpeg_next::decoder::Subtitle,
    tb: f64,
    header: AssHeader,
    bitmap: bool,
}

/// Декодеры субтитров одного ролика — живут в потоке декодера видео.
pub(crate) struct SubtitleDecoders {
    store: Arc<SubtitleStore>,
    /// Номер потока → параметры (декодер открывается по первому пакету).
    params: HashMap<usize, (ffmpeg_next::codec::Parameters, ffmpeg_next::Rational)>,
    open: HashMap<usize, Option<Track>>,
    video_size: (u32, u32),
    /// Дочитка субтитров перед точкой перемотки (локальные файлы).
    preroll: Option<Preroll>,
}

/// Сколько секунд до точки перемотки дочитывать: реплика, начавшаяся
/// раньше ключевого кадра, на котором встал демуксер, иначе пропала бы
/// (при «продолжить с места» — первая же).
const PREROLL_SEC: f64 = 8.0;
/// Больше пакетов за одну дочитку не читать.
const PREROLL_MAX_PACKETS: usize = 40_000;

/// Второй демуксер того же файла: после перемотки на `t` читает пакеты с
/// `t − PREROLL_SEC` до `t` и отдаёт субтитры в хранилище (повторы оно
/// отбрасывает само). Работает в своём потоке, не задерживая картинку.
pub(crate) struct Preroll {
    tx: std::sync::mpsc::Sender<f64>,
    generation: Arc<std::sync::atomic::AtomicU64>,
}

impl Preroll {
    pub(crate) fn spawn(path: String, store: Arc<SubtitleStore>, video_size: (u32, u32)) -> Option<Self> {
        let (tx, rx) = std::sync::mpsc::channel::<f64>();
        let generation = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let gen = generation.clone();
        std::thread::Builder::new()
            .name("syngui-subs-preroll".into())
            .spawn(move || run_preroll(path, store, video_size, rx, gen))
            .ok()?;
        Some(Self { tx, generation })
    }

    fn request(&self, t: f64) {
        self.generation.fetch_add(1, std::sync::atomic::Ordering::Release);
        let _ = self.tx.send(t);
    }
}

fn run_preroll(
    path: String,
    store: Arc<SubtitleStore>,
    video_size: (u32, u32),
    rx: std::sync::mpsc::Receiver<f64>,
    generation: Arc<std::sync::atomic::AtomicU64>,
) {
    use std::sync::atomic::Ordering;
    let mut ctx: Option<(Input, SubtitleDecoders, Vec<f64>)> = None;
    while let Ok(mut t) = rx.recv() {
        while let Ok(n) = rx.try_recv() {
            t = n;
        }
        let gen = generation.load(Ordering::Acquire);
        if ctx.is_none() {
            match ffmpeg_next::format::input(&path) {
                Ok(ictx) => {
                    let decs = SubtitleDecoders::new(&ictx, store.clone(), video_size, None);
                    let tbs = ictx.streams().map(|s| s.time_base()).map(|tb| tb.numerator() as f64 / tb.denominator().max(1) as f64).collect();
                    ctx = Some((ictx, decs, tbs));
                }
                Err(e) => {
                    log::warn!("субтитры: дочитка не открыла {path}: {e}");
                    return;
                }
            }
        }
        let Some((ictx, decs, tbs)) = ctx.as_mut() else { return };
        for t in decs.open.values_mut().flatten() {
            // SAFETY: декодер открыт.
            unsafe { ffi::avcodec_flush_buffers(t.dec.as_mut_ptr()) };
        }
        let from = (t - PREROLL_SEC).max(0.0);
        let ts = (from * 1_000_000.0) as i64;
        if let Err(e) = ictx.seek(ts, ..ts) {
            log::debug!("субтитры: дочитка — перемотка на {from:.1}: {e}");
            continue;
        }
        let started = std::time::Instant::now();
        let mut n = 0usize;
        let mut found = 0usize;
        for (stream, packet) in ictx.packets() {
            n += 1;
            if n > PREROLL_MAX_PACKETS || generation.load(Ordering::Acquire) != gen {
                break;
            }
            let idx = stream.index();
            let pts = packet.pts().or(packet.dts()).map(|p| p as f64 * tbs.get(idx).copied().unwrap_or(0.0));
            if decs.params.contains_key(&idx) {
                // реплики, начавшиеся до цели, — ради них и читаем
                if pts.is_some_and(|p| p <= t + 0.2) {
                    decs.on_packet(&packet);
                    found += 1;
                }
            } else if pts.is_some_and(|p| p > t + 0.5) {
                break;
            }
        }
        log::debug!("субтитры: дочитка {from:.1}–{t:.1} с: {n} пакетов, субтитров {found}, {:?}", started.elapsed());
    }
}

/// `AVSubtitle` с освобождением.
struct RawSubtitle(ffi::AVSubtitle);

impl Drop for RawSubtitle {
    fn drop(&mut self) {
        // SAFETY: структура заполнена avcodec_decode_subtitle2 (или нулевая).
        unsafe { ffi::avsubtitle_free(&mut self.0) };
    }
}

impl SubtitleDecoders {
    /// `path` — локальный файл: после перемотки субтитры перед точкой
    /// перемотки дочитывает [`Preroll`].
    pub(crate) fn new(ictx: &Input, store: Arc<SubtitleStore>, video_size: (u32, u32), path: Option<&str>) -> Self {
        let params: HashMap<_, _> = ictx
            .streams()
            .filter(|s| s.parameters().medium() == MediaType::Subtitle)
            .map(|s| (s.index(), (s.parameters(), s.time_base())))
            .collect();
        let preroll = match path {
            Some(p) if !params.is_empty() && std::path::Path::new(p).is_file() => Preroll::spawn(p.to_string(), store.clone(), video_size),
            _ => None,
        };
        Self { store, params, open: HashMap::new(), video_size, preroll }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.params.is_empty()
    }

    fn open_track(&self, idx: usize) -> Option<Track> {
        let (par, tb) = self.params.get(&idx)?;
        let mut ctx = ffmpeg_next::codec::context::Context::from_parameters(par.clone()).ok()?;
        // SAFETY: контекст ещё не открыт; pkt_timebase нужен текстовым
        // декодерам для длительности событий.
        unsafe {
            (*ctx.as_mut_ptr()).pkt_timebase = ffi::AVRational { num: tb.numerator(), den: tb.denominator() };
        }
        let dec = match ctx.decoder().subtitle() {
            Ok(d) => d,
            Err(e) => {
                log::warn!("субтитры: поток {idx} не открылся: {e}");
                return None;
            }
        };
        // Заголовок ASS (стили, разрешение) — у открытого декодера.
        let header = unsafe {
            let p = dec.as_ptr();
            let h = (*p).subtitle_header;
            let n = (*p).subtitle_header_size;
            if h.is_null() || n <= 0 {
                AssHeader::default()
            } else {
                AssHeader::parse(&String::from_utf8_lossy(std::slice::from_raw_parts(h, n as usize)))
            }
        };
        let (_, bitmap) = codec_label(par.id());
        Some(Track { dec, tb: tb.numerator() as f64 / tb.denominator().max(1) as f64, header, bitmap })
    }

    /// Пакет из демуксера: если это поток субтитров, который сейчас нужен,
    /// — декодировать и положить события в хранилище.
    pub(crate) fn on_packet(&mut self, packet: &ffmpeg_next::Packet) {
        let idx = packet.stream();
        if !self.params.contains_key(&idx) || !self.store.wants_stream(idx) {
            return;
        }
        if !self.open.contains_key(&idx) {
            let t = self.open_track(idx);
            self.open.insert(idx, t);
        }
        let video_size = self.video_size;
        let Some(Some(track)) = self.open.get_mut(&idx) else { return };
        let mut sub = RawSubtitle(unsafe { std::mem::zeroed() });
        let mut got: i32 = 0;
        // SAFETY: декодер открыт, пакет жив на время вызова.
        let r = unsafe {
            ffi::avcodec_decode_subtitle2(track.dec.as_mut_ptr(), &mut sub.0, &mut got, packet.as_ptr() as *mut _)
        };
        if r < 0 || got == 0 {
            return;
        }
        let pkt_start = packet.pts().or(packet.dts()).map(|p| p as f64 * track.tb);
        let start_base = if sub.0.pts != ffi::AV_NOPTS_VALUE {
            sub.0.pts as f64 / 1_000_000.0
        } else if let Some(s) = pkt_start {
            s
        } else {
            return;
        };
        let start = start_base + sub.0.start_display_time as f64 / 1000.0;
        let end = match sub.0.end_display_time {
            0 | u32::MAX => match packet.duration() {
                d if d > 0 && !track.bitmap => start_base + d as f64 * track.tb,
                _ => f64::INFINITY,
            },
            e => start_base + e as f64 / 1000.0,
        };
        let content = if track.bitmap {
            bitmap_content(&sub.0, track, video_size)
        } else {
            text_content(&sub.0, &track.header).map(SubContent::Text)
        };
        self.store.push_embedded(idx, start, end, content);
    }

    /// Перемотка: декодеры сбрасываются (PGS собирает картинку из
    /// нескольких пакетов), у картинок — незакрытые события прочь.
    pub(crate) fn on_seek(&mut self, target: f64) {
        for t in self.open.values_mut().flatten() {
            // SAFETY: декодер открыт.
            unsafe { ffi::avcodec_flush_buffers(t.dec.as_mut_ptr()) };
        }
        self.store.on_seek();
        if let Some(p) = &self.preroll {
            p.request(target);
        }
    }
}

/// Текстовые прямоугольники события → один текст (строки подряд).
fn text_content(sub: &ffi::AVSubtitle, header: &AssHeader) -> Option<SubText> {
    let mut out: Option<SubText> = None;
    for i in 0..sub.num_rects as usize {
        // SAFETY: rects — массив из num_rects указателей.
        let rect = unsafe { &**sub.rects.add(i) };
        let text = unsafe {
            if !rect.ass.is_null() {
                std::ffi::CStr::from_ptr(rect.ass).to_string_lossy().into_owned()
            } else if !rect.text.is_null() {
                ass::srt_to_ass(&std::ffi::CStr::from_ptr(rect.text).to_string_lossy())
            } else {
                continue;
            }
        };
        let parsed = if rect.ass.is_null() {
            ass::parse_text(&text, "Default", header, 0)
        } else {
            ass::parse_dialogue(&text, &header.event_format).and_then(|d| ass::parse_text(d.text, d.style, header, d.layer))
        };
        if let Some(t) = parsed {
            match out.as_mut() {
                Some(o) => o.lines.extend(t.lines),
                None => out = Some(t),
            }
        }
    }
    out
}

/// Картинки с палитрой → RGBA.
fn bitmap_content(sub: &ffi::AVSubtitle, track: &Track, video_size: (u32, u32)) -> Option<SubContent> {
    let mut rects = Vec::new();
    for i in 0..sub.num_rects as usize {
        // SAFETY: rects — массив из num_rects указателей; у картинки data[0] —
        // индексы (linesize[0] на строку), data[1] — палитра u32 0xAARRGGBB.
        let r = unsafe { &**sub.rects.add(i) };
        if r.w <= 0 || r.h <= 0 || r.data[0].is_null() || r.data[1].is_null() {
            continue;
        }
        let (w, h) = (r.w as usize, r.h as usize);
        let stride = r.linesize[0] as usize;
        let colors = r.nb_colors.clamp(0, 256) as usize;
        let palette = unsafe { std::slice::from_raw_parts(r.data[1] as *const u32, colors) };
        let idx = unsafe { std::slice::from_raw_parts(r.data[0], stride * h) };
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let c = palette.get(idx[y * stride + x] as usize).copied().unwrap_or(0);
                let o = (y * w + x) * 4;
                rgba[o] = (c >> 16) as u8;
                rgba[o + 1] = (c >> 8) as u8;
                rgba[o + 2] = c as u8;
                rgba[o + 3] = (c >> 24) as u8;
            }
        }
        rects.push(SubBitmap { x: r.x, y: r.y, w: w as u32, h: h as u32, rgba: rgba.into() });
    }
    if rects.is_empty() {
        return None;
    }
    // Холст — размер, который знает декодер (PGS, DVB), иначе кадр видео.
    let (cw, ch) = unsafe {
        let p = track.dec.as_ptr();
        ((*p).width.max(0) as u32, (*p).height.max(0) as u32)
    };
    let mut canvas = if cw > 0 && ch > 0 { (cw, ch) } else { video_size };
    // VobSub без размера: холст не меньше картинок.
    for b in &rects {
        canvas.0 = canvas.0.max((b.x.max(0) as u32) + b.w);
        canvas.1 = canvas.1.max((b.y.max(0) as u32) + b.h);
    }
    Some(SubContent::Bitmap { rects, canvas })
}
