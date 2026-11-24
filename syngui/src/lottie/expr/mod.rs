//! Выражения After Effects (`x` у свойств, экспорт bodymovin) — свой
//! интерпретатор подмножества JavaScript без сторонних движков:
//! [`parser`] (лексер и AST), [`interp`] (значения, операторы, встроенные
//! функции), [`host`] (объекты After Effects поверх модели Lottie).
//!
//! Выражение хранится в [`crate::lottie::anim::Animated::Expr`] поверх
//! ключевых кадров и вычисляется при каждом `value(frame)`: `value` —
//! значение по ключам, результат — `$bm_rt` (или последнее выражение).
//! Ошибка вычисления оставляет значение по ключам (сообщение — один раз в
//! лог). Ссылки на другие слои и свойства берутся из контекста слоя,
//! который рендер ставит через [`host::enter`].

pub mod host;
pub mod interp;
pub mod parser;

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub use host::{enter, CtxGuard, PropRef};
pub use interp::Value;

use crate::lottie::anim::Animated;
use crate::lottie::geom::ShapePath;

/// Скомпилированное выражение.
pub struct Program {
    stmts: Vec<parser::Stmt>,
    src: Arc<str>,
    reported: AtomicBool,
}

impl std::fmt::Debug for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Program({:?})", &self.src[..self.src.len().min(60)])
    }
}

impl Program {
    pub fn compile(src: &str) -> Result<Program, String> {
        Ok(Program {
            stmts: parser::parse(src)?,
            src: src.into(),
            reported: AtomicBool::new(false),
        })
    }

    pub fn source(&self) -> &str {
        &self.src
    }
}

/// Значение свойства ↔ значение выражения.
pub trait ExprValue: Sized + Clone {
    fn to_value(&self) -> Value;
    fn from_value(v: &Value, fallback: &Self) -> Option<Self>;
    fn prop_ref(a: &Animated<Self>) -> PropRef;
}

fn as_f32s(v: &Value) -> Option<Vec<f32>> {
    match v {
        Value::Num(n) => Some(vec![*n as f32]),
        Value::Bool(b) => Some(vec![*b as i32 as f32]),
        Value::Arr(a) => Some(
            a.borrow()
                .iter()
                .map(|x| interp::to_num(x) as f32)
                .collect(),
        ),
        _ => None,
    }
}

impl ExprValue for f32 {
    fn to_value(&self) -> Value {
        Value::Num(*self as f64)
    }
    fn from_value(v: &Value, _: &Self) -> Option<Self> {
        as_f32s(v)
            .and_then(|x| x.first().copied())
            .filter(|x| x.is_finite())
    }
    fn prop_ref(a: &Animated<Self>) -> PropRef {
        PropRef::F32(a)
    }
}

impl ExprValue for Vec<f32> {
    fn to_value(&self) -> Value {
        if self.len() == 1 {
            Value::Num(self[0] as f64)
        } else {
            interp::arr(self.iter().map(|x| Value::Num(*x as f64)).collect())
        }
    }
    fn from_value(v: &Value, fallback: &Self) -> Option<Self> {
        let mut x = as_f32s(v)?;
        if x.iter().any(|c| !c.is_finite()) {
            return None;
        }
        // Число на многомерное свойство — во все компоненты; недостающие
        // компоненты — из значения по ключам.
        if x.len() == 1 && fallback.len() > 1 {
            x = vec![x[0]; fallback.len()];
        }
        let have = x.len();
        x.extend(fallback.iter().skip(have));
        Some(x)
    }
    fn prop_ref(a: &Animated<Self>) -> PropRef {
        PropRef::Vec(a)
    }
}

