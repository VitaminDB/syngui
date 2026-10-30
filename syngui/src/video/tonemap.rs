//! HDR → SDR для показа на обычном (SDR) выводе: кадры PQ (HDR10, HDR10+, Dolby Vision с
//! HDR10-совместимой базой) и HLG переводятся в sRGB/BT.709 с тонмаппингом, а не «как есть» —
//! swscale кладёт сигнал PQ прямо в 8 бит, и картинка выходит блеклой и серой.
//!
//! Путь пикселя: Y'CbCr 10 бит (BT.2020, ТВ-диапазон) → R'G'B' → EOTF (PQ или HLG с OOTF) →
//! линейный свет в нитах → BT.2390 EETF по максимуму RGB (оттенок сохраняется) от пика ролика
//! (MaxCLL, иначе мастеринг, иначе 1000 нит) к белому SDR 203 нит (BT.2408) → BT.2020 → BT.709 →
//! цвет вне охвата сводится к яркости → sRGB. Дорогие функции — таблицами; кадр считается
//! параллельно полосами, как у swscale.

use std::sync::{Arc, Mutex, OnceLock};

use ffmpeg_next::color::TransferCharacteristic;
use ffmpeg_next::ffi;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame;

/// Белый SDR (BT.2408 «HDR reference white»): 203 нит → 1.0 на выходе.
pub const SDR_WHITE_NITS: f32 = 203.0;
/// Пик, если ролик о нём молчит (так мастерят большинство HDR10).
const DEFAULT_PEAK_NITS: f32 = 1000.0;
/// HLG: номинальный пик дисплея, для которого считается OOTF (BT.2100).
const HLG_PEAK_NITS: f32 = 1000.0;

/// Таблица EOTF/OETF: столько узлов на отрезке [0, 1] (линейная интерполяция).
const LUT_N: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Transfer {
    Pq,
    Hlg,
}

/// Кадр HDR и как его сводить.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hdr {
    pub transfer: Transfer,
    /// Самое яркое в ролике, нит.
    pub peak_nits: f32,
}

/// Формат 10/12/16 бит 4:2:0, который понимает [`convert`]: (плоскостей, сдвиг вправо, чтобы
/// получить 10-битное значение).
fn layout(fmt: Pixel) -> Option<(usize, u32)> {
    Some(match fmt {
        // NV12-подобные: значение в старших битах 16-битного слова
        Pixel::P010LE => (2, 6),
        Pixel::P016LE => (2, 6),
        // планарные: значение в младших битах
        Pixel::YUV420P10LE => (3, 0),
        Pixel::YUV420P12LE => (3, 2),
        _ => return None,
    })
}

/// Нужен ли кадру тонмаппинг: передаточная функция PQ или HLG и знакомый формат.
pub fn detect(f: &frame::Video) -> Option<Hdr> {
    let transfer = match f.color_transfer_characteristic() {
        TransferCharacteristic::SMPTE2084 => Transfer::Pq,
        TransferCharacteristic::ARIB_STD_B67 => Transfer::Hlg,
        _ => return None,
    };
    layout(f.format())?;
    let peak_nits = match transfer {
        Transfer::Hlg => HLG_PEAK_NITS,
        Transfer::Pq => peak_from_side_data(f).unwrap_or(DEFAULT_PEAK_NITS),
    };
    // ниже белого SDR сжимать нечего; выше 10 000 PQ не бывает
    Some(Hdr { transfer, peak_nits: peak_nits.clamp(SDR_WHITE_NITS * 1.25, 10_000.0) })
}

/// `AVContentLightMetadata` (libavutil/mastering_display_metadata.h) — в привязках ffmpeg-sys её нет.
#[repr(C)]
struct ContentLight {
    max_cll: u32,
    _max_fall: u32,
}

/// `AVMasteringDisplayMetadata` (там же).
#[repr(C)]
struct Mastering {
    display_primaries: [[ffi::AVRational; 2]; 3],
    white_point: [ffi::AVRational; 2],
    min_luminance: ffi::AVRational,
    max_luminance: ffi::AVRational,
    has_primaries: i32,
    has_luminance: i32,
}

