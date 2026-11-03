//! Геометрия Lottie: аффинные матрицы, безье-контуры (`v/i/o`), уплощение в
//! ломаные, модификаторы (скругление углов, trim paths, пунктир) и
//! превращение обводки в полигоны для заливки по правилу nonzero.

/// Аффинная матрица: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Mat {
    pub const IDENTITY: Mat = Mat {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(x: f32, y: f32) -> Mat {
        Mat {
            e: x,
            f: y,
            ..Mat::IDENTITY
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Mat {
        Mat {
            a: sx,
            d: sy,
            ..Mat::IDENTITY
        }
    }

    /// Поворот на `deg` градусов; при y вниз положительный угол — по часовой.
    pub fn rotate(deg: f32) -> Mat {
        let (s, c) = deg.to_radians().sin_cos();
        Mat {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Сдвиг по X: `x' = x + t·y`.
    pub fn skew_x(t: f32) -> Mat {
        Mat {
            c: t,
            ..Mat::IDENTITY
        }
    }

    /// `self · other`: к точке сначала применяется `other`, затем `self`.
    pub fn mul(&self, o: &Mat) -> Mat {
        Mat {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }

    #[inline]
    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        [
            self.a * p[0] + self.c * p[1] + self.e,
            self.b * p[0] + self.d * p[1] + self.f,
        ]
    }

    /// Только линейная часть (для касательных).
    #[inline]
    pub fn apply_vec(&self, v: [f32; 2]) -> [f32; 2] {
        [self.a * v[0] + self.c * v[1], self.b * v[0] + self.d * v[1]]
    }

    pub fn det(&self) -> f32 {
        self.a * self.d - self.b * self.c
    }

    /// Средний масштаб длин: √|det|.
    pub fn scale_factor(&self) -> f32 {
        self.det().abs().sqrt()
    }

    pub fn invert(&self) -> Option<Mat> {
        let det = self.det();
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        Some(Mat {
            a: self.d * inv,
            b: -self.b * inv,
            c: -self.c * inv,
            d: self.a * inv,
            e: (self.c * self.f - self.d * self.e) * inv,
            f: (self.b * self.e - self.a * self.f) * inv,
        })
    }

    pub fn is_identity(&self) -> bool {
        *self == Mat::IDENTITY
    }
}

/// Матрица трансформации Lottie (слоя или группы):
/// `T(position) · R(rotation) · Skew(skew, axis) · S(scale) · T(-anchor)`.
pub fn compose_transform(
    anchor: [f32; 2],
    position: [f32; 2],
    scale: [f32; 2],
    rotation_deg: f32,
    skew_deg: f32,
    skew_axis_deg: f32,
) -> Mat {
    let mut m = Mat::translate(position[0], position[1]);
    if rotation_deg != 0.0 {
        m = m.mul(&Mat::rotate(rotation_deg));
    }
    if skew_deg != 0.0 {
        m = m
            .mul(&Mat::rotate(skew_axis_deg))
            .mul(&Mat::skew_x((-skew_deg).to_radians().tan()))
            .mul(&Mat::rotate(-skew_axis_deg));
    }
    if scale != [1.0, 1.0] {
        m = m.mul(&Mat::scale(scale[0], scale[1]));
    }
    if anchor != [0.0, 0.0] {
        m = m.mul(&Mat::translate(-anchor[0], -anchor[1]));
    }
    m
}

#[inline]
fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

#[inline]
fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

#[inline]
fn mulf(a: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] * k, a[1] * k]
}

#[inline]
fn len(a: [f32; 2]) -> f32 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}

#[inline]
fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    len(sub(a, b))
}

#[inline]
fn lerp2(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

#[inline]
fn is_zero(v: [f32; 2]) -> bool {
    v[0].abs() < 1e-5 && v[1].abs() < 1e-5
}

/// Контур Lottie: вершины и касательные (`i` входящая, `o` исходящая —
/// относительно вершины). Сегмент k→k+1 — кубическая кривая с контрольными
/// точками `v[k]+o[k]`, `v[k+1]+i[k+1]`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapePath {
    pub verts: Vec<[f32; 2]>,
    pub ins: Vec<[f32; 2]>,
    pub outs: Vec<[f32; 2]>,
    pub closed: bool,
}

impl ShapePath {
    pub fn with_capacity(n: usize, closed: bool) -> Self {
        Self {
            verts: Vec::with_capacity(n),
            ins: Vec::with_capacity(n),
            outs: Vec::with_capacity(n),
            closed,
        }
    }

