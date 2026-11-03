//! Растеризатор с точным покрытием (накопление знаковой площади, как в
//! font-rs), краски (цвет, линейный/радиальный градиент, картинка) и
//! композитинг слоёв в премультиплицированном f32-RGBA (sRGB, как в lottie-web).

use std::sync::Arc;

use super::geom::{Mat, Polyline};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

/// Покрытие 0..1 над прямоугольником `x0,y0 .. x0+w,y0+h` в пикселях холста.
#[derive(Clone, Debug, Default)]
pub struct Mask {
    pub x0: i32,
    pub y0: i32,
    pub w: usize,
    pub h: usize,
    pub cov: Vec<f32>,
}

impl Mask {
    pub fn full(w: usize, h: usize) -> Mask {
        Mask {
            x0: 0,
            y0: 0,
            w,
            h,
            cov: vec![1.0; w * h],
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> f32 {
        let lx = x - self.x0;
        let ly = y - self.y0;
        if lx < 0 || ly < 0 || lx as usize >= self.w || ly as usize >= self.h {
            0.0
        } else {
            self.cov[ly as usize * self.w + lx as usize]
        }
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// Строка `y` от `x` длиной `w`; вызывающий гарантирует, что отрезок
    /// лежит внутри охвата маски.
    #[inline]
    pub fn row(&self, y: i32, x: i32, w: usize) -> &[f32] {
        let start = (y - self.y0) as usize * self.w + (x - self.x0) as usize;
        &self.cov[start..start + w]
    }

    /// Пересечение строки `y` с отрезком `[x0, x1)`: `(начало, срез)` или `None`.
    #[inline]
    pub fn row_span(&self, y: i32, x0: i32, x1: i32) -> Option<(i32, &[f32])> {
        if y < self.y0 || y >= self.y0 + self.h as i32 {
            return None;
        }
        let a = x0.max(self.x0);
        let b = x1.min(self.x0 + self.w as i32);
        if b <= a {
            return None;
        }
        Some((a, self.row(y, a, (b - a) as usize)))
    }

    /// Пересечение с другим покрытием (умножение).
    pub fn intersect(&mut self, other: &Mask) {
        for ly in 0..self.h {
            for lx in 0..self.w {
                let v = other.get(self.x0 + lx as i32, self.y0 + ly as i32);
                self.cov[ly * self.w + lx] *= v;
            }
        }
    }

    /// Покрывает ли маска весь прямоугольник `w×h` полностью (единицами).
    pub fn covers_all(&self, w: usize, h: usize) -> bool {
        if self.x0 > 0 || self.y0 > 0 || (self.x0 as i64 + self.w as i64) < w as i64 || (self.y0 as i64 + self.h as i64) < h as i64 {
            return false;
        }
        for y in 0..h {
            for x in 0..w {
                if self.get(x as i32, y as i32) < 0.999 {
                    return false;
                }
            }
        }
        true
    }
}

/// Растеризатор: переиспользует буфер накопления и буферы покрытия между
/// вызовами (свежие крупные выделения стоят page fault'ов на каждый кадр).
#[derive(Default)]
pub struct Rasterizer {
    acc: Vec<f32>,
    spare: Vec<Vec<f32>>,
}

impl Rasterizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Буфер из пула длиной `n`, заполненный `fill`.
    fn alloc(&mut self, n: usize, fill: f32) -> Vec<f32> {
        // Самый вместительный — чтобы реже перевыделять.
        let best = (0..self.spare.len()).max_by_key(|&i| self.spare[i].capacity());
        let mut v = match best {
            Some(i) => self.spare.swap_remove(i),
            None => Vec::new(),
        };
        v.clear();
        v.resize(n, fill);
        v
    }

    /// Вернуть буфер маски в пул.
    pub fn recycle(&mut self, m: Mask) {
        if m.cov.capacity() > 0 && self.spare.len() < 16 {
            self.spare.push(m.cov);
        }
    }

    /// Маска размером `w×h` в позиции `(x0, y0)`, заполненная `fill`.
    pub fn new_mask(&mut self, x0: i32, y0: i32, w: usize, h: usize, fill: f32) -> Mask {
        let cov = self.alloc(w * h, fill);
        Mask { x0, y0, w, h, cov }
    }

    /// Булева операция над покрытиями (для merge paths): входы уходят в пул.
    /// `None` — пустое покрытие.
    pub fn combine(&mut self, a: Option<Mask>, b: Option<Mask>, op: CoverageOp) -> Option<Mask> {
        let norm = |m: Option<Mask>| m.filter(|m| !m.is_empty());
        let (a, b) = (norm(a), norm(b));
        let (a, b) = match (a, b) {
            (None, None) => return None,
            (None, Some(b)) => {
                return match op {
                    CoverageOp::Union | CoverageOp::Xor => Some(b),
                    CoverageOp::Subtract | CoverageOp::Intersect => {
                        self.recycle(b);
                        None
                    }
                };
            }
            (Some(a), None) => {
                return match op {
                    CoverageOp::Intersect => {
                        self.recycle(a);
                        None
                    }
                    _ => Some(a),
                };
            }
            (Some(a), Some(b)) => (a, b),
        };
        if op == CoverageOp::Intersect {
            let m = self.intersect(&a, &b);
            self.recycle(a);
            self.recycle(b);
            return norm(Some(m));
        }
        // Охват результата: у вычитания — охват `a`, иначе объединение.
        let (x0, y0, x1, y1) = match op {
            CoverageOp::Subtract => (a.x0, a.y0, a.x0 + a.w as i32, a.y0 + a.h as i32),
            _ => (
                a.x0.min(b.x0),
                a.y0.min(b.y0),
                (a.x0 + a.w as i32).max(b.x0 + b.w as i32),
                (a.y0 + a.h as i32).max(b.y0 + b.h as i32),
            ),
        };
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let mut out = self.new_mask(x0, y0, w, h, 0.0);
        for y in 0..h {
            let row = &mut out.cov[y * w..(y + 1) * w];
            for (x, r) in row.iter_mut().enumerate() {
                let (px, py) = (x0 + x as i32, y0 + y as i32);
                let va = a.get(px, py);
                let vb = b.get(px, py);
                *r = match op {
                    CoverageOp::Union => va + vb - va * vb,
                    CoverageOp::Subtract => va * (1.0 - vb),
                    CoverageOp::Xor => va + vb - 2.0 * va * vb,
                    CoverageOp::Intersect => unreachable!(),
                };
            }
        }
        self.recycle(a);
        self.recycle(b);
        Some(out)
    }

    /// Пересечение двух покрытий (произведение) над пересечением охватов.
    pub fn intersect(&mut self, a: &Mask, b: &Mask) -> Mask {
        let x0 = a.x0.max(b.x0);
        let y0 = a.y0.max(b.y0);
        let x1 = (a.x0 as i64 + a.w as i64).min(b.x0 as i64 + b.w as i64);
        let y1 = (a.y0 as i64 + a.h as i64).min(b.y0 as i64 + b.h as i64);
        if x1 <= x0 as i64 || y1 <= y0 as i64 {
            return Mask::default();
        }
        let w = (x1 - x0 as i64) as usize;
        let h = (y1 - y0 as i64) as usize;
        let mut out = self.new_mask(x0, y0, w, h, 0.0);
        for y in 0..h {
            let ra = a.row(y0 + y as i32, x0, w);
            let rb = b.row(y0 + y as i32, x0, w);
            for ((o, va), vb) in out.cov[y * w..(y + 1) * w].iter_mut().zip(ra).zip(rb) {
                *o = va * vb;
            }
        }
        out
    }

    /// Заливка набора замкнутых ломаных в маску покрытия (обрезка по `clip_w×clip_h`).
    pub fn fill(&mut self, polys: &[Polyline], rule: FillRule, clip_w: usize, clip_h: usize) -> Option<Mask> {
        let mut bb: Option<[f32; 4]> = None;
        for p in polys {
            if let Some(b) = p.bbox() {
                bb = Some(match bb {
                    None => b,
                    Some(o) => [o[0].min(b[0]), o[1].min(b[1]), o[2].max(b[2]), o[3].max(b[3])],
                });
            }
        }
        let bb = bb?;
        if !bb.iter().all(|v| v.is_finite()) {
            return None;
        }
        let x0 = (bb[0].floor().max(0.0)) as i64;
        let y0 = (bb[1].floor().max(0.0)) as i64;
        let x1 = (bb[2].ceil()).min(clip_w as f32) as i64;
        let y1 = (bb[3].ceil()).min(clip_h as f32) as i64;
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let bw = (x1 - x0) as usize;
        let bh = (y1 - y0) as usize;
        let stride = bw + 2;
        self.acc.clear();
        self.acc.resize(stride * bh + 2, 0.0);
        let off = [x0 as f32, y0 as f32];
        for poly in polys {
            let n = poly.pts.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                let a = poly.pts[i];
                let b = poly.pts[(i + 1) % n];
                let p0 = [a[0] - off[0], a[1] - off[1]];
                let p1 = [b[0] - off[0], b[1] - off[1]];
                self.line_clipped(p0, p1, bw, bh, stride);
            }
        }
        let mut cov = self.alloc(bw * bh, 0.0);
        for y in 0..bh {
            let mut sum = 0.0f32;
            let row = &self.acc[y * stride..(y + 1) * stride];
            let out = &mut cov[y * bw..(y + 1) * bw];
            for x in 0..bw {
                sum += row[x];
                let c = match rule {
                    FillRule::NonZero => sum.abs().min(1.0),
                    FillRule::EvenOdd => {
                        let a = sum.abs() % 2.0;
                        if a > 1.0 { 2.0 - a } else { a }
                    }
                };
                out[x] = if c < 1.0 / 1024.0 { 0.0 } else { c };
            }
        }
        Some(Mask {
            x0: x0 as i32,
            y0: y0 as i32,
            w: bw,
            h: bh,
            cov,
        })
    }

    /// Отрезок с разрезанием по вертикальным границам x=0 и x=bw и
    /// прижатием x к границам: вклад внешних частей сохраняет сумму строки.
    fn line_clipped(&mut self, p0: [f32; 2], p1: [f32; 2], bw: usize, bh: usize, stride: usize) {
        if (p0[1] - p1[1]).abs() <= 1e-7 {
            return;
        }
        let w = bw as f32;
        let mut pts = [p0, p0, p0, p1];
        let mut n = 1usize;
        let mut ts: [f32; 2] = [2.0, 2.0];
        let dx = p1[0] - p0[0];
        if dx.abs() > 1e-9 {
            for (k, bx) in [0.0f32, w].into_iter().enumerate() {
                let t = (bx - p0[0]) / dx;
                if t > 1e-6 && t < 1.0 - 1e-6 {
                    ts[k] = t;
                }
            }
        }
        if ts[0] > ts[1] {
            ts.swap(0, 1);
        }
        for t in ts {
            if t <= 1.0 {
                pts[n] = [p0[0] + dx * t, p0[1] + (p1[1] - p0[1]) * t];
                n += 1;
            }
        }
        pts[n] = p1;
        for i in 0..n {
            let mut a = pts[i];
            let mut b = pts[i + 1];
            a[0] = a[0].clamp(0.0, w);
            b[0] = b[0].clamp(0.0, w);
            self.accumulate(a, b, bh, stride);
        }
    }

    /// Накопление знаковой площади отрезка (алгоритм font-rs).
    fn accumulate(&mut self, p0: [f32; 2], p1: [f32; 2], h: usize, stride: usize) {
        if (p0[1] - p1[1]).abs() <= 1e-7 {
            return;
        }
        let (dir, p0, p1) = if p0[1] < p1[1] { (1.0f32, p0, p1) } else { (-1.0f32, p1, p0) };
        if p1[1] <= 0.0 || p0[1] >= h as f32 {
            return;
        }
        let dxdy = (p1[0] - p0[0]) / (p1[1] - p0[1]);
        let mut x = p0[0];
        let y0 = p0[1].max(0.0) as usize;
        if p0[1] < 0.0 {
            x -= p0[1] * dxdy;
        }
        let y_end = (p1[1].ceil().max(0.0) as usize).min(h);
        let max_x = (stride - 1) as f32;
        let a = &mut self.acc;
        for y in y0..y_end {
            let linestart = y * stride;
            let dy = ((y + 1) as f32).min(p1[1]) - (y as f32).max(p0[1]);
            let xnext = x + dxdy * dy;
            let d = dy * dir;
            let (x0, x1) = if x < xnext { (x, xnext) } else { (xnext, x) };
            let x0 = x0.clamp(0.0, max_x);
            let x1 = x1.clamp(0.0, max_x);
            let x0floor = x0.floor();
            let x0i = x0floor as usize;
            let x1ceil = x1.ceil();
            let x1i = x1ceil as usize;
            if x1i <= x0i + 1 {
                let xmf = 0.5 * (x0 + x1) - x0floor;
                let i = linestart + x0i;
                if i + 1 < a.len() {
                    a[i] += d - d * xmf;
                    a[i + 1] += d * xmf;
                }
            } else {
                let s = (x1 - x0).recip();
                let x0f = x0 - x0floor;
                let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
                let x1f = x1 - x1ceil + 1.0;
                let am = 0.5 * s * x1f * x1f;
                let base = linestart + x0i;
                if linestart + x1i < a.len() {
                    a[base] += d * a0;
                    if x1i == x0i + 2 {
                        a[base + 1] += d * (1.0 - a0 - am);
                    } else {
                        let a1 = s * (1.5 - x0f);
                        a[base + 1] += d * (a1 - a0);
                        for xi in x0i + 2..x1i - 1 {
                            a[linestart + xi] += d * s;
                        }
                        let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                        a[linestart + x1i - 1] += d * (1.0 - a2 - am);
                    }
                    a[linestart + x1i] += d * am;
                }
            }
            x = xnext;
        }
    }
}

/// Градиентные стопы: цветовые (offset, rgb) и альфа (offset, a) раздельно.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GradientStops {
    pub colors: Vec<(f32, [f32; 3])>,
    pub alphas: Vec<(f32, f32)>,
}

