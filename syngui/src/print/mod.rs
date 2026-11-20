//! Печать: принтеры CUPS, окно печати с предпросмотром и настройками,
//! печать в PDF-файл.
//!
//! Программа описывает, что печатать, через [`PrintDocument`] (готовое —
//! [`ImageDocument`] для картинок), и показывает [`PrintDialog`]:
//!
//! ```ignore
//! let open = use_signal(false);
//! let doc = ImageDocument::new("photo.jpg", 1, 0, |_| PrintImage::load(path));
//! PrintDialog::new(open, Arc::new(doc)).on_done(|r| flash(r)).build()
//! ```
//!
//! Окно само узнаёт принтеры и их возможности (размеры бумаги, поля,
//! цвет, двусторонняя печать, остальные настройки драйвера — из PPD),
//! рисует листы так, как они лягут на бумагу, собирает PDF и отдаёт его
//! `lp`. Последние выбранные настройки запоминаются
//! (`$XDG_CONFIG_HOME/syngui/print.conf`).

pub mod cups;
mod dialog;
pub mod paper;
pub mod pdf;
pub mod ppd;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub use cups::{Job, Printer, PrinterState};
pub use dialog::{outcome_text, PrintDialog};
pub use paper::Paper;
pub use ppd::{Choice, Ppd, PpdOption};

/// Назначение «Сохранить в PDF» вместо принтера.
pub const PDF_DESTINATION: &str = "\u{1}pdf";

/// Картинка для печати: RGBA, 8 бит.
#[derive(Clone)]
pub struct PrintImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

impl PartialEq for PrintImage {
    fn eq(&self, o: &Self) -> bool {
        self.width == o.width && self.height == o.height && Arc::ptr_eq(&self.rgba, &o.rgba)
    }
}

impl std::fmt::Debug for PrintImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PrintImage({}×{})", self.width, self.height)
    }
}

impl PrintImage {
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self { width, height, rgba: Arc::new(rgba) }
    }

    /// Прочитать файл (с поворотом по EXIF). SVG — растром по длинной стороне 3000 точек.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        Self::decode(&bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        use image::ImageDecoder;
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
        match reader.into_decoder() {
            Ok(mut dec) => {
                let orientation = dec.orientation().ok();
                let mut img = image::DynamicImage::from_decoder(dec).map_err(|e| e.to_string())?;
                if let Some(o) = orientation {
                    img.apply_orientation(o);
                }
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                Ok(Self::from_rgba(w, h, rgba.into_raw()))
            }
            #[cfg(feature = "svg")]
            Err(_) => Self::decode_svg(bytes, 3000),
            #[cfg(not(feature = "svg"))]
            Err(e) => Err(e.to_string()),
        }
    }

    #[cfg(feature = "svg")]
    fn decode_svg(bytes: &[u8], side: u32) -> Result<Self, String> {
        use resvg::{tiny_skia, usvg};
        let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).map_err(|e| e.to_string())?;
        let size = tree.size();
        let k = side as f32 / size.width().max(size.height()).max(1.0);
        let (w, h) = (((size.width() * k).round() as u32).max(1), ((size.height() * k).round() as u32).max(1));
        let mut pm = tiny_skia::Pixmap::new(w, h).ok_or("svg: пустой размер")?;
        resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pm.as_mut());
        // tiny-skia хранит premultiplied — вернуть прямую альфу.
        let mut data = pm.take();
        for p in data.chunks_exact_mut(4) {
            let a = p[3] as u32;
            if a > 0 && a < 255 {
                for c in &mut p[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        Ok(Self::from_rgba(w, h, data))
    }

    /// Повернуть на `turns` четвертей по часовой и отразить (сначала поворот).
    pub fn transformed(&self, turns: i32, flip_h: bool, flip_v: bool) -> Self {
        let turns = turns.rem_euclid(4);
        if turns == 0 && !flip_h && !flip_v {
            return self.clone();
        }
        let Some(img) = image::RgbaImage::from_raw(self.width, self.height, (*self.rgba).clone()) else { return self.clone() };
        let mut img = match turns {
            1 => image::imageops::rotate90(&img),
            2 => image::imageops::rotate180(&img),
            3 => image::imageops::rotate270(&img),
            _ => img,
        };
        if flip_h {
            image::imageops::flip_horizontal_in_place(&mut img);
        }
        if flip_v {
            image::imageops::flip_vertical_in_place(&mut img);
        }
        let (w, h) = img.dimensions();
        Self::from_rgba(w, h, img.into_raw())
    }

    /// Уменьшить, чтобы длинная сторона была не больше `side` (пропорции сохраняются).
    pub fn shrink_to(&self, side: u32) -> Self {
        let long = self.width.max(self.height);
        if long <= side || side == 0 {
            return self.clone();
        }
        let k = side as f32 / long as f32;
        let (w, h) = (((self.width as f32 * k).round() as u32).max(1), ((self.height as f32 * k).round() as u32).max(1));
        let Some(img) = image::RgbaImage::from_raw(self.width, self.height, (*self.rgba).clone()) else { return self.clone() };
        let out = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
        Self::from_rgba(w, h, out.into_raw())
    }

    fn landscape(&self) -> bool {
        self.width > self.height
    }
}

/// Прямоугольник на листе, мм от левого верхнего угла.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectMm {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Картинка на листе: где лежит (`rect`) и по какой рамке обрезана (`clip`).
#[derive(Debug, Clone)]
pub struct PlacedImage {
    pub image: PrintImage,
    pub rect: RectMm,
    pub clip: RectMm,
}

/// Лист: размер бумаги с учётом ориентации и область печати внутри полей.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sheet {
    pub width_mm: f32,
    pub height_mm: f32,
    pub printable: RectMm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageOrientation {
    /// По содержимому: широкая картинка — альбомная.
    #[default]
    Auto,
    Portrait,
    Landscape,
}

/// Как картинка ложится в ячейку.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Целиком, с полями.
    #[default]
    Fit,
    /// На всю ячейку, лишнее обрезается.
    Fill,
}