    /// Добавить вершину с абсолютными касательными не нужно — здесь относительные.
    pub fn push(&mut self, v: [f32; 2], in_rel: [f32; 2], out_rel: [f32; 2]) {
        self.verts.push(v);
        self.ins.push(in_rel);
        self.outs.push(out_rel);
    }

    pub fn is_empty(&self) -> bool {
        self.verts.is_empty()
    }

    /// Обратить направление обхода (`d: 3` у фигур).
    pub fn reverse(&mut self) {
        self.verts.reverse();
        self.ins.reverse();
        self.outs.reverse();
        std::mem::swap(&mut self.ins, &mut self.outs);
    }

    pub fn transform(&mut self, m: &Mat) {
        for v in &mut self.verts {
            *v = m.apply(*v);
        }
        for t in self.ins.iter_mut().chain(self.outs.iter_mut()) {
            *t = m.apply_vec(*t);
        }
    }

    /// Уплощение в ломаную с допуском `tol` (в тех же единицах, что и точки).
    pub fn flatten(&self, tol: f32) -> Polyline {
        let n = self.verts.len();
        let mut pts = Vec::with_capacity(n * 4);
        if n == 0 {
            return Polyline { pts, closed: self.closed };
        }
        pts.push(self.verts[0]);
        let segs = if self.closed { n } else { n - 1 };
        for i in 0..segs {
            let j = (i + 1) % n;
            let p0 = self.verts[i];
            let p3 = self.verts[j];
            let o = self.outs[i];
            let inn = self.ins[j];
            if is_zero(o) && is_zero(inn) {
                pts.push(p3);
            } else {
                flatten_cubic(p0, add(p0, o), add(p3, inn), p3, tol, &mut pts);
            }
        }
        if self.closed && pts.len() > 1 {
            // Последняя точка совпадает с первой — не дублируем.
            pts.pop();
        }
        Polyline { pts, closed: self.closed }
    }

    /// Модификатор «скруглить углы» (как в lottie-web): острые вершины
    /// заменяются парой точек с касательными.
    pub fn round_corners(&self, radius: f32) -> ShapePath {
        const ROUND: f32 = 0.5519;
        let n = self.verts.len();
        if radius <= 0.0 || n < 2 {
            return self.clone();
        }
        let mut out = ShapePath::with_capacity(n * 2, self.closed);
        for i in 0..n {
            let v = self.verts[i];
            let sharp = is_zero(self.ins[i]) && is_zero(self.outs[i]);
            if !sharp || (!self.closed && (i == 0 || i == n - 1)) {
                out.push(v, self.ins[i], self.outs[i]);
                continue;
            }
            let prev = self.verts[(i + n - 1) % n];
            let next = self.verts[(i + 1) % n];
            let d = dist(v, prev);
            let perc = if d > 0.0 { (d / 2.0).min(radius) / d } else { 0.0 };
            let iv = lerp2(v, prev, perc);
            let o = sub(iv, mulf(sub(iv, v), ROUND));
            out.push(iv, [0.0, 0.0], sub(o, iv));
            let d = dist(v, next);
            let perc = if d > 0.0 { (d / 2.0).min(radius) / d } else { 0.0 };
            let iv = lerp2(v, next, perc);
            let o = sub(iv, mulf(sub(iv, v), ROUND));
            out.push(iv, sub(o, iv), [0.0, 0.0]);
        }
        out
    }
}

#[inline]
fn eval_cubic(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let mt = 1.0 - t;
    let a = mt * mt * mt;
    let b = 3.0 * mt * mt * t;
    let c = 3.0 * mt * t * t;
    let d = t * t * t;
    [
        a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
        a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
    ]
}

/// Разбить кубическую кривую на отрезки; `out` уже содержит `p0`.
pub fn flatten_cubic(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], tol: f32, out: &mut Vec<[f32; 2]>) {
    let dd1 = [p0[0] - 2.0 * p1[0] + p2[0], p0[1] - 2.0 * p1[1] + p2[1]];
    let dd2 = [p1[0] - 2.0 * p2[0] + p3[0], p1[1] - 2.0 * p2[1] + p3[1]];
    let dd = len(dd1).max(len(dd2));
    let tol = tol.max(1e-4);
    let n = ((dd * 0.75 / tol).sqrt()).ceil();
    let n = if n.is_finite() { (n as usize).clamp(1, 160) } else { 16 };
    for k in 1..=n {
        out.push(eval_cubic(p0, p1, p2, p3, k as f32 / n as f32));
    }
}

