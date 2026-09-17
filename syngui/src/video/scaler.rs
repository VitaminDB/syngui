use std::os::raw::c_int;
use std::ptr;
use std::sync::Arc;

use ffmpeg_next::ffi;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame;
use ffmpeg_next::software::scaling::{Context, Flags};

use super::error::VideoError;

/// Сколько RGBA-буферов держать наготове. Больше, чем кадров в очереди
/// плеера (`VIDEO_QUEUE_CAP`) плюс показываемый, не нужно.
const BUFFER_POOL_CAP: usize = 12;

/// Потолок памяти под оборот буферов: кадр 4K в RGBA — 33 МБ, и держать
/// дюжину таких незачем. Что не влезло — разовый буфер на кадр.
const BUFFER_POOL_BYTES: usize = 192 * 1024 * 1024;

/// Запас в конце буфера: SIMD-пути swscale дописывают хвост за последним
/// пикселем строки. Лишнее не мешает — потребитель берёт `width*height*4`
/// байт (`ImageData` знает размеры кадра).
const SWS_TAIL_PADDING: usize = 64;

/// Потолок числа полос: больше потоков на кадр не окупается — каждая полоса
/// стоит запуска потока, а sws и так упирается в память.
const MAX_SLICES: usize = 8;

/// Минимальная высота полосы. Мельче дробить бессмысленно (1080p — 2–4
/// полосы, 4K — 8), да и хроме нужна чётность строк.
const MIN_SLICE_ROWS: u32 = 128;

/// Полоса кадра со своим контекстом swscale: контекст не потокобезопасен,
/// поэтому у каждого потока он свой. Контекст настроен на размер полосы —
/// каждая считается как самостоятельное изображение (срезы одного контекста
/// swscale принимает только подряд, сверху вниз).
struct Slice {
    ctx: Context,
    y0: u32,
    rows: u32,
}

// SAFETY: контекст swscale не потокобезопасен, но у каждой полосы он свой и
// за кадр им пользуется ровно один поток.
unsafe impl Send for Slice {}

/// Кадр и выходной буфер для потоков полос. Полосы не пересекаются по
/// строкам, поэтому одновременная запись в общий буфер безопасна.
struct SliceTarget {
    src: *const ffi::AVFrame,
    dst: *mut u8,
    row_bytes: usize,
    /// Во сколько раз плоскости цветности ниже яркостной: у YUV420/NV12 вдвое,
    /// и начало полосы в них сдвигается на `y0 >> log2_chroma_h`.
    log2_chroma_h: u32,
}

// SAFETY: указатели живут всё время `thread::scope` в `convert`, кадр только
// читается, а каждая полоса пишет свой диапазон строк выходного буфера.
unsafe impl Send for SliceTarget {}
unsafe impl Sync for SliceTarget {}

pub struct Scaler {
    /// Полосы кадра; при одной полосе всё считается в вызывающем потоке.
    slices: Vec<Slice>,
    in_fmt: Pixel,
    in_w: u32,
    in_h: u32,
    out_w: u32,
    out_h: u32,
    /// Кадр 4K в RGBA — 33 МБ. Выделять их заново на каждый кадр дороже самой
    /// конверсии: ядро отдаёт чистые страницы, и первое касание каждой стоит
    /// page fault (на 4K это 15–20 ms вместо 3). Буфер возвращается в оборот,
    /// как только показ отпустил свой `Arc`.
    pool: Vec<Arc<[u8]>>,
}

impl Scaler {
    pub fn new(
        in_fmt: Pixel,
        in_w: u32,
        in_h: u32,
        out_w: u32,
        out_h: u32,
    ) -> Result<Self, VideoError> {
        if in_w == 0 || in_h == 0 || out_w == 0 || out_h == 0 {
            return Err(VideoError::Scaler(format!(
                "нулевой размер: in {in_w}x{in_h} → out {out_w}x{out_h}"
            )));
        }
        let slices = Self::build_slices(in_fmt, in_w, in_h, out_w, out_h)?;
        Ok(Self {
            slices,
            in_fmt,
            in_w,
            in_h,
            out_w,
            out_h,
            pool: Vec::new(),
        })
    }