/// Какие страницы (картинки) печатать.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PageRange {
    #[default]
    All,
    Current,
    /// «1-3, 5, 8-» — номера с единицы.
    Custom(String),
}

impl PageRange {
    /// Номера (с нуля) выбранных страниц из `total`.
    pub fn select(&self, total: usize, current: usize) -> Vec<usize> {
        match self {
            PageRange::All => (0..total).collect(),
            PageRange::Current => vec![current.min(total.saturating_sub(1))].into_iter().filter(|_| total > 0).collect(),
            PageRange::Custom(s) => parse_range(s, total).unwrap_or_default(),
        }
    }
}

/// «1-3, 5, 8-» → номера с нуля. `None` — ошибка в записи.
pub fn parse_range(s: &str, total: usize) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    for part in s.split([',', ';', ' ']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once(['-', '–']) {
            Some((a, b)) => {
                let a = if a.trim().is_empty() { 1 } else { a.trim().parse::<usize>().ok()? };
                let b = if b.trim().is_empty() { total } else { b.trim().parse::<usize>().ok()? };
                (a, b)
            }
            None => {
                let n = part.parse::<usize>().ok()?;
                (n, n)
            }
        };
        if a == 0 || b < a {
            return None;
        }
        for n in a..=b.min(total) {
            if !out.contains(&(n - 1)) {
                out.push(n - 1);
            }
        }
    }
    Some(out)
}