impl GradientStops {
    /// Из плоского массива Lottie: `count` цветов по 4 числа, затем пары (offset, alpha).
    pub fn from_raw(raw: &[f32], count: usize) -> GradientStops {
        let mut colors = Vec::with_capacity(count);
        for i in 0..count {
            let b = i * 4;
            if b + 3 < raw.len() {
                colors.push((raw[b], [raw[b + 1], raw[b + 2], raw[b + 3]]));
            }
        }
        let mut alphas = Vec::new();
        let mut i = count * 4;
        while i + 1 < raw.len() {
            alphas.push((raw[i], raw[i + 1]));
            i += 2;
        }
        GradientStops { colors, alphas }
    }

    #[inline]
    pub fn sample(&self, t: f32) -> [f32; 4] {
        let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
        let rgb = match self.colors.len() {
            0 => [0.0, 0.0, 0.0],
            1 => self.colors[0].1,
            _ => {
                let cs = &self.colors;
                if t <= cs[0].0 {
                    cs[0].1
                } else if t >= cs[cs.len() - 1].0 {
                    cs[cs.len() - 1].1
                } else {
                    let mut i = 1;
                    while i < cs.len() - 1 && cs[i].0 < t {
                        i += 1;
                    }
                    let (o0, c0) = cs[i - 1];
                    let (o1, c1) = cs[i];
                    let k = if o1 > o0 { (t - o0) / (o1 - o0) } else { 1.0 };
                    [c0[0] + (c1[0] - c0[0]) * k, c0[1] + (c1[1] - c0[1]) * k, c0[2] + (c1[2] - c0[2]) * k]
                }
            }
        };
        let a = match self.alphas.len() {
            0 => 1.0,
            1 => self.alphas[0].1,
            _ => {
                let al = &self.alphas;
                if t <= al[0].0 {
                    al[0].1
                } else if t >= al[al.len() - 1].0 {
                    al[al.len() - 1].1
                } else {
                    let mut i = 1;
                    while i < al.len() - 1 && al[i].0 < t {
                        i += 1;
                    }
                    let (o0, a0) = al[i - 1];
                    let (o1, a1) = al[i];
                    let k = if o1 > o0 { (t - o0) / (o1 - o0) } else { 1.0 };
                    a0 + (a1 - a0) * k
                }
            }
        };
        [rgb[0], rgb[1], rgb[2], a]
    }
}

