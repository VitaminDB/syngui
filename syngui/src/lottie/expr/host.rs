//! Объекты After Effects для выражений поверх модели Lottie: композиция,
//! слои, трансформации, содержимое shape-слоёв, эффекты и анимируемые
//! свойства (значение, ключи, `valueAtTime`, `velocityAtTime`, `loopOut`,
//! `wiggle`…).
//!
//! Ссылки на модель — сырые указатели: выражение вычисляется синхронно,
//! пока рендер держит композицию (контекст ставится [`enter`] на время
//! отрисовки слоя), и значения-ссылки не переживают вычисление.

use std::cell::RefCell;

use super::interp::{
    arr, as_nums, define, err, native, num_arr, obj, to_num, to_str, Env, Interp, Value, R,
};
use crate::lottie::anim::Animated;
use crate::lottie::geom::ShapePath;
use crate::lottie::model::{Composition, EffectCtl, Layer, LayerKind, Position, Shape, Transform};

/// Слой, свойства которого сейчас вычисляются.
#[derive(Clone, Copy)]
pub struct LayerCtx {
    comp: *const Composition,
    layers: *const [Layer],
    idx: usize,
}

thread_local! {
    static STACK: RefCell<Vec<LayerCtx>> = const { RefCell::new(Vec::new()) };
}

/// Снимает контекст слоя при выходе из области.
pub struct CtxGuard(());

impl Drop for CtxGuard {
    fn drop(&mut self) {
        STACK.with(|s| {
            s.borrow_mut().pop();
        });
    }
}

/// Поставить контекст: композиция, список слоёв и слой `idx` в нём.
/// Композиция и слои должны жить, пока жив страж.
pub fn enter(comp: &Composition, layers: &[Layer], idx: usize) -> CtxGuard {
    STACK.with(|s| s.borrow_mut().push(LayerCtx { comp, layers, idx }));
    CtxGuard(())
}

pub fn current() -> Option<LayerCtx> {
    STACK.with(|s| s.borrow().last().copied())
}

/// Частота кадров композиции из контекста.
pub fn current_fps() -> Option<f32> {
    current().map(|c| unsafe { (*c.comp).frame_rate })
}

/// Ссылка на анимируемое свойство модели.
#[derive(Clone, Copy)]
pub enum PropRef {
    None,
    F32(*const Animated<f32>),
    Vec(*const Animated<Vec<f32>>),
    Path(*const Animated<ShapePath>),
    /// Раздельная позиция (`x`, `y`).
    Split(*const Animated<f32>, *const Animated<f32>),
}

fn vec_value(v: &[f32]) -> Value {
    if v.len() == 1 {
        Value::Num(v[0] as f64)
    } else {
        arr(v.iter().map(|x| Value::Num(*x as f64)).collect())
    }
}

impl PropRef {
    /// Значение на кадре (с выражением свойства, если оно есть).
    pub fn value_at(&self, frame: f32) -> Value {
        // SAFETY: см. описание модуля — указатели живы во время вычисления.
        unsafe {
            match *self {
                PropRef::None => Value::Undef,
                PropRef::F32(p) => Value::Num((*p).value(frame) as f64),
                PropRef::Vec(p) => vec_value(&(*p).value(frame)),
                PropRef::Path(p) => Value::Path((*p).value(frame).into()),
                PropRef::Split(x, y) => {
                    num_arr(&[(*x).value(frame) as f64, (*y).value(frame) as f64])
                }
            }
        }
    }

    /// Значение по ключевым кадрам, без выражения.
    pub fn base_at(&self, frame: f32) -> Value {
        unsafe {
            match *self {
                PropRef::None => Value::Undef,
                PropRef::F32(p) => Value::Num((*p).base().value(frame) as f64),
                PropRef::Vec(p) => vec_value(&(*p).base().value(frame)),
                PropRef::Path(p) => Value::Path((*p).base().value(frame).into()),
                PropRef::Split(x, y) => num_arr(&[
                    (*x).base().value(frame) as f64,
                    (*y).base().value(frame) as f64,
                ]),
            }
        }
    }

    /// Времена ключевых кадров (в кадрах).
    pub fn key_times(&self) -> Vec<f32> {
        unsafe {
            match *self {
                PropRef::None => Vec::new(),
                PropRef::F32(p) => (*p).key_times(),
                PropRef::Vec(p) => (*p).key_times(),
                PropRef::Path(p) => (*p).key_times(),
                PropRef::Split(x, _) => (*x).key_times(),
            }
        }
    }
}

/// Объект After Effects.
#[derive(Clone, Copy)]
pub enum Host {
    Comp(*const [Layer]),
    Layer(*const [Layer], usize),
    Transform(*const Transform),
    Shape(*const Shape),
    /// Содержимое shape-слоя или группы.
    Content(*const [Shape], *const [String]),
    Effect(*const EffectCtl),
    Prop(PropRef),
    /// Маркеры композиции.
    Markers(*const Composition),
    /// Текст слоя (`text.sourceText`).
    Text(*const [Layer], usize),
}

impl Host {
    pub fn kind(&self) -> &'static str {
        match self {
            Host::Comp(_) => "Comp",
            Host::Layer(..) => "Layer",
            Host::Transform(_) => "Transform",
            Host::Shape(_) | Host::Content(..) => "PropertyGroup",
            Host::Effect(_) => "Effect",
            Host::Prop(_) => "Property",
            Host::Markers(_) => "MarkerProperty",
            Host::Text(..) => "TextProperty",
        }
    }
}

