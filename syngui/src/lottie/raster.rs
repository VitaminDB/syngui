//! Растеризатор с точным покрытием (накопление знаковой площади, как в
//! font-rs), краски (цвет, линейный/радиальный градиент, картинка) и
//! композитинг слоёв в премультиплицированном f32-RGBA (sRGB, как в lottie-web).

use std::sync::Arc;

use super::geom::{Hom, Mat, Polyline};

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

/// Точка изолинии на ребре клетки: (id ребра, координаты).
type EdgePt = (u64, [f32; 2]);

impl Mask {
    /// Контуры области `покрытие ≥ iso` (marching squares по центрам
    /// пикселей с линейной интерполяцией): замкнутые ломаные в координатах
    /// холста. Седловые клетки разрешаются по среднему значению.
    pub fn contours(&self, iso: f32) -> Vec<Polyline> {
        let (w, h) = (self.w as i64, self.h as i64);
        if w == 0 || h == 0 || self.cov.len() < self.w * self.h {
            return Vec::new();
        }
        // Поле с рамкой из нулей: узлы i ∈ -1..=w, j ∈ -1..=h.
        let val = |i: i64, j: i64| -> f32 {
            if i < 0 || j < 0 || i >= w || j >= h {
                0.0
            } else {
                self.cov[(j * w + i) as usize]
            }
        };
        let gw = w + 2; // число узлов по x
                        // Идентификаторы рёбер: горизонтальное (i,j)-(i+1,j) и вертикальное (i,j)-(i,j+1).
        let hid = |i: i64, j: i64| (((j + 1) * gw + (i + 1)) * 2) as u64;
        let vid = |i: i64, j: i64| (((j + 1) * gw + (i + 1)) * 2 + 1) as u64;
        let pt_h = |i: i64, j: i64| -> [f32; 2] {
            let (a, b) = (val(i, j), val(i + 1, j));
            let t = if (b - a).abs() < 1e-9 {
                0.5
            } else {
                ((iso - a) / (b - a)).clamp(0.0, 1.0)
            };
            [
                self.x0 as f32 + i as f32 + 0.5 + t,
                self.y0 as f32 + j as f32 + 0.5,
            ]
        };
        let pt_v = |i: i64, j: i64| -> [f32; 2] {
            let (a, b) = (val(i, j), val(i, j + 1));
            let t = if (b - a).abs() < 1e-9 {
                0.5
            } else {
                ((iso - a) / (b - a)).clamp(0.0, 1.0)
            };
            [
                self.x0 as f32 + i as f32 + 0.5,
                self.y0 as f32 + j as f32 + 0.5 + t,
            ]
        };
        // Отрезки: начало → конец (по рёбрам), внутренность слева.
        let mut next: std::collections::HashMap<u64, (u64, [f32; 2])> =
            std::collections::HashMap::new();
        let mut starts: Vec<(u64, [f32; 2])> = Vec::new();
        for j in -1..h {
            for i in -1..w {
                let (a, b, c, d) = (val(i, j), val(i + 1, j), val(i + 1, j + 1), val(i, j + 1));
                let code = ((a >= iso) as u8)
                    | (((b >= iso) as u8) << 1)
                    | (((c >= iso) as u8) << 2)
                    | (((d >= iso) as u8) << 3);
                if code == 0 || code == 15 {
                    continue;
                }
                // Рёбра клетки: верх, право, низ, лево.
                let top = (hid(i, j), pt_h(i, j));
                let right = (vid(i + 1, j), pt_v(i + 1, j));
                let bottom = (hid(i, j + 1), pt_h(i, j + 1));
                let left = (vid(i, j), pt_v(i, j));
                let center_in = (a + b + c + d) / 4.0 >= iso;
                let segs: &[(EdgePt, EdgePt)] = &match code {
                    1 => [(left, top), (left, left)],
                    2 => [(top, right), (top, top)],
                    3 => [(left, right), (left, left)],
                    4 => [(right, bottom), (right, right)],
                    5 => {
                        if center_in {
                            [(left, bottom), (right, top)]
                        } else {
                            [(left, top), (right, bottom)]
                        }
                    }
                    6 => [(top, bottom), (top, top)],
                    7 => [(left, bottom), (left, left)],
                    8 => [(bottom, left), (bottom, bottom)],
                    9 => [(bottom, top), (bottom, bottom)],
                    10 => {
                        if center_in {
                            [(top, left), (bottom, right)]
                        } else {
                            [(top, right), (bottom, left)]
                        }
                    }
                    11 => [(bottom, right), (bottom, bottom)],
                    12 => [(right, left), (right, right)],
                    13 => [(right, top), (right, right)],
                    _ => [(top, left), (top, top)], // 14
                };
                for &(s0, s1) in segs {
                    if s0.0 == s1.0 {
                        continue; // заглушка (одна пара)
                    }
                    next.insert(s0.0, (s1.0, s1.1));
                    starts.push(s0);
                }
            }
        }
        // Склейка отрезков в замкнутые контуры.
        let mut out = Vec::new();
        for (sid, spt) in starts {
            if !next.contains_key(&sid) {
                continue;
            }
            let mut pts = vec![spt];
            let mut cur = sid;
            while let Some((nid, npt)) = next.remove(&cur) {
                if nid == sid {
                    break;
                }
                pts.push(npt);
                cur = nid;
            }
            if pts.len() >= 3 {
                out.push(Polyline { pts, closed: true });
            }
        }
        out
    }

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
        if self.x0 > 0
            || self.y0 > 0
            || (self.x0 as i64 + self.w as i64) < w as i64
            || (self.y0 as i64 + self.h as i64) < h as i64
        {
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
    pub fn fill(
        &mut self,
        polys: &[Polyline],
        rule: FillRule,
        clip_w: usize,
        clip_h: usize,
    ) -> Option<Mask> {
        let mut bb: Option<[f32; 4]> = None;
        for p in polys {
            if let Some(b) = p.bbox() {
                bb = Some(match bb {
                    None => b,
                    Some(o) => [
                        o[0].min(b[0]),
                        o[1].min(b[1]),
                        o[2].max(b[2]),
                        o[3].max(b[3]),
                    ],
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
                        if a > 1.0 {
                            2.0 - a
                        } else {
                            a
                        }
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
        let (dir, p0, p1) = if p0[1] < p1[1] {
            (1.0f32, p0, p1)
        } else {
            (-1.0f32, p1, p0)
        };
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
        let t = if t.is_finite() {
            t.clamp(0.0, 1.0)
        } else {
            0.0
        };
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
                    [
                        c0[0] + (c1[0] - c0[0]) * k,
                        c0[1] + (c1[1] - c0[1]) * k,
                        c0[2] + (c1[2] - c0[2]) * k,
                    ]
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
            [
                d[i] as f32 / 255.0 * a,
                d[i + 1] as f32 / 255.0 * a,
                d[i + 2] as f32 / 255.0 * a,
                a,
            ]
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
                let t = if dd > 0.0 {
                    ((p[0] - start[0]) * d[0] + (p[1] - start[1]) * d[1]) / dd
                } else {
                    0.0
                };
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
            Paint::Image {
                inv,
                image,
                opacity,
            } => {
                let p = inv.apply([x, y]);
                let c = image.sample(p[0], p[1]);
                [
                    c[0] * opacity,
                    c[1] * opacity,
                    c[2] * opacity,
                    c[3] * opacity,
                ]
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
    if t1 >= 0.0 {
        t1.max(t2.min(t1)).max(0.0)
    } else {
        t2.max(0.0)
    }
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
                let d = if cb <= 0.25 {
                    ((16.0 * cb - 12.0) * cb + 4.0) * cb
                } else {
                    cb.sqrt()
                };
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
                let mut v = if luma {
                    0.299 * m[0] + 0.587 * m[1] + 0.114 * m[2]
                } else {
                    m[3]
                };
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
                    [
                        s[0] + d[0] * inv,
                        s[1] + d[1] * inv,
                        s[2] + d[2] * inv,
                        s[3] + d[3] * inv,
                    ]
                } else {
                    let (sa, da) = (s[3], d[3]);
                    let cs = if sa > 0.0 {
                        [s[0] / sa, s[1] / sa, s[2] / sa]
                    } else {
                        [0.0; 3]
                    };
                    let cb = if da > 0.0 {
                        [d[0] / da, d[1] / da, d[2] / da]
                    } else {
                        [0.0; 3]
                    };
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
            let wl = if wi.is_multiple_of(2) {
                wi.saturating_sub(1)
            } else {
                wi
            }
            .max(1);
            let wu = wl + 2;
            let m = ((12.0 * sigma * sigma - (3 * wl * wl + 12 * wl + 9) as f32)
                / (-4.0 * wl as f32 - 4.0))
                .round() as usize;
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
    /// Перспективная деформация: `fwd` переводит пиксели `src` в пиксели
    /// холста, каждый пиксель холста берётся из `src` обратным отображением
    /// с билинейной выборкой (премультиплицированные цвета).
    pub fn warp_from(&mut self, src: &Pixmap, fwd: &Hom) {
        self.clear();
        let Some([sx0, sy0, sx1, sy1]) = src.dirty else {
            return;
        };
        let Some(inv) = fwd.invert() else {
            return;
        };
        // Охват: углы затронутой области источника (только перед камерой).
        let mut bb = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for c in [
            [sx0 as f32, sy0 as f32],
            [sx1 as f32, sy0 as f32],
            [sx1 as f32, sy1 as f32],
            [sx0 as f32, sy1 as f32],
        ] {
            let p = fwd.apply3(c);
            if p[2] <= 1e-6 {
                // Часть плоскости за камерой — берём весь холст.
                bb = [0.0, 0.0, self.w as f32, self.h as f32];
                break;
            }
            let (x, y) = (p[0] / p[2], p[1] / p[2]);
            bb = [bb[0].min(x), bb[1].min(y), bb[2].max(x), bb[3].max(y)];
        }
        let x0 = (bb[0].floor().max(0.0) as usize).min(self.w);
        let y0 = (bb[1].floor().max(0.0) as usize).min(self.h);
        let x1 = (bb[2].ceil().max(0.0) as usize + 1).min(self.w);
        let y1 = (bb[3].ceil().max(0.0) as usize + 1).min(self.h);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let sample = |x: i64, y: i64| -> [f32; 4] {
            if x < sx0 as i64 || y < sy0 as i64 || x >= sx1 as i64 || y >= sy1 as i64 {
                [0.0; 4]
            } else {
                src.px[y as usize * src.w + x as usize]
            }
        };
        for y in y0..y1 {
            for x in x0..x1 {
                let p = inv.apply3([x as f32 + 0.5, y as f32 + 0.5]);
                if p[2] <= 1e-6 {
                    continue;
                }
                let fx = p[0] / p[2] - 0.5;
                let fy = p[1] / p[2] - 0.5;
                if fx < sx0 as f32 - 1.0
                    || fy < sy0 as f32 - 1.0
                    || fx > sx1 as f32
                    || fy > sy1 as f32
                {
                    continue;
                }
                let (ix, iy) = (fx.floor() as i64, fy.floor() as i64);
                let (tx, ty) = (fx - ix as f32, fy - iy as f32);
                let (a, b, c, d) = (
                    sample(ix, iy),
                    sample(ix + 1, iy),
                    sample(ix, iy + 1),
                    sample(ix + 1, iy + 1),
                );
                let mut o = [0f32; 4];
                for k in 0..4 {
                    let top = a[k] + (b[k] - a[k]) * tx;
                    let bot = c[k] + (d[k] - c[k]) * tx;
                    o[k] = top + (bot - top) * ty;
                }
                if o[3] > 0.0 {
                    self.px[y * self.w + x] = o;
                }
            }
        }
        self.mark(x0, y0, x1, y1);
    }

    /// Прибавить `src·k` (накопление для усреднения кадров).
    pub fn accumulate(&mut self, src: &Pixmap, k: f32) {
        let Some([x0, y0, x1, y1]) = src.dirty else {
            return;
        };
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * self.w + x;
                let s = src.px[i];
                let d = &mut self.px[i];
                for c in 0..4 {
                    d[c] += s[c] * k;
                }
            }
        }
        self.mark(x0, y0, x1, y1);
    }

    /// Инверсия цвета (альфа сохраняется); `keep` — доля исходного.
    pub fn effect_invert(&mut self, keep: f32) {
        let Some([x0, y0, x1, y1]) = self.dirty else {
            return;
        };
        let keep = keep.clamp(0.0, 1.0);
        for y in y0..y1 {
            for p in &mut self.px[y * self.w + x0..y * self.w + x1] {
                let a = p[3];
                for c in p.iter_mut().take(3) {
                    let inv = a - *c;
                    *c = inv + (*c - inv) * keep;
                }
            }
        }
    }

    /// Копия содержимого `src` (того же размера) — только затронутая область.
    pub fn copy_from(&mut self, src: &Pixmap) {
        self.clear();
        let Some([x0, y0, x1, y1]) = src.dirty else {
            return;
        };
        for y in y0..y1 {
            let r = y * self.w + x0..y * self.w + x1;
            self.px[r.clone()].copy_from_slice(&src.px[r]);
        }
        self.dirty = src.dirty;
    }

    /// Обратить альфу силуэта и залить цветом: `a' = (1 − a)·opacity` в
    /// пределах затронутой области (свечение от центра).
    pub fn invert_alpha_colored(&mut self, color: [f32; 3], opacity: f32) {
        let Some([x0, y0, x1, y1]) = self.dirty else {
            return;
        };
        let op = opacity.clamp(0.0, 1.0);
        for y in y0..y1 {
            for p in &mut self.px[y * self.w + x0..y * self.w + x1] {
                let a = (1.0 - p[3]).clamp(0.0, 1.0) * op;
                *p = [color[0] * a, color[1] * a, color[2] * a, a];
            }
        }
    }

    /// Силуэт `src` цветом `color`: альфа источника, сдвинутая на `dx,dy`,
    /// с расширением `spread` (0..1, как Spread/Choke в After Effects) и,
    /// если `inverted`, обращённая (для внутренних теней и свечений —
    /// тогда силуэт занимает охват источника с полями `margin`).
    #[allow(clippy::too_many_arguments)]
    pub fn silhouette(
        &mut self,
        src: &Pixmap,
        color: [f32; 3],
        opacity: f32,
        dx: i32,
        dy: i32,
        spread: f32,
        inverted: bool,
        margin: usize,
    ) {
        self.clear();
        let Some([sx0, sy0, sx1, sy1]) = src.dirty else {
            return;
        };
        let m = margin as i64;
        let (x0, y0, x1, y1) = if inverted {
            (
                sx0 as i64 - m,
                sy0 as i64 - m,
                sx1 as i64 + m,
                sy1 as i64 + m,
            )
        } else {
            (
                sx0 as i64 + dx as i64,
                sy0 as i64 + dy as i64,
                sx1 as i64 + dx as i64,
                sy1 as i64 + dy as i64,
            )
        };
        let x0 = x0.clamp(0, self.w as i64) as usize;
        let y0 = y0.clamp(0, self.h as i64) as usize;
        let x1 = x1.clamp(0, self.w as i64) as usize;
        let y1 = y1.clamp(0, self.h as i64) as usize;
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let op = opacity.clamp(0.0, 1.0);
        let k = 1.0 / (1.0 - spread.clamp(0.0, 0.999));
        for y in y0..y1 {
            let sy = y as i64 - dy as i64;
            for x in x0..x1 {
                let sx = x as i64 - dx as i64;
                let inside =
                    sx >= sx0 as i64 && sx < sx1 as i64 && sy >= sy0 as i64 && sy < sy1 as i64;
                let mut a = if inside {
                    src.px[sy as usize * src.w + sx as usize][3]
                } else {
                    0.0
                };
                if inverted {
                    a = 1.0 - a;
                }
                let a = (a * k).min(1.0) * op;
                if a > 0.0 {
                    self.px[y * self.w + x] = [color[0] * a, color[1] * a, color[2] * a, a];
                }
            }
        }
        self.mark(x0, y0, x1, y1);
    }

    /// Умножить содержимое на альфу `src` (вне её охвата — ноль).
    pub fn mask_by_alpha(&mut self, src: &Pixmap) {
        let Some([x0, y0, x1, y1]) = self.dirty else {
            return;
        };
        let Some([sx0, sy0, sx1, sy1]) = src.dirty else {
            self.clear();
            return;
        };
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * self.w + x;
                let a = if x >= sx0 && x < sx1 && y >= sy0 && y < sy1 {
                    src.px[i][3]
                } else {
                    0.0
                };
                if a < 1.0 {
                    for c in self.px[i].iter_mut() {
                        *c *= a;
                    }
                }
            }
        }
    }

    /// Круговое расширение альфы `src` на `radius` пикселей, залитое цветом
    /// (обводка снаружи — layer style Stroke). Максимум по окну для каждой
    /// строки круга — алгоритм ван Херка/Гила–Вермана, O(r·N).
    pub fn dilated_of(&mut self, src: &Pixmap, color: [f32; 3], opacity: f32, radius: f32) {
        self.clear();
        let Some([sx0, sy0, sx1, sy1]) = src.dirty else {
            return;
        };
        let r = radius.max(0.0).ceil() as usize;
        let x0 = sx0.saturating_sub(r);
        let y0 = sy0.saturating_sub(r);
        let x1 = (sx1 + r).min(self.w);
        let y1 = (sy1 + r).min(self.h);
        let (rw, rh) = (x1 - x0, y1 - y0);
        if rw == 0 || rh == 0 {
            return;
        }
        let alpha = |x: usize, y: usize| -> f32 {
            if x >= sx0 && x < sx1 && y >= sy0 && y < sy1 {
                src.px[y * src.w + x][3]
            } else {
                0.0
            }
        };
        let mut out = vec![0f32; rw * rh];
        let mut row = vec![0f32; rw];
        let mut hm = vec![0f32; rw];
        let mut g = vec![0f32; rw];
        let mut h = vec![0f32; rw];
        let rr = radius.max(0.0);
        for dy in -(r as i64)..=(r as i64) {
            let half = (rr * rr - (dy * dy) as f32).max(0.0).sqrt();
            let wnd = half.floor() as usize;
            for yy in 0..rh {
                let sy = yy as i64 + y0 as i64 + dy;
                if sy < sy0 as i64 || sy >= sy1 as i64 {
                    continue;
                }
                for (xx, v) in row.iter_mut().enumerate() {
                    *v = alpha(x0 + xx, sy as usize);
                }
                // Скользящий максимум окна 2·wnd+1.
                let k = 2 * wnd + 1;
                for i in 0..rw {
                    g[i] = if i % k == 0 {
                        row[i]
                    } else {
                        g[i - 1].max(row[i])
                    };
                }
                for i in (0..rw).rev() {
                    h[i] = if i == rw - 1 || (i + 1) % k == 0 {
                        row[i]
                    } else {
                        h[i + 1].max(row[i])
                    };
                }
                for (i, v) in hm.iter_mut().enumerate() {
                    if i < wnd || i + wnd >= rw {
                        // Укороченное окно у края: напрямую.
                        let lo = i.saturating_sub(wnd);
                        let hi = (i + wnd).min(rw - 1);
                        *v = row[lo..=hi].iter().copied().fold(0.0, f32::max);
                    } else {
                        *v = h[i - wnd].max(g[i + wnd]);
                    }
                }
                let o = &mut out[yy * rw..(yy + 1) * rw];
                for (ov, hv) in o.iter_mut().zip(&hm) {
                    if *hv > *ov {
                        *ov = *hv;
                    }
                }
            }
        }
        let op = opacity.clamp(0.0, 1.0);
        for yy in 0..rh {
            for xx in 0..rw {
                let a = out[yy * rw + xx] * op;
                if a > 0.0 {
                    self.px[(y0 + yy) * self.w + x0 + xx] =
                        [color[0] * a, color[1] * a, color[2] * a, a];
                }
            }
        }
        self.mark(x0, y0, x1, y1);
    }

    /// Color Overlay: цвет смешивается с исходным с долей `opacity`, альфа
    /// сохраняется.
    pub fn color_overlay(&mut self, color: [f32; 3], opacity: f32) {
        let Some([x0, y0, x1, y1]) = self.dirty else {
            return;
        };
        let op = opacity.clamp(0.0, 1.0);
        for y in y0..y1 {
            for p in &mut self.px[y * self.w + x0..y * self.w + x1] {
                let a = p[3];
                for k in 0..3 {
                    p[k] += (color[k] * a - p[k]) * op;
                }
            }
        }
    }

    /// Tritone: яркость → тени/полутона/света; `blend` — доля исходного.
    pub fn effect_tritone(&mut self, high: [f32; 3], mid: [f32; 3], shadow: [f32; 3], blend: f32) {
        let Some([x0, y0, x1, y1]) = self.dirty else {
            return;
        };
        let keep = blend.clamp(0.0, 1.0);
        for y in y0..y1 {
            for p in &mut self.px[y * self.w + x0..y * self.w + x1] {
                let a = p[3];
                if a <= 0.0 {
                    continue;
                }
                let l = ((0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]) / a).clamp(0.0, 1.0);
                for k in 0..3 {
                    let c = if l < 0.5 {
                        shadow[k] + (mid[k] - shadow[k]) * l * 2.0
                    } else {
                        mid[k] + (high[k] - mid[k]) * (l - 0.5) * 2.0
                    };
                    p[k] = c * a + (p[k] - c * a) * keep;
                }
            }
        }
    }

    /// Уровни (Levels): `rgb`, `r`, `g`, `b`, `a` — по пять чисел: вход
    /// чёрное/белое, гамма, выход чёрное/белое (0..1).
    pub fn effect_levels(&mut self, ch: &[[f32; 5]; 5]) {
        let Some([x0, y0, x1, y1]) = self.dirty else {
            return;
        };
        let lv = |v: f32, p: &[f32; 5]| -> f32 {
            let [ib, iw, g, ob, ow] = *p;
            let d = iw - ib;
            let mut t = if d.abs() < 1e-6 {
                if v >= iw {
                    1.0
                } else {
                    0.0
                }
            } else {
                ((v - ib) / d).clamp(0.0, 1.0)
            };
            if g > 0.0 && (g - 1.0).abs() > 1e-6 {
                t = t.powf(1.0 / g);
            }
            ob + t * (ow - ob)
        };
        let ident = |p: &[f32; 5]| *p == [0.0, 1.0, 1.0, 0.0, 1.0];
        let (do_rgb, do_a) = (
            !(ident(&ch[0]) && ident(&ch[1]) && ident(&ch[2]) && ident(&ch[3])),
            !ident(&ch[4]),
        );
        for y in y0..y1 {
            for p in &mut self.px[y * self.w + x0..y * self.w + x1] {
                let a = p[3];
                if a <= 0.0 {
                    continue;
                }
                let mut na = a;
                if do_a {
                    na = lv(a, &ch[4]).clamp(0.0, 1.0);
                }
                for k in 0..3 {
                    let mut c = p[k] / a;
                    if do_rgb {
                        c = lv(lv(c, &ch[0]), &ch[k + 1]);
                    }
                    p[k] = c.clamp(0.0, 1.0) * na;
                }
                p[3] = na;
            }
        }
    }

    /// Radial Wipe: видимая часть — сектор от угла `start` (градусы от
    /// вертикали вверх, по часовой) на `(1 − completion)·360°`; `dir`:
    /// 1 — по часовой, 2 — против, 3 — в обе стороны; `feather` — мягкость
    /// края в пикселях.
    pub fn radial_wipe(
        &mut self,
        center: [f32; 2],
        start: f32,
        completion: f32,
        dir: i32,
        feather: f32,
    ) {
        let Some([x0, y0, x1, y1]) = self.dirty else {
            return;
        };
        let c = completion.clamp(0.0, 1.0);
        if c <= 0.0 {
            return;
        }
        if c >= 1.0 {
            self.clear();
            return;
        }
        let wiped = c * 360.0;
        for y in y0..y1 {
            for x in x0..x1 {
                let dxp = x as f32 + 0.5 - center[0];
                let dyp = y as f32 + 0.5 - center[1];
                // Угол по часовой от «вверх».
                let mut ang = dxp.atan2(-dyp).to_degrees() - start;
                ang = ang.rem_euclid(360.0);
                // Расстояние (в градусах) до границы стёртого сектора.
                let edge = match dir {
                    2 => (360.0 - ang) - wiped,
                    3 => {
                        let d = ang.min(360.0 - ang);
                        d - wiped / 2.0
                    }
                    _ => ang - wiped,
                };
                let v = if feather > 0.0 {
                    let r = (dxp * dxp + dyp * dyp).sqrt().max(1.0);
                    let px = edge.to_radians() * r;
                    (px / feather + 0.5).clamp(0.0, 1.0)
                } else if edge >= 0.0 {
                    1.0
                } else {
                    0.0
                };
                if v < 1.0 {
                    for k in self.px[y * self.w + x].iter_mut() {
                        *k *= v;
                    }
                }
            }
        }
    }

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
        let m = r
            .fill(&[square(1.5, 1.5, 3.5, 3.5)], FillRule::NonZero, 8, 8)
            .unwrap();
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
        let m = r
            .fill(&[square(-5.0, -5.0, 15.0, 15.0)], FillRule::NonZero, 6, 6)
            .unwrap();
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
        let g = GradientStops::from_raw(
            &[0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0],
            2,
        );
        let c = g.sample(0.5);
        assert!(
            (c[0] - 0.5).abs() < 1e-5 && (c[2] - 0.5).abs() < 1e-5 && (c[3] - 0.5).abs() < 1e-5
        );
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

#[cfg(test)]
mod contour_tests {
    use super::*;

    #[test]
    fn square_contour_is_closed_and_sized() {
        // Квадрат 4×4 полного покрытия в маске 8×8.
        let mut m = Mask {
            x0: 10,
            y0: 20,
            w: 8,
            h: 8,
            cov: vec![0.0; 64],
        };
        for y in 2..6 {
            for x in 2..6 {
                m.cov[y * 8 + x] = 1.0;
            }
        }
        let c = m.contours(0.5);
        assert_eq!(c.len(), 1);
        let bb = c[0].bbox().unwrap();
        // Граница — посередине между центрами пикселей: 12..16 × 22..26.
        assert!((bb[0] - 12.0).abs() < 1e-4 && (bb[2] - 16.0).abs() < 1e-4);
        assert!((bb[1] - 22.0).abs() < 1e-4 && (bb[3] - 26.0).abs() < 1e-4);
    }

    #[test]
    fn ring_has_two_contours() {
        let mut m = Mask {
            x0: 0,
            y0: 0,
            w: 9,
            h: 9,
            cov: vec![0.0; 81],
        };
        for y in 1..8 {
            for x in 1..8 {
                let inner = (3..6).contains(&x) && (3..6).contains(&y);
                m.cov[y * 9 + x] = if inner { 0.0 } else { 1.0 };
            }
        }
        assert_eq!(m.contours(0.5).len(), 2);
    }
}