/// Настройки задания.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintSettings {
    /// Очередь CUPS или [`PDF_DESTINATION`].
    pub printer: String,
    pub copies: u32,
    pub collate: bool,
    /// Ключ размера бумаги (`A4`, `Letter`, …) — как у `PageSize` в PPD.
    pub paper: String,
    pub orientation: PageOrientation,
    pub gray: bool,
    /// Вариант `Duplex` из PPD (`None`, `DuplexNoTumble`, `DuplexTumble`); пусто — не задано.
    pub duplex: String,
    pub range: PageRange,
    /// Поля, мм (не меньше непечатаемых полей принтера).
    pub margin_mm: f32,
    pub fit: Fit,
    /// Картинок на листе (1, 2, 4, 6, 9).
    pub per_sheet: u32,
    /// Остальные настройки драйвера (`MediaType=Labels` и т. п.).
    pub extra: Vec<(String, String)>,
    /// Путь PDF-файла для [`PDF_DESTINATION`].
    pub pdf_path: PathBuf,
    /// Непечатаемые поля принтера, мм (лево, верх, право, низ) — из PPD.
    pub hw_margins: [f32; 4],
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            printer: String::new(),
            copies: 1,
            collate: true,
            paper: paper::default_key().into(),
            orientation: PageOrientation::Auto,
            gray: false,
            duplex: String::new(),
            range: PageRange::All,
            margin_mm: 5.0,
            fit: Fit::Fit,
            per_sheet: 1,
            extra: Vec::new(),
            pdf_path: PathBuf::new(),
            hw_margins: [0.0; 4],
        }
    }
}

/// Что печатать. Пункты — страницы документа или картинки.
pub trait PrintDocument: Send + Sync + 'static {
    fn title(&self) -> String;
    /// Сколько пунктов (страниц, картинок).
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Текущий пункт (для «Текущая»).
    fn current(&self) -> usize {
        0
    }
    /// Что выбрано в «Страницах» при открытии окна.
    fn default_range(&self) -> PageRange {
        PageRange::All
    }
    /// Картинки: в окне есть «на листе» и «вписать/заполнить».
    fn is_images(&self) -> bool {
        false
    }
    /// Альбомный ли лист в режиме «Авто» для этих пунктов.
    fn prefers_landscape(&self, _items: &[usize]) -> bool {
        false
    }
    /// Разложить пункты на лист. `preview` — длинная сторона растра для предпросмотра
    /// (картинки можно отдать уменьшенными); `None` — для печати.
    fn render(&self, items: &[usize], sheet: &Sheet, settings: &PrintSettings, preview: Option<u32>) -> Result<Vec<PlacedImage>, String>;
}

type Loader = Arc<dyn Fn(usize) -> Result<PrintImage, String> + Send + Sync>;

/// Картинки: по одной или несколько на лист, вписанные или на всю ячейку.
pub struct ImageDocument {
    title: String,
    count: usize,
    current: usize,
    loader: Loader,
    range: PageRange,
    full: Mutex<Vec<(usize, PrintImage)>>,
    small: Mutex<Vec<((usize, u32), PrintImage)>>,
}

const FULL_CACHE: usize = 3;
const SMALL_CACHE: usize = 32;

impl ImageDocument {
    /// `loader(i)` — картинка `i` (уже повёрнутая как надо).
    pub fn new(title: impl Into<String>, count: usize, current: usize, loader: impl Fn(usize) -> Result<PrintImage, String> + Send + Sync + 'static) -> Self {
        Self {
            title: title.into(),
            count,
            current,
            loader: Arc::new(loader),
            range: PageRange::All,
            full: Mutex::new(Vec::new()),
            small: Mutex::new(Vec::new()),
        }
    }

    /// Что выбрано в «Страницах» при открытии окна (просмотрщик: текущая картинка).
    pub fn default_range(mut self, r: PageRange) -> Self {
        self.range = r;
        self
    }

    fn image(&self, i: usize) -> Result<PrintImage, String> {
        if let Some(img) = self.full.lock().unwrap().iter().find(|(k, _)| *k == i).map(|(_, v)| v.clone()) {
            return Ok(img);
        }
        let img = (self.loader)(i)?;
        let mut c = self.full.lock().unwrap();
        c.push((i, img.clone()));
        if c.len() > FULL_CACHE {
            c.remove(0);
        }
        Ok(img)
    }

    fn preview_image(&self, i: usize, side: u32) -> Result<PrintImage, String> {
        if let Some(img) = self.small.lock().unwrap().iter().find(|(k, _)| *k == (i, side)).map(|(_, v)| v.clone()) {
            return Ok(img);
        }
        let img = self.image(i)?.shrink_to(side);
        let mut c = self.small.lock().unwrap();
        c.push(((i, side), img.clone()));
        if c.len() > SMALL_CACHE {
            c.remove(0);
        }
        Ok(img)
    }