/// MaxCLL (самый яркий пиксель ролика), иначе пик мастеринг-дисплея.
fn peak_from_side_data(f: &frame::Video) -> Option<f32> {
    // SAFETY: side data кадра — массив libavutil; структуры читаются только при совпадении типа и
    // достаточном размере.
    unsafe {
        let p = f.as_ptr();
        let sd = ffi::av_frame_get_side_data(p, ffi::AVFrameSideDataType::AV_FRAME_DATA_CONTENT_LIGHT_LEVEL);
        if !sd.is_null() && (*sd).size >= std::mem::size_of::<ContentLight>() {
            let cll = &*((*sd).data as *const ContentLight);
            if cll.max_cll > 0 {
                return Some(cll.max_cll as f32);
            }
        }
        let sd = ffi::av_frame_get_side_data(p, ffi::AVFrameSideDataType::AV_FRAME_DATA_MASTERING_DISPLAY_METADATA);
        if !sd.is_null() && (*sd).size >= std::mem::size_of::<Mastering>() {
            let m = &*((*sd).data as *const Mastering);
            if m.has_luminance != 0 && m.max_luminance.den != 0 {
                let v = m.max_luminance.num as f32 / m.max_luminance.den as f32;
                if v > 0.0 {
                    return Some(v);
                }
            }
        }
    }
    None
}

// ─── передаточные функции ───────────────────────────────────────────────────

const PQ_M1: f64 = 2610.0 / 16384.0;
const PQ_M2: f64 = 2523.0 / 4096.0 * 128.0;
const PQ_C1: f64 = 3424.0 / 4096.0;
const PQ_C2: f64 = 2413.0 / 4096.0 * 32.0;
const PQ_C3: f64 = 2392.0 / 4096.0 * 32.0;

/// PQ: сигнал [0, 1] → нит.
pub fn pq_eotf(e: f64) -> f64 {
    let p = e.clamp(0.0, 1.0).powf(1.0 / PQ_M2);
    10_000.0 * ((p - PQ_C1).max(0.0) / (PQ_C2 - PQ_C3 * p)).powf(1.0 / PQ_M1)
}

/// PQ: нит → сигнал [0, 1].
pub fn pq_inv(nits: f64) -> f64 {
    let y = (nits / 10_000.0).clamp(0.0, 1.0).powf(PQ_M1);
    ((PQ_C1 + PQ_C2 * y) / (1.0 + PQ_C3 * y)).powf(PQ_M2)
}

/// HLG: сигнал [0, 1] → относительная сцена [0, 1] (обратная OETF, BT.2100).
fn hlg_inv_oetf(e: f64) -> f64 {
    const A: f64 = 0.178_832_77;
    const B: f64 = 0.284_668_92;
    const C: f64 = 0.559_910_73;
    let e = e.clamp(0.0, 1.0);
    if e <= 0.5 {
        e * e / 3.0
    } else {
        (((e - C) / A).exp() + B) / 12.0
    }
}

