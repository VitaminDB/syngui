//! Отрисовка текста субтитров в картинку: шрифт пользователя (с запасными
//! для символов, которых в нём нет), сбалансированный перенос строк,
//! сглаженный контур любой толщины (карта расстояний до глифов), мягкая
//! тень (гауссово размытие) и плашка под строками.
//!
//! Картинка рисуется в физических пикселях один раз на событие (в потоке
//! слоя показа, заранее — пока событие только приближается); анимации
//! появления и исчезновения — уже на GPU (прозрачность, сдвиг, масштаб).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Angle, Format, Transform, Vector};
use swash::FontRef;

use super::{Span, SubText};

/// Как субтитр появляется и исчезает.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubtitleAnimation {
    /// Сразу.
    None,
    /// Растворение.
    Fade,
    /// Всплывает снизу (исчезая — уходит чуть вверх).
    Rise,
    /// Из чуть меньшего размера.
    Zoom,
}

/// Оформление субтитров. Размеры — доли кегля или кадра, чтобы вид не
/// зависел от размера окна.
#[derive(Clone, Debug, PartialEq)]
pub struct SubtitleStyle {
    pub font_family: String,
    pub font_weight: u16,
    /// Кегль — доля высоты кадра (0.055 ≈ 59 px на 1080p).
    pub size: f32,
    /// Не мельче этого (логические px) — в маленьком окне и в портрете.
    pub min_size_px: f32,
    pub color: [u8; 3],
    /// Непрозрачность текста с контуром (0…1).
    pub opacity: f32,
    /// Толщина контура — доля кегля; 0 — без контура.
    pub outline: f32,
    pub outline_color: [u8; 3],
    /// Размытие края контура — доля кегля.
    pub outline_softness: f32,
    /// Сдвиг тени вниз-вправо — доля кегля.
    pub shadow: f32,
    /// Размытие тени — доля кегля.
    pub shadow_blur: f32,
    pub shadow_color: [u8; 3],
    /// Непрозрачность тени; 0 — без тени.
    pub shadow_opacity: f32,
    pub box_color: [u8; 3],
    /// Плашка под строками: непрозрачность, 0 — без плашки.
    pub box_opacity: f32,
    /// Высота строки — множитель высоты шрифта.
    pub line_spacing: f32,
    /// Межбуквенный интервал — доля кегля.
    pub letter_spacing: f32,
    /// Ширина строки не больше этой доли ширины кадра.
    pub max_width: f32,
    /// Отступ от нижнего (верхнего) края — доля высоты.
    pub margin: f32,
    /// Привязка к кадру (иначе — к окну: субтитры могут лечь на чёрные
    /// поля под кадром).
    pub in_frame: bool,
    pub anim_in: SubtitleAnimation,
    pub anim_in_ms: u32,
    pub anim_out: SubtitleAnimation,
    pub anim_out_ms: u32,
    /// Цвета из файла (`{\c}`, `<font color>`).
    pub file_colors: bool,
    /// Позиции из файла (`\pos`: надписи у предметов в кадре).
    pub file_positions: bool,
    /// Непрозрачность картинок (PGS, VobSub).
    pub bitmap_opacity: f32,
}

impl Default for SubtitleStyle {
    fn default() -> Self {
        Self {
            font_family: "Golos Text".into(),
            font_weight: 600,
            size: 0.055,
            min_size_px: 15.0,
            color: [255, 255, 255],
            opacity: 1.0,
            outline: 0.07,
            outline_color: [0, 0, 0],
            outline_softness: 0.015,
            shadow: 0.05,
            shadow_blur: 0.08,
            shadow_color: [0, 0, 0],
            shadow_opacity: 0.55,
            box_color: [0, 0, 0],
            box_opacity: 0.0,
            line_spacing: 1.12,
            letter_spacing: 0.0,
            max_width: 0.86,
            margin: 0.055,
            in_frame: true,
            anim_in: SubtitleAnimation::Rise,
            anim_in_ms: 180,
            anim_out: SubtitleAnimation::Fade,
            anim_out_ms: 160,
            file_colors: true,
            file_positions: true,
            bitmap_opacity: 1.0,
        }
    }
}

impl SubtitleStyle {
    /// Отпечаток того, что влияет на картинку (не на положение и анимацию).
    pub fn raster_key(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.font_family.hash(&mut h);
        self.font_weight.hash(&mut h);
        for v in [
            self.opacity,
            self.outline,
            self.outline_softness,
            self.shadow,
            self.shadow_blur,
            self.shadow_opacity,
            self.box_opacity,
            self.line_spacing,
            self.letter_spacing,
        ] {
            v.to_bits().hash(&mut h);
        }
        (self.color, self.outline_color, self.shadow_color, self.box_color, self.file_colors).hash(&mut h);
        h.finish()
    }
}