    /// Размеры картинки без декодирования в превью-кэш (для ориентации).
    fn is_landscape(&self, i: usize) -> bool {
        self.preview_image(i, 256).map(|p| p.landscape()).unwrap_or(false)
    }
}

/// Сетка (столбцы, строки) для `n` картинок на листе.
pub fn grid(n: u32, landscape_sheet: bool) -> (u32, u32) {
    let (c, r) = match n {
        0 | 1 => (1, 1),
        2 => (1, 2),
        3 => (1, 3),
        4 => (2, 2),
        6 => (2, 3),
        8 => (2, 4),
        9 => (3, 3),
        n => {
            let c = (n as f32).sqrt().ceil() as u32;
            (c, n.div_ceil(c))
        }
    };
    if landscape_sheet {
        (r, c)
    } else {
        (c, r)
    }
}

/// Промежуток между картинками на листе, мм.
const GAP_MM: f32 = 4.0;

/// Положить картинку с пропорцией `aspect` (ш/в) в ячейку.
pub fn place(cell: RectMm, aspect: f32, fit: Fit) -> RectMm {
    let cell_aspect = cell.w / cell.h.max(0.001);
    let (w, h) = match fit {
        Fit::Fit if aspect > cell_aspect => (cell.w, cell.w / aspect),
        Fit::Fit => (cell.h * aspect, cell.h),
        Fit::Fill if aspect > cell_aspect => (cell.h * aspect, cell.h),
        Fit::Fill => (cell.w, cell.w / aspect),
    };
    RectMm { x: cell.x + (cell.w - w) / 2.0, y: cell.y + (cell.h - h) / 2.0, w, h }
}

impl PrintDocument for ImageDocument {
    fn title(&self) -> String {
        self.title.clone()
    }
    fn len(&self) -> usize {
        self.count
    }
    fn current(&self) -> usize {
        self.current
    }
    fn is_images(&self) -> bool {
        true
    }
    fn default_range(&self) -> PageRange {
        self.range.clone()
    }
    fn prefers_landscape(&self, items: &[usize]) -> bool {
        // Одна картинка на листе — по ней; несколько — книжный лист.
        items.len() == 1 && self.is_landscape(items[0])
    }

    fn render(&self, items: &[usize], sheet: &Sheet, s: &PrintSettings, preview: Option<u32>) -> Result<Vec<PlacedImage>, String> {
        let n = s.per_sheet.max(1);
        let (cols, rows) = grid(n, sheet.width_mm > sheet.height_mm);
        let p = sheet.printable;
        let gap = if n > 1 { GAP_MM } else { 0.0 };
        let cw = (p.w - gap * (cols - 1) as f32) / cols as f32;
        let ch = (p.h - gap * (rows - 1) as f32) / rows as f32;
        let mut out = Vec::new();
        for (k, &i) in items.iter().enumerate() {
            let (col, row) = (k as u32 % cols, k as u32 / cols);
            let cell = RectMm { x: p.x + col as f32 * (cw + gap), y: p.y + row as f32 * (ch + gap), w: cw, h: ch };
            let img = match preview {
                // Предпросмотр: картинка размером примерно с ячейку на экране.
                Some(side) => {
                    let need = (side as f32 * cw.max(ch) / sheet.width_mm.max(sheet.height_mm)).ceil() as u32;
                    self.preview_image(i, need.clamp(64, 2048).next_power_of_two())?
                }
                None => self.image(i)?,
            };
            let aspect = img.width as f32 / img.height.max(1) as f32;
            let rect = place(cell, aspect, s.fit);
            let image = match preview {
                // Печать: больше 300 точек на дюйм не нужно.
                None => {
                    let max = (rect.w.max(rect.h) / 25.4 * 300.0).ceil() as u32;
                    img.shrink_to(max.max(64))
                }
                Some(_) => img,
            };
            out.push(PlacedImage { image, rect, clip: cell });
        }
        Ok(out)
    }
}