/// Контекст вычисляемого свойства.
pub struct EvalFrame {
    pub frame: f32,
    pub fps: f32,
    pub value: Value,
    pub prop: PropRef,
    pub ctx: Option<LayerCtx>,
    rng: u64,
    rng_base: u64,
}

fn mix(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

impl EvalFrame {
    pub fn new(frame: f32, fps: f32, value: Value, prop: PropRef, seed: u64) -> Self {
        let ctx = current();
        EvalFrame {
            frame,
            fps,
            value,
            prop,
            ctx,
            // Как в After Effects: random() меняется от кадра к кадру.
            rng: mix(seed ^ mix(frame.to_bits() as u64)),
            rng_base: mix(seed),
        }
    }

    pub fn time(&self) -> f64 {
        self.frame as f64 / self.fps as f64
    }

    pub fn next_random(&mut self) -> f64 {
        self.rng = self.rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        (mix(self.rng) >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn seed_random(&mut self, seed: f64, timeless: bool) {
        let base = mix(seed.to_bits());
        self.rng = if timeless {
            base
        } else {
            mix(base ^ mix(self.frame.to_bits() as u64))
        };
    }

    fn comp(&self) -> Option<&Composition> {
        self.ctx.map(|c| unsafe { &*c.comp })
    }
}

fn str_arg(it: &mut Interp, a: &[Value], i: usize) -> Value {
    let v = a.get(i).cloned().unwrap_or(Value::Undef);
    it.deref(v)
}

/// Свойство текущего вызова: `this`-свойство или свойство выражения.
fn prop_of(it: &Interp, this: &Value) -> PropRef {
    match this {
        Value::Host(Host::Prop(p)) => *p,
        _ => it.frame.prop,
    }
}

/// Слой по имени или номеру (с 1, как `ind`).
fn find_layer(layers: &[Layer], key: &Value) -> Option<usize> {
    match key {
        Value::Num(n) => {
            let n = *n as i64;
            layers
                .iter()
                .position(|l| l.index == n)
                .or_else(|| (n >= 1 && (n as usize) <= layers.len()).then(|| n as usize - 1))
        }
        other => {
            let name = to_str(other);
            layers.iter().position(|l| l.name == name)
        }
    }
}

fn find_named<'a, T>(items: &'a [T], names: &[String], key: &Value) -> Option<&'a T> {
    match key {
        Value::Num(n) => {
            let i = *n as usize;
            (i >= 1).then(|| items.get(i - 1)).flatten()
        }
        other => {
            let name = to_str(other);
            names
                .iter()
                .position(|n| *n == name)
                .and_then(|i| items.get(i))
        }
    }
}

// --- методы-функции -------------------------------------------------------

fn h_layer(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let key = str_arg(it, a, 0);
    let Value::Host(Host::Comp(layers)) = this else {
        return err("layer(): не композиция");
    };
    let ls = unsafe { &**layers };
    match find_layer(ls, &key) {
        Some(i) => Ok(Value::Host(Host::Layer(*layers, i))),
        None => err(format!("слой `{}` не найден", to_str(&key))),
    }
}

fn layer_content(layer: &Layer) -> Option<Host> {
    match &layer.kind {
        LayerKind::Shape(items) => Some(Host::Content(
            items.as_slice() as *const [Shape],
            layer.shape_names.as_slice() as *const [String],
        )),
        _ => None,
    }
}

fn h_content(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let key = str_arg(it, a, 0);
    let content = match this {
        Value::Host(Host::Layer(ls, i)) => layer_content(unsafe { &(**ls)[*i] }),
        Value::Host(Host::Shape(s)) => match unsafe { &**s } {
            Shape::Group {
                items, item_names, ..
            } => Some(Host::Content(items.as_slice(), item_names.as_slice())),
            _ => None,
        },
        Value::Host(h @ Host::Content(..)) => Some(*h),
        _ => it
            .frame
            .ctx
            .and_then(|c| layer_content(unsafe { &(*c.layers)[c.idx] })),
    };
    let Some(Host::Content(items, names)) = content else {
        return err("content(): нет содержимого");
    };
    let (items, names) = unsafe { (&*items, &*names) };
    match find_named(items, names, &key) {
        Some(s) => Ok(Value::Host(Host::Shape(s))),
        None => err(format!("content(`{}`) не найден", to_str(&key))),
    }
}

fn h_effect(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let key = str_arg(it, a, 0);
    let layer = match this {
        Value::Host(Host::Layer(ls, i)) => unsafe { &(**ls)[*i] },
        _ => match it.frame.ctx {
            Some(c) => unsafe { &(*c.layers)[c.idx] },
            None => return err("effect(): нет слоя"),
        },
    };
    let names: Vec<String> = layer
        .effect_controls
        .iter()
        .map(|e| e.name.clone())
        .collect();
    let found = find_named(&layer.effect_controls, &names, &key).or_else(|| {
        let k = to_str(&key);
        layer.effect_controls.iter().find(|e| e.match_name == k)
    });
    match found {
        Some(e) => Ok(Value::Host(Host::Effect(e))),
        None => err(format!("эффект `{}` не найден", to_str(&key))),
    }
}

fn effect_param(e: &EffectCtl, key: &Value) -> Option<PropRef> {
    let p = match key {
        Value::Num(n) => {
            let i = *n as usize;
            (i >= 1).then(|| e.params.get(i - 1)).flatten()
        }
        other => {
            let k = to_str(other);
            e.params
                .iter()
                .find(|p| p.name == k)
                .or_else(|| e.params.iter().find(|p| p.match_name == k))
        }
    }?;
    Some(PropRef::Vec(&p.value))
}

fn h_param(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let key = str_arg(it, a, 0);
    match this {
        Value::Host(Host::Effect(e)) => match effect_param(unsafe { &**e }, &key) {
            Some(p) => Ok(Value::Host(Host::Prop(p))),
            None => err(format!("параметр эффекта `{}` не найден", to_str(&key))),
        },
        _ => err("param(): не эффект"),
    }
}

fn h_comp(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let key = to_str(&str_arg(it, a, 0));
    let Some(comp) = it.frame.comp() else {
        return err("comp(): нет композиции");
    };
    if comp.name == key {
        return Ok(Value::Host(Host::Comp(comp.layers.as_slice())));
    }
    let id = comp.precomp_names.get(&key).cloned().unwrap_or(key.clone());
    match comp.precomps.get(&id) {
        Some(ls) => Ok(Value::Host(Host::Comp(ls.as_slice()))),
        // Экспорт без имени главной композиции: ссылка — на неё.
        None if comp.name.is_empty() => Ok(Value::Host(Host::Comp(comp.layers.as_slice()))),
        None => err(format!("композиция `{key}` не найдена")),
    }
}

fn h_value_at_time(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let p = prop_of(it, this);
    let t = it.num(&a.first().cloned().unwrap_or(Value::Num(it.frame.time())));
    let f = (t * it.frame.fps as f64) as f32;
    // Своё свойство — по ключам (без рекурсии в выражение).
    Ok(if matches!(this, Value::Host(_)) {
        p.value_at(f)
    } else {
        p.base_at(f)
    })
}

fn velocity_of(it: &Interp, p: PropRef, t: f64, base: bool) -> Value {
    let fps = it.frame.fps as f64;
    let h = 0.1; // в кадрах
    let f = (t * fps) as f32;
    let at = |x: f32| if base { p.base_at(x) } else { p.value_at(x) };
    let (a, b) = (at(f - h), at(f + h));
    let k = fps / (2.0 * h as f64);
    match (as_nums(&a), as_nums(&b)) {
        (Some(x), Some(y)) => num_arr(
            &x.iter()
                .zip(&y)
                .map(|(p, q)| (q - p) * k)
                .collect::<Vec<_>>(),
        ),
        _ => Value::Num((to_num(&b) - to_num(&a)) * k),
    }
}

fn h_velocity_at_time(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let p = prop_of(it, this);
    let t = it.num(&a.first().cloned().unwrap_or(Value::Num(it.frame.time())));
    Ok(velocity_of(it, p, t, !matches!(this, Value::Host(_))))
}

fn speed_of(v: &Value) -> Value {
    match as_nums(v) {
        Some(x) => Value::Num(x.iter().map(|c| c * c).sum::<f64>().sqrt()),
        None => Value::Num(to_num(v).abs()),
    }
}

fn h_speed_at_time(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let v = h_velocity_at_time(it, this, a)?;
    Ok(speed_of(&v))
}

/// Объект ключа: `time`, `value`, `index` и компоненты `[0]`, `[1]`…
fn key_obj(p: PropRef, i: usize, fps: f32) -> Option<Value> {
    let times = p.key_times();
    let t = *times.get(i)?;
    let v = p.base_at(t);
    let mut fields = vec![
        ("time", Value::Num(t as f64 / fps as f64)),
        ("index", Value::Num(i as f64 + 1.0)),
        ("value", v.clone()),
    ];
    let comps: Vec<(String, Value)> = match &v {
        Value::Arr(a) => a
            .borrow()
            .iter()
            .enumerate()
            .map(|(k, x)| (k.to_string(), x.clone()))
            .collect(),
        Value::Num(_) => vec![("0".to_string(), v.clone())],
        _ => Vec::new(),
    };
    let mut o = obj(std::mem::take(&mut fields));
    if let Value::Obj(f) = &mut o {
        for (k, x) in comps {
            f.borrow_mut().push((k.into(), x));
        }
    }
    Some(o)
}

fn h_key(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let p = prop_of(it, this);
    let n = it.num(&a.first().cloned().unwrap_or(Value::Num(1.0)));
    if n < 1.0 {
        return err("key(): индекс должен быть ≥ 1");
    }
    match key_obj(p, n as usize - 1, it.frame.fps) {
        Some(k) => Ok(k),
        None => err(format!("key({n}): нет такого ключа")),
    }
}

fn h_nearest_key(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let p = prop_of(it, this);
    let t = it.num(&a.first().cloned().unwrap_or(Value::Num(it.frame.time())));
    let f = (t * it.frame.fps as f64) as f32;
    let times = p.key_times();
    let i = times
        .iter()
        .enumerate()
        .min_by(|x, y| (x.1 - f).abs().total_cmp(&(y.1 - f).abs()))
        .map(|(i, _)| i);
    match i.and_then(|i| key_obj(p, i, it.frame.fps)) {
        Some(k) => Ok(k),
        None => err("nearestKey(): нет ключей"),
    }
}

/// `loopOut`/`loopIn` (и варианты `Duration`).
fn loop_impl(it: &mut Interp, this: &Value, a: &[Value], out: bool, duration: bool) -> R<Value> {
    let p = prop_of(it, this);
    let kind = match a.first() {
        Some(v) => to_str(&it.deref(v.clone())).to_lowercase(),
        None => "cycle".into(),
    };
    let num = a.get(1).map(|v| it.num(v)).unwrap_or(0.0);
    let times = p.key_times();
    let f = it.frame.frame;
    let own = !matches!(this, Value::Host(_));
    let current = || {
        if own {
            it.frame.value.clone()
        } else {
            p.value_at(f)
        }
    };
    if times.len() < 2 {
        return Ok(current());
    }
    let n = times.len();
    let fps = it.frame.fps;
    let (first, last) = if out {
        let last = times[n - 1];
        let first = if duration && num > 0.0 {
            last - num as f32 * fps
        } else if num >= 1.0 {
            times[n.saturating_sub(1 + num as usize)]
        } else {
            times[0]
        };
        (first, last)
    } else {
        let first = times[0];
        let last = if duration && num > 0.0 {
            first + num as f32 * fps
        } else if num >= 1.0 {
            times[(num as usize).min(n - 1)]
        } else {
            times[n - 1]
        };
        (first, last)
    };
    if (out && f <= last) || (!out && f >= first) {
        return Ok(current());
    }
    let period = last - first;
    if period <= 0.0 {
        return Ok(p.base_at(if out { last } else { first }));
    }
    let rel = if out { f - first } else { last - f };
    let cycles = (rel / period).floor();
    let local = rel.rem_euclid(period);
    Ok(match kind.as_str() {
        "pingpong" => {
            // Чётный проход повторяет отрезок, нечётный — идёт обратно.
            let odd = (cycles as i64) % 2 == 1;
            let tt = match (out, odd) {
                (true, false) | (false, true) => first + local,
                _ => last - local,
            };
            p.base_at(tt)
        }
        "offset" => {
            let tt = if out { first + local } else { last - local };
            let v = p.base_at(tt);
            let delta = it.binop(super::parser::BinOp::Sub, p.base_at(last), p.base_at(first))?;
            let k = if out { cycles } else { -cycles };
            let d = it.binop(super::parser::BinOp::Mul, delta, Value::Num(k as f64))?;
            it.binop(super::parser::BinOp::Add, v, d)?
        }
        "continue" => {
            let edge = if out { last } else { first };
            let vel = velocity_of(
                it,
                p,
                (edge as f64 + if out { -0.05 } else { 0.05 }) / fps as f64,
                true,
            );
            let dt = (f - edge) as f64 / fps as f64;
            let d = it.binop(super::parser::BinOp::Mul, vel, Value::Num(dt))?;
            it.binop(super::parser::BinOp::Add, p.base_at(edge), d)?
        }
        // cycle и неизвестные.
        _ => p.base_at(if out { first + local } else { last - local }),
    })
}

fn h_loop_out(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    loop_impl(it, this, a, true, false)
}
fn h_loop_in(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    loop_impl(it, this, a, false, false)
}
fn h_loop_out_duration(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    loop_impl(it, this, a, true, true)
}
fn h_loop_in_duration(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    loop_impl(it, this, a, false, true)
}

/// Гладкий шум 1D в −1..1 (значения в целых точках + сглаживание).
fn noise1(x: f64, seed: u64) -> f64 {
    let i = x.floor();
    let f = x - i;
    let h = |k: f64| (mix(seed ^ mix(k as i64 as u64)) >> 11) as f64 / (1u64 << 52) as f64 - 1.0;
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    h(i) + (h(i + 1.0) - h(i)) * u
}

fn h_wiggle(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let nums: Vec<f64> = a.iter().map(|v| it.num(v)).collect();
    let freq = nums.first().copied().unwrap_or(1.0);
    let amp = nums.get(1).copied().unwrap_or(0.0);
    let octaves = nums.get(2).copied().unwrap_or(1.0).max(1.0) as usize;
    let amp_mult = nums.get(3).copied().unwrap_or(0.5);
    let t = nums.get(4).copied().unwrap_or(it.frame.time());
    let base = match this {
        Value::Host(Host::Prop(p)) => p.value_at((t * it.frame.fps as f64) as f32),
        _ => it.frame.value.clone(),
    };
    let seed = it.frame.rng_seed();
    let wig = |dim: u64| {
        let mut s = 0.0;
        let mut k = 1.0;
        let mut norm = 0.0;
        for o in 0..octaves.min(8) {
            let fo = freq * 2f64.powi(o as i32);
            s += noise1(t * fo, seed ^ mix(dim * 131 + o as u64)) * k;
            norm += k;
            k *= amp_mult;
        }
        s / norm.max(1e-9) * amp
    };
    Ok(match as_nums(&base) {
        Some(v) => num_arr(
            &v.iter()
                .enumerate()
                .map(|(i, x)| x + wig(i as u64))
                .collect::<Vec<_>>(),
        ),
        None => Value::Num(to_num(&base) + wig(0)),
    })
}

fn h_frames_to_time(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let f = it.num(&a.first().cloned().unwrap_or(Value::Num(0.0)));
    let fps = a.get(1).map(|v| it.num(v)).unwrap_or(it.frame.fps as f64);
    Ok(Value::Num(f / fps))
}

fn h_time_to_frames(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let t = a.first().map(|v| it.num(v)).unwrap_or(it.frame.time());
    let fps = a.get(1).map(|v| it.num(v)).unwrap_or(it.frame.fps as f64);
    Ok(Value::Num((t * fps).floor()))
}

/// `posterizeTime(fps)`: дальше время и значение — с шагом 1/fps.
fn h_posterize_time(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let fps = a.first().map(|v| it.num(v)).unwrap_or(0.0);
    if fps > 0.0 {
        let t = (it.frame.time() * fps).floor() / fps;
        let f = (t * it.frame.fps as f64) as f32;
        let v = it.frame.prop.base_at(f);
        it.frame.frame = f;
        if !matches!(v, Value::Undef) {
            it.frame.value = v.clone();
        }
        if let Some(root) = it.root.clone() {
            define(&root, "time", Value::Num(t));
            define(&root, "value", it.frame.value.clone());
        }
    }
    Ok(it.frame.value.clone())
}

/// `smooth(width=0.2, samples=5, t=time)`: среднее значение по окну.
fn h_smooth(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let p = prop_of(it, this);
    let width = a.first().map(|v| it.num(v)).unwrap_or(0.2);
    let samples = a.get(1).map(|v| it.num(v)).unwrap_or(5.0).max(1.0) as usize;
    let t = a.get(2).map(|v| it.num(v)).unwrap_or(it.frame.time());
    if samples == 1 || width <= 0.0 {
        return Ok(p.base_at((t * it.frame.fps as f64) as f32));
    }
    let mut acc = Value::Num(0.0);
    for k in 0..samples {
        let tk = t - width / 2.0 + width * k as f64 / (samples - 1) as f64;
        let v = p.base_at((tk * it.frame.fps as f64) as f32);
        acc = it.binop(super::parser::BinOp::Add, acc, v)?;
    }
    it.binop(super::parser::BinOp::Div, acc, Value::Num(samples as f64))
}

/// Матрица слоя в координаты композиции (с родителями, 3D — ортогонально).
fn layer_to_comp(ls: &[Layer], i: usize, frame: f32) -> crate::lottie::geom::Mat {
    let mut m = crate::lottie::geom::Mat::IDENTITY;
    let mut cur = Some(i);
    let mut guard = 0;
    while let Some(k) = cur {
        let l = &ls[k];
        let own = if l.three_d {
            l.transform.matrix4(frame, 0.0).ortho()
        } else {
            l.transform.matrix(frame)
        };
        m = own.mul(&m);
        cur = l.parent.filter(|p| *p != k && *p < ls.len());
        guard += 1;
        if guard > 64 {
            break;
        }
    }
    m
}

fn this_layer(it: &Interp, this: &Value) -> Option<(*const [Layer], usize)> {
    match this {
        Value::Host(Host::Layer(ls, i)) => Some((*ls, *i)),
        _ => it.frame.ctx.map(|c| (c.layers, c.idx)),
    }
}

fn point_transform(it: &mut Interp, this: &Value, a: &[Value], to_comp: bool) -> R<Value> {
    let Some((ls, i)) = this_layer(it, this) else {
        return err("toComp(): нет слоя");
    };
    let p = it.deref(a.first().cloned().unwrap_or(Value::Undef));
    let pt = as_nums(&p).unwrap_or_else(|| vec![to_num(&p), 0.0]);
    let f = match a.get(1) {
        Some(t) => (it.num(t) * it.frame.fps as f64) as f32,
        None => it.frame.frame,
    };
    let m = layer_to_comp(unsafe { &*ls }, i, f);
    let m = if to_comp {
        m
    } else {
        match m.invert() {
            Some(inv) => inv,
            None => return Ok(p),
        }
    };
    let q = m.apply([
        pt.first().copied().unwrap_or(0.0) as f32,
        pt.get(1).copied().unwrap_or(0.0) as f32,
    ]);
    let mut out = vec![q[0] as f64, q[1] as f64];
    out.extend(pt.iter().skip(2));
    Ok(num_arr(&out))
}

fn h_to_comp(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    point_transform(it, this, a, true)
}

fn h_from_comp(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    point_transform(it, this, a, false)
}

fn h_source_rect(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let Some((ls, i)) = this_layer(it, this) else {
        return err("sourceRectAtTime(): нет слоя");
    };
    let f = match a.first() {
        Some(t) => (it.num(t) * it.frame.fps as f64) as f32,
        None => it.frame.frame,
    };
    let Some(comp) = it.frame.comp() else {
        return err("sourceRectAtTime(): нет композиции");
    };
    let l = unsafe { &(*ls)[i] };
    let b = crate::lottie::render::layer_bounds(comp, l, f).unwrap_or([0.0; 4]);
    Ok(obj(vec![
        ("left", Value::Num(b[0] as f64)),
        ("top", Value::Num(b[1] as f64)),
        ("width", Value::Num(b[2] as f64)),
        ("height", Value::Num(b[3] as f64)),
    ]))
}

fn h_mask(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let key = str_arg(it, a, 0);
    let Some((ls, i)) = this_layer(it, this) else {
        return err("mask(): нет слоя");
    };
    let l = unsafe { &(*ls)[i] };
    match find_named(&l.mask_paths, &l.mask_names, &key) {
        Some(p) => Ok(obj(vec![
            ("maskPath", Value::Host(Host::Prop(PropRef::Path(p)))),
            ("name", Value::Str(to_str(&key).into())),
        ])),
        None => err(format!("маска `{}` не найдена", to_str(&key))),
    }
}

/// Маркер композиции как объект: `time`, `duration`, `comment`, `index`.
fn marker_obj(comp: &Composition, i: usize, fps: f32) -> Option<Value> {
    let m = comp.markers.get(i)?;
    Some(obj(vec![
        ("time", Value::Num(m.frame as f64 / fps as f64)),
        ("duration", Value::Num(m.duration as f64 / fps as f64)),
        ("comment", Value::Str(m.name.as_str().into())),
        ("index", Value::Num(i as f64 + 1.0)),
    ]))
}

fn h_marker_key(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let Value::Host(Host::Markers(c)) = this else {
        return err("key(): не маркеры");
    };
    let comp = unsafe { &**c };
    let key = str_arg(it, a, 0);
    let i = match &key {
        Value::Num(n) if *n >= 1.0 => Some(*n as usize - 1),
        Value::Num(_) => None,
        other => {
            let k = to_str(other);
            comp.markers.iter().position(|m| m.name == k)
        }
    };
    match i.and_then(|i| marker_obj(comp, i, it.frame.fps)) {
        Some(m) => Ok(m),
        None => err(format!("маркер `{}` не найден", to_str(&key))),
    }
}

fn h_marker_nearest(it: &mut Interp, this: &Value, a: &[Value]) -> R<Value> {
    let Value::Host(Host::Markers(c)) = this else {
        return err("nearestKey(): не маркеры");
    };
    let comp = unsafe { &**c };
    let t = a.first().map(|v| it.num(v)).unwrap_or(it.frame.time());
    let f = (t * it.frame.fps as f64) as f32;
    let i = comp
        .markers
        .iter()
        .enumerate()
        .min_by(|x, y| (x.1.frame - f).abs().total_cmp(&(y.1.frame - f).abs()))
        .map(|(i, _)| i);
    match i.and_then(|i| marker_obj(comp, i, it.frame.fps)) {
        Some(m) => Ok(m),
        None => err("nearestKey(): нет маркеров"),
    }
}

impl EvalFrame {
    /// Зерно для шума (стабильно по времени, в отличие от `random`).
    pub fn rng_seed(&self) -> u64 {
        self.rng_base
    }
}

pub fn install_globals(g: &Env) {
    let funcs: &[(&str, super::interp::NativeFn)] = &[
        ("comp", h_comp),
        ("content", h_content),
        ("effect", h_effect),
        ("key", h_key),
        ("nearestKey", h_nearest_key),
        ("valueAtTime", h_value_at_time),
        ("velocityAtTime", h_velocity_at_time),
        ("speedAtTime", h_speed_at_time),
        ("loopOut", h_loop_out),
        ("loopIn", h_loop_in),
        ("loopOutDuration", h_loop_out_duration),
        ("loopInDuration", h_loop_in_duration),
        ("wiggle", h_wiggle),
        ("framesToTime", h_frames_to_time),
        ("timeToFrames", h_time_to_frames),
        ("posterizeTime", h_posterize_time),
        ("smooth", h_smooth),
        ("toComp", h_to_comp),
        ("fromComp", h_from_comp),
        ("toWorld", h_to_comp),
        ("fromWorld", h_from_comp),
        ("sourceRectAtTime", h_source_rect),
        ("mask", h_mask),
    ];
    for (name, f) in funcs {
        define(g, name, native(*f, Value::Undef));
    }
}

/// Значения, зависящие от вычисляемого свойства и слоя.
pub fn install_frame(env: &Env, it: &Interp) {
    let fr = &it.frame;
    define(env, "value", fr.value.clone());
    define(env, "time", Value::Num(fr.time()));
    define(env, "thisProperty", Value::Host(Host::Prop(fr.prop)));
    define(env, "numKeys", Value::Num(fr.prop.key_times().len() as f64));
    define(env, "velocity", velocity_of(it, fr.prop, fr.time(), true));
    let sp = speed_of(&velocity_of(it, fr.prop, fr.time(), true));
    define(env, "speed", sp);
    if let Some(c) = fr.ctx {
        define(env, "thisComp", Value::Host(Host::Comp(c.layers)));
        let layer = Value::Host(Host::Layer(c.layers, c.idx));
        let l = unsafe { &(*c.layers)[c.idx] };
        define(env, "thisLayer", layer);
        define(env, "transform", Value::Host(Host::Transform(&l.transform)));
        define(
            env,
            "inPoint",
            Value::Num(l.in_point as f64 / fr.fps as f64),
        );
        define(
            env,
            "outPoint",
            Value::Num(l.out_point as f64 / fr.fps as f64),
        );
        define(
            env,
            "startTime",
            Value::Num(l.start_time as f64 / fr.fps as f64),
        );
        define(env, "index", Value::Num(l.index as f64));
        define(env, "width", Value::Num(l.width as f64));
        define(env, "height", Value::Num(l.height as f64));
        define(env, "hasParent", Value::Bool(l.parent.is_some()));
    }
}

fn transform_prop(t: &Transform, name: &str) -> Option<PropRef> {
    Some(match name {
        "position" => match &t.position {
            Position::Joint(p) => PropRef::Vec(p),
            Position::Split { x, y, .. } => PropRef::Split(x, y),
        },
        "xPosition" => match &t.position {
            Position::Split { x, .. } => PropRef::F32(x),
            _ => return None,
        },
        "yPosition" => match &t.position {
            Position::Split { y, .. } => PropRef::F32(y),
            _ => return None,
        },
        "anchorPoint" | "anchor" => PropRef::Vec(&t.anchor),
        "scale" => PropRef::Vec(&t.scale),
        "rotation" | "zRotation" => PropRef::F32(&t.rotation),
        "opacity" => PropRef::F32(&t.opacity),
        "skew" => PropRef::F32(&t.skew),
        "skewAxis" => PropRef::F32(&t.skew_axis),
        "xRotation" => PropRef::F32(t.rotation_x.as_ref()?),
        "yRotation" => PropRef::F32(t.rotation_y.as_ref()?),
        "orientation" => PropRef::Vec(t.orientation.as_ref()?),
        _ => return None,
    })
}

fn shape_prop(s: &Shape, name: &str) -> Option<PropRef> {
    Some(match (s, name) {
        (Shape::Rect { size, .. }, "size") => PropRef::Vec(size),
        (Shape::Rect { position, .. }, "position") => PropRef::Vec(position),
        (Shape::Rect { roundness, .. }, "roundness") => PropRef::F32(roundness),
        (Shape::Ellipse { size, .. }, "size") => PropRef::Vec(size),
        (Shape::Ellipse { position, .. }, "position") => PropRef::Vec(position),
        (Shape::Polystar { position, .. }, "position") => PropRef::Vec(position),
        (Shape::Polystar { rotation, .. }, "rotation") => PropRef::F32(rotation),
        (Shape::Polystar { points, .. }, "points") => PropRef::F32(points),
        (Shape::Polystar { outer_radius, .. }, "outerRadius") => PropRef::F32(outer_radius),
        (Shape::Polystar { inner_radius, .. }, "innerRadius") => PropRef::F32(inner_radius),
        (
            Shape::Polystar {
                outer_roundness, ..
            },
            "outerRoundness",
        ) => PropRef::F32(outer_roundness),
        (
            Shape::Polystar {
                inner_roundness, ..
            },
            "innerRoundness",
        ) => PropRef::F32(inner_roundness),
        (Shape::Path { path, .. }, "path") => PropRef::Path(path),
        (Shape::Fill { color, .. } | Shape::Stroke { color, .. }, "color") => PropRef::Vec(color),
        (
            Shape::Fill { opacity, .. }
            | Shape::Stroke { opacity, .. }
            | Shape::GradientFill { opacity, .. }
            | Shape::GradientStroke { opacity, .. },
            "opacity",
        ) => PropRef::F32(opacity),
        (Shape::Stroke { style, .. } | Shape::GradientStroke { style, .. }, "strokeWidth") => {
            PropRef::F32(&style.width)
        }
        (
            Shape::GradientFill { gradient, .. } | Shape::GradientStroke { gradient, .. },
            "startPoint",
        ) => PropRef::Vec(&gradient.start),
        (
            Shape::GradientFill { gradient, .. } | Shape::GradientStroke { gradient, .. },
            "endPoint",
        ) => PropRef::Vec(&gradient.end),
        (Shape::Trim { start, .. }, "start") => PropRef::F32(start),
        (Shape::Trim { end, .. }, "end") => PropRef::F32(end),
        (Shape::Trim { offset, .. }, "offset") => PropRef::F32(offset),
        (Shape::Repeater { copies, .. }, "copies") => PropRef::F32(copies),
        (Shape::Repeater { offset, .. }, "offset") => PropRef::F32(offset),
        (Shape::RoundCorners { radius, .. }, "radius") => PropRef::F32(radius),
        (Shape::OffsetPath { amount, .. }, "amount") => PropRef::F32(amount),
        (Shape::ZigZag { size, .. }, "size") => PropRef::F32(size),
        (Shape::ZigZag { ridges, .. }, "ridges") => PropRef::F32(ridges),
        (Shape::PuckerBloat { amount, .. }, "amount") => PropRef::F32(amount),
        (Shape::Twist { angle, .. }, "angle") => PropRef::F32(angle),
        _ => return None,
    })
}

/// Член объекта After Effects.
pub fn member(it: &mut Interp, h: Host, name: &str) -> R<Value> {
    let this = Value::Host(h);
    let method = |f: super::interp::NativeFn| Ok(native(f, this.clone()));
    match h {
        Host::Comp(layers) => {
            let ls = unsafe { &*layers };
            let comp = it.frame.comp();
            match name {
                "layer" => method(h_layer),
                "numLayers" => Ok(Value::Num(ls.len() as f64)),
                "width" => Ok(Value::Num(comp.map(|c| c.width).unwrap_or(0.0) as f64)),
                "height" => Ok(Value::Num(comp.map(|c| c.height).unwrap_or(0.0) as f64)),
                "frameDuration" => Ok(Value::Num(1.0 / it.frame.fps as f64)),
                "duration" => Ok(Value::Num(
                    comp.map(|c| c.duration_secs()).unwrap_or(0.0) as f64
                )),
                "name" => Ok(Value::Str(
                    comp.map(|c| c.name.as_str()).unwrap_or("").into(),
                )),
                "marker" => match comp {
                    Some(c) => Ok(Value::Host(Host::Markers(c))),
                    None => Ok(Value::Undef),
                },
                _ => Ok(Value::Undef),
            }
        }
        Host::Layer(ls, i) => {
            let l = unsafe { &(*ls)[i] };
            let fps = it.frame.fps as f64;
            match name {
                "content" => method(h_content),
                "effect" => method(h_effect),
                "toComp" | "toWorld" => method(h_to_comp),
                "fromComp" | "fromWorld" => method(h_from_comp),
                "sourceRectAtTime" => method(h_source_rect),
                "mask" => method(h_mask),
                "text" => Ok(Value::Host(Host::Text(ls, i))),
                "transform" => Ok(Value::Host(Host::Transform(&l.transform))),
                "name" => Ok(Value::Str(l.name.as_str().into())),
                "index" => Ok(Value::Num(l.index as f64)),
                "inPoint" => Ok(Value::Num(l.in_point as f64 / fps)),
                "outPoint" => Ok(Value::Num(l.out_point as f64 / fps)),
                "startTime" => Ok(Value::Num(l.start_time as f64 / fps)),
                "width" => Ok(Value::Num(l.width as f64)),
                "height" => Ok(Value::Num(l.height as f64)),
                "hasParent" => Ok(Value::Bool(l.parent.is_some())),
                "parent" => match l.parent {
                    Some(p) => Ok(Value::Host(Host::Layer(ls, p))),
                    None => Ok(Value::Undef),
                },
                other => match transform_prop(&l.transform, other) {
                    Some(p) => Ok(Value::Host(Host::Prop(p))),
                    None => Ok(Value::Undef),
                },
            }
        }
        Host::Transform(t) => match transform_prop(unsafe { &*t }, name) {
            Some(p) => Ok(Value::Host(Host::Prop(p))),
            None => Ok(Value::Undef),
        },
        Host::Shape(s) => {
            let shape = unsafe { &*s };
            match (shape, name) {
                (Shape::Group { .. }, "content") => method(h_content),
                (Shape::Group { transform, .. }, "transform") => {
                    Ok(Value::Host(Host::Transform(transform)))
                }
                (Shape::Repeater { transform, .. }, "transform") => {
                    Ok(Value::Host(Host::Transform(transform)))
                }
                (Shape::Group { name: n, .. }, "name") => Ok(Value::Str(n.as_str().into())),
                _ => match shape_prop(shape, name) {
                    Some(p) => Ok(Value::Host(Host::Prop(p))),
                    None => Ok(Value::Undef),
                },
            }
        }
        Host::Content(..) => match name {
            "content" => method(h_content),
            _ => Ok(Value::Undef),
        },
        Host::Effect(e) => match name {
            "param" => method(h_param),
            "name" => Ok(Value::Str(unsafe { (*e).name.as_str() }.into())),
            _ => Ok(Value::Undef),
        },
        Host::Markers(c) => markers_member(it, c, name),
        Host::Text(ls, i) => text_member(it, ls, i, name),
        Host::Prop(p) => match name {
            "value" => Ok(p.value_at(it.frame.frame)),
            "valueAtTime" => method(h_value_at_time),
            "velocityAtTime" => method(h_velocity_at_time),
            "speedAtTime" => method(h_speed_at_time),
            "velocity" => Ok(velocity_of(it, p, it.frame.time(), false)),
            "speed" => Ok(speed_of(&velocity_of(it, p, it.frame.time(), false))),
            "key" => method(h_key),
            "nearestKey" => method(h_nearest_key),
            "numKeys" => Ok(Value::Num(p.key_times().len() as f64)),
            "loopOut" => method(h_loop_out),
            "loopIn" => method(h_loop_in),
            "loopOutDuration" => method(h_loop_out_duration),
            "loopInDuration" => method(h_loop_in_duration),
            "wiggle" => method(h_wiggle),
            "smooth" => method(h_smooth),
            "length" => {
                let v = p.value_at(it.frame.frame);
                it.member(&v, "length")
            }
            _ => Ok(Value::Undef),
        },
    }
}

fn markers_member(it: &mut Interp, c: *const Composition, name: &str) -> R<Value> {
    let comp = unsafe { &*c };
    let this = Value::Host(Host::Markers(c));
    match name {
        "numKeys" => Ok(Value::Num(comp.markers.len() as f64)),
        "key" => Ok(native(h_marker_key, this)),
        "nearestKey" => Ok(native(h_marker_nearest, this)),
        _ => {
            let _ = it;
            Ok(Value::Undef)
        }
    }
}

fn text_member(it: &mut Interp, ls: *const [Layer], i: usize, name: &str) -> R<Value> {
    let l = unsafe { &(*ls)[i] };
    match (name, &l.kind) {
        ("sourceText", LayerKind::Text(td)) => {
            let f = it.frame.frame;
            Ok(td
                .doc_at(f)
                .map(|d| Value::Str(d.text.as_str().into()))
                .unwrap_or(Value::Str("".into())))
        }
        _ => Ok(Value::Undef),
    }
}

/// `obj[i]` у объекта After Effects: компоненты значения свойства.
pub fn index(it: &mut Interp, h: Host, i: &Value) -> R<Value> {
    match h {
        Host::Prop(p) => {
            let v = p.value_at(it.frame.frame);
            it.index(&v, i)
        }
        _ => member(it, h, &to_str(i)),
    }
}

/// Вызов объекта: `effect('X')('Slider')`.
pub fn call(it: &mut Interp, h: Host, a: &[Value]) -> R<Value> {
    match h {
        Host::Effect(_) => h_param(it, &Value::Host(h), a),
        _ => err(format!("{} is not a function", h.kind())),
    }
}

/// Значение объекта в арифметике: свойство → его значение на кадре.
pub fn host_value(it: &mut Interp, h: Host) -> Option<Value> {
    match h {
        Host::Prop(p) => Some(p.value_at(it.frame.frame)),
        _ => None,
    }
}