/// Готовая картинка: RGBA8 (альфа не умножена), и где в ней текст.
#[derive(Clone, Debug)]
pub struct Rendered {
    pub w: u32,
    pub h: u32,
    pub rgba: Arc<[u8]>,
    /// Прямоугольник строк (без контура, тени и плашки) в пикселях картинки:
    /// x, y, ширина, высота — по нему картинка ставится на место.
    pub text: [f32; 4],
}

// ─── шрифты ─────────────────────────────────────────────────────────────────

struct Face {
    data: Arc<Vec<u8>>,
    index: u32,
    /// Пределы оси насыщенности переменного шрифта.
    wght: Option<(f32, f32)>,
    italic: bool,
    weight: u16,
}

impl Face {
    fn load(data: Vec<u8>, index: u32) -> Option<Self> {
        // fontconfig кладёт номер именованного экземпляра переменного
        // шрифта в старшие 16 бит («SemiBold» = 6 << 16): насыщенность мы
        // ставим осью сами, грань — младшие биты.
        let index = index & 0xFFFF;
        let font = FontRef::from_index(&data, index as usize)?;
        let wght = font
            .variations()
            .find(|v| v.tag() == swash::tag_from_bytes(b"wght"))
            .map(|v| (v.min_value(), v.max_value()));
        let attrs = font.attributes();
        let italic = attrs.style() != swash::Style::Normal;
        let weight = attrs.weight().0;
        Some(Self { data: Arc::new(data), index, wght, italic, weight })
    }

    fn font(&self) -> FontRef<'_> {
        FontRef::from_index(&self.data, self.index as usize).expect("шрифт проверен при загрузке")
    }

    fn has(&self, ch: char) -> bool {
        self.font().charmap().map(ch) != 0
    }

    /// Значение оси `wght` под нужную насыщенность (у переменного шрифта).
    fn wght_for(&self, weight: u16) -> Option<f32> {
        self.wght.map(|(lo, hi)| (weight as f32).clamp(lo, hi))
    }
}

/// Шрифты по (семейство, насыщенность, курсив) и запасные.
#[derive(Default)]
struct Fonts {
    faces: Vec<Arc<Face>>,
    by_key: HashMap<(String, u16, bool), usize>,
    /// Запасные: системный без засечек, DejaVu, CJK по требованию.
    fallbacks: Option<Vec<usize>>,
    cjk: HashMap<u8, Option<usize>>,
}

/// Начертание, которым рисуется кусок.
#[derive(Clone, Copy)]
struct Pick {
    face: usize,
    weight: u16,
    /// Наклонить (в шрифте нет курсива).
    synth_italic: bool,
    /// Утолщить (в шрифте нет нужной насыщенности).
    synth_bold: bool,
}

impl Fonts {
    fn push(&mut self, f: Face) -> usize {
        self.faces.push(Arc::new(f));
        self.faces.len() - 1
    }

    #[cfg(not(target_os = "android"))]
    fn system(family: Option<&str>, weight: u16, italic: bool) -> Option<(Vec<u8>, u32)> {
        use crate::text::font_discovery as fd;
        match family {
            Some(f) => fd::discover_styled(f, weight, italic),
            None => Some(fd::discover_weight_font(None, weight)).filter(|(d, _)| !d.is_empty()),
        }
    }

    #[cfg(target_os = "android")]
    fn system(_family: Option<&str>, _weight: u16, _italic: bool) -> Option<(Vec<u8>, u32)> {
        None
    }

    /// Основное начертание семейства; нет такого семейства — системное.
    fn primary(&mut self, family: &str, weight: u16, italic: bool) -> Option<usize> {
        let key = (family.to_string(), weight, italic);
        if let Some(&i) = self.by_key.get(&key) {
            return Some(i);
        }
        let loaded = Self::system(Some(family), weight, italic)
            .or_else(|| Self::system(Some(family), weight, false))
            .or_else(|| Self::system(None, weight, false))
            .and_then(|(d, i)| Face::load(d, i));
        let idx = match loaded {
            Some(f) => {
                // тот же файл уже загружен — не держать вторую копию
                match self.faces.iter().position(|g| g.index == f.index && g.data.len() == f.data.len() && *g.data == *f.data) {
                    Some(i) => i,
                    None => self.push(f),
                }
            }
            None => return None,
        };
        self.by_key.insert(key, idx);
        Some(idx)
    }