/// Растровая картинка (прямая альфа, sRGB8).
#[derive(Debug)]
pub struct ImageRgba {
    pub width: u32,
    pub height: u32,
    pub data: Arc<[u8]>,
}

impl ImageRgba {
    /// Билинейная выборка в координатах картинки (пиксельные центры в .5), прямая альфа 0..1.
    pub fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0; 4];
        }
        let fx = x - 0.5;
        let fy = y - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let px = |xi: i64, yi: i64| -> [f32; 4] {
            if xi < 0 || yi < 0 || xi >= self.width as i64 || yi >= self.height as i64 {
                return [0.0; 4];
            }
            let i = (yi as usize * self.width as usize + xi as usize) * 4;
            let d = &self.data;
            if i + 3 >= d.len() {
                return [0.0; 4];
            }
            let a = d[i + 3] as f32 / 255.0;
            // Премультиплицируем для корректной интерполяции.
            [d[i] as f32 / 255.0 * a, d[i + 1] as f32 / 255.0 * a, d[i + 2] as f32 / 255.0 * a, a]
        };
        let (xi, yi) = (x0 as i64, y0 as i64);
        let p00 = px(xi, yi);
        let p10 = px(xi + 1, yi);
        let p01 = px(xi, yi + 1);
        let p11 = px(xi + 1, yi + 1);
        let mut out = [0f32; 4];
        for k in 0..4 {
            let top = p00[k] + (p10[k] - p00[k]) * tx;
            let bot = p01[k] + (p11[k] - p01[k]) * tx;
            out[k] = top + (bot - top) * ty;
        }
        out
    }
}

