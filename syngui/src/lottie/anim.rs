//! Анимированные свойства Lottie: статическое значение или ключевые кадры
//! с кубическим безье-easing'ом (`i`/`o`), удержанием (`h`) и
//! пространственными кривыми позиции (`to`/`ti`, параметризация по длине дуги).

use super::geom::ShapePath;
use super::json::Json;

/// Значение, которое умеет интерполироваться между ключевыми кадрами.
/// `ts` — прогресс по измерениям (для многомерных свойств у каждого
/// измерения может быть свой easing); при нехватке берётся последний.
pub trait Interpolate: Clone {
    fn lerp(a: &Self, b: &Self, ts: &[f32]) -> Self;
}

#[inline]
fn t_for(ts: &[f32], dim: usize) -> f32 {
    ts.get(dim).or_else(|| ts.last()).copied().unwrap_or(0.0)
}

impl Interpolate for f32 {
    fn lerp(a: &Self, b: &Self, ts: &[f32]) -> Self {
        let t = t_for(ts, 0);
        a + (b - a) * t
    }
}

impl Interpolate for Vec<f32> {
    fn lerp(a: &Self, b: &Self, ts: &[f32]) -> Self {
        a.iter()
            .enumerate()
            .map(|(i, av)| {
                let bv = b.get(i).copied().unwrap_or(*av);
                av + (bv - av) * t_for(ts, i)
            })
            .collect()
    }
}

impl Interpolate for ShapePath {
    fn lerp(a: &Self, b: &Self, ts: &[f32]) -> Self {
        if a.verts.len() != b.verts.len() {
            // Разное число вершин: интерполяция невозможна, держим начало.
            return a.clone();
        }
        let t = t_for(ts, 0);
        let lp = |p: [f32; 2], q: [f32; 2]| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
        ShapePath {
            verts: a.verts.iter().zip(&b.verts).map(|(p, q)| lp(*p, *q)).collect(),
            ins: a.ins.iter().zip(&b.ins).map(|(p, q)| lp(*p, *q)).collect(),
            outs: a.outs.iter().zip(&b.outs).map(|(p, q)| lp(*p, *q)).collect(),
            closed: a.closed,
        }
    }
}

/// Разбор значения свойства из JSON.
pub trait FromJson: Sized {
    fn from_json(v: &Json) -> Option<Self>;
}

impl FromJson for f32 {
    fn from_json(v: &Json) -> Option<Self> {
        match v {
            Json::Arr(a) => a.first().and_then(Json::as_f32),
            other => other.as_f32(),
        }
    }
}

impl FromJson for Vec<f32> {
    fn from_json(v: &Json) -> Option<Self> {
        match v {
            Json::Arr(a) => Some(a.iter().map(|x| x.as_f32().unwrap_or(0.0)).collect()),
            Json::Num(n) => Some(vec![*n as f32]),
            _ => None,
        }
    }
}