    fn fallbacks(&mut self) -> Vec<usize> {
        if let Some(f) = &self.fallbacks {
            return f.clone();
        }
        let mut v = Vec::new();
        for fam in ["Noto Sans", "DejaVu Sans", "Noto Sans Symbols 2", "Noto Sans Symbols"] {
            if let Some(f) = Self::system(Some(fam), 400, false).and_then(|(d, i)| Face::load(d, i)) {
                v.push(self.push(f));
            }
        }
        self.fallbacks = Some(v.clone());
        v
    }

    /// Шрифт для иероглифов, каны, хангыля.
    #[cfg(not(target_os = "android"))]
    fn cjk(&mut self, ch: char) -> Option<usize> {
        let script = crate::text::script_of(ch)?;
        let k = script as u8;
        if let Some(v) = self.cjk.get(&k) {
            return *v;
        }
        let v = crate::text::font_discovery::discover_fallback_font(script, false, false)
            .and_then(|(d, i)| Face::load(d, i))
            .map(|f| self.push(f));
        self.cjk.insert(k, v);
        v
    }

    #[cfg(target_os = "android")]
    fn cjk(&mut self, _ch: char) -> Option<usize> {
        None
    }

    /// Начертание для куска `span` при оформлении `style`.
    fn pick(&mut self, style: &SubtitleStyle, span: &Span) -> Option<Pick> {
        let weight = if span.bold { style.font_weight.saturating_add(250).clamp(700, 900) } else { style.font_weight };
        let face = self.primary(&style.font_family, weight, span.italic)?;
        let f = &self.faces[face];
        let synth_italic = span.italic && !f.italic;
        let synth_bold = f.wght.is_none() && weight >= 600 && (f.weight as i32) < weight as i32 - 150;
        Some(Pick { face, weight, synth_italic, synth_bold })
    }

    /// Чем рисовать `ch`: основным, иначе первым запасным, где он есть.
    fn face_for(&mut self, primary: usize, ch: char) -> usize {
        if ch.is_whitespace() || ch.is_control() || self.faces[primary].has(ch) {
            return primary;
        }
        if let Some(i) = self.cjk(ch).filter(|&i| self.faces[i].has(ch)) {
            return i;
        }
        for i in self.fallbacks() {
            if self.faces[i].has(ch) {
                return i;
            }
        }
        primary
    }
}

// ─── раскладка ──────────────────────────────────────────────────────────────

struct Glyph {
    face: usize,
    id: u16,
    /// Положение от начала куска, px.
    x: f32,
    y: f32,
}

/// Слово с пробелом после него — единица переноса.
struct Piece {
    glyphs: Vec<Glyph>,
    /// Ширина без пробелов в конце.
    width: f32,
    /// Ширина вместе с пробелами в конце.
    advance: f32,
    pick: Pick,
    span: usize,
}

struct Line {
    pieces: Vec<usize>,
    width: f32,
}

/// Отрисовщик (шрифты кэшируются, общий на процесс — см. [`with_renderer`]).
pub struct SubtitleRenderer {
    fonts: Fonts,
    scale: ScaleContext,
}

static RENDERER: OnceLock<Mutex<SubtitleRenderer>> = OnceLock::new();

/// Общий отрисовщик процесса: шрифты грузятся один раз.
pub fn with_renderer<R>(f: impl FnOnce(&mut SubtitleRenderer) -> R) -> R {
    let m = RENDERER.get_or_init(|| Mutex::new(SubtitleRenderer { fonts: Fonts::default(), scale: ScaleContext::new() }));
    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}

/// Нарисовать текст (например, образец в настройках) тем же путём, что и
/// субтитры: строки через `\n`, разметка ASS разрешена.
pub fn render_sample(text: &str, style: &SubtitleStyle, font_px: f32, max_width_px: f32) -> Option<Rendered> {
    let header = super::ass::AssHeader::default();
    let t = super::ass::parse_text(&text.replace('\n', "\\N"), "Default", &header, 0)?;
    with_renderer(|r| r.render(&t, style, font_px, max_width_px))
}