/// Краска: даёт премультиплицированный цвет в точке холста.
#[derive(Clone, Debug)]
pub enum Paint {
    Solid([f32; 4]),
    Linear {
        /// Из координат холста в локальные координаты градиента.
        inv: Mat,
        start: [f32; 2],
        end: [f32; 2],
        stops: GradientStops,
        opacity: f32,
    },
    Radial {
        inv: Mat,
        center: [f32; 2],
        radius: f32,
        focal: [f32; 2],
        stops: GradientStops,
        opacity: f32,
    },
    Image {
        inv: Mat,
        image: Arc<ImageRgba>,
        opacity: f32,
    },
}

impl Paint {
    pub fn solid(rgb: [f32; 3], alpha: f32) -> Paint {
        let a = alpha.clamp(0.0, 1.0);
        Paint::Solid([rgb[0] * a, rgb[1] * a, rgb[2] * a, a])
    }

    /// Премультиплицированный цвет для пикселя с центром `(x, y)`.
    #[inline]
    pub fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        match self {
            Paint::Solid(c) => *c,
            Paint::Linear {
                inv,
                start,
                end,
                stops,
                opacity,
            } => {
                let p = inv.apply([x, y]);
                let d = [end[0] - start[0], end[1] - start[1]];
                let dd = d[0] * d[0] + d[1] * d[1];
                let t = if dd > 0.0 { ((p[0] - start[0]) * d[0] + (p[1] - start[1]) * d[1]) / dd } else { 0.0 };
                premul(stops.sample(t), *opacity)
            }
            Paint::Radial {
                inv,
                center,
                radius,
                focal,
                stops,
                opacity,
            } => {
                let p = inv.apply([x, y]);
                let t = radial_t(p, *center, *radius, *focal);
                premul(stops.sample(t), *opacity)
            }
            Paint::Image { inv, image, opacity } => {
                let p = inv.apply([x, y]);
                let c = image.sample(p[0], p[1]);
                [c[0] * opacity, c[1] * opacity, c[2] * opacity, c[3] * opacity]
            }
        }
    }
}

#[inline]
fn premul(c: [f32; 4], opacity: f32) -> [f32; 4] {
    let a = (c[3] * opacity).clamp(0.0, 1.0);
    [c[0] * a, c[1] * a, c[2] * a, a]
}