    /// Нарезать кадр на полосы — по одному контексту swscale на полосу.
    /// Конверсия NV12 (в нём кадры приходят из GPU) в RGBA идёт у swscale
    /// без быстрого пути и на 4K стоит ~13 ms в один поток — это и есть
    /// потолок частоты кадров, если не разложить работу по ядрам.
    fn build_slices(
        in_fmt: Pixel,
        in_w: u32,
        in_h: u32,
        out_w: u32,
        out_h: u32,
    ) -> Result<Vec<Slice>, VideoError> {
        // Полосами режем только конверсию один-в-один: при масштабировании
        // строки входа и выхода не совпадают.
        let splittable = in_w == out_w && in_h == out_h;
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let by_height = (in_h / MIN_SLICE_ROWS).max(1) as usize;
        let count = if splittable {
            cores.min(MAX_SLICES).min(by_height).max(1)
        } else {
            1
        };

        let mut slices = Vec::with_capacity(count);
        // Границы полос — по чётным строкам: у NV12/YUV420 хрома общая на
        // пару строк.
        let rows_each = ((in_h as usize / count) as u32 + 1) & !1;
        let mut y0 = 0;
        for i in 0..count {
            let rows = if i + 1 == count {
                in_h - y0
            } else {
                rows_each.min(in_h - y0)
            };
            if rows == 0 {
                break;
            }
            let ctx = Context::get(
                in_fmt,
                in_w,
                rows,
                Pixel::RGBA,
                out_w,
                rows,
                Flags::BILINEAR,
            )
            .map_err(|e| VideoError::Scaler(format!("sws_getContext: {e}")))?;
            slices.push(Slice { ctx, y0, rows });
            y0 += rows;
        }
        Ok(slices)
    }

    fn ensure_input(&mut self, in_fmt: Pixel, in_w: u32, in_h: u32) -> Result<(), VideoError> {
        if in_fmt == self.in_fmt && in_w == self.in_w && in_h == self.in_h {
            return Ok(());
        }
        log::debug!(
            "scaler: input изменился: {:?} {}x{} → {:?} {}x{}",
            self.in_fmt,
            self.in_w,
            self.in_h,
            in_fmt,
            in_w,
            in_h
        );
        self.slices = Self::build_slices(in_fmt, in_w, in_h, self.out_w, self.out_h)?;
        self.in_fmt = in_fmt;
        self.in_w = in_w;
        self.in_h = in_h;
        Ok(())
    }

    pub fn convert(&mut self, frame: &frame::Video) -> Result<Arc<[u8]>, VideoError> {
        self.ensure_input(frame.format(), frame.width(), frame.height())?;
        let row_bytes = (self.out_w as usize) * 4;
        let len = row_bytes * (self.out_h as usize) + SWS_TAIL_PADDING;
        let mut buf = self.take_buffer(len);
        let Some(dst) = Arc::get_mut(&mut buf) else {
            return Err(VideoError::Scaler("буфер кадра занят".into()));
        };
        // SAFETY: кадр жив до конца конверсии, полосы его только читают.
        let target = SliceTarget {
            src: unsafe { frame.as_ptr() },
            dst: dst.as_mut_ptr(),
            row_bytes,
            log2_chroma_h: chroma_shift(frame.format()),
        };
        if self.slices.len() == 1 {
            run_slice(&mut self.slices[0], &target);
        } else {
            // Полосы считаются параллельно: каждая пишет свои строки.
            std::thread::scope(|scope| {
                for slice in self.slices.iter_mut() {
                    let target = &target;
                    scope.spawn(move || run_slice(slice, target));
                }
            });
        }
        // Возвращаем буфер в оборот: пул держит вторую ссылку, поэтому
        // переиспользуем его только после того, как показ отпустит свою.
        if self.pool.len() < BUFFER_POOL_CAP && (self.pool.len() + 1) * len <= BUFFER_POOL_BYTES {
            self.pool.push(buf.clone());
        }
        Ok(buf)
    }

    /// Буфер, который уже никто не держит, — или новый, если все заняты.
    fn take_buffer(&mut self, len: usize) -> Arc<[u8]> {
        self.pool.retain(|b| b.len() == len);
        match self
            .pool
            .iter()
            .position(|b| Arc::strong_count(b) == 1 && Arc::weak_count(b) == 0)
        {
            Some(i) => self.pool.swap_remove(i),
            None => Arc::from(vec![0u8; len].into_boxed_slice()),
        }
    }

    pub fn out_size(&self) -> (u32, u32) {
        (self.out_w, self.out_h)
    }
}

/// Конверсия одной полосы кадра. SAFETY: `slice.ctx` настроен на формат и
/// размер этого кадра (`ensure_input`), кадр жив на время вызова, а полоса
/// пишет только свои строки выходного буфера — sws адресует их от начала
/// кадра с учётом `y0`.
fn run_slice(slice: &mut Slice, target: &SliceTarget) {
    unsafe {
        let src = &*target.src;
        // Начало полосы в каждой плоскости кадра: яркость — со строки y0,
        // цветность — со строки y0 >> log2_chroma_h.
        let mut src_data: [*const u8; 4] = [ptr::null(); 4];
        for (i, slot) in src_data.iter_mut().enumerate() {
            if src.data[i].is_null() {
                continue;
            }
            let shift = if i == 1 || i == 2 {
                target.log2_chroma_h
            } else {
                0
            };
            let rows = (slice.y0 >> shift) as isize;
            *slot = src.data[i].offset(rows * src.linesize[i] as isize);
        }
        let dst_data: [*mut u8; 4] = [
            target.dst.add(slice.y0 as usize * target.row_bytes),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        ];
        let dst_linesize: [c_int; 4] = [target.row_bytes as c_int, 0, 0, 0];
        ffi::sws_scale(
            slice.ctx.as_mut_ptr(),
            src_data.as_ptr(),
            src.linesize.as_ptr(),
            0,
            slice.rows as c_int,
            dst_data.as_ptr(),
            dst_linesize.as_ptr(),
        );
    }
}