/// Разбить кусок текста на слова с пробелами после них; иероглифы — по
/// одному (переносятся где угодно).
fn words(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = false;
    for (i, c) in text.char_indices() {
        let cjk = crate::text::script_of(c).is_some();
        if (in_space && !c.is_whitespace()) || cjk {
            if i > start {
                out.push(&text[start..i]);
            }
            start = i;
            in_space = false;
        }
        if c.is_whitespace() && c != '\u{a0}' {
            in_space = true;
        }
        if cjk {
            let end = i + c.len_utf8();
            out.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// Жадный перенос по ширине `w`: строки как номера кусков.
fn greedy(pieces: &[Piece], range: std::ops::Range<usize>, w: f32) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut cur = Line { pieces: Vec::new(), width: 0.0 };
    let mut pen = 0.0f32;
    for i in range {
        let p = &pieces[i];
        if !cur.pieces.is_empty() && pen + p.width > w {
            lines.push(std::mem::replace(&mut cur, Line { pieces: Vec::new(), width: 0.0 }));
            pen = 0.0;
        }
        cur.pieces.push(i);
        cur.width = pen + p.width;
        pen += p.advance;
    }
    if !cur.pieces.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Перенос с выравниванием длин строк: самая узкая ширина, при которой
/// строк столько же, сколько у жадного переноса по `max`. Две строки
/// примерно равной длины читаются легче, чем длинная и огрызок.
fn balanced(pieces: &[Piece], range: std::ops::Range<usize>, max: f32) -> Vec<Line> {
    let first = greedy(pieces, range.clone(), max);
    if first.len() < 2 {
        return first;
    }
    let n = first.len();
    let (mut lo, mut hi) = (range.clone().map(|i| pieces[i].width).fold(0.0f32, f32::max), max);
    for _ in 0..18 {
        let mid = (lo + hi) / 2.0;
        if greedy(pieces, range.clone(), mid).len() <= n {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    greedy(pieces, range, hi + 0.01)
}

// ─── растр ──────────────────────────────────────────────────────────────────

/// Расстояние до ближайшей точки маски (Felzenszwalb–Huttenlocher), в квадрате.
fn edt_1d(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    let para = |q: usize, p: usize| ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        let mut s = para(q, v[k]);
        while s <= z[k] {
            k -= 1;
            s = para(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let dq = q as f32 - v[k] as f32;
        *out = dq * dq + f[v[k]];
    }
}

/// Евклидово расстояние от каждого пикселя до ближайшего пикселя, где
/// `inside` истинно.
fn distance_field(inside: &[bool], w: usize, h: usize) -> Vec<f32> {
    const INF: f32 = 1e20;
    let mut grid: Vec<f32> = inside.iter().map(|&b| if b { 0.0 } else { INF }).collect();
    let n = w.max(h);
    let (mut f, mut d, mut v, mut z) = (vec![0.0f32; n], vec![0.0f32; n], vec![0usize; n], vec![0.0f32; n + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        edt_1d(&f[..h], &mut d[..h], &mut v[..h], &mut z[..h + 1]);
        for y in 0..h {
            grid[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&grid[y * w..y * w + w]);
        edt_1d(&f[..w], &mut d[..w], &mut v[..w], &mut z[..w + 1]);
        for x in 0..w {
            grid[y * w + x] = d[x].sqrt();
        }
    }
    grid
}

/// Гауссово размытие альфы: малые σ — честным ядром, большие — тремя
/// проходами скользящего среднего (время не зависит от радиуса).
fn blur(a: &mut [f32], w: usize, h: usize, sigma: f32) {
    if sigma < 0.3 {
        return;
    }
    let mut tmp = vec![0.0f32; a.len()];
    if sigma < 2.0 {
        let r = (sigma * 3.0).ceil() as i32;
        let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
        let sum: f32 = k.iter().sum();
        let k: Vec<f32> = k.iter().map(|v| v / sum).collect();
        for y in 0..h {
            for x in 0..w {
                let mut s = 0.0;
                for (j, kv) in k.iter().enumerate() {
                    let xx = (x as i32 + j as i32 - r).clamp(0, w as i32 - 1) as usize;
                    s += a[y * w + xx] * kv;
                }
                tmp[y * w + x] = s;
            }
        }
        for y in 0..h {
            for x in 0..w {
                let mut s = 0.0;
                for (j, kv) in k.iter().enumerate() {
                    let yy = (y as i32 + j as i32 - r).clamp(0, h as i32 - 1) as usize;
                    s += tmp[yy * w + x] * kv;
                }
                a[y * w + x] = s;
            }
        }
        return;
    }
    // Размеры трёх ящиков под σ (W. Jarosz / I. Kutskir).
    let n = 3.0f32;
    let wl = ((12.0 * sigma * sigma / n + 1.0).sqrt()).floor() as i32;
    let wl = if wl % 2 == 0 { wl - 1 } else { wl };
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0)).round() as i32;
    for pass in 0..3 {
        let r = ((if pass < m { wl } else { wu }) - 1) / 2;
        box_pass(a, &mut tmp, w, h, r, true);
        box_pass(&tmp, a, w, h, r, false);
    }
}

fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: i32, horizontal: bool) {
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    let at = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut acc = 0.0f32;
        for i in -r..=r {
            acc += src[at(line, i.clamp(0, len as i32 - 1) as usize)];
        }
        for i in 0..len {
            dst[at(line, i)] = acc * norm;
            let add = (i as i32 + r + 1).min(len as i32 - 1) as usize;
            let sub = (i as i32 - r).max(0) as usize;
            acc += src[at(line, add)] - src[at(line, sub)];
        }
    }
}

/// Покрытие скруглённого прямоугольника (сглаженное) — в `a` по максимуму.
fn fill_round_rect(a: &mut [f32], w: usize, h: usize, rect: [f32; 4], radius: f32) {
    let [rx, ry, rw, rh] = rect;
    let (cx, cy) = (rx + rw / 2.0, ry + rh / 2.0);
    let (hx, hy) = (rw / 2.0, rh / 2.0);
    let r = radius.min(hx).min(hy);
    let x0 = (rx - 1.0).floor().max(0.0) as usize;
    let y0 = (ry - 1.0).floor().max(0.0) as usize;
    let x1 = ((rx + rw + 1.0).ceil() as usize).min(w);
    let y1 = ((ry + rh + 1.0).ceil() as usize).min(h);
    for y in y0..y1 {
        for x in x0..x1 {
            let px = (x as f32 + 0.5 - cx).abs() - (hx - r);
            let py = (y as f32 + 0.5 - cy).abs() - (hy - r);
            let outside = (px.max(0.0).powi(2) + py.max(0.0).powi(2)).sqrt() + px.max(py).min(0.0) - r;
            let cov = (0.5 - outside).clamp(0.0, 1.0);
            let o = &mut a[y * w + x];
            *o = o.max(cov);
        }
    }
}

impl SubtitleRenderer {
    /// Нарисовать `text` кеглем `font_px` (физические px) в ширину не
    /// больше `max_width_px`. `None` — рисовать нечего или нет шрифта.
    pub fn render(&mut self, text: &SubText, style: &SubtitleStyle, font_px: f32, max_width_px: f32) -> Option<Rendered> {
        let font_px = font_px.max(4.0);
        let spans: Vec<&Span> = text.lines.iter().flatten().collect();
        if spans.is_empty() {
            return None;
        }
        // Метрики строки — по основному начертанию.
        let base = self.fonts.primary(&style.font_family, style.font_weight, false)?;
        let (ascent, descent) = {
            let f = self.fonts.faces[base].font();
            let m = f.metrics(&[]).scale(font_px);
            (m.ascent, m.descent.abs())
        };
        let line_h = (ascent + descent) * style.line_spacing.max(0.8);
        let spacing = style.letter_spacing * font_px;

        // Куски по словам, по строкам; номер первого куска каждой строки.
        let mut pieces: Vec<Piece> = Vec::new();
        let mut line_ranges: Vec<std::ops::Range<usize>> = Vec::new();
        let mut span_no = 0usize;
        for line in &text.lines {
            let first = pieces.len();
            for span in line {
                let pick = self.fonts.pick(style, span)?;
                for word in words(&span.text) {
                    pieces.push(self.shape(word, pick, font_px, spacing, span_no));
                }
                span_no += 1;
            }
            line_ranges.push(first..pieces.len());
        }

        // Перенос: каждая строка файла — отдельно, по ширине `max_width_px`.
        let mut lines: Vec<Line> = Vec::new();
        for r in line_ranges {
            lines.extend(balanced(&pieces, r, max_width_px.max(font_px * 2.0)));
        }
        if lines.is_empty() {
            return None;
        }
        let block_w = lines.iter().map(|l| l.width).fold(0.0f32, f32::max).ceil();
        let block_h = (line_h * (lines.len() - 1) as f32 + ascent + descent).ceil();

        // Поля под контур, плашку и тень.
        let outline_px = style.outline.max(0.0) * font_px;
        let soft_px = style.outline_softness.max(0.0) * font_px;
        let box_on = style.box_opacity > 0.003;
        let box_pad = if box_on { (0.28 * font_px, 0.06 * font_px) } else { (0.0, 0.0) };
        let shadow_on = style.shadow_opacity > 0.003;
        let shadow_off = if shadow_on { style.shadow.max(0.0) * font_px } else { 0.0 };
        let shadow_sigma = if shadow_on { style.shadow_blur.max(0.0) * font_px / 2.0 } else { 0.0 };
        let pad = (outline_px.max(box_pad.0) + soft_px * 2.0 + shadow_off + shadow_sigma * 3.0 + 3.0).ceil();
        let w = (block_w + 2.0 * pad) as usize;
        let h = (block_h + 2.0 * pad) as usize;
        if w == 0 || h == 0 || w * h > 40_000_000 {
            return None;
        }

        // Заливка: цвет (с умноженной альфой) и покрытие.
        let mut fill_a = vec![0.0f32; w * h];
        let mut fill_c = vec![[0.0f32; 3]; w * h];
        let mut boxes = vec![0.0f32; w * h];
        let h_frac = match text.align {
            1 | 4 | 7 => 0.0,
            3 | 6 | 9 => 1.0,
            _ => 0.5,
        };
        let colors: Vec<[f32; 3]> = text
            .lines
            .iter()
            .flatten()
            .map(|s| {
                let c = if style.file_colors { s.color.unwrap_or(style.color) } else { style.color };
                [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
            })
            .collect();
        let decorations: Vec<(bool, bool)> = text.lines.iter().flatten().map(|s| (s.underline, s.strike)).collect();
        for (li, line) in lines.iter().enumerate() {
            let x0 = pad + (block_w - line.width) * h_frac;
            let baseline = pad + ascent + line_h * li as f32;
            if box_on {
                let top = baseline - ascent - box_pad.1 - (line_h - ascent - descent).max(0.0) / 2.0;
                let bh = line_h.max(ascent + descent) + 2.0 * box_pad.1;
                fill_round_rect(&mut boxes, w, h, [x0 - box_pad.0, top, line.width + 2.0 * box_pad.0, bh], font_px * 0.22);
            }
            let mut pen = x0;
            for (k, &pi) in line.pieces.iter().enumerate() {
                let p = &pieces[pi];
                let col = colors[p.span];
                for g in &p.glyphs {
                    self.draw_glyph(g, p.pick, font_px, pen + g.x, baseline + g.y, w, h, &mut fill_a, &mut fill_c, col);
                }
                let (under, strike) = decorations[p.span];
                let last = k + 1 == line.pieces.len();
                let span_w = if last { p.width } else { p.advance };
                for (on, y) in [(under, baseline + 0.12 * font_px), (strike, baseline - 0.3 * font_px)] {
                    if on {
                        let t = (0.065 * font_px).max(1.0);
                        let r = [pen, y - t / 2.0, span_w, t];
                        for yy in (r[1].floor().max(0.0) as usize)..((r[1] + r[3]).ceil() as usize).min(h) {
                            for xx in (r[0].floor().max(0.0) as usize)..((r[0] + r[2]).ceil() as usize).min(w) {
                                let o = yy * w + xx;
                                fill_c[o] = col;
                                fill_a[o] = 1.0;
                            }
                        }
                    }
                }
                pen += p.advance;
            }
        }

        // Контур: расстояние до глифов (маска по середине покрытия).
        let mut outline_a = vec![0.0f32; w * h];
        if outline_px > 0.05 {
            let inside: Vec<bool> = fill_a.iter().map(|&a| a >= 0.5).collect();
            let dist = distance_field(&inside, w, h);
            for i in 0..w * h {
                let o = (outline_px + 0.5 - dist[i]).clamp(0.0, 1.0);
                outline_a[i] = o.max(fill_a[i]);
            }
            blur(&mut outline_a, w, h, 0.45 + soft_px);
        }

        // Тень — размытый силуэт всего (текст, контур, плашка), со сдвигом.
        let mut shadow_a = vec![0.0f32; w * h];
        if shadow_on {
            let dx = (shadow_off * 0.45).round() as i32;
            let dy = shadow_off.round() as i32;
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let (sx, sy) = (x - dx, y - dy);
                    if sx < 0 || sy < 0 || sx >= w as i32 || sy >= h as i32 {
                        continue;
                    }
                    let s = (sy as usize) * w + sx as usize;
                    shadow_a[y as usize * w + x as usize] = fill_a[s].max(outline_a[s]).max(boxes[s] * style.box_opacity.min(1.0));
                }
            }
            blur(&mut shadow_a, w, h, shadow_sigma);
        }

        // Сведение слоёв (альфа умножена): тень → плашка → контур → текст.
        let col = |c: [u8; 3]| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
        let (sc, bc, oc) = (col(style.shadow_color), col(style.box_color), col(style.outline_color));
        let opacity = style.opacity.clamp(0.0, 1.0);
        // Цвет «пустых» пикселей — чтобы при сглаживании текстуры по краю
        // не вылезала тёмная кайма.
        let edge = if outline_px > 0.05 { oc } else if box_on { bc } else { sc };
        let mut rgba = vec![0u8; w * h * 4];
        for i in 0..w * h {
            let mut c = [0.0f32; 3];
            let mut a = 0.0f32;
            let mut over = |lc: [f32; 3], la: f32| {
                if la <= 0.0 {
                    return;
                }
                for k in 0..3 {
                    c[k] = lc[k] * la + c[k] * (1.0 - la);
                }
                a = la + a * (1.0 - la);
            };
            over(sc, shadow_a[i] * style.shadow_opacity);
            over(bc, boxes[i] * style.box_opacity);
            over(oc, outline_a[i] * opacity);
            if fill_a[i] > 0.0 {
                let fa = fill_a[i];
                let fc = [fill_c[i][0] / fa, fill_c[i][1] / fa, fill_c[i][2] / fa];
                over(fc, fa * opacity);
            }
            let o = i * 4;
            let out = if a > 0.002 { [c[0] / a, c[1] / a, c[2] / a] } else { edge };
            rgba[o] = (out[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            rgba[o + 1] = (out[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            rgba[o + 2] = (out[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            rgba[o + 3] = (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
        Some(Rendered { w: w as u32, h: h as u32, rgba: rgba.into(), text: [pad, pad, block_w, block_h] })
    }

    /// Слово → глифы (с запасными шрифтами для отсутствующих символов).
    fn shape(&mut self, word: &str, pick: Pick, font_px: f32, spacing: f32, span: usize) -> Piece {
        // Кусками по шрифту.
        let mut runs: Vec<(usize, String)> = Vec::new();
        for ch in word.chars() {
            let f = self.fonts.face_for(pick.face, ch);
            match runs.last_mut() {
                Some((rf, s)) if *rf == f => s.push(ch),
                _ => runs.push((f, ch.to_string())),
            }
        }
        let mut glyphs = Vec::new();
        let mut pen = 0.0f32;
        let mut trailing_space = 0.0f32;
        for (fi, s) in runs {
            let face = self.fonts.faces[fi].clone();
            let Some(mut rb) = rustybuzz::Face::from_slice(&face.data, face.index) else { continue };
            if let Some(v) = face.wght_for(pick.weight) {
                rb.set_variations(&[rustybuzz::Variation { tag: rustybuzz::ttf_parser::Tag::from_bytes(b"wght"), value: v }]);
            }
            let upem = rb.units_per_em().max(1) as f32;
            let k = font_px / upem;
            let mut buf = rustybuzz::UnicodeBuffer::new();
            buf.push_str(&s);
            buf.guess_segment_properties();
            let out = rustybuzz::shape(&rb, &[], buf);
            let chars: Vec<(usize, char)> = s.char_indices().collect();
            for (info, pos) in out.glyph_infos().iter().zip(out.glyph_positions()) {
                let ch = chars.iter().find(|(i, _)| *i == info.cluster as usize).map(|(_, c)| *c).unwrap_or(' ');
                let adv = pos.x_advance as f32 * k + spacing;
                if ch.is_whitespace() && ch != '\u{a0}' {
                    trailing_space += adv;
                } else {
                    trailing_space = 0.0;
                    glyphs.push(Glyph { face: fi, id: info.glyph_id as u16, x: pen + pos.x_offset as f32 * k, y: -(pos.y_offset as f32) * k });
                }
                pen += adv;
            }
        }
        // Утолщение и наклон расширяют глифы — немного места справа.
        let extra = if pick.synth_bold { font_px * 0.02 } else { 0.0 };
        Piece { glyphs, width: (pen - trailing_space).max(0.0) + extra, advance: pen + extra, pick, span }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_glyph(
        &mut self,
        g: &Glyph,
        pick: Pick,
        font_px: f32,
        x: f32,
        baseline: f32,
        w: usize,
        h: usize,
        fill_a: &mut [f32],
        fill_c: &mut [[f32; 3]],
        col: [f32; 3],
    ) {
        let face = self.fonts.faces[g.face].clone();
        let font = face.font();
        let mut b = self.scale.builder(font).size(font_px).hint(false);
        if let Some(v) = face.wght_for(pick.weight) {
            b = b.variations([("wght", v)]);
        }
        let mut scaler = b.build();
        let (ix, iy) = (x.floor(), baseline.floor());
        let mut r = Render::new(&[Source::Outline]);
        r.format(Format::Alpha).offset(Vector::new(x - ix, -(baseline - iy)));
        // Запасной шрифт не утолщаем и не наклоняем: у него своё начертание.
        if g.face == pick.face {
            if pick.synth_bold {
                r.embolden(font_px * 0.025);
            }
            if pick.synth_italic {
                r.transform(Some(Transform::skew(Angle::from_degrees(12.0), Angle::ZERO)));
            }
        }
        let Some(img) = r.render(&mut scaler, g.id) else { return };
        let pl = img.placement;
        if pl.width == 0 || pl.height == 0 {
            return;
        }
        let gx = ix as i32 + pl.left;
        let gy = iy as i32 - pl.top;
        for yy in 0..pl.height as i32 {
            let py = gy + yy;
            if py < 0 || py >= h as i32 {
                continue;
            }
            for xx in 0..pl.width as i32 {
                let px = gx + xx;
                if px < 0 || px >= w as i32 {
                    continue;
                }
                let a = img.data[(yy * pl.width as i32 + xx) as usize] as f32 / 255.0;
                if a <= 0.0 {
                    continue;
                }
                let o = py as usize * w + px as usize;
                // «поверх»: перекрытия соседних глифов не темнеют и не светлеют
                for k in 0..3 {
                    fill_c[o][k] = col[k] * a + fill_c[o][k] * (1.0 - a);
                }
                fill_a[o] = a + fill_a[o] * (1.0 - a);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces_of(widths: &[f32]) -> Vec<Piece> {
        widths
            .iter()
            .map(|&w| Piece {
                glyphs: Vec::new(),
                width: w,
                advance: w + 5.0,
                pick: Pick { face: 0, weight: 400, synth_italic: false, synth_bold: false },
                span: 0,
            })
            .collect()
    }

    #[test]
    fn balanced_wrap_evens_lines() {
        // жадно: 4 слова + огрызок; выровнено — 3 и 2
        let p = pieces_of(&[40.0, 40.0, 40.0, 40.0, 40.0]);
        let g = greedy(&p, 0..5, 190.0);
        assert_eq!(g.iter().map(|l| l.pieces.len()).collect::<Vec<_>>(), vec![4, 1]);
        let b = balanced(&p, 0..5, 190.0);
        assert_eq!(b.iter().map(|l| l.pieces.len()).collect::<Vec<_>>(), vec![3, 2]);
    }

    #[test]
    fn distance_field_is_euclidean() {
        let (w, h) = (9, 9);
        let mut inside = vec![false; w * h];
        inside[4 * w + 4] = true;
        let d = distance_field(&inside, w, h);
        assert_eq!(d[4 * w + 4], 0.0);
        assert!((d[4 * w + 7] - 3.0).abs() < 1e-4);
        assert!((d[7 * w + 8] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn blur_keeps_mass() {
        let (w, h) = (64, 64);
        for sigma in [1.0f32, 5.0] {
            let mut a = vec![0.0f32; w * h];
            a[32 * w + 32] = 1.0;
            blur(&mut a, w, h, sigma);
            let sum: f32 = a.iter().sum();
            assert!((sum - 1.0).abs() < 0.02, "{sigma}: {sum}");
            assert!(a[32 * w + 32] < 0.5);
        }
    }

    #[test]
    fn words_split_after_spaces() {
        assert_eq!(words("Привет,  мир!"), vec!["Привет,  ", "мир!"]);
        assert_eq!(words("日本語 ok"), vec!["日", "本", "語", " ", "ok"]);
    }

    #[test]
    fn renders_text_with_outline_and_shadow() {
        let style = SubtitleStyle::default();
        let Some(r) = render_sample("Проверка\nсубтитров", &style, 48.0, 2000.0) else {
            // нет ни одного шрифта в системе — нечего проверять
            return;
        };
        assert!(r.w > 100 && r.h > 80, "{}×{}", r.w, r.h);
        // где-то текст белый, где-то контур чёрный, края прозрачные
        let px = |i: usize| (r.rgba[i * 4], r.rgba[i * 4 + 3]);
        let n = (r.w * r.h) as usize;
        assert!((0..n).any(|i| px(i).0 > 240 && px(i).1 > 240));
        assert!((0..n).any(|i| px(i).0 < 20 && px(i).1 > 240));
        assert_eq!(px(0).1, 0);
        // перенос: узкая ширина — больше строк, картинка выше
        let narrow = render_sample("Очень длинная строка субтитров для переноса", &style, 48.0, 300.0).unwrap();
        let wide = render_sample("Очень длинная строка субтитров для переноса", &style, 48.0, 3000.0).unwrap();
        assert!(narrow.h > wide.h && narrow.w < wide.w);
    }
}

/// Образец в файл для проверки глазами: `SUBS_DUMP=путь.rgba cargo test …
/// dump_sample -- --ignored` (рядом пишется `.txt` с размером).
#[cfg(test)]
#[test]
#[ignore]
fn dump_sample() {
    let Ok(path) = std::env::var("SUBS_DUMP") else { return };
    let mut style = SubtitleStyle::default();
    if let Ok(f) = std::env::var("SUBS_FONT") {
        style.font_family = f;
    }
    if std::env::var("SUBS_BOX").is_ok() {
        style.box_opacity = 0.6;
        style.outline = 0.0;
        style.shadow_opacity = 0.0;
    }
    let r = render_sample("Субтитры: {\\i1}курсив{\\i0}, {\\b1}жирный{\\b0},\n{\\c&H00FFFF&}жёлтый{\\c} и обычный текст — ёЁ 123 ♪", &style, 64.0, 1400.0).unwrap();
    std::fs::write(&path, &*r.rgba).unwrap();
    std::fs::write(format!("{path}.txt"), format!("{} {}", r.w, r.h)).unwrap();
}