/// Параметр двухточечного радиального градиента: фокус `f` (r=0) → окружность `(c, r)`.
#[inline]
fn radial_t(p: [f32; 2], c: [f32; 2], r: f32, f: [f32; 2]) -> f32 {
    if r <= 0.0 {
        return 1.0;
    }
    let cd = [c[0] - f[0], c[1] - f[1]];
    let pd = [p[0] - f[0], p[1] - f[1]];
    let a = cd[0] * cd[0] + cd[1] * cd[1] - r * r;
    let pd2 = pd[0] * pd[0] + pd[1] * pd[1];
    if a.abs() < 1e-6 {
        // Фокус на окружности: вырожденный случай.
        let b = pd[0] * cd[0] + pd[1] * cd[1];
        return if b.abs() > 1e-9 { pd2 / (2.0 * b) } else { 1.0 };
    }
    let b = pd[0] * cd[0] + pd[1] * cd[1];
    let disc = b * b - a * pd2;
    if disc < 0.0 {
        return 1.0;
    }
    let sq = disc.sqrt();
    let t1 = (b - sq) / a;
    let t2 = (b + sq) / a;
    if t1 >= 0.0 { t1.max(t2.min(t1)).max(0.0) } else { t2.max(0.0) }
}

/// Булева операция над масками покрытия (merge paths).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageOp {
    Union,
    Subtract,
    Intersect,
    Xor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Add,
}

impl BlendMode {
    pub fn from_lottie(v: i64) -> BlendMode {
        match v {
            1 => BlendMode::Multiply,
            2 => BlendMode::Screen,
            3 => BlendMode::Overlay,
            4 => BlendMode::Darken,
            5 => BlendMode::Lighten,
            6 => BlendMode::ColorDodge,
            7 => BlendMode::ColorBurn,
            8 => BlendMode::HardLight,
            9 => BlendMode::SoftLight,
            10 => BlendMode::Difference,
            11 => BlendMode::Exclusion,
            16 => BlendMode::Add,
            _ => BlendMode::Normal,
        }
    }
}

#[inline]
fn blend_channel(mode: BlendMode, cb: f32, cs: f32) -> f32 {
    match mode {
        BlendMode::Normal => cs,
        BlendMode::Multiply => cb * cs,
        BlendMode::Screen => cb + cs - cb * cs,
        BlendMode::Overlay => blend_channel(BlendMode::HardLight, cs, cb),
        BlendMode::Darken => cb.min(cs),
        BlendMode::Lighten => cb.max(cs),
        BlendMode::ColorDodge => {
            if cb <= 0.0 {
                0.0
            } else if cs >= 1.0 {
                1.0
            } else {
                (cb / (1.0 - cs)).min(1.0)
            }
        }
        BlendMode::ColorBurn => {
            if cb >= 1.0 {
                1.0
            } else if cs <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - cb) / cs).min(1.0)
            }
        }
        BlendMode::HardLight => {
            if cs <= 0.5 {
                cb * 2.0 * cs
            } else {
                let s = 2.0 * cs - 1.0;
                cb + s - cb * s
            }
        }
        BlendMode::SoftLight => {
            if cs <= 0.5 {
                cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
            } else {
                let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
                cb + (2.0 * cs - 1.0) * (d - cb)
            }
        }
        BlendMode::Difference => (cb - cs).abs(),
        BlendMode::Exclusion => cb + cs - 2.0 * cb * cs,
        BlendMode::Add => (cb + cs).min(1.0),
    }
}

/// Холст: премультиплицированный RGBA f32, с отслеживанием затронутой области.
#[derive(Clone, Debug)]
pub struct Pixmap {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 4]>,
    /// `[x0, y0, x1, y1)` затронутых пикселей; `None` — пусто.
    pub dirty: Option<[usize; 4]>,
}

impl Pixmap {
    pub fn new(w: usize, h: usize) -> Pixmap {
        Pixmap {
            w,
            h,
            px: vec![[0.0; 4]; w * h],
            dirty: None,
        }
    }

    pub fn clear(&mut self) {
        if let Some([x0, y0, x1, y1]) = self.dirty.take() {
            for y in y0..y1 {
                for p in &mut self.px[y * self.w + x0..y * self.w + x1] {
                    *p = [0.0; 4];
                }
            }
        }
    }

    fn mark(&mut self, x0: usize, y0: usize, x1: usize, y1: usize) {
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        self.dirty = Some(match self.dirty {
            None => [x0, y0, x1, y1],
            Some(d) => [d[0].min(x0), d[1].min(y0), d[2].max(x1), d[3].max(y1)],
        });
    }