/// На сколько ступеней по вертикали прорежена цветность (`log2_chroma_h`).
fn chroma_shift(fmt: Pixel) -> u32 {
    // SAFETY: дескриптор формата — статическая таблица libavutil.
    unsafe {
        let desc = ffi::av_pix_fmt_desc_get(fmt.into());
        if desc.is_null() {
            0
        } else {
            (*desc).log2_chroma_h as u32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ffmpeg_next::frame;

    fn gray_yuv(w: u32, h: u32) -> frame::Video {
        let mut yuv = frame::Video::new(Pixel::YUV420P, w, h);
        for plane in 0..3 {
            let stride = yuv.stride(plane);
            let rows = if plane == 0 {
                h as usize
            } else {
                (h as usize) / 2
            };
            let data = yuv.data_mut(plane);
            for byte in data.iter_mut().take(stride * rows) {
                *byte = 128;
            }
        }
        yuv
    }

    #[test]
    fn yuv420p_to_rgba_basic() {
        ffmpeg_next::init().ok();
        let mut yuv = frame::Video::new(Pixel::YUV420P, 4, 4);
        for plane in 0..3 {
            let stride = yuv.stride(plane);
            let data = yuv.data_mut(plane);
            let val = if plane == 0 { 128u8 } else { 128u8 };
            for byte in data
                .iter_mut()
                .take(stride * if plane == 0 { 4 } else { 2 })
            {
                *byte = val;
            }
        }

        let mut scaler = Scaler::new(Pixel::YUV420P, 4, 4, 4, 4).expect("scaler");
        let rgba = scaler.convert(&yuv).expect("convert");
        let frame_bytes = 4 * 4 * 4;
        assert!(
            rgba.len() >= frame_bytes,
            "в буфере должен помещаться кадр 4×4 плюс запас под хвост swscale"
        );
        for pixel in rgba[..frame_bytes].chunks_exact(4) {
            assert_eq!(pixel[3], 255, "alpha=255 для непрозрачной RGBA-конверсии");
        }
    }

    #[test]
    fn buffer_is_reused_once_frame_is_released() {
        ffmpeg_next::init().ok();
        let yuv = gray_yuv(16, 16);
        let mut scaler = Scaler::new(Pixel::YUV420P, 16, 16, 16, 16).expect("scaler");

        let first = scaler.convert(&yuv).expect("convert");
        let first_ptr = first.as_ptr();
        drop(first);
        let second = scaler.convert(&yuv).expect("convert");
        assert_eq!(
            second.as_ptr(),
            first_ptr,
            "отпущенный буфер должен уйти в следующий кадр, а не выделяться заново"
        );

        // Пока кадр держат, следующий получает другой буфер.
        let third = scaler.convert(&yuv).expect("convert");
        assert_ne!(
            third.as_ptr(),
            second.as_ptr(),
            "занятый буфер переиспользовать нельзя"
        );
    }

    #[test]
    fn multi_slice_conversion_covers_every_row() {
        ffmpeg_next::init().ok();
        // Высота больше двух минимальных полос — конверсия пойдёт по полосам.
        let (w, h) = (64u32, (MIN_SLICE_ROWS * 3) as u32);
        let mut yuv = frame::Video::new(Pixel::YUV420P, w, h);
        // Яркость растёт сверху вниз: пропущенная или сдвинутая полоса сразу
        // видна как разрыв градиента.
        let y_stride = yuv.stride(0);
        for y in 0..h as usize {
            let v = (16 + (y * 200) / h as usize) as u8;
            let row = &mut yuv.data_mut(0)[y * y_stride..y * y_stride + w as usize];
            row.fill(v);
        }
        for plane in 1..3 {
            let stride = yuv.stride(plane);
            let rows = (h / 2) as usize;
            yuv.data_mut(plane)[..stride * rows].fill(128);
        }

        let mut scaler = Scaler::new(Pixel::YUV420P, w, h, w, h).expect("scaler");
        assert!(scaler.slices.len() > 1, "кадр должен резаться на полосы");
        let rgba = scaler.convert(&yuv).expect("convert");

        let row_bytes = w as usize * 4;
        let mut prev = 0u8;
        for y in 0..h as usize {
            let px = &rgba[y * row_bytes..y * row_bytes + 4];
            assert!(
                px[0] >= prev,
                "строка {y}: яркость {} упала после {prev} — полоса потеряна или сдвинута",
                px[0]
            );
            assert_eq!(px[3], 255, "строка {y}: альфа должна быть 255");
            prev = px[0];
        }
        assert!(
            prev > 200,
            "низ кадра должен быть светлее верха, получено {prev}"
        );
    }
}