/// sRGB OETF: линейный [0, 1] → сигнал.
fn srgb_oetf(l: f64) -> f64 {
    let l = l.clamp(0.0, 1.0);
    if l <= 0.003_130_8 {
        12.92 * l
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// BT.2390 EETF: нормированный PQ-сигнал источника [0, 1] (1 — его пик) → то же для приёмника с
/// пиком `max_lum` (нормированным так же). Ниже колена — без изменений, выше — сплайн Эрмита.
fn bt2390(e: f64, max_lum: f64) -> f64 {
    let ks = 1.5 * max_lum - 0.5;
    if e < ks {
        return e;
    }
    let t = (e - ks) / (1.0 - ks);
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * ks + (t3 - 2.0 * t2 + t) * (1.0 - ks) + (-2.0 * t3 + 3.0 * t2) * max_lum
}

// ─── таблицы ────────────────────────────────────────────────────────────────

/// Узлов в таблицах по сигналу: шаг 1/1023 — точность самого 10-битного сигнала.
const SIG_N: usize = 1024;

/// Таблицы под передаточную функцию и пик. Все вместе ~12 КБ — помещаются в L1: на 4K это
/// важнее любой арифметики (с таблицами по 16 КБ пиксель стоил 32 нс из-за промахов кеша).
/// EOTF монотонна, поэтому максимум RGB для тонмаппинга берётся прямо по сигналу.
pub struct Tables {
    hdr: Hdr,
    /// сигнал (1024 узла) → линейный свет, нит/203
    eotf: Vec<f32>,
    /// сигнал максимума RGB → множитель тонмаппинга
    scale: Vec<f32>,
    /// линейный [0, 1] (4096 узлов) → sRGB 8 бит
    oetf: Vec<u8>,
}

/// Вклад цветности в R', G', B' — общий у пары пикселей 4:2:0.
#[derive(Clone, Copy)]
pub struct Chroma {
    r: f32,
    g: f32,
    b: f32,
}

impl Tables {
    pub fn new(hdr: Hdr) -> Self {
        let sig = |i: usize| i as f64 / (SIG_N - 1) as f64;
        let nits_of = |e: f64| match hdr.transfer {
            Transfer::Pq => pq_eotf(e),
            // OOTF BT.2100 по каждому каналу (без поправки на яркость — для SDR достаточно)
            Transfer::Hlg => HLG_PEAK_NITS as f64 * hlg_inv_oetf(e).powf(1.2),
        };
        let eotf: Vec<f32> = (0..SIG_N).map(|i| (nits_of(sig(i)) / SDR_WHITE_NITS as f64) as f32).collect();
        // BT.2390 в домене PQ: от пика ролика к белому SDR
        let src_peak = pq_inv(hdr.peak_nits as f64);
        let dst_peak = pq_inv(SDR_WHITE_NITS as f64) / src_peak;
        let scale: Vec<f32> = (0..SIG_N)
            .map(|i| {
                let nits = nits_of(sig(i));
                if nits <= 1e-6 {
                    return 1.0;
                }
                let en = (pq_inv(nits) / src_peak).min(1.0);
                let out = pq_eotf(bt2390(en, dst_peak) * src_peak);
                (out / nits) as f32
            })
            .collect();
        let oetf = (0..LUT_N).map(|i| (srgb_oetf(i as f64 / (LUT_N - 1) as f64) * 255.0 + 0.5) as u8).collect();
        Self { hdr, eotf, scale, oetf }
    }

    pub fn hdr(&self) -> Hdr {
        self.hdr
    }

    /// Сигнал [0, 1] → индекс таблицы по сигналу.
    #[inline(always)]
    fn sidx(v: f32) -> usize {
        // as usize насыщает: отрицательное → 0
        ((v * (SIG_N - 1) as f32 + 0.5) as usize).min(SIG_N - 1)
    }

    /// Множитель тонмаппинга по сигналу максимума RGB.
    #[cfg(test)]
    fn tone_sig(&self, max_sig: f32) -> f32 {
        self.scale[Self::sidx(max_sig)]
    }

    #[inline(always)]
    fn encode(&self, l: f32) -> u8 {
        self.oetf[((l * (LUT_N - 1) as f32 + 0.5) as usize).min(LUT_N - 1)]
    }

    /// Cb, Cr (10 бит) → вклад в R'G'B' (BT.2020 NCL).
    #[inline(always)]
    pub fn chroma(cb: u16, cr: u16, full_range: bool) -> Chroma {
        let d = if full_range { 1023.0 } else { 896.0 };
        let (cbn, crn) = ((cb as f32 - 512.0) / d, (cr as f32 - 512.0) / d);
        Chroma { r: 1.4746 * crn, g: -0.164_553 * cbn - 0.571_353 * crn, b: 1.8814 * cbn }
    }

    /// Y' (10 бит) и вклад цветности → sRGB.
    #[inline(always)]
    pub fn pixel_c(&self, y: u16, c: Chroma, full_range: bool) -> [u8; 3] {
        let yn = if full_range { y as f32 / 1023.0 } else { (y as f32 - 64.0) / 876.0 };
        let (ri, gi, bi) = (Self::sidx(yn + c.r), Self::sidx(yn + c.g), Self::sidx(yn + c.b));
        let k = self.scale[ri.max(gi).max(bi)];
        let (r, g, b) = (self.eotf[ri] * k, self.eotf[gi] * k, self.eotf[bi] * k);
        // BT.2020 → BT.709 (линейный свет)
        let mut o = [
            1.660_491 * r - 0.587_641 * g - 0.072_850 * b,
            -0.124_550 * r + 1.132_900 * g - 0.008_349 * b,
            -0.018_151 * r - 0.100_579 * g + 1.118_730 * b,
        ];
        // вне охвата BT.709 (отрицательный канал): к яркости, пока не войдёт
        let min = o[0].min(o[1]).min(o[2]);
        if min < 0.0 {
            let lum = 0.2126 * o[0] + 0.7152 * o[1] + 0.0722 * o[2];
            if lum > 0.0 {
                let t = lum / (lum - min);
                for c in &mut o {
                    *c = lum + (*c - lum) * t;
                }
            } else {
                o = [0.0; 3];
            }
        }
        [self.encode(o[0].min(1.0)), self.encode(o[1].min(1.0)), self.encode(o[2].min(1.0))]
    }

    /// Один пиксель: Y', Cb, Cr (10 бит) → sRGB.
    pub fn pixel(&self, y: u16, cb: u16, cr: u16, full_range: bool) -> [u8; 3] {
        self.pixel_c(y, Self::chroma(cb, cr, full_range), full_range)
    }
}

/// Таблицы кешируются по (передача, пик): пересчёт — только при смене ролика.
pub fn tables(hdr: Hdr) -> Arc<Tables> {
    static CACHE: OnceLock<Mutex<Option<Arc<Tables>>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner());
    match c.as_ref() {
        Some(t) if t.hdr == hdr => t.clone(),
        _ => {
            let t = Arc::new(Tables::new(hdr));
            *c = Some(t.clone());
            t
        }
    }
}

/// Строки `y0..y0+rows` кадра HDR в RGBA8 (`dst` — начало всего выходного кадра, `row_bytes` —
/// его строка). Цветность 4:2:0 — по ближайшему отсчёту.
///
/// SAFETY: кадр одного из форматов [`layout`], `dst` вмещает `(y0 + rows) * row_bytes` байт, строки
/// полосы пишет только этот вызов.
pub unsafe fn convert_rows(f: *const ffi::AVFrame, t: &Tables, dst: *mut u8, row_bytes: usize, y0: u32, rows: u32) {
    let f = &*f;
    let fmt = Pixel::from(std::mem::transmute::<i32, ffi::AVPixelFormat>(f.format));
    let Some((planes, shift)) = layout(fmt) else { return };
    let full = f.color_range == ffi::AVColorRange::AVCOL_RANGE_JPEG;
    let w = f.width as usize;
    let rd = |p: *const u8, i: usize| -> u16 { (std::ptr::read_unaligned((p as *const u16).add(i)) >> shift).min(1023) };
    for y in y0 as usize..(y0 + rows) as usize {
        let yl = f.data[0].add(y * f.linesize[0] as usize);
        let cy = y / 2;
        let out = std::slice::from_raw_parts_mut(dst.add(y * row_bytes), w * 4);
        let (cu, cv, step) = if planes == 2 {
            let uv = f.data[1].add(cy * f.linesize[1] as usize);
            (uv, uv.add(2), 2usize) // Cb, Cr чередуются: пара на два пикселя
        } else {
            (f.data[1].add(cy * f.linesize[1] as usize), f.data[2].add(cy * f.linesize[2] as usize), 1usize)
        };
        for (pair, o) in out.chunks_mut(8).enumerate() {
            let c = Tables::chroma(rd(cu, pair * step), rd(cv, pair * step), full);
            let x = pair * 2;
            let a = t.pixel_c(rd(yl, x), c, full);
            o[..4].copy_from_slice(&[a[0], a[1], a[2], 255]);
            if o.len() == 8 {
                let b = t.pixel_c(rd(yl, x + 1), c, full);
                o[4..].copy_from_slice(&[b[0], b[1], b[2], 255]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pq_known_points() {
        // эталонные точки ST 2084: 100 нит ≈ 0.5081, 203 ≈ 0.5806, 1000 ≈ 0.7518
        assert!((pq_inv(100.0) - 0.5081).abs() < 1e-3);
        assert!((pq_inv(203.0) - 0.5806).abs() < 1e-3);
        assert!((pq_inv(1000.0) - 0.7518).abs() < 1e-3);
        assert!((pq_eotf(pq_inv(437.0)) - 437.0).abs() < 0.01);
    }

    #[test]
    fn eetf_keeps_shadows_compresses_peak() {
        let t = Tables::new(Hdr { transfer: Transfer::Pq, peak_nits: 1000.0 });
        let tone = |nits: f32| t.tone_sig(pq_inv(nits as f64) as f32);
        // тени — без изменений
        assert!((tone(10.0) - 1.0).abs() < 1e-3);
        // пик ролика ложится в белый SDR
        let k = tone(1000.0);
        assert!((1000.0 / SDR_WHITE_NITS * k - 1.0).abs() < 0.01, "пик → {}", 1000.0 / SDR_WHITE_NITS * k);
        // монотонно
        let mut prev = 0.0;
        for n in [1.0f32, 50.0, 100.0, 203.0, 400.0, 700.0, 1000.0] {
            let out = n / SDR_WHITE_NITS * tone(n);
            assert!(out >= prev, "{n}: {out} < {prev}");
            prev = out;
        }
    }

    /// 10-битный код PQ для заданных нит.
    fn code(nits: f64) -> u16 {
        (64.0 + pq_inv(nits) * 876.0).round() as u16
    }

    #[test]
    fn gray_white_black() {
        let t = Tables::new(Hdr { transfer: Transfer::Pq, peak_nits: 1000.0 });
        // чёрный
        assert_eq!(t.pixel(64, 512, 512, false), [0, 0, 0]);
        // серый 20 нит (≈ 10 % белого SDR) — нейтральный, без сдвига оттенка
        let g = t.pixel(code(20.0), 512, 512, false);
        assert!(g[0] == g[1] && g[1] == g[2], "{g:?}");
        assert!((g[0] as i32 - (srgb_oetf(20.0 / 203.0) * 255.0) as i32).abs() <= 2, "{g:?}");
        // пик 1000 нит — почти белый, а не «пересвет 255 с 400 нит»
        let w = t.pixel(code(1000.0), 512, 512, false);
        assert!(w[0] >= 250, "{w:?}");
        let mid = t.pixel(code(400.0), 512, 512, false);
        assert!(mid[0] < w[0], "400 нит {mid:?} не должен совпасть с пиком {w:?}");
    }

    #[test]
    fn saturated_red_stays_red() {
        let t = Tables::new(Hdr { transfer: Transfer::Pq, peak_nits: 1000.0 });
        // чистый красный BT.2020 (Cr максимум) — вне охвата 709, но остаётся красным, без чёрных дыр
        let p = t.pixel(code(100.0), 512, 960, false);
        assert!(p[0] > p[1] && p[0] > p[2], "{p:?}");
    }
}

#[cfg(test)]
mod bench {
    use super::*;

    /// `cargo test --release -p syngui --features ffmpeg --lib tonemap::bench -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn pixel_speed() {
        let t = Tables::new(Hdr { transfer: Transfer::Pq, peak_nits: 1000.0 });
        let n = 3840 * 2160;
        let mut acc = 0u32;
        let t0 = std::time::Instant::now();
        for i in 0..n {
            let p = t.pixel((64 + i % 876) as u16, (64 + (i / 7) % 896) as u16, (64 + (i / 13) % 896) as u16, false);
            acc = acc.wrapping_add(p[0] as u32 + p[1] as u32 + p[2] as u32);
        }
        let dt = t0.elapsed();
        eprintln!("4K кадр в один поток: {:.1} мс ({:.1} нс/пиксель) [{acc}]", dt.as_secs_f64() * 1e3, dt.as_nanos() as f64 / n as f64);
    }
}