/// Ломаная (уплощённый контур).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline {
    pub pts: Vec<[f32; 2]>,
    pub closed: bool,
}

impl Polyline {
    pub fn transform(&mut self, m: &Mat) {
        for p in &mut self.pts {
            *p = m.apply(*p);
        }
    }

    pub fn seg_count(&self) -> usize {
        match self.pts.len() {
            0 | 1 => 0,
            n if self.closed => n,
            n => n - 1,
        }
    }

    #[inline]
    fn pt(&self, i: usize) -> [f32; 2] {
        self.pts[i % self.pts.len()]
    }

    pub fn length(&self) -> f32 {
        (0..self.seg_count()).map(|i| dist(self.pt(i), self.pt(i + 1))).sum()
    }

    /// Прямоугольник, охватывающий точки: `[minx, miny, maxx, maxy]`.
    pub fn bbox(&self) -> Option<[f32; 4]> {
        let mut it = self.pts.iter();
        let first = *it.next()?;
        let mut bb = [first[0], first[1], first[0], first[1]];
        for p in it {
            bb[0] = bb[0].min(p[0]);
            bb[1] = bb[1].min(p[1]);
            bb[2] = bb[2].max(p[0]);
            bb[3] = bb[3].max(p[1]);
        }
        Some(bb)
    }

    /// Часть ломаной между длинами `l0..l1` от начала — открытая ломаная.
    pub fn slice(&self, l0: f32, l1: f32) -> Option<Polyline> {
        if l1 <= l0 || self.pts.is_empty() {
            return None;
        }
        let mut out: Vec<[f32; 2]> = Vec::new();
        let mut acc = 0.0f32;
        for s in 0..self.seg_count() {
            let a = self.pt(s);
            let b = self.pt(s + 1);
            let l = dist(a, b);
            if l <= 0.0 {
                continue;
            }
            let next = acc + l;
            if next < l0 {
                acc = next;
                continue;
            }
            if acc > l1 {
                break;
            }
            if out.is_empty() {
                let t = ((l0 - acc) / l).clamp(0.0, 1.0);
                out.push(lerp2(a, b, t));
            }
            if next <= l1 {
                out.push(b);
            } else {
                let t = ((l1 - acc) / l).clamp(0.0, 1.0);
                out.push(lerp2(a, b, t));
                break;
            }
            acc = next;
        }
        if out.len() >= 2 {
            Some(Polyline { pts: out, closed: false })
        } else {
            None
        }
    }

    /// Открытая копия замкнутой ломаной (конец возвращается в начало).
    fn opened(&self) -> Polyline {
        if self.closed && !self.pts.is_empty() {
            let mut pts = self.pts.clone();
            pts.push(self.pts[0]);
            Polyline { pts, closed: false }
        } else {
            self.clone()
        }
    }
}