impl ExprValue for ShapePath {
    fn to_value(&self) -> Value {
        Value::Path(self.clone().into())
    }
    fn from_value(v: &Value, _: &Self) -> Option<Self> {
        match v {
            Value::Path(p) => Some((**p).clone()),
            _ => None,
        }
    }
    fn prop_ref(a: &Animated<Self>) -> PropRef {
        PropRef::Path(a)
    }
}

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Глубина вложенных вычислений (выражение ссылается на свойство с
/// выражением): защита от циклов.
const MAX_DEPTH: u32 = 12;

/// Вычислить выражение свойства на кадре `frame`; `None` — оставить
/// значение по ключам.
pub fn evaluate<T: ExprValue>(
    prog: &Program,
    frame: f32,
    base: &T,
    prop: &Animated<T>,
    seed: u64,
) -> Option<T> {
    let depth = DEPTH.with(|d| d.get());
    if depth >= MAX_DEPTH {
        return None;
    }
    DEPTH.with(|d| d.set(depth + 1));
    struct Dec;
    impl Drop for Dec {
        fn drop(&mut self) {
            DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        }
    }
    let _dec = Dec;
    let fps = host::current().map(|_| host_fps()).unwrap_or(30.0);
    let frame_ctx = host::EvalFrame::new(frame, fps, base.to_value(), T::prop_ref(prop), seed);
    let mut it = interp::Interp::new(frame_ctx);
    let env = interp::eval_scope();
    host::install_frame(&env, &it);
    match it.run(&prog.stmts, &env) {
        Ok(v) => {
            let v = it.deref(v);
            T::from_value(&v, base)
        }
        Err(e) => {
            if !prog.reported.swap(true, Ordering::Relaxed) {
                log::debug!(
                    "lottie: ошибка выражения ({}): {}",
                    interp::to_str(&e),
                    &prog.src[..prog.src.len().min(120)]
                );
            }
            None
        }
    }
}

fn host_fps() -> f32 {
    host::current_fps().unwrap_or(30.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lottie::json;

    fn prop_f32(json_src: &str) -> Animated<f32> {
        Animated::parse(Some(&json::parse(json_src).unwrap())).unwrap()
    }

    fn prop_vec(json_src: &str) -> Animated<Vec<f32>> {
        Animated::parse(Some(&json::parse(json_src).unwrap())).unwrap()
    }

    fn with_expr(base: &str, x: &str) -> String {
        let b = base.trim_end_matches('}');
        format!("{b},\"x\":{}}}", json_str(x))
    }

    fn json_str(s: &str) -> String {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                '\n' => o.push_str("\\n"),
                c => o.push(c),
            }
        }
        o.push('"');
        o
    }

    const KEYS: &str = r#"{"a":1,"k":[{"t":0,"s":[0],"i":{"x":[1],"y":[1]},"o":{"x":[0],"y":[0]}},{"t":10,"s":[100]}]}"#;

    #[test]
    fn arithmetic_and_bm_helpers() {
        let p = prop_f32(&with_expr(
            r#"{"a":0,"k":5}"#,
            "var $bm_rt;\n$bm_rt = $bm_sum($bm_mul(value, 2), 1);",
        ));
        assert_eq!(p.value(0.0), 11.0);
        let p = prop_f32(&with_expr(
            r#"{"a":0,"k":250}"#,
            "var $bm_rt;\n$bm_rt = clamp(value, 0, 100);",
        ));
        assert_eq!(p.value(0.0), 100.0);
        let v = prop_vec(&with_expr(
            r#"{"a":0,"k":[1,2]}"#,
            "$bm_rt = $bm_sum(value, [10, 20]);",
        ));
        assert_eq!(v.value(0.0), vec![11.0, 22.0]);
    }

    #[test]
    fn loop_out_cycle_and_pingpong() {
        let p = prop_f32(&with_expr(KEYS, "var $bm_rt;\n$bm_rt = loopOut('cycle');"));
        // Линейные ключи 0→100 за 10 кадров; кадр 13 = кадр 3.
        assert!((p.value(13.0) - 30.0).abs() < 1e-3, "{}", p.value(13.0));
        let p = prop_f32(&with_expr(KEYS, "$bm_rt = loopOut('pingpong');"));
        assert!((p.value(13.0) - 70.0).abs() < 1e-3, "{}", p.value(13.0));
        let p = prop_f32(&with_expr(KEYS, "$bm_rt = loopOut('offset');"));
        assert!((p.value(13.0) - 130.0).abs() < 1e-3, "{}", p.value(13.0));
        // Внутри ключей — значение как есть.
        assert!((p.value(5.0) - 50.0).abs() < 1e-3);
    }

    #[test]
    fn script_with_functions_switch_try() {
        let src = r#"var $bm_rt;
function f(t, b, c, d) { var v; if ((t /= d / 2) < 1) { v = t; } else { v = 2; } return $bm_sum(b, $bm_mul(v, c)); }
function g() {
    var n = 0, dim = 1;
    try { key(1)[1].length; dim = 2; } catch (e) {}
    switch (dim) { case 1: return f(1, 10, 3, 4); break; default: return null; }
}
$bm_rt = g() || value;"#;
        let p = prop_f32(&with_expr(KEYS, src));
        // t=1, d=4 → t/=2 → 0.5 < 1 → v = 0.5 → 10 + 1.5.
        assert!((p.value(0.0) - 11.5).abs() < 1e-4, "{}", p.value(0.0));
    }

    #[test]
    fn comma_ternary_and_keys() {
        let src = "var $bm_rt; var n, t;\nn = 0, 0 < numKeys && (n = nearestKey(time).index, key(n).time > time && n--), t = 0 === n ? 0 : $bm_sub(time, key(n).time), $bm_rt = 0 < n ? $bm_sum(value, t) : value;";
        let p = prop_f32(&with_expr(KEYS, src));
        // Без контекста fps = 30: кадр 15 → time 0.5, ближайший ключ 2 (t=10 → 1/3 с).
        let v = p.value(15.0);
        assert!((v - (100.0 + 0.5 - 10.0 / 30.0)).abs() < 1e-3, "{v}");
    }

    #[test]
    fn wiggle_is_deterministic_and_bounded() {
        let p = prop_vec(&with_expr(
            r#"{"a":0,"k":[100,100]}"#,
            "var $bm_rt;\n$bm_rt = wiggle(2, 20);",
        ));
        let a = p.value(7.0);
        let b = p.value(7.0);
        assert_eq!(a, b);
        assert!(a.iter().all(|c| (c - 100.0).abs() <= 20.0 + 1e-3));
        assert_ne!(p.value(7.0), p.value(20.0));
    }

    #[test]
    fn errors_fall_back_to_keyframes() {
        let p = prop_f32(&with_expr(
            r#"{"a":0,"k":42}"#,
            "$bm_rt = thisComp.layer('нет').transform.opacity;",
        ));
        assert_eq!(p.value(0.0), 42.0);
        let p = prop_f32(&with_expr(r#"{"a":0,"k":42}"#, "while (true) {}"));
        assert_eq!(p.value(0.0), 42.0);
    }
}