/// Раскладка задания: листы с номерами пунктов на них.
pub fn sheets(doc: &dyn PrintDocument, s: &PrintSettings) -> Vec<Vec<usize>> {
    let items = s.range.select(doc.len(), doc.current());
    let per = if doc.is_images() { s.per_sheet.max(1) as usize } else { 1 };
    items.chunks(per).map(|c| c.to_vec()).collect()
}

/// Геометрия листа для пунктов `items`.
pub fn sheet_for(doc: &dyn PrintDocument, items: &[usize], s: &PrintSettings) -> Sheet {
    let (pw, ph) = paper::size_mm(&s.paper).unwrap_or((210.0, 297.0));
    let landscape = match s.orientation {
        PageOrientation::Portrait => false,
        PageOrientation::Landscape => true,
        PageOrientation::Auto => doc.prefers_landscape(items),
    };
    let (w, h) = if landscape { (pw.max(ph), pw.min(ph)) } else { (pw.min(ph), pw.max(ph)) };
    // Непечатаемые поля даны для книжного листа: в альбомном они поворачиваются.
    let [l, t, r, b] = s.hw_margins;
    let hw = if landscape { [b, l, t, r] } else { [l, t, r, b] };
    let m = |i: usize| s.margin_mm.max(hw[i]);
    let printable = RectMm { x: m(0), y: m(1), w: (w - m(0) - m(2)).max(1.0), h: (h - m(1) - m(3)).max(1.0) };
    Sheet { width_mm: w, height_mm: h, printable }
}

/// Растр листа для предпросмотра: длинная сторона — `side` точек.
pub fn render_preview(doc: &dyn PrintDocument, items: &[usize], s: &PrintSettings, side: u32) -> Result<(Sheet, PrintImage), String> {
    let sheet = sheet_for(doc, items, s);
    let k = side as f32 / sheet.width_mm.max(sheet.height_mm);
    let (w, h) = (((sheet.width_mm * k).round() as u32).max(1), ((sheet.height_mm * k).round() as u32).max(1));
    let mut px = vec![255u8; (w * h * 4) as usize];
    for item in doc.render(items, &sheet, s, Some(side))? {
        blit(&mut px, w, h, k, &item, s.gray);
    }
    Ok((sheet, PrintImage::from_rgba(w, h, px)))
}