    /// Заливка по маске покрытия краской, source-over. `clip` — дополнительное покрытие.
    pub fn fill_mask(&mut self, mask: &Mask, paint: &Paint, clip: Option<&Mask>) {
        let mut x0 = mask.x0.max(0) as usize;
        let mut y0 = mask.y0.max(0) as usize;
        let mut x1 = ((mask.x0 as i64 + mask.w as i64).max(0) as usize).min(self.w);
        let mut y1 = ((mask.y0 as i64 + mask.h as i64).max(0) as usize).min(self.h);
        if let Some(cl) = clip {
            // Вне охвата клипа покрытие нулевое — туда не заходим.
            x0 = x0.max(cl.x0.max(0) as usize);
            y0 = y0.max(cl.y0.max(0) as usize);
            x1 = x1.min((cl.x0 as i64 + cl.w as i64).max(0) as usize);
            y1 = y1.min((cl.y0 as i64 + cl.h as i64).max(0) as usize);
        }
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let solid = match paint {
            Paint::Solid(c) => Some(*c),
            _ => None,
        };
        for y in y0..y1 {
            let mrow = mask.row(y as i32, x0 as i32, x1 - x0);
            let crow = clip.map(|cl| cl.row(y as i32, x0 as i32, x1 - x0));
            for x in x0..x1 {
                let mut c = mrow[x - x0];
                if c <= 0.0 {
                    continue;
                }
                if let Some(cr) = crow {
                    c *= cr[x - x0];
                    if c <= 0.0 {
                        continue;
                    }
                }
                let src = match solid {
                    Some(s) => s,
                    None => paint.sample(x as f32 + 0.5, y as f32 + 0.5),
                };
                let dst = &mut self.px[y * self.w + x];
                let sa = src[3] * c;
                let inv = 1.0 - sa;
                dst[0] = src[0] * c + dst[0] * inv;
                dst[1] = src[1] * c + dst[1] * inv;
                dst[2] = src[2] * c + dst[2] * inv;
                dst[3] = sa + dst[3] * inv;
            }
        }
        self.mark(x0, y0, x1, y1);
    }

    /// Умножить все каналы на покрытие (маски слоя, обрезка прекомпозиции).
    pub fn multiply_coverage(&mut self, cov: &Mask) {
        let Some([dx0, dy0, dx1, dy1]) = self.dirty else {
            return;
        };
        for y in dy0..dy1 {
            for x in dx0..dx1 {
                let c = cov.get(x as i32, y as i32);
                if c < 1.0 {
                    let p = &mut self.px[y * self.w + x];
                    if c <= 0.0 {
                        *p = [0.0; 4];
                    } else {
                        for k in p.iter_mut() {
                            *k *= c;
                        }
                    }
                }
            }
        }
    }

    /// Track matte: умножить на альфу/яркость другого холста (с инверсией).
    pub fn apply_matte(&mut self, matte: &Pixmap, luma: bool, inverted: bool) {
        let Some([dx0, dy0, dx1, dy1]) = self.dirty else {
            return;
        };
        for y in dy0..dy1 {
            for x in dx0..dx1 {
                let m = matte.px[y * self.w + x];
                let mut v = if luma { 0.299 * m[0] + 0.587 * m[1] + 0.114 * m[2] } else { m[3] };
                if inverted {
                    v = 1.0 - v;
                }
                let v = v.clamp(0.0, 1.0);
                if v < 1.0 {
                    let p = &mut self.px[y * self.w + x];
                    for k in p.iter_mut() {
                        *k *= v;
                    }
                }
            }
        }
    }