/// Trim Paths: `start`/`end` в долях 0..1, `offset` в градусах.
/// `individually` — все контуры как одна последовательность (`m: 2`).
pub fn trim_paths(polys: Vec<Polyline>, start: f32, end: f32, offset_deg: f32, individually: bool) -> Vec<Polyline> {
    let mut o = (offset_deg % 360.0) / 360.0;
    if o < 0.0 {
        o += 1.0;
    }
    let mut s = start.clamp(0.0, 1.0) + o;
    let mut e = end.clamp(0.0, 1.0) + o;
    if s > e {
        std::mem::swap(&mut s, &mut e);
    }
    s = (s * 10000.0).round() * 0.0001;
    e = (e * 10000.0).round() * 0.0001;
    if (e - s).abs() < 1e-6 {
        return Vec::new();
    }
    if s <= 0.0 && e >= 1.0 {
        return polys;
    }
    // Отрезки в долях [0,1]; при переходе через 1 — два куска (конец + начало).
    let (seg_a, seg_b): ((f32, f32), Option<(f32, f32)>) = if e <= 1.0 {
        ((s, e), None)
    } else if s >= 1.0 {
        ((s - 1.0, e - 1.0), None)
    } else {
        ((s, 1.0), Some((0.0, e - 1.0)))
    };

    let mut out = Vec::new();
    if !individually {
        for poly in &polys {
            let l = poly.length();
            if l <= 0.0 {
                continue;
            }
            let first = poly.slice(l * seg_a.0, l * seg_a.1);
            match seg_b {
                None => out.extend(first),
                Some(sb) => {
                    let second = poly.slice(l * sb.0, l * sb.1);
                    // Замкнутый контур: хвост и голова стыкуются без разрыва.
                    match (first, second, poly.closed) {
                        (Some(mut a), Some(b), true) => {
                            a.pts.extend_from_slice(&b.pts[1.min(b.pts.len())..]);
                            out.push(a);
                        }
                        (a, b, _) => {
                            out.extend(a);
                            out.extend(b);
                        }
                    }
                }
            }
        }
    } else {
        let total: f32 = polys.iter().map(Polyline::length).sum();
        if total <= 0.0 {
            return Vec::new();
        }
        let mut ranges = vec![(seg_a.0 * total, seg_a.1 * total)];
        if let Some(sb) = seg_b {
            ranges.push((sb.0 * total, sb.1 * total));
        }
        for (r0, r1) in ranges {
            let mut off = 0.0f32;
            for poly in &polys {
                let l = poly.length();
                let a = r0.max(off) - off;
                let b = r1.min(off + l) - off;
                if b > a {
                    out.extend(poly.slice(a, b));
                }
                off += l;
            }
        }
    }
    out
}

/// Пунктир: `pattern` — чередование штрих/пробел, `offset` — сдвиг начала.
pub fn dash_polyline(poly: &Polyline, pattern: &[f32], offset: f32) -> Vec<Polyline> {
    let mut pat: Vec<f32> = pattern.iter().map(|v| v.max(0.0)).collect();
    if pat.len() % 2 == 1 {
        let copy = pat.clone();
        pat.extend(copy);
    }
    let period: f32 = pat.iter().sum();
    if pat.is_empty() || period <= 1e-6 {
        return vec![poly.clone()];
    }
    let open = poly.opened();
    let total = open.length();
    if total <= 0.0 {
        return Vec::new();
    }
    // Фаза: продвигаемся по шаблону на offset.
    let mut phase = offset % period;
    if phase < 0.0 {
        phase += period;
    }
    let mut idx = 0usize;
    while phase >= pat[idx] && phase > 0.0 {
        phase -= pat[idx];
        idx = (idx + 1) % pat.len();
        if phase <= 1e-6 {
            phase = 0.0;
            break;
        }
    }
    let mut out = Vec::new();
    let mut pos = 0.0f32;
    let mut remaining = pat[idx] - phase;
    let mut guard = 0usize;
    while pos < total && guard < 100_000 {
        guard += 1;
        let end = (pos + remaining).min(total);
        if idx.is_multiple_of(2) {
            if end > pos {
                out.extend(open.slice(pos, end));
            } else if remaining <= 0.0 {
                // Штрих нулевой длины — точка (для круглых концов).
                if let Some(p) = open.slice(pos, (pos + 1e-3).min(total)) {
                    out.push(Polyline { pts: vec![p.pts[0]], closed: false });
                }
            }
        }
        pos = end;
        idx = (idx + 1) % pat.len();
        remaining = pat[idx];
    }
    out
}