/// Нарисовать картинку на растре листа (масштаб `k` точек на мм), билинейно, с альфой на белом.
fn blit(px: &mut [u8], w: u32, h: u32, k: f32, item: &PlacedImage, gray: bool) {
    let r = item.rect;
    let c = item.clip;
    let x0 = (r.x.max(c.x) * k).floor().max(0.0) as i64;
    let y0 = (r.y.max(c.y) * k).floor().max(0.0) as i64;
    let x1 = (((r.x + r.w).min(c.x + c.w)) * k).ceil().min(w as f32) as i64;
    let y1 = (((r.y + r.h).min(c.y + c.h)) * k).ceil().min(h as f32) as i64;
    let img = &item.image;
    let (iw, ih) = (img.width as f32, img.height as f32);
    for y in y0..y1 {
        let v = ((y as f32 + 0.5) / k - r.y) / r.h * ih - 0.5;
        for x in x0..x1 {
            let u = ((x as f32 + 0.5) / k - r.x) / r.w * iw - 0.5;
            let s = sample(img, u, v);
            let a = s[3] / 255.0;
            let o = ((y as u32 * w + x as u32) * 4) as usize;
            let mut rgb = [0u8; 3];
            for ch in 0..3 {
                rgb[ch] = (s[ch] * a + px[o + ch] as f32 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
            }
            if gray {
                let l = pdf::luma(rgb[0], rgb[1], rgb[2]);
                rgb = [l, l, l];
            }
            px[o..o + 3].copy_from_slice(&rgb);
        }
    }
}

fn sample(img: &PrintImage, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (img.width as i64, img.height as i64);
    let (fx, fy) = (u.floor(), v.floor());
    let (tx, ty) = (u - fx, v - fy);
    let get = |x: i64, y: i64| {
        let (x, y) = (x.clamp(0, w - 1), y.clamp(0, h - 1));
        let o = ((y * w + x) * 4) as usize;
        [img.rgba[o] as f32, img.rgba[o + 1] as f32, img.rgba[o + 2] as f32, img.rgba[o + 3] as f32]
    };
    let (x, y) = (fx as i64, fy as i64);
    let (a, b, c, d) = (get(x, y), get(x + 1, y), get(x, y + 1), get(x + 1, y + 1));
    let mut out = [0.0; 4];
    for i in 0..4 {
        let top = a[i] + (b[i] - a[i]) * tx;
        let bot = c[i] + (d[i] - c[i]) * tx;
        out[i] = top + (bot - top) * ty;
    }
    out
}

/// Итог задания.
#[derive(Debug, Clone, PartialEq)]
pub enum PrintOutcome {
    /// Отправлено на принтер: номер задания, принтер.
    Sent { job: String, printer: String, sheets: usize },
    /// Сохранено в PDF.
    Saved(PathBuf),
}

/// Собрать PDF всех листов.
pub fn write_pdf(doc: &dyn PrintDocument, s: &PrintSettings, path: &Path, progress: &dyn Fn(usize, usize)) -> Result<usize, String> {
    let list = sheets(doc, s);
    if list.is_empty() {
        return Err(crate::i18n::builtin("print.nothing", "Nothing to print"));
    }
    let mut pages_data = Vec::new();
    for (n, items) in list.iter().enumerate() {
        progress(n, list.len());
        let sheet = sheet_for(doc, items, s);
        let placed = doc.render(items, &sheet, s, None)?;
        pages_data.push((sheet, placed));
    }
    let pages: Vec<pdf::PdfPage> = pages_data.iter().map(|(sh, items)| pdf::PdfPage { width_mm: sh.width_mm, height_mm: sh.height_mm, items }).collect();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("pdf.part");
    let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?);
    pdf::write(&mut f, &doc.title(), &pages, s.gray)?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(list.len())
}

/// Напечатать (или сохранить в PDF). Долго — вызывать не из потока интерфейса.
pub fn print(doc: &dyn PrintDocument, s: &PrintSettings, progress: &dyn Fn(usize, usize)) -> Result<PrintOutcome, String> {
    if s.printer == PDF_DESTINATION {
        let path = if s.pdf_path.as_os_str().is_empty() { default_pdf_path(&doc.title()) } else { s.pdf_path.clone() };
        write_pdf(doc, s, &path, progress)?;
        return Ok(PrintOutcome::Saved(path));
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let tmp = dir.join(format!("syngui-print-{}-{}.pdf", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)));
    let n = write_pdf(doc, s, &tmp, progress)?;
    let r = cups::submit(&s.printer, &tmp, &doc.title(), s.copies, &lp_options(s));
    let _ = std::fs::remove_file(&tmp);
    Ok(PrintOutcome::Sent { job: r?, printer: s.printer.clone(), sheets: n })
}

/// Настройки задания → `-o` для `lp`.
pub fn lp_options(s: &PrintSettings) -> Vec<(String, String)> {
    // `media` — для очередей IPP Everywhere, `PageSize` — для драйверов PPD.
    let mut o: Vec<(String, String)> = vec![("PageSize".into(), s.paper.clone()), ("media".into(), s.paper.clone())];
    if !s.duplex.is_empty() {
        o.push(("Duplex".into(), s.duplex.clone()));
    }
    if s.copies > 1 {
        o.push(("collate".into(), if s.collate { "true" } else { "false" }.into()));
    }
    if s.gray {
        o.push(("print-color-mode".into(), "monochrome".into()));
    }
    for (k, v) in &s.extra {
        if !o.iter().any(|(ok, _)| ok == k) {
            o.push((k.clone(), v.clone()));
        }
    }
    o
}