impl FromJson for ShapePath {
    fn from_json(v: &Json) -> Option<Self> {
        let obj = match v {
            Json::Arr(a) => a.first()?,
            other => other,
        };
        if !obj.is_object() {
            return None;
        }
        let pts = |key: &str| -> Vec<[f32; 2]> {
            obj.get(key)
                .and_then(Json::as_array)
                .map(|arr| {
                    arr.iter()
                        .map(|p| {
                            let n = p.num_array();
                            [n.first().copied().unwrap_or(0.0), n.get(1).copied().unwrap_or(0.0)]
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let verts = pts("v");
        let mut ins = pts("i");
        let mut outs = pts("o");
        ins.resize(verts.len(), [0.0, 0.0]);
        outs.resize(verts.len(), [0.0, 0.0]);
        Some(ShapePath {
            verts,
            ins,
            outs,
            closed: obj.bool_or("c", false),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Keyframe<T> {
    pub time: f32,
    pub start: T,
    /// Явное конечное значение (старый формат `e`); иначе — `start` следующего кадра.
    pub end: Option<T>,
    pub hold: bool,
    /// Контрольные точки easing'а: выход `o` и вход `i`, по измерениям.
    pub ease_out: Vec<[f32; 2]>,
    pub ease_in: Vec<[f32; 2]>,
    /// Пространственные касательные позиции (относительно start/end).
    pub to: Option<[f32; 2]>,
    pub ti: Option<[f32; 2]>,
}

#[derive(Clone, Debug)]
pub enum Animated<T> {
    Static(T),
    Keyframes(Vec<Keyframe<T>>),
}

impl<T: Clone> Animated<T> {
    pub fn is_animated(&self) -> bool {
        matches!(self, Animated::Keyframes(k) if k.len() > 1)
    }

    /// Преобразовать все значения (нормализация цветов и т. п.).
    pub fn map<F: FnMut(&T) -> T>(&self, mut f: F) -> Animated<T> {
        match self {
            Animated::Static(v) => Animated::Static(f(v)),
            Animated::Keyframes(kfs) => Animated::Keyframes(
                kfs.iter()
                    .map(|k| Keyframe {
                        time: k.time,
                        start: f(&k.start),
                        end: k.end.as_ref().map(&mut f),
                        hold: k.hold,
                        ease_out: k.ease_out.clone(),
                        ease_in: k.ease_in.clone(),
                        to: k.to,
                        ti: k.ti,
                    })
                    .collect(),
            ),
        }
    }

    pub fn first_value(&self) -> &T {
        match self {
            Animated::Static(v) => v,
            Animated::Keyframes(k) => &k[0].start,
        }
    }
}

/// Найти текущий интервал ключевых кадров: `None` — до первого / после последнего.
fn segment<T>(kfs: &[Keyframe<T>], frame: f32) -> Result<(usize, f32), &T> {
    if frame <= kfs[0].time {
        return Err(&kfs[0].start);
    }
    let last = kfs.len() - 1;
    if frame >= kfs[last].time {
        return Err(&kfs[last].start);
    }
    // Бинарный поиск по времени.
    let mut lo = 0usize;
    let mut hi = last;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if kfs[mid].time <= frame {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let span = kfs[hi].time - kfs[lo].time;
    let p = if span > 0.0 { ((frame - kfs[lo].time) / span).clamp(0.0, 1.0) } else { 1.0 };
    Ok((lo, p))
}

/// Прогресс по измерениям с учётом easing'а кадра.
fn eased_progress(k: &Keyframe<impl Clone>, p: f32, out: &mut [f32; 8]) -> usize {
    let dims = k.ease_out.len().max(k.ease_in.len()).clamp(1, 8);
    for (d, slot) in out.iter_mut().enumerate().take(dims) {
        let o = k
            .ease_out
            .get(d)
            .or_else(|| k.ease_out.last())
            .copied()
            .unwrap_or([0.0, 0.0]);
        let i = k
            .ease_in
            .get(d)
            .or_else(|| k.ease_in.last())
            .copied()
            .unwrap_or([1.0, 1.0]);
        *slot = cubic_bezier_ease(o[0], o[1], i[0], i[1], p);
    }
    dims
}

impl<T: Interpolate> Animated<T> {
    pub fn value(&self, frame: f32) -> T {
        match self {
            Animated::Static(v) => v.clone(),
            Animated::Keyframes(kfs) => match segment(kfs, frame) {
                Err(v) => v.clone(),
                Ok((i, p)) => {
                    let k = &kfs[i];
                    if k.hold {
                        return k.start.clone();
                    }
                    let end = k.end.as_ref().unwrap_or(&kfs[i + 1].start);
                    let mut ts = [0f32; 8];
                    let n = eased_progress(k, p, &mut ts);
                    T::lerp(&k.start, end, &ts[..n])
                }
            },
        }
    }
}

impl Animated<Vec<f32>> {
    /// Позиция с учётом пространственных касательных `to`/`ti`
    /// (движение по кривой, равномерное по длине дуги).
    pub fn position(&self, frame: f32) -> [f32; 2] {
        let two = |v: &Vec<f32>| [v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0)];
        match self {
            Animated::Static(v) => two(v),
            Animated::Keyframes(kfs) => match segment(kfs, frame) {
                Err(v) => two(v),
                Ok((i, p)) => {
                    let k = &kfs[i];
                    if k.hold {
                        return two(&k.start);
                    }
                    let end = k.end.as_ref().unwrap_or(&kfs[i + 1].start);
                    let a = two(&k.start);
                    let b = two(end);
                    let mut ts = [0f32; 8];
                    let n = eased_progress(k, p, &mut ts);
                    let (to, ti) = match (k.to, k.ti) {
                        (Some(to), Some(ti)) if to != [0.0, 0.0] || ti != [0.0, 0.0] => (to, ti),
                        _ => {
                            return [a[0] + (b[0] - a[0]) * ts[0], a[1] + (b[1] - a[1]) * ts[1.min(n - 1)]];
                        }
                    };
                    let c1 = [a[0] + to[0], a[1] + to[1]];
                    let c2 = [b[0] + ti[0], b[1] + ti[1]];
                    point_on_cubic_by_length(a, c1, c2, b, ts[0])
                }
            },
        }
    }
}

/// Точка на кубической кривой на доле `frac` её длины.
fn point_on_cubic_by_length(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], frac: f32) -> [f32; 2] {
    const N: usize = 32;
    let eval = |t: f32| -> [f32; 2] {
        let mt = 1.0 - t;
        let a = mt * mt * mt;
        let b = 3.0 * mt * mt * t;
        let c = 3.0 * mt * t * t;
        let d = t * t * t;
        [
            a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
            a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
        ]
    };
    let mut lens = [0f32; N + 1];
    let mut prev = p0;
    for i in 1..=N {
        let q = eval(i as f32 / N as f32);
        lens[i] = lens[i - 1] + ((q[0] - prev[0]).powi(2) + (q[1] - prev[1]).powi(2)).sqrt();
        prev = q;
    }
    let total = lens[N];
    if total <= 0.0 {
        return eval(frac);
    }
    let target = frac.clamp(0.0, 1.0) * total;
    let mut i = 1;
    while i < N && lens[i] < target {
        i += 1;
    }
    let seg = lens[i] - lens[i - 1];
    let local = if seg > 0.0 { (target - lens[i - 1]) / seg } else { 0.0 };
    eval(((i - 1) as f32 + local) / N as f32)
}

/// CSS-подобный cubic-bezier(x1,y1,x2,y2): по оси X — время, по Y — значение.
pub fn cubic_bezier_ease(x1: f32, y1: f32, x2: f32, y2: f32, p: f32) -> f32 {
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return 1.0;
    }
    // Линейный случай — без численного решения.
    if (x1 - y1).abs() < 1e-6 && (x2 - y2).abs() < 1e-6 {
        return p;
    }
    let x1 = x1.clamp(0.0, 1.0);
    let x2 = x2.clamp(0.0, 1.0);
    let bx = |t: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * x1 + 3.0 * mt * t * t * x2 + t * t * t
    };
    let by = |t: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * y1 + 3.0 * mt * t * t * y2 + t * t * t
    };
    let dbx = |t: f32| {
        let mt = 1.0 - t;
        3.0 * mt * mt * x1 + 6.0 * mt * t * (x2 - x1) + 3.0 * t * t * (1.0 - x2)
    };
    // Ньютон с бисекцией как страховкой.
    let mut t = p;
    for _ in 0..8 {
        let err = bx(t) - p;
        if err.abs() < 1e-5 {
            return by(t);
        }
        let d = dbx(t);
        if d.abs() < 1e-6 {
            break;
        }
        t -= err / d;
        if !(0.0..=1.0).contains(&t) {
            break;
        }
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    t = p;
    for _ in 0..24 {
        let x = bx(t);
        if (x - p).abs() < 1e-5 {
            break;
        }
        if x < p {
            lo = t;
        } else {
            hi = t;
        }
        t = 0.5 * (lo + hi);
    }
    by(t)
}

/// Разбор пары `{x, y}` easing'а: числа или массивы по измерениям.
fn parse_ease(v: Option<&Json>) -> Vec<[f32; 2]> {
    let Some(v) = v else {
        return Vec::new();
    };
    let xs = v.get("x").map(Json::num_array).unwrap_or_default();
    let ys = v.get("y").map(Json::num_array).unwrap_or_default();
    let n = xs.len().max(ys.len());
    (0..n)
        .map(|i| {
            [
                xs.get(i).or_else(|| xs.last()).copied().unwrap_or(0.0),
                ys.get(i).or_else(|| ys.last()).copied().unwrap_or(0.0),
            ]
        })
        .collect()
}

fn parse_pair(v: Option<&Json>) -> Option<[f32; 2]> {
    let n = v?.num_array();
    if n.is_empty() {
        return None;
    }
    Some([n[0], n.get(1).copied().unwrap_or(0.0)])
}

impl<T: FromJson + Clone> Animated<T> {
    /// Разобрать свойство `{a, k}`; `None`, если ключа нет или формат неизвестен.
    pub fn parse(prop: Option<&Json>) -> Option<Self> {
        let prop = prop?;
        // Иногда вместо объекта лежит сырое значение.
        let k = match prop.get("k") {
            Some(k) => k,
            None => return T::from_json(prop).map(Animated::Static),
        };
        let is_keyframes = matches!(k, Json::Arr(items) if items.first().map(|f| f.is_object() && f.get("t").is_some()).unwrap_or(false));
        if !is_keyframes {
            return T::from_json(k).map(Animated::Static);
        }
        let items = k.as_array()?;
        let mut kfs: Vec<Keyframe<T>> = Vec::with_capacity(items.len());
        let mut pending_start: Option<T> = None;
        for it in items {
            let time = it.f32_or("t", 0.0);
            let start = it.get("s").and_then(T::from_json).or_else(|| pending_start.take());
            let end = it.get("e").and_then(T::from_json);
            let Some(start) = start else {
                // Последний кадр старого формата без значения: интервал закрыт `e` предыдущего.
                continue;
            };
            pending_start = end.clone();
            kfs.push(Keyframe {
                time,
                start,
                end,
                hold: it.i64_or("h", 0) == 1,
                ease_out: parse_ease(it.get("o")),
                ease_in: parse_ease(it.get("i")),
                to: parse_pair(it.get("to")),
                ti: parse_pair(it.get("ti")),
            });
        }
        // Явный `e` последнего кадра — финальное значение после него.
        if let Some(last) = kfs.last() {
            if let Some(e) = last.end.clone() {
                if items.len() == kfs.len() {
                    // Все кадры со `s`, но последний имеет `e`: держим e после конца.
                    let t = last.time;
                    kfs.push(Keyframe {
                        time: t + 1e-3,
                        start: e,
                        end: None,
                        hold: true,
                        ease_out: Vec::new(),
                        ease_in: Vec::new(),
                        to: None,
                        ti: None,
                    });
                }
            }
        }
        if kfs.is_empty() {
            return None;
        }
        kfs.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
        Some(Animated::Keyframes(kfs))
    }

    pub fn parse_or(prop: Option<&Json>, default: T) -> Self {
        Self::parse(prop).unwrap_or(Animated::Static(default))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_endpoints_and_linear() {
        assert_eq!(cubic_bezier_ease(0.3, 0.3, 0.7, 0.7, 0.42), 0.42);
        assert!((cubic_bezier_ease(0.42, 0.0, 0.58, 1.0, 0.5) - 0.5).abs() < 1e-3);
        assert!(cubic_bezier_ease(0.42, 0.0, 1.0, 1.0, 0.25) < 0.25);
    }

    #[test]
    fn keyframes_legacy_end() {
        let j = super::super::json::parse(
            r#"{"a":1,"k":[{"t":0,"s":[0],"e":[10],"i":{"x":[1],"y":[1]},"o":{"x":[0],"y":[0]}},{"t":10}]}"#,
        )
        .unwrap();
        let a: Animated<f32> = Animated::parse(Some(&j)).unwrap();
        assert_eq!(a.value(-5.0), 0.0);
        assert!((a.value(5.0) - 5.0).abs() < 1e-4);
        assert_eq!(a.value(20.0), 10.0);
    }

    #[test]
    fn hold_keyframes() {
        let j = super::super::json::parse(r#"{"a":1,"k":[{"t":0,"s":[1],"h":1},{"t":10,"s":[2],"h":1}]}"#).unwrap();
        let a: Animated<f32> = Animated::parse(Some(&j)).unwrap();
        assert_eq!(a.value(9.9), 1.0);
        assert_eq!(a.value(10.0), 2.0);
    }
}