/// Offset paths: параллельный сдвиг ломаной на `amount` (положительный —
/// наружу у замкнутых). Самопересечения не устраняются.
pub fn offset_polyline(poly: &Polyline, amount: f32, join: LineJoin, miter_limit: f32) -> Polyline {
    let n = poly.pts.len();
    if n < 2 || amount.abs() < 1e-6 {
        return poly.clone();
    }
    let seg_normal = |i: usize| -> [f32; 2] {
        let a = poly.pts[i % n];
        let b = poly.pts[(i + 1) % n];
        let d = sub(b, a);
        let l = len(d);
        if l > 0.0 { [-d[1] / l, d[0] / l] } else { [0.0, 0.0] }
    };
    let offset_with = |k: f32| -> Vec<[f32; 2]> {
        (0..n)
            .map(|i| {
                let v = poly.pts[i];
                let (n0, n1) = if poly.closed {
                    (seg_normal((i + n - 1) % n), seg_normal(i))
                } else if i == 0 {
                    (seg_normal(0), seg_normal(0))
                } else if i == n - 1 {
                    (seg_normal(n - 2), seg_normal(n - 2))
                } else {
                    (seg_normal(i - 1), seg_normal(i))
                };
                let dot = (n0[0] * n1[0] + n0[1] * n1[1]).clamp(-1.0, 1.0);
                let avg = [n0[0] + n1[0], n0[1] + n1[1]];
                let al = len(avg);
                if al < 1e-6 {
                    return add(v, mulf(n1, amount * k));
                }
                // Митра: длина 1/cos(θ/2), с ограничением (как Bevel при превышении).
                let mut m = 1.0 / ((0.5 * (1.0 + dot)).max(1e-4)).sqrt();
                let limit = if join == LineJoin::Miter { miter_limit.max(1.0) } else { 1.5 };
                m = m.min(limit);
                add(v, mulf([avg[0] / al, avg[1] / al], amount * k * m))
            })
            .collect()
    };
    let area = |pts: &[[f32; 2]]| -> f32 {
        let mut a = 0.0;
        for i in 0..pts.len() {
            let p = pts[i];
            let q = pts[(i + 1) % pts.len()];
            a += p[0] * q[1] - q[0] * p[1];
        }
        a * 0.5
    };
    let mut pts = offset_with(1.0);
    if poly.closed {
        // Наружу — туда, где площадь растёт при amount > 0.
        let grow = area(&pts).abs() >= area(&poly.pts).abs();
        if grow != (amount > 0.0) {
            pts = offset_with(-1.0);
        }
    }
    Polyline { pts, closed: poly.closed }
}

/// Zig Zag: `ridges` зубцов на сегмент контура, амплитуда `size`;
/// вершины остаются на контуре, пики смещаются по нормали поочерёдно.
pub fn zigzag_path(sp: &ShapePath, size: f32, ridges: f32, out_tol: f32) -> Polyline {
    let n = sp.verts.len();
    let segs = if sp.closed { n } else { n.saturating_sub(1) };
    let mut out = Vec::new();
    if n == 0 {
        return Polyline { pts: out, closed: sp.closed };
    }
    let r = ridges.round().max(1.0) as usize;
    let steps = 2 * r; // пики на нечётных отсчётах
    let _ = out_tol;
    let mut flip = 1.0f32;
    out.push(sp.verts[0]);
    for i in 0..segs {
        let j = (i + 1) % n;
        let p0 = sp.verts[i];
        let p3 = sp.verts[j];
        let p1 = add(p0, sp.outs[i]);
        let p2 = add(p3, sp.ins[j]);
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let pos = eval_cubic(p0, p1, p2, p3, t);
            if k == steps {
                out.push(p3);
            } else if k % 2 == 1 {
                // Нормаль из производной кривой.
                let mt = 1.0 - t;
                let d = [
                    3.0 * mt * mt * (p1[0] - p0[0]) + 6.0 * mt * t * (p2[0] - p1[0]) + 3.0 * t * t * (p3[0] - p2[0]),
                    3.0 * mt * mt * (p1[1] - p0[1]) + 6.0 * mt * t * (p2[1] - p1[1]) + 3.0 * t * t * (p3[1] - p2[1]),
                ];
                let l = len(d).max(1e-6);
                let nrm = [-d[1] / l, d[0] / l];
                out.push(add(pos, mulf(nrm, size * flip)));
                flip = -flip;
            } else {
                out.push(pos);
            }
        }
    }
    if sp.closed && out.len() > 1 {
        out.pop();
    }
    Polyline { pts: out, closed: sp.closed }
}

/// Pucker & Bloat: `amount` в процентах; положительный тянет вершины к
/// центру, а касательные — от центра (отрицательный — наоборот).
pub fn pucker_bloat(sp: &ShapePath, amount_percent: f32) -> ShapePath {
    let n = sp.verts.len();
    if n == 0 || amount_percent.abs() < 1e-6 {
        return sp.clone();
    }
    let p = amount_percent / 100.0;
    let mut c = [0.0f32, 0.0];
    for v in &sp.verts {
        c = add(c, *v);
    }
    c = mulf(c, 1.0 / n as f32);
    let mut out = ShapePath::with_capacity(n, sp.closed);
    for i in 0..n {
        let v = sp.verts[i];
        let nv = add(v, mulf(sub(c, v), p));
        // Абсолютные позиции касательных двигаются от центра.
        let hi = add(v, sp.ins[i]);
        let ho = add(v, sp.outs[i]);
        let nhi = add(hi, mulf(sub(hi, c), p));
        let nho = add(ho, mulf(sub(ho, c), p));
        out.push(nv, sub(nhi, nv), sub(nho, nv));
    }
    out
}