/// Папка «Документы» (`xdg-user-dir DOCUMENTS`), иначе домашняя.
pub fn documents_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
    let cfg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    if let Ok(text) = std::fs::read_to_string(cfg.join("user-dirs.dirs")) {
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("XDG_DOCUMENTS_DIR=") {
                let v = v.trim().trim_matches('"').replace("$HOME", &home.to_string_lossy());
                let p = PathBuf::from(v);
                if p.is_dir() {
                    return p;
                }
            }
        }
    }
    home
}

/// Имя PDF по названию документа, без перезаписи существующего.
pub fn default_pdf_path(title: &str) -> PathBuf {
    let stem = Path::new(title).file_stem().map(|s| s.to_string_lossy().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "print".into());
    let stem: String = stem.chars().map(|c| if c == '/' || c == '\0' { '_' } else { c }).collect();
    let dir = documents_dir();
    let mut p = dir.join(format!("{stem}.pdf"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{stem} ({n}).pdf"));
        n += 1;
    }
    p
}

// ─────────────────────────────────────────────────────────────── память настроек

fn store_path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let cfg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    cfg.join("syngui").join("print.conf")
}

/// Запомненное: `ключ=значение` построчно.
pub(crate) fn load_store() -> Vec<(String, String)> {
    std::fs::read_to_string(store_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect()
}

pub(crate) fn save_store(pairs: &[(String, String)]) {
    let p = store_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let text: String = pairs.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    let _ = std::fs::write(p, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("1-3, 5", 10).unwrap(), vec![0, 1, 2, 4]);
        assert_eq!(parse_range("8-", 10).unwrap(), vec![7, 8, 9]);
        assert_eq!(parse_range("-2", 10).unwrap(), vec![0, 1]);
        assert_eq!(parse_range("3,3,12", 10).unwrap(), vec![2]);
        assert!(parse_range("0", 10).is_none());
        assert!(parse_range("5-2", 10).is_none());
        assert!(parse_range("x", 10).is_none());
        assert_eq!(PageRange::Current.select(5, 3), vec![3]);
        assert!(PageRange::Current.select(0, 0).is_empty());
    }

    #[test]
    fn fit_and_fill() {
        let cell = RectMm { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        let r = place(cell, 2.0, Fit::Fit);
        assert_eq!((r.w, r.h, r.y), (100.0, 50.0, 25.0));
        let r = place(cell, 2.0, Fit::Fill);
        assert_eq!((r.w, r.h, r.x), (200.0, 100.0, -50.0));
    }

    fn doc(n: usize, w: u32, h: u32) -> ImageDocument {
        ImageDocument::new("t.png", n, 0, move |_| Ok(PrintImage::from_rgba(w, h, vec![0; (w * h * 4) as usize])))
    }

    #[test]
    fn auto_orientation_and_margins() {
        let d = doc(3, 400, 200);
        let mut s = PrintSettings { paper: "A4".into(), margin_mm: 5.0, hw_margins: [4.0, 4.0, 4.0, 10.0], ..Default::default() };
        let sh = sheet_for(&d, &[0], &s);
        assert_eq!((sh.width_mm, sh.height_mm), (297.0, 210.0), "широкая картинка — альбомный лист");
        // нижнее поле принтера 10 мм в альбомном — слева
        assert_eq!(sh.printable.x, 10.0);
        s.per_sheet = 2;
        assert_eq!(sheets(&d, &s), vec![vec![0, 1], vec![2]]);
        assert_eq!(sheet_for(&d, &[0, 1], &s).height_mm, 297.0, "несколько на листе — книжный");
        s.orientation = PageOrientation::Portrait;
        s.per_sheet = 1;
        assert_eq!(sheet_for(&d, &[0], &s).width_mm, 210.0);
    }

    #[test]
    fn preview_draws_image_in_place() {
        // Красная картинка 2:1 на книжном A4 без полей — полоса посередине.
        let d = ImageDocument::new("r", 1, 0, |_| Ok(PrintImage::from_rgba(2, 1, vec![255, 0, 0, 255, 255, 0, 0, 255])));
        let s = PrintSettings { paper: "A4".into(), margin_mm: 0.0, orientation: PageOrientation::Portrait, ..Default::default() };
        let (_, img) = render_preview(&d, &[0], &s, 297).unwrap();
        assert_eq!((img.width, img.height), (210, 297));
        let at = |x: u32, y: u32| &img.rgba[((y * img.width + x) * 4) as usize..][..3];
        assert_eq!(at(105, 148), &[255, 0, 0]);
        assert_eq!(at(105, 10), &[255, 255, 255]);
        let s = PrintSettings { gray: true, ..s };
        let (_, img) = render_preview(&d, &[0], &s, 297).unwrap();
        let p = &img.rgba[((148 * img.width + 105) * 4) as usize..][..3];
        assert!(p[0] == p[1] && p[1] == p[2] && p[0] < 128);
    }

    #[test]
    fn pdf_file_round_trip() {
        let d = doc(2, 30, 20);
        let dir = std::env::temp_dir().join(format!("syngui-print-{}", std::process::id()));
        let path = dir.join("out.pdf");
        let s = PrintSettings { paper: "A5".into(), per_sheet: 1, ..Default::default() };
        let n = write_pdf(&d, &s, &path, &|_, _| {}).unwrap();
        assert_eq!(n, 2);
        let bytes = std::fs::read(&path).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("/Count 2"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn options_for_lp() {
        let s = PrintSettings { paper: "A4".into(), duplex: "DuplexNoTumble".into(), copies: 2, gray: true, extra: vec![("MediaType".into(), "Labels".into()), ("PageSize".into(), "x".into())], ..Default::default() };
        let o = lp_options(&s);
        assert!(o.contains(&("PageSize".into(), "A4".into())));
        assert!(o.contains(&("Duplex".into(), "DuplexNoTumble".into())));
        assert!(o.contains(&("collate".into(), "true".into())));
        assert!(o.contains(&("print-color-mode".into(), "monochrome".into())));
        assert!(o.contains(&("MediaType".into(), "Labels".into())));
        assert_eq!(o.iter().filter(|(k, _)| k == "PageSize").count(), 1);
    }
}

/// Живая проверка на машине с CUPS: `cargo test --features print live_cups -- --ignored --nocapture`.
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn live_cups() {
        let list = cups::printers().expect("CUPS");
        for p in &list {
            let ppd = cups::printer_ppd(&p.name);
            match ppd {
                Ok(ppd) => {
                    let sizes = ppd.option("PageSize").map(|o| o.choices.len()).unwrap_or(0);
                    eprintln!(
                        "{} [{}] {:?} цвет={} бумаг={} поля A4={:?} duplex={:?} опций={}",
                        p.title(),
                        p.name,
                        p.state,
                        ppd.color,
                        sizes,
                        ppd.hw_margins_mm("A4"),
                        ppd.option("Duplex").map(|o| o.choices.iter().map(|c| c.key.clone()).collect::<Vec<_>>()),
                        ppd.options.len()
                    );
                }
                Err(e) => eprintln!("{}: нет PPD: {e}", p.name),
            }
        }
        let d = ImageDocument::new("Проверка печати.png", 3, 1, |i| {
            let (w, h) = if i == 1 { (1200, 800) } else { (600, 900) };
            let mut px = Vec::with_capacity(w * h * 4);
            for y in 0..h {
                for x in 0..w {
                    px.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, (i * 120) as u8, 255]);
                }
            }
            Ok(PrintImage::from_rgba(w as u32, h as u32, px))
        });
        let out = std::env::var("PRINT_OUT").unwrap_or_else(|_| "/tmp/syngui-print-check.pdf".into());
        let s = PrintSettings { printer: PDF_DESTINATION.into(), paper: "A4".into(), per_sheet: 2, pdf_path: out.clone().into(), ..Default::default() };
        let r = print(&d, &s, &|i, n| eprintln!("лист {}/{}", i + 1, n)).unwrap();
        eprintln!("{r:?}");
    }
}