    /// Наложить `src` на себя с непрозрачностью и режимом смешивания.
    pub fn composite(&mut self, src: &Pixmap, opacity: f32, mode: BlendMode) {
        let Some([x0, y0, x1, y1]) = src.dirty else {
            return;
        };
        let x1 = x1.min(self.w);
        let y1 = y1.min(self.h);
        let op = opacity.clamp(0.0, 1.0);
        if op <= 0.0 {
            return;
        }
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * self.w + x;
                let mut s = src.px[i];
                if s[3] <= 0.0 {
                    continue;
                }
                if op < 1.0 {
                    for k in s.iter_mut() {
                        *k *= op;
                    }
                }
                let d = self.px[i];
                let out = if mode == BlendMode::Normal {
                    let inv = 1.0 - s[3];
                    [s[0] + d[0] * inv, s[1] + d[1] * inv, s[2] + d[2] * inv, s[3] + d[3] * inv]
                } else {
                    let (sa, da) = (s[3], d[3]);
                    let cs = if sa > 0.0 { [s[0] / sa, s[1] / sa, s[2] / sa] } else { [0.0; 3] };
                    let cb = if da > 0.0 { [d[0] / da, d[1] / da, d[2] / da] } else { [0.0; 3] };
                    let mut o = [0f32; 4];
                    for k in 0..3 {
                        let b = blend_channel(mode, cb[k], cs[k]);
                        o[k] = s[k] * (1.0 - da) + d[k] * (1.0 - sa) + sa * da * b;
                    }
                    o[3] = sa + da - sa * da;
                    o
                };
                self.px[i] = out;
            }
        }
        self.mark(x0, y0, x1, y1);
    }

    /// Эффект Fill: все непрозрачные пиксели перекрашиваются в `color`,
    /// альфа умножается на `opacity`.
    pub fn effect_fill(&mut self, color: [f32; 3], opacity: f32) {
        let Some([x0, y0, x1, y1]) = self.dirty else {
            return;
        };
        let op = opacity.clamp(0.0, 1.0);
        for y in y0..y1 {
            for p in &mut self.px[y * self.w + x0..y * self.w + x1] {
                let a = p[3] * op;
                *p = [color[0] * a, color[1] * a, color[2] * a, a];
            }
        }
    }

    /// Эффект Tint: цвет → lerp(black, white, яркость), сила `amount` 0..1.
    pub fn effect_tint(&mut self, black: [f32; 3], white: [f32; 3], amount: f32) {
        let Some([x0, y0, x1, y1]) = self.dirty else {
            return;
        };
        let amt = amount.clamp(0.0, 1.0);
        for y in y0..y1 {
            for p in &mut self.px[y * self.w + x0..y * self.w + x1] {
                let a = p[3];
                if a <= 0.0 {
                    continue;
                }
                let luma = (0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]) / a;
                for k in 0..3 {
                    let target = (black[k] + (white[k] - black[k]) * luma) * a;
                    p[k] += (target - p[k]) * amt;
                }
            }
        }
    }

    /// Гауссово размытие (3 прохода box blur) с сигмами по осям в пикселях.
    pub fn blur(&mut self, sigma_x: f32, sigma_y: f32) {
        let Some(d) = self.dirty else {
            return;
        };
        let boxes = |sigma: f32| -> [usize; 3] {
            if sigma < 0.3 {
                return [0, 0, 0];
            }
            // Ширины боксов, дающие приближение гауссианы (W3C-алгоритм).
            let wi = (sigma * (12.0f32 / 3.0).sqrt() + 1.0).floor() as usize;
            let wl = if wi.is_multiple_of(2) { wi.saturating_sub(1) } else { wi }.max(1);
            let wu = wl + 2;
            let m = ((12.0 * sigma * sigma - (3 * wl * wl + 12 * wl + 9) as f32) / (-4.0 * wl as f32 - 4.0)).round() as usize;
            let m = m.min(3);
            let mut out = [wl; 3];
            for o in out.iter_mut().skip(m) {
                *o = wu;
            }
            out.map(|w| w / 2)
        };
        let bx = boxes(sigma_x);
        let by = boxes(sigma_y);
        let reach_x = bx.iter().sum::<usize>();
        let reach_y = by.iter().sum::<usize>();
        if reach_x == 0 && reach_y == 0 {
            return;
        }
        let x0 = d[0].saturating_sub(reach_x);
        let y0 = d[1].saturating_sub(reach_y);
        let x1 = (d[2] + reach_x).min(self.w);
        let y1 = (d[3] + reach_y).min(self.h);
        let (rw, rh) = (x1 - x0, y1 - y0);
        if rw == 0 || rh == 0 {
            return;
        }
        let mut tmp = vec![[0f32; 4]; rw.max(rh)];
        // Горизонтальные проходы.
        for &r in bx.iter().filter(|r| **r > 0) {
            let inv = 1.0 / (2 * r + 1) as f32;
            for y in y0..y1 {
                let row = &mut self.px[y * self.w + x0..y * self.w + x1];
                tmp[..rw].copy_from_slice(row);
                let mut acc = [0f32; 4];
                for t in tmp.iter().take(r.min(rw - 1) + 1) {
                    for k in 0..4 {
                        acc[k] += t[k];
                    }
                }
                for (x, out) in row.iter_mut().enumerate() {
                    let add_i = x + r + 1;
                    let sub_i = x as isize - r as isize;
                    for k in 0..4 {
                        out[k] = acc[k] * inv;
                    }
                    if add_i < rw {
                        for k in 0..4 {
                            acc[k] += tmp[add_i][k];
                        }
                    }
                    if sub_i >= 0 {
                        let i = sub_i as usize;
                        if i < rw {
                            for k in 0..4 {
                                acc[k] -= tmp[i][k];
                            }
                        }
                    }
                }
            }
        }
        // Вертикальные проходы.
        for &r in by.iter().filter(|r| **r > 0) {
            let inv = 1.0 / (2 * r + 1) as f32;
            for x in x0..x1 {
                for (i, t) in tmp[..rh].iter_mut().enumerate() {
                    *t = self.px[(y0 + i) * self.w + x];
                }
                let mut acc = [0f32; 4];
                for t in tmp.iter().take((r + 1).min(rh)) {
                    for k in 0..4 {
                        acc[k] += t[k];
                    }
                }
                for yy in 0..rh {
                    let add_i = yy + r + 1;
                    let sub_i = yy as isize - r as isize;
                    let p = &mut self.px[(y0 + yy) * self.w + x];
                    for k in 0..4 {
                        p[k] = acc[k] * inv;
                    }
                    if add_i < rh {
                        for k in 0..4 {
                            acc[k] += tmp[add_i][k];
                        }
                    }
                    if sub_i >= 0 {
                        let i = sub_i as usize;
                        if i < rh {
                            for k in 0..4 {
                                acc[k] -= tmp[i][k];
                            }
                        }
                    }
                }
            }
        }
        self.mark(x0, y0, x1, y1);
    }

    /// Заполнить себя тенью `src`: цвет × альфа `src`, сдвиг на (dx, dy).
    pub fn shadow_of(&mut self, src: &Pixmap, color: [f32; 3], opacity: f32, dx: i32, dy: i32) {
        self.clear();
        let Some([sx0, sy0, sx1, sy1]) = src.dirty else {
            return;
        };
        let x0 = (sx0 as i64 + dx as i64).clamp(0, self.w as i64) as usize;
        let y0 = (sy0 as i64 + dy as i64).clamp(0, self.h as i64) as usize;
        let x1 = (sx1 as i64 + dx as i64).clamp(0, self.w as i64) as usize;
        let y1 = (sy1 as i64 + dy as i64).clamp(0, self.h as i64) as usize;
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let op = opacity.clamp(0.0, 1.0);
        for y in y0..y1 {
            let sy = y as i64 - dy as i64;
            for x in x0..x1 {
                let sx = x as i64 - dx as i64;
                let a = src.px[sy as usize * src.w + sx as usize][3] * op;
                if a > 0.0 {
                    self.px[y * self.w + x] = [color[0] * a, color[1] * a, color[2] * a, a];
                }
            }
        }
        self.mark(x0, y0, x1, y1);
    }

    /// В 8-битный RGBA с прямой альфой (для текстур).
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.w * self.h * 4];
        // Вне затронутой области холст прозрачен — там уже нули.
        let Some([x0, y0, x1, y1]) = self.dirty else {
            return out;
        };
        for y in y0..y1 {
            let row = &self.px[y * self.w + x0..y * self.w + x1];
            let dst = &mut out[(y * self.w + x0) * 4..(y * self.w + x1) * 4];
            for (p, o) in row.iter().zip(dst.chunks_exact_mut(4)) {
                let a = p[3].min(1.0);
                if a <= 0.0 {
                    continue;
                }
                let k = 255.0 / a;
                o[0] = (p[0] * k + 0.5).min(255.0) as u8;
                o[1] = (p[1] * k + 0.5).min(255.0) as u8;
                o[2] = (p[2] * k + 0.5).min(255.0) as u8;
                o[3] = (a * 255.0 + 0.5) as u8;
            }
        }
        out
    }

    /// В 8-битный премультиплицированный RGBA.
    pub fn to_rgba8_premultiplied(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.w * self.h * 4];
        for (i, p) in self.px.iter().enumerate() {
            for k in 0..4 {
                out[i * 4 + k] = (p[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f32, y0: f32, x1: f32, y1: f32) -> Polyline {
        Polyline {
            pts: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            closed: true,
        }
    }

    #[test]
    fn coverage_exact_area() {
        let mut r = Rasterizer::new();
        let m = r.fill(&[square(1.5, 1.5, 3.5, 3.5)], FillRule::NonZero, 8, 8).unwrap();
        let total: f32 = m.cov.iter().sum();
        assert!((total - 4.0).abs() < 1e-3, "{total}");
        assert!((m.get(2, 2) - 1.0).abs() < 1e-4);
        assert!((m.get(1, 1) - 0.25).abs() < 1e-4);
        assert!((m.get(1, 2) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn reversed_orientation_same_coverage() {
        let mut r = Rasterizer::new();
        let mut rev = square(1.0, 1.0, 3.0, 3.0);
        rev.pts.reverse();
        let m = r.fill(&[rev], FillRule::NonZero, 8, 8).unwrap();
        assert!((m.cov.iter().sum::<f32>() - 4.0).abs() < 1e-3);
    }

    #[test]
    fn even_odd_hole() {
        let mut r = Rasterizer::new();
        let polys = [square(0.0, 0.0, 10.0, 10.0), square(3.0, 3.0, 7.0, 7.0)];
        let m = r.fill(&polys, FillRule::EvenOdd, 10, 10).unwrap();
        assert!(m.get(5, 5) < 1e-4);
        assert!((m.get(1, 1) - 1.0).abs() < 1e-4);
        let m = r.fill(&polys, FillRule::NonZero, 10, 10).unwrap();
        assert!((m.get(5, 5) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn clipping_outside_canvas_keeps_inside_coverage() {
        let mut r = Rasterizer::new();
        // Квадрат, выходящий за все границы: внутри холст целиком покрыт.
        let m = r.fill(&[square(-5.0, -5.0, 15.0, 15.0)], FillRule::NonZero, 6, 6).unwrap();
        assert_eq!((m.w, m.h), (6, 6));
        assert!(m.cov.iter().all(|c| (c - 1.0).abs() < 1e-4));
        // Треугольник, частично слева от холста.
        let tri = Polyline {
            pts: vec![[-4.0, 0.0], [4.0, 0.0], [4.0, 8.0]],
            closed: true,
        };
        let m = r.fill(&[tri], FillRule::NonZero, 8, 8).unwrap();
        // Точка (3.5, 1.5): внутри треугольника (y < x+4 → 1.5 < 7.5) ✓
        assert!((m.get(3, 1) - 1.0).abs() < 1e-3, "{}", m.get(3, 1));
        // Точка (0.5, 6.5): снаружи (6.5 > 4.5).
        assert!(m.get(0, 6) < 1e-3);
    }

    #[test]
    fn gradient_stops_sample() {
        let g = GradientStops::from_raw(&[0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0], 2);
        let c = g.sample(0.5);
        assert!((c[0] - 0.5).abs() < 1e-5 && (c[2] - 0.5).abs() < 1e-5 && (c[3] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn composite_normal_and_to_rgba8() {
        let mut a = Pixmap::new(2, 1);
        let mut b = Pixmap::new(2, 1);
        let m = Mask::full(2, 1);
        a.fill_mask(&m, &Paint::solid([1.0, 0.0, 0.0], 1.0), None);
        b.fill_mask(&m, &Paint::solid([0.0, 0.0, 1.0], 0.5), None);
        a.composite(&b, 1.0, BlendMode::Normal);
        let px = a.to_rgba8();
        assert_eq!(&px[..4], &[128, 0, 128, 255]);
    }
}