/// Twist: поворот точек вокруг `center`; угол максимален в центре и
/// спадает к нулю на краю (как в After Effects).
pub fn twist_polyline(poly: &mut Polyline, angle_deg: f32, center: [f32; 2]) {
    if angle_deg.abs() < 1e-6 || poly.pts.is_empty() {
        return;
    }
    let maxd = poly
        .pts
        .iter()
        .map(|p| dist(*p, center))
        .fold(0.0f32, f32::max)
        .max(1e-6);
    for p in &mut poly.pts {
        let d = dist(*p, center);
        let theta = (angle_deg * (1.0 - d / maxd)).to_radians();
        let (sn, cs) = theta.sin_cos();
        let r = sub(*p, center);
        *p = add(center, [r[0] * cs - r[1] * sn, r[0] * sn + r[1] * cs]);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

fn push_polygon(out: &mut Vec<Polyline>, mut pts: Vec<[f32; 2]>) {
    if pts.len() < 3 {
        return;
    }
    // Единая ориентация всех кусков: иначе nonzero даст дыры в пересечениях.
    let mut area = 0.0f32;
    for i in 0..pts.len() {
        let p = pts[i];
        let q = pts[(i + 1) % pts.len()];
        area += p[0] * q[1] - q[0] * p[1];
    }
    if area.abs() < 1e-9 {
        return;
    }
    if area < 0.0 {
        pts.reverse();
    }
    out.push(Polyline { pts, closed: true });
}

fn circle_polygon(c: [f32; 2], r: f32, tol: f32) -> Vec<[f32; 2]> {
    let n = if r <= tol {
        8
    } else {
        let step = (1.0 - tol / r).clamp(-1.0, 1.0).acos();
        ((std::f32::consts::TAU / step.max(1e-3)).ceil() as usize).clamp(8, 96)
    };
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

/// Обводка ломаной → набор замкнутых полигонов (заливать по nonzero).
/// `tol` — допуск уплощения круглых соединений.
pub fn stroke_polyline(
    poly: &Polyline,
    width: f32,
    cap: LineCap,
    join: LineJoin,
    miter_limit: f32,
    tol: f32,
    out: &mut Vec<Polyline>,
) {
    let hw = width * 0.5;
    if hw <= 0.0 {
        return;
    }
    // Убираем повторяющиеся точки.
    let mut pts: Vec<[f32; 2]> = Vec::with_capacity(poly.pts.len());
    for p in &poly.pts {
        if pts.last().map(|l| dist(*l, *p) > 1e-6).unwrap_or(true) {
            pts.push(*p);
        }
    }
    let mut closed = poly.closed;
    if closed && pts.len() > 1 && dist(pts[0], pts[pts.len() - 1]) <= 1e-6 {
        pts.pop();
    }
    if pts.len() < 3 {
        closed = false;
    }
    if pts.is_empty() {
        return;
    }
    if pts.len() == 1 {
        // Точка: видна только с круглыми/квадратными концами.
        let c = pts[0];
        match cap {
            LineCap::Round => push_polygon(out, circle_polygon(c, hw, tol)),
            LineCap::Square => push_polygon(
                out,
                vec![
                    [c[0] - hw, c[1] - hw],
                    [c[0] + hw, c[1] - hw],
                    [c[0] + hw, c[1] + hw],
                    [c[0] - hw, c[1] + hw],
                ],
            ),
            LineCap::Butt => {}
        }
        return;
    }
    let n = pts.len();
    let segs = if closed { n } else { n - 1 };
    let dir = |i: usize| -> [f32; 2] {
        let a = pts[i % n];
        let b = pts[(i + 1) % n];
        let d = sub(b, a);
        let l = len(d);
        if l > 0.0 { mulf(d, 1.0 / l) } else { [1.0, 0.0] }
    };
    // Прямоугольники сегментов.
    for i in 0..segs {
        let a = pts[i % n];
        let b = pts[(i + 1) % n];
        let d = dir(i);
        let nrm = [-d[1] * hw, d[0] * hw];
        push_polygon(out, vec![add(a, nrm), add(b, nrm), sub(b, nrm), sub(a, nrm)]);
    }
    // Соединения.
    let join_at = |i: usize, out: &mut Vec<Polyline>| {
        let d0 = dir(i + n - 1);
        let d1 = dir(i);
        let v = pts[i % n];
        let cross = d0[0] * d1[1] - d0[1] * d1[0];
        let dot = d0[0] * d1[0] + d0[1] * d1[1];
        if cross.abs() < 1e-6 && dot > 0.0 {
            return; // прямое продолжение
        }
        match join {
            LineJoin::Round => push_polygon(out, circle_polygon(v, hw, tol)),
            LineJoin::Bevel | LineJoin::Miter => {
                let s = if cross > 0.0 { -1.0 } else { 1.0 };
                let n0 = mulf([-d0[1], d0[0]], hw * s);
                let n1 = mulf([-d1[1], d1[0]], hw * s);
                let p0 = add(v, n0);
                let p1 = add(v, n1);
                let use_miter = join == LineJoin::Miter && {
                    // Отношение длины митры к ширине: 1/cos(θ/2).
                    let cos_half = ((1.0 + dot) * 0.5).max(0.0).sqrt();
                    cos_half > 1e-4 && 1.0 / cos_half <= miter_limit.max(1.0)
                };
                if use_miter {
                    let k = 1.0 / (1.0 + dot);
                    let tip = add(v, mulf(add(n0, n1), k));
                    push_polygon(out, vec![v, p0, tip, p1]);
                } else {
                    push_polygon(out, vec![v, p0, p1]);
                }
            }
        }
    };
    if closed {
        for i in 0..n {
            join_at(i, out);
        }
    } else {
        for i in 1..n - 1 {
            join_at(i, out);
        }
        // Концы.
        let cap_at = |p: [f32; 2], d: [f32; 2], out: &mut Vec<Polyline>| match cap {
            LineCap::Butt => {}
            LineCap::Round => push_polygon(out, circle_polygon(p, hw, tol)),
            LineCap::Square => {
                let nrm = [-d[1] * hw, d[0] * hw];
                let ext = mulf(d, hw);
                push_polygon(
                    out,
                    vec![add(p, nrm), add(add(p, nrm), ext), add(sub(p, nrm), ext), sub(p, nrm)],
                );
            }
        };
        cap_at(pts[0], mulf(dir(0), -1.0), out);
        cap_at(pts[n - 1], dir(n - 2), out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_inverse_roundtrip() {
        let m = compose_transform([10.0, 20.0], [100.0, 50.0], [2.0, 0.5], 30.0, 10.0, 45.0);
        let inv = m.invert().unwrap();
        let p = [3.0, -7.0];
        let q = inv.apply(m.apply(p));
        assert!((q[0] - p[0]).abs() < 1e-3 && (q[1] - p[1]).abs() < 1e-3);
        // Поворот на 90°: (1,0) → (0,1) — по часовой при y вниз.
        let r = Mat::rotate(90.0).apply([1.0, 0.0]);
        assert!(r[0].abs() < 1e-5 && (r[1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn trim_simultaneous_and_wrap() {
        let sq = Polyline {
            pts: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            closed: true,
        };
        let t = trim_paths(vec![sq.clone()], 0.0, 0.5, 0.0, false);
        assert_eq!(t.len(), 1);
        assert!((t[0].length() - 20.0).abs() < 1e-4);
        // Переход через начало (offset 90° = +0.25): один непрерывный кусок длиной 20.
        let t = trim_paths(vec![sq], 0.5, 1.0, 90.0, false);
        assert_eq!(t.len(), 1);
        assert!((t[0].length() - 20.0).abs() < 1e-3, "{}", t[0].length());
    }

    #[test]
    fn dashes_cover_half() {
        let line = Polyline { pts: vec![[0.0, 0.0], [100.0, 0.0]], closed: false };
        let d = dash_polyline(&line, &[10.0, 10.0], 0.0);
        assert_eq!(d.len(), 5);
        let total: f32 = d.iter().map(Polyline::length).sum();
        assert!((total - 50.0).abs() < 1e-3);
    }

    #[test]
    fn stroke_rect_pieces_oriented() {
        let line = Polyline { pts: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]], closed: false };
        let mut out = Vec::new();
        stroke_polyline(&line, 2.0, LineCap::Square, LineJoin::Miter, 4.0, 0.25, &mut out);
        // 2 сегмента + 1 митра + 2 конца.
        assert_eq!(out.len(), 5);
    }
}
