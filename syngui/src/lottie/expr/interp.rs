//! Интерпретатор выражений: значения JS, области видимости, операторы
//! (арифметика над массивами — как в After Effects), встроенные объекты
//! `Math` и функции выражений (clamp, linear/ease, length, random…).
//! Объекты After Effects (слои, свойства, эффекты) — в [`super::host`].

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::host::{self, Host};
use super::parser::{format_num, BinOp, Expr, FuncDef, Stmt, UnOp};
use crate::lottie::geom::ShapePath;

pub type NativeFn = fn(&mut Interp, &Value, &[Value]) -> R<Value>;

/// Поля объекта (порядок сохраняется).
pub type Fields = Rc<RefCell<Vec<(Rc<str>, Value)>>>;

/// Значение выражения.
#[derive(Clone)]
pub enum Value {
    Undef,
    Null,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    Arr(Rc<RefCell<Vec<Value>>>),
    Obj(Fields),
    Func(Rc<Closure>),
    /// Встроенная функция; `this` — объект, у которого она взята как метод.
    Native(NativeFn, Rc<Value>),
    /// Контур (значение свойства `path`).
    Path(Rc<ShapePath>),
    /// Объект After Effects.
    Host(Host),
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Undef => write!(f, "undefined"),
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Num(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Arr(a) => f.debug_list().entries(a.borrow().iter()).finish(),
            Value::Obj(_) => write!(f, "[object]"),
            Value::Func(_) | Value::Native(..) => write!(f, "[function]"),
            Value::Path(_) => write!(f, "[path]"),
            Value::Host(h) => write!(f, "[{}]", h.kind()),
        }
    }
}

pub struct Closure {
    pub def: Arc<FuncDef>,
    pub env: Env,
}

/// Ошибка или брошенное значение (`throw`, ошибки времени исполнения).
pub type R<T> = Result<T, Value>;

pub fn err<T>(msg: impl Into<String>) -> R<T> {
    let s: String = msg.into();
    Err(Value::Str(s.into()))
}

pub type Env = Rc<RefCell<Scope>>;

pub struct Scope {
    vars: Vec<(Rc<str>, Value)>,
    parent: Option<Env>,
    /// Граница функции: сюда попадают `var` и присваивания необъявленным.
    func: bool,
}

impl Scope {
    pub fn new(parent: Option<Env>, func: bool) -> Env {
        Rc::new(RefCell::new(Scope {
            vars: Vec::new(),
            parent,
            func,
        }))
    }
}

fn lookup(env: &Env, name: &str) -> Option<Value> {
    let s = env.borrow();
    if let Some((_, v)) = s.vars.iter().find(|(k, _)| &**k == name) {
        return Some(v.clone());
    }
    s.parent.as_ref().and_then(|p| lookup(p, name))
}

fn func_scope(env: &Env) -> Env {
    let mut cur = env.clone();
    loop {
        let next = {
            let s = cur.borrow();
            if s.func {
                None
            } else {
                s.parent.clone()
            }
        };
        match next {
            Some(p) => cur = p,
            None => return cur,
        }
    }
}

pub fn define(env: &Env, name: &str, v: Value) {
    let mut s = env.borrow_mut();
    if let Some(slot) = s.vars.iter_mut().find(|(k, _)| &**k == name) {
        slot.1 = v;
    } else {
        s.vars.push((name.into(), v));
    }
}

/// Присвоить: ближайшая область, где имя объявлено (кроме общих
/// встроенных), иначе — область текущей функции.
fn assign_var(env: &Env, name: &str, v: Value) {
    let mut cur = Some(env.clone());
    while let Some(e) = cur {
        let mut s = e.borrow_mut();
        if s.parent.is_none() {
            // Общие встроенные не меняем — затеняем.
            break;
        }
        if let Some(slot) = s.vars.iter_mut().find(|(k, _)| &**k == name) {
            slot.1 = v;
            return;
        }
        cur = s.parent.clone();
    }
    define(&func_scope(env), name, v);
}

enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

/// Предел шагов одного вычисления (защита от бесконечных циклов).
const MAX_STEPS: u64 = 2_000_000;

pub struct Interp {
    steps: u64,
    /// Контекст вычисляемого свойства.
    pub frame: host::EvalFrame,
    /// Корневая область вычисления (для `posterizeTime`).
    pub root: Option<Env>,
}

impl Interp {
    pub fn new(frame: host::EvalFrame) -> Self {
        Interp {
            steps: 0,
            frame,
            root: None,
        }
    }

    fn tick(&mut self) -> R<()> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return err("превышен предел шагов выражения");
        }
        Ok(())
    }

    /// Выполнить программу; результат — `$bm_rt` или значение последнего
    /// выражения.
    pub fn run(&mut self, prog: &[Stmt], env: &Env) -> R<Value> {
        hoist(prog, env);
        let mut last = Value::Undef;
        for s in prog {
            if let Stmt::Expr(e) = s {
                last = self.eval(e, env)?;
                continue;
            }
            if let Flow::Return(v) = self.exec(s, env)? {
                return Ok(v);
            }
        }
        match lookup(env, "$bm_rt") {
            Some(Value::Undef) | None => Ok(last),
            Some(v) => Ok(v),
        }
    }

    fn exec_block(&mut self, body: &[Stmt], env: &Env) -> R<Flow> {
        for s in body {
            match self.exec(s, env)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec(&mut self, s: &Stmt, env: &Env) -> R<Flow> {
        self.tick()?;
        match s {
            Stmt::Expr(e) => {
                self.eval(e, env)?;
            }
            Stmt::Var(decls) => {
                let fs = func_scope(env);
                for (name, init) in decls {
                    match init {
                        Some(e) => {
                            let v = self.eval(e, env)?;
                            define(&fs, name, v);
                        }
                        None => {
                            if !fs.borrow().vars.iter().any(|(k, _)| **k == **name) {
                                define(&fs, name, Value::Undef);
                            }
                        }
                    }
                }
            }
            Stmt::Func(_) | Stmt::Empty => {}
            Stmt::If(c, a, b) => {
                let cv = self.eval(c, env)?;
                if truthy(&cv) {
                    return self.exec(a, env);
                } else if let Some(b) = b {
                    return self.exec(b, env);
                }
            }
            Stmt::Block(body) => return self.exec_block(body, env),
            Stmt::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(e, env)?,
                    None => Value::Undef,
                };
                return Ok(Flow::Return(v));
            }
            Stmt::While(c, body) => loop {
                let cv = self.eval(c, env)?;
                if !truthy(&cv) {
                    break;
                }
                match self.exec(body, env)? {
                    Flow::Break => break,
                    Flow::Return(v) => return Ok(Flow::Return(v)),
                    _ => {}
                }
            },
            Stmt::DoWhile(body, c) => loop {
                match self.exec(body, env)? {
                    Flow::Break => break,
                    Flow::Return(v) => return Ok(Flow::Return(v)),
                    _ => {}
                }
                let cv = self.eval(c, env)?;
                if !truthy(&cv) {
                    break;
                }
            },
            Stmt::For(init, cond, upd, body) => {
                if let Some(i) = init {
                    self.exec(i, env)?;
                }
                loop {
                    if let Some(c) = cond {
                        let cv = self.eval(c, env)?;
                        if !truthy(&cv) {
                            break;
                        }
                    }
                    match self.exec(body, env)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                    if let Some(u) = upd {
                        self.eval(u, env)?;
                    }
                }
            }
            Stmt::Break => return Ok(Flow::Break),
            Stmt::Continue => return Ok(Flow::Continue),
            Stmt::Throw(e) => {
                let v = self.eval(e, env)?;
                return Err(v);
            }
            Stmt::Try(body, catch, fin) => {
                let res = self.exec_block(body, env);
                let res = match (res, catch) {
                    (Err(e), Some((name, cbody))) => {
                        if self.steps > MAX_STEPS {
                            return Err(e);
                        }
                        if let Some(n) = name {
                            define(&func_scope(env), n, e);
                        }
                        self.exec_block(cbody, env)
                    }
                    (r, _) => r,
                };
                if let Some(f) = fin {
                    match self.exec_block(f, env)? {
                        Flow::Normal => {}
                        other => return Ok(other),
                    }
                }
                return res;
            }
            Stmt::Switch(d, cases) => {
                let dv = self.eval(d, env)?;
                let mut matched = None;
                for (i, (test, _)) in cases.iter().enumerate() {
                    if let Some(t) = test {
                        let tv = self.eval(t, env)?;
                        if strict_eq(&dv, &tv) {
                            matched = Some(i);
                            break;
                        }
                    }
                }
                let start = matched.or_else(|| cases.iter().position(|(t, _)| t.is_none()));
                if let Some(start) = start {
                    for (_, body) in &cases[start..] {
                        match self.exec_block(body, env)? {
                            Flow::Normal => {}
                            Flow::Break => break,
                            other => return Ok(other),
                        }
                    }
                }
            }
        }
        Ok(Flow::Normal)
    }

    pub fn eval(&mut self, e: &Expr, env: &Env) -> R<Value> {
        self.tick()?;
        Ok(match e {
            Expr::Num(n) => Value::Num(*n),
            Expr::Str(s) => Value::Str(Rc::from(&**s)),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Null => Value::Null,
            Expr::Undef => Value::Undef,
            Expr::This => lookup(env, "this").unwrap_or(Value::Undef),
            Expr::Ident(name) => match lookup(env, name) {
                Some(v) => v,
                None => return err(format!("{name} is not defined")),
            },
            Expr::Array(items) => {
                let mut v = Vec::with_capacity(items.len());
                for it in items {
                    v.push(self.eval(it, env)?);
                }
                arr(v)
            }
            Expr::Object(fields) => {
                let mut v = Vec::with_capacity(fields.len());
                for (k, it) in fields {
                    v.push((Rc::from(&**k), self.eval(it, env)?));
                }
                Value::Obj(Rc::new(RefCell::new(v)))
            }
            Expr::Member(o, name) => {
                let ov = self.eval(o, env)?;
                self.member(&ov, name)?
            }
            Expr::Index(o, i) => {
                let ov = self.eval(o, env)?;
                let iv = self.eval(i, env)?;
                self.index(&ov, &iv)?
            }
            Expr::Call(callee, args) => {
                let (f, this) = match &**callee {
                    Expr::Member(o, name) => {
                        let ov = self.eval(o, env)?;
                        (self.member(&ov, name)?, ov)
                    }
                    Expr::Index(o, i) => {
                        let ov = self.eval(o, env)?;
                        let iv = self.eval(i, env)?;
                        (self.index(&ov, &iv)?, ov)
                    }
                    other => (self.eval(other, env)?, Value::Undef),
                };
                let mut av = Vec::with_capacity(args.len());
                for a in args {
                    av.push(self.eval(a, env)?);
                }
                self.call(&f, this, &av)?
            }
            Expr::New(callee, args) => {
                // `new Array(n)` и подобные — как вызов.
                let f = self.eval(callee, env)?;
                let mut av = Vec::with_capacity(args.len());
                for a in args {
                    av.push(self.eval(a, env)?);
                }
                self.call(&f, Value::Undef, &av)?
            }
            Expr::Unary(op, a) => {
                let v = self.eval(a, env)?;
                match op {
                    UnOp::Not => Value::Bool(!truthy(&v)),
                    UnOp::Typeof => Value::Str(type_of(&v).into()),
                    UnOp::Plus => Value::Num(self.num(&v)),
                    UnOp::BitNot => Value::Num(!(self.num(&v) as i64 as i32) as f64),
                    UnOp::Neg => neg(&self.deref(v)),
                }
            }
            Expr::Update(step, prefix, target) => {
                let old = self.eval(target, env)?;
                let o = self.num(&old);
                let nv = Value::Num(o + step);
                self.store(target, nv.clone(), env)?;
                if *prefix {
                    nv
                } else {
                    Value::Num(o)
                }
            }
            Expr::Binary(op, a, b) => {
                let av = self.eval(a, env)?;
                let bv = self.eval(b, env)?;
                self.binop(*op, av, bv)?
            }
            Expr::And(a, b) => {
                let av = self.eval(a, env)?;
                if truthy(&av) {
                    self.eval(b, env)?
                } else {
                    av
                }
            }
            Expr::Or(a, b) => {
                let av = self.eval(a, env)?;
                if truthy(&av) {
                    av
                } else {
                    self.eval(b, env)?
                }
            }
            Expr::Nullish(a, b) => {
                let av = self.eval(a, env)?;
                if matches!(av, Value::Null | Value::Undef) {
                    self.eval(b, env)?
                } else {
                    av
                }
            }
            Expr::Cond(c, a, b) => {
                let cv = self.eval(c, env)?;
                if truthy(&cv) {
                    self.eval(a, env)?
                } else {
                    self.eval(b, env)?
                }
            }
            Expr::Assign(op, target, value) => {
                let v = self.eval(value, env)?;
                let v = match op {
                    None => v,
                    Some(op) => {
                        let old = self.eval(target, env)?;
                        self.binop(*op, old, v)?
                    }
                };
                self.store(target, v.clone(), env)?;
                v
            }
            Expr::Seq(items) => {
                let mut last = Value::Undef;
                for it in items {
                    last = self.eval(it, env)?;
                }
                last
            }
            Expr::Func(def) => Value::Func(Rc::new(Closure {
                def: def.clone(),
                env: env.clone(),
            })),
        })
    }

    fn store(&mut self, target: &Expr, v: Value, env: &Env) -> R<()> {
        match target {
            Expr::Ident(name) => {
                assign_var(env, name, v);
                Ok(())
            }
            Expr::Member(o, name) => {
                let ov = self.eval(o, env)?;
                set_member(&ov, name, v)
            }
            Expr::Index(o, i) => {
                let ov = self.eval(o, env)?;
                let iv = self.eval(i, env)?;
                match (&ov, &iv) {
                    (Value::Arr(a), Value::Num(n)) if *n >= 0.0 => {
                        let i = *n as usize;
                        let mut a = a.borrow_mut();
                        if i >= a.len() {
                            a.resize(i + 1, Value::Undef);
                        }
                        a[i] = v;
                        Ok(())
                    }
                    _ => set_member(&ov, &to_str(&iv), v),
                }
            }
            _ => err("присваивание не переменной"),
        }
    }

    /// Вызов функции.
    pub fn call(&mut self, f: &Value, this: Value, args: &[Value]) -> R<Value> {
        self.tick()?;
        match f {
            Value::Func(c) => {
                let env = Scope::new(Some(c.env.clone()), true);
                for (i, p) in c.def.params.iter().enumerate() {
                    define(&env, p, args.get(i).cloned().unwrap_or(Value::Undef));
                }
                define(&env, "this", this);
                define(&env, "arguments", arr(args.to_vec()));
                hoist(&c.def.body, &env);
                match self.exec_block(&c.def.body, &env)? {
                    Flow::Return(v) => Ok(v),
                    _ => Ok(Value::Undef),
                }
            }
            Value::Native(nf, bound) => {
                let this = if matches!(**bound, Value::Undef) {
                    this
                } else {
                    (**bound).clone()
                };
                nf(self, &this, args)
            }
            Value::Host(h) => host::call(self, *h, args),
            other => err(format!("{} is not a function", type_of(other))),
        }
    }

    /// Свойство After Effects → его значение (для арифметики).
    pub fn deref(&mut self, v: Value) -> Value {
        match v {
            Value::Host(h) => host::host_value(self, h).unwrap_or(Value::Host(h)),
            other => other,
        }
    }

    pub fn num(&mut self, v: &Value) -> f64 {
        to_num(&self.deref(v.clone()))
    }

    pub fn member(&mut self, o: &Value, name: &str) -> R<Value> {
        match o {
            Value::Undef | Value::Null => {
                err(format!("Cannot read property '{name}' of {}", type_of(o)))
            }
            Value::Arr(a) => Ok(match name {
                "length" => Value::Num(a.borrow().len() as f64),
                "push" => native(arr_push, o.clone()),
                "slice" => native(arr_slice, o.clone()),
                "concat" => native(arr_concat, o.clone()),
                "join" => native(arr_join, o.clone()),
                "indexOf" => native(arr_index_of, o.clone()),
                _ => match name.parse::<usize>() {
                    Ok(i) => a.borrow().get(i).cloned().unwrap_or(Value::Undef),
                    Err(_) => Value::Undef,
                },
            }),
            Value::Str(s) => Ok(match name {
                "length" => Value::Num(s.chars().count() as f64),
                _ => Value::Undef,
            }),
            Value::Obj(fields) => Ok(fields
                .borrow()
                .iter()
                .find(|(k, _)| &**k == name)
                .map(|(_, v)| v.clone())
                .unwrap_or(Value::Undef)),
            Value::Host(h) => host::member(self, *h, name),
            Value::Num(_) => Ok(match name {
                "toFixed" => native(num_to_fixed, o.clone()),
                _ => Value::Undef,
            }),
            _ => Ok(Value::Undef),
        }
    }

    pub fn index(&mut self, o: &Value, i: &Value) -> R<Value> {
        match (o, i) {
            (Value::Arr(a), Value::Num(n)) => Ok(if *n >= 0.0 && n.fract() == 0.0 {
                a.borrow().get(*n as usize).cloned().unwrap_or(Value::Undef)
            } else {
                Value::Undef
            }),
            (Value::Str(s), Value::Num(n)) => Ok(s
                .chars()
                .nth(*n as usize)
                .map(|c| Value::Str(c.to_string().into()))
                .unwrap_or(Value::Undef)),
            (Value::Host(h), _) => host::index(self, *h, i),
            _ => self.member(o, &to_str(i)),
        }
    }

    pub fn binop(&mut self, op: BinOp, a: Value, b: Value) -> R<Value> {
        let a = self.deref(a);
        let b = self.deref(b);
        Ok(match op {
            BinOp::Add => {
                if matches!(a, Value::Str(_)) || matches!(b, Value::Str(_)) {
                    Value::Str(format!("{}{}", to_str(&a), to_str(&b)).into())
                } else {
                    vec_op(&a, &b, |x, y| x + y, true)
                }
            }
            BinOp::Sub => vec_op(&a, &b, |x, y| x - y, true),
            BinOp::Mul => vec_op(&a, &b, |x, y| x * y, false),
            BinOp::Div => vec_op(&a, &b, |x, y| x / y, false),
            BinOp::Mod => vec_op(&a, &b, |x, y| x % y, false),
            BinOp::Pow => Value::Num(to_num(&a).powf(to_num(&b))),
            BinOp::Eq => Value::Bool(loose_eq(&a, &b)),
            BinOp::Ne => Value::Bool(!loose_eq(&a, &b)),
            BinOp::StrictEq => Value::Bool(strict_eq(&a, &b)),
            BinOp::StrictNe => Value::Bool(!strict_eq(&a, &b)),
            BinOp::Lt => Value::Bool(compare(&a, &b, |o| o.is_lt())),
            BinOp::Le => Value::Bool(compare(&a, &b, |o| o.is_le())),
            BinOp::Gt => Value::Bool(compare(&a, &b, |o| o.is_gt())),
            BinOp::Ge => Value::Bool(compare(&a, &b, |o| o.is_ge())),
            BinOp::BitAnd => {
                Value::Num(((to_num(&a) as i64 as i32) & (to_num(&b) as i64 as i32)) as f64)
            }
            BinOp::BitOr => {
                Value::Num(((to_num(&a) as i64 as i32) | (to_num(&b) as i64 as i32)) as f64)
            }
            BinOp::BitXor => {
                Value::Num(((to_num(&a) as i64 as i32) ^ (to_num(&b) as i64 as i32)) as f64)
            }
            BinOp::Shl => {
                Value::Num(((to_num(&a) as i64 as i32) << ((to_num(&b) as i64) & 31)) as f64)
            }
            BinOp::Shr => {
                Value::Num(((to_num(&a) as i64 as i32) >> ((to_num(&b) as i64) & 31)) as f64)
            }
            BinOp::UShr => {
                Value::Num(((to_num(&a) as i64 as u32) >> ((to_num(&b) as i64) & 31)) as f64)
            }
        })
    }
}

/// Поднять объявления функций в начало области.
fn hoist(body: &[Stmt], env: &Env) {
    for s in body {
        if let Stmt::Func(def) = s {
            if let Some(name) = &def.name {
                define(
                    env,
                    name,
                    Value::Func(Rc::new(Closure {
                        def: def.clone(),
                        env: env.clone(),
                    })),
                );
            }
        }
    }
}

pub fn arr(v: Vec<Value>) -> Value {
    Value::Arr(Rc::new(RefCell::new(v)))
}

pub fn num_arr(v: &[f64]) -> Value {
    arr(v.iter().map(|x| Value::Num(*x)).collect())
}

pub fn native(f: NativeFn, this: Value) -> Value {
    Value::Native(f, Rc::new(this))
}

pub fn obj(fields: Vec<(&str, Value)>) -> Value {
    Value::Obj(Rc::new(RefCell::new(
        fields.into_iter().map(|(k, v)| (Rc::from(k), v)).collect(),
    )))
}

fn set_member(o: &Value, name: &str, v: Value) -> R<()> {
    match o {
        Value::Obj(fields) => {
            let mut f = fields.borrow_mut();
            if let Some(slot) = f.iter_mut().find(|(k, _)| &**k == name) {
                slot.1 = v;
            } else {
                f.push((name.into(), v));
            }
            Ok(())
        }
        Value::Undef | Value::Null => {
            err(format!("Cannot set property '{name}' of {}", type_of(o)))
        }
        // Свойства After Effects и прочее из выражения не меняются.
        _ => Ok(()),
    }
}

pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Undef | Value::Null => false,
        Value::Bool(b) => *b,
        Value::Num(n) => *n != 0.0 && !n.is_nan(),
        Value::Str(s) => !s.is_empty(),
        _ => true,
    }
}

pub fn to_num(v: &Value) -> f64 {
    match v {
        Value::Num(n) => *n,
        Value::Bool(b) => *b as i32 as f64,
        Value::Null => 0.0,
        Value::Str(s) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse().unwrap_or(f64::NAN)
            }
        }
        Value::Arr(a) => {
            let a = a.borrow();
            match a.len() {
                0 => 0.0,
                1 => to_num(&a[0]),
                _ => f64::NAN,
            }
        }
        _ => f64::NAN,
    }
}

pub fn to_str(v: &Value) -> String {
    match v {
        Value::Undef => "undefined".into(),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => format_num(*n),
        Value::Str(s) => s.to_string(),
        Value::Arr(a) => a.borrow().iter().map(to_str).collect::<Vec<_>>().join(","),
        _ => format!("{v:?}"),
    }
}

fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Undef => "undefined",
        Value::Null | Value::Arr(_) | Value::Obj(_) | Value::Path(_) | Value::Host(_) => "object",
        Value::Bool(_) => "boolean",
        Value::Num(_) => "number",
        Value::Str(_) => "string",
        Value::Func(_) | Value::Native(..) => "function",
    }
}

/// Числа массива (для векторной арифметики); `None` — не числовой массив.
pub fn as_nums(v: &Value) -> Option<Vec<f64>> {
    match v {
        Value::Arr(a) => Some(a.borrow().iter().map(to_num).collect()),
        _ => None,
    }
}

/// Арифметика After Effects: число с числом, массив с массивом
/// (поэлементно; для сложения/вычитания недостающие — 0), массив с числом.
fn vec_op(a: &Value, b: &Value, f: impl Fn(f64, f64) -> f64, additive: bool) -> Value {
    match (as_nums(a), as_nums(b)) {
        (None, None) => Value::Num(f(to_num(a), to_num(b))),
        (Some(x), None) => {
            let y = to_num(b);
            num_arr(&x.iter().map(|v| f(*v, y)).collect::<Vec<_>>())
        }
        (None, Some(y)) => {
            let x = to_num(a);
            num_arr(&y.iter().map(|v| f(x, *v)).collect::<Vec<_>>())
        }
        (Some(x), Some(y)) => {
            let n = if additive {
                x.len().max(y.len())
            } else {
                x.len().min(y.len())
            };
            num_arr(
                &(0..n)
                    .map(|i| {
                        f(
                            x.get(i).copied().unwrap_or(0.0),
                            y.get(i).copied().unwrap_or(0.0),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        }
    }
}

fn neg(v: &Value) -> Value {
    match as_nums(v) {
        Some(x) => num_arr(&x.iter().map(|v| -v).collect::<Vec<_>>()),
        None => Value::Num(-to_num(v)),
    }
}

fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undef, Value::Undef) | (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Arr(x), Value::Arr(y)) => Rc::ptr_eq(x, y),
        (Value::Obj(x), Value::Obj(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

fn loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undef | Value::Null, Value::Undef | Value::Null) => true,
        (Value::Undef | Value::Null, _) | (_, Value::Undef | Value::Null) => false,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Arr(_), Value::Arr(_)) | (Value::Obj(_), Value::Obj(_)) => strict_eq(a, b),
        _ => to_num(a) == to_num(b),
    }
}

fn compare(a: &Value, b: &Value, f: impl Fn(std::cmp::Ordering) -> bool) -> bool {
    if let (Value::Str(x), Value::Str(y)) = (a, b) {
        return f(x.cmp(y));
    }
    match to_num(a).partial_cmp(&to_num(b)) {
        Some(o) => f(o),
        None => false,
    }
}

fn arr_push(_: &mut Interp, this: &Value, args: &[Value]) -> R<Value> {
    if let Value::Arr(a) = this {
        a.borrow_mut().extend(args.iter().cloned());
        return Ok(Value::Num(a.borrow().len() as f64));
    }
    Ok(Value::Undef)
}

fn arr_slice(_: &mut Interp, this: &Value, args: &[Value]) -> R<Value> {
    if let Value::Arr(a) = this {
        let a = a.borrow();
        let n = a.len() as i64;
        let norm = |v: Option<&Value>, d: i64| -> usize {
            let x = v.map(|v| to_num(v) as i64).unwrap_or(d);
            (if x < 0 { (n + x).max(0) } else { x.min(n) }) as usize
        };
        let s = norm(args.first(), 0);
        let e = norm(args.get(1), n);
        return Ok(arr(if s < e { a[s..e].to_vec() } else { Vec::new() }));
    }
    Ok(Value::Undef)
}

fn arr_concat(_: &mut Interp, this: &Value, args: &[Value]) -> R<Value> {
    if let Value::Arr(a) = this {
        let mut v = a.borrow().clone();
        for x in args {
            match x {
                Value::Arr(b) => v.extend(b.borrow().iter().cloned()),
                other => v.push(other.clone()),
            }
        }
        return Ok(arr(v));
    }
    Ok(Value::Undef)
}

fn arr_join(_: &mut Interp, this: &Value, args: &[Value]) -> R<Value> {
    if let Value::Arr(a) = this {
        let sep = args.first().map(to_str).unwrap_or_else(|| ",".into());
        let s = a.borrow().iter().map(to_str).collect::<Vec<_>>().join(&sep);
        return Ok(Value::Str(s.into()));
    }
    Ok(Value::Undef)
}

fn arr_index_of(_: &mut Interp, this: &Value, args: &[Value]) -> R<Value> {
    if let (Value::Arr(a), Some(x)) = (this, args.first()) {
        let i = a.borrow().iter().position(|v| strict_eq(v, x));
        return Ok(Value::Num(i.map(|i| i as f64).unwrap_or(-1.0)));
    }
    Ok(Value::Num(-1.0))
}

fn num_to_fixed(_: &mut Interp, this: &Value, args: &[Value]) -> R<Value> {
    let d = args.first().map(to_num).unwrap_or(0.0).clamp(0.0, 20.0) as usize;
    Ok(Value::Str(format!("{:.*}", d, to_num(this)).into()))
}

// ---------------------------------------------------------------------------
// Встроенные функции.

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Undef)
}

fn math1(args: &[Value], f: fn(f64) -> f64) -> R<Value> {
    Ok(Value::Num(f(to_num(&arg(args, 0)))))
}

macro_rules! m1 {
    ($name:ident, $f:expr) => {
        fn $name(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
            let a: Vec<Value> = a.iter().map(|v| it.deref(v.clone())).collect();
            math1(&a, $f)
        }
    };
}

m1!(m_sin, f64::sin);
m1!(m_cos, f64::cos);
m1!(m_tan, f64::tan);
m1!(m_asin, f64::asin);
m1!(m_acos, f64::acos);
m1!(m_atan, f64::atan);
m1!(m_sqrt, f64::sqrt);
m1!(m_exp, f64::exp);
m1!(m_log, f64::ln);
m1!(m_abs, f64::abs);
m1!(m_floor, f64::floor);
m1!(m_ceil, f64::ceil);
m1!(m_round, |x: f64| (x + 0.5).floor());
m1!(m_sign, f64::signum);
m1!(m_trunc, f64::trunc);
m1!(m_deg2rad, f64::to_radians);
m1!(m_rad2deg, f64::to_degrees);

fn nums(it: &mut Interp, a: &[Value]) -> Vec<f64> {
    a.iter().map(|v| it.num(v)).collect()
}

fn m_atan2(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let n = nums(it, a);
    Ok(Value::Num(
        n.first()
            .copied()
            .unwrap_or(f64::NAN)
            .atan2(n.get(1).copied().unwrap_or(f64::NAN)),
    ))
}

fn m_pow(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let n = nums(it, a);
    Ok(Value::Num(
        n.first()
            .copied()
            .unwrap_or(f64::NAN)
            .powf(n.get(1).copied().unwrap_or(f64::NAN)),
    ))
}

fn m_min(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    Ok(Value::Num(
        nums(it, a).into_iter().fold(f64::INFINITY, f64::min),
    ))
}

fn m_max(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    Ok(Value::Num(
        nums(it, a).into_iter().fold(f64::NEG_INFINITY, f64::max),
    ))
}

fn m_random(it: &mut Interp, _: &Value, _: &[Value]) -> R<Value> {
    Ok(Value::Num(it.frame.next_random()))
}

/// `random()`, `random(max)`, `random(min, max)` — числа и массивы.
fn g_random(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let a: Vec<Value> = a.iter().map(|v| it.deref(v.clone())).collect();
    let (lo, hi) = match a.len() {
        0 => (Value::Num(0.0), Value::Num(1.0)),
        1 => (
            match as_nums(&a[0]) {
                Some(v) => num_arr(&vec![0.0; v.len()]),
                None => Value::Num(0.0),
            },
            a[0].clone(),
        ),
        _ => (a[0].clone(), a[1].clone()),
    };
    match (as_nums(&lo), as_nums(&hi)) {
        (None, None) => {
            let r = it.frame.next_random();
            Ok(Value::Num(to_num(&lo) + (to_num(&hi) - to_num(&lo)) * r))
        }
        (l, h) => {
            let h = h.unwrap_or_default();
            let l = l.unwrap_or_else(|| vec![to_num(&lo); h.len()]);
            let n = l.len().max(h.len());
            let v: Vec<f64> = (0..n)
                .map(|i| {
                    let (x, y) = (
                        l.get(i).copied().unwrap_or(0.0),
                        h.get(i).copied().unwrap_or(0.0),
                    );
                    x + (y - x) * it.frame.next_random()
                })
                .collect();
            Ok(num_arr(&v))
        }
    }
}

fn g_gauss_random(it: &mut Interp, s: &Value, a: &[Value]) -> R<Value> {
    // Сумма трёх равномерных — приближение нормального распределения.
    let r = (it.frame.next_random() + it.frame.next_random() + it.frame.next_random()) / 3.0;
    let _ = s;
    let a: Vec<Value> = a.iter().map(|v| it.deref(v.clone())).collect();
    match a.len() {
        0 => Ok(Value::Num(r)),
        1 => Ok(vec_op(&a[0], &Value::Num(r), |x, y| x * y, false)),
        _ => {
            let span = vec_op(&a[1], &a[0], |x, y| x - y, true);
            Ok(vec_op(
                &a[0],
                &vec_op(&span, &Value::Num(r), |x, y| x * y, false),
                |x, y| x + y,
                true,
            ))
        }
    }
}

fn g_seed_random(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let seed = it.num(&arg(a, 0));
    let timeless = truthy(&arg(a, 1));
    it.frame.seed_random(seed, timeless);
    Ok(Value::Undef)
}

fn bm_sum(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    it.binop(BinOp::Add, arg(a, 0), arg(a, 1))
}
fn bm_sub(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    it.binop(BinOp::Sub, arg(a, 0), arg(a, 1))
}
fn bm_mul(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    it.binop(BinOp::Mul, arg(a, 0), arg(a, 1))
}
fn bm_div(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    it.binop(BinOp::Div, arg(a, 0), arg(a, 1))
}
fn bm_mod(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    it.binop(BinOp::Mod, arg(a, 0), arg(a, 1))
}
fn bm_neg(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let v = it.deref(arg(a, 0));
    Ok(neg(&v))
}
fn bm_is_array(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let v = it.deref(arg(a, 0));
    Ok(Value::Bool(matches!(v, Value::Arr(_))))
}

fn g_clamp(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let v = it.deref(arg(a, 0));
    let lo = it.deref(arg(a, 1));
    let hi = it.deref(arg(a, 2));
    match as_nums(&v) {
        Some(x) => {
            let l = as_nums(&lo);
            let h = as_nums(&hi);
            let r: Vec<f64> = x
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let lo = l
                        .as_ref()
                        .map(|l| l.get(i).copied().unwrap_or(f64::MIN))
                        .unwrap_or(to_num(&lo));
                    let hi = h
                        .as_ref()
                        .map(|h| h.get(i).copied().unwrap_or(f64::MAX))
                        .unwrap_or(to_num(&hi));
                    v.max(lo).min(hi)
                })
                .collect();
            Ok(num_arr(&r))
        }
        None => {
            let (l, h) = (to_num(&lo), to_num(&hi));
            let (l, h) = if l <= h { (l, h) } else { (h, l) };
            Ok(Value::Num(to_num(&v).max(l).min(h)))
        }
    }
}

/// Общая интерполяция `linear/ease/easeIn/easeOut(t, [tMin, tMax,] v1, v2)`.
fn interp_fn(it: &mut Interp, a: &[Value], ease: fn(f64) -> f64) -> R<Value> {
    let a: Vec<Value> = a.iter().map(|v| it.deref(v.clone())).collect();
    let t = to_num(&arg(&a, 0));
    let (t0, t1, v1, v2) = if a.len() >= 5 {
        (to_num(&a[1]), to_num(&a[2]), a[3].clone(), a[4].clone())
    } else {
        (0.0, 1.0, arg(&a, 1), arg(&a, 2))
    };
    let p = if t1 == t0 {
        if t >= t1 {
            1.0
        } else {
            0.0
        }
    } else {
        ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
    };
    let k = ease(if t1 < t0 { 1.0 - p } else { p });
    let d = vec_op(&v2, &v1, |x, y| x - y, true);
    Ok(vec_op(
        &v1,
        &vec_op(&d, &Value::Num(k), |x, y| x * y, false),
        |x, y| x + y,
        true,
    ))
}

fn g_linear(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    interp_fn(it, a, |p| p)
}
fn g_ease(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    interp_fn(it, a, |p| {
        crate::lottie::anim::cubic_bezier_ease(0.33, 0.0, 0.67, 1.0, p as f32) as f64
    })
}
fn g_ease_in(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    interp_fn(it, a, |p| {
        crate::lottie::anim::cubic_bezier_ease(0.33, 0.0, 1.0, 1.0, p as f32) as f64
    })
}
fn g_ease_out(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    interp_fn(it, a, |p| {
        crate::lottie::anim::cubic_bezier_ease(0.0, 0.0, 0.67, 1.0, p as f32) as f64
    })
}

fn g_length(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let x = it.deref(arg(a, 0));
    let v = if a.len() >= 2 {
        let y = it.deref(arg(a, 1));
        vec_op(&x, &y, |p, q| p - q, true)
    } else {
        x
    };
    let n = as_nums(&v).unwrap_or_else(|| vec![to_num(&v)]);
    Ok(Value::Num(n.iter().map(|c| c * c).sum::<f64>().sqrt()))
}

fn g_normalize(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let v = it.deref(arg(a, 0));
    let n = as_nums(&v).unwrap_or_else(|| vec![to_num(&v)]);
    let l = n.iter().map(|c| c * c).sum::<f64>().sqrt();
    if l == 0.0 {
        return Ok(num_arr(&n));
    }
    Ok(num_arr(&n.iter().map(|c| c / l).collect::<Vec<_>>()))
}

fn g_dot(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let x = it.deref(arg(a, 0));
    let y = it.deref(arg(a, 1));
    let (x, y) = (
        as_nums(&x).unwrap_or_default(),
        as_nums(&y).unwrap_or_default(),
    );
    Ok(Value::Num(x.iter().zip(&y).map(|(p, q)| p * q).sum()))
}

fn g_parse_float(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let v = it.deref(arg(a, 0));
    let s = to_str(&v);
    let t = s.trim();
    let end = t
        .char_indices()
        .take_while(|(i, c)| {
            c.is_ascii_digit()
                || *c == '.'
                || ((*c == '-' || *c == '+') && *i == 0)
                || *c == 'e'
                || *c == 'E'
        })
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    Ok(Value::Num(t[..end].parse().unwrap_or(f64::NAN)))
}

fn g_parse_int(it: &mut Interp, s: &Value, a: &[Value]) -> R<Value> {
    let v = g_parse_float(it, s, a)?;
    Ok(Value::Num(to_num(&v).trunc()))
}

fn g_is_nan(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    Ok(Value::Bool(it.num(&arg(a, 0)).is_nan()))
}

fn g_number(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    Ok(Value::Num(if a.is_empty() { 0.0 } else { it.num(&a[0]) }))
}

fn g_string(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    let v = it.deref(arg(a, 0));
    Ok(Value::Str(to_str(&v).into()))
}

fn g_array(it: &mut Interp, _: &Value, a: &[Value]) -> R<Value> {
    if a.len() == 1 {
        if let Value::Num(n) = a[0] {
            return Ok(arr(vec![Value::Undef; n.max(0.0) as usize]));
        }
    }
    let _ = it;
    Ok(arr(a.to_vec()))
}

thread_local! {
    static GLOBALS: Env = build_globals();
}

fn build_globals() -> Env {
    let g = Scope::new(None, false);
    let math = obj(vec![
        ("PI", Value::Num(std::f64::consts::PI)),
        ("E", Value::Num(std::f64::consts::E)),
        ("LN2", Value::Num(std::f64::consts::LN_2)),
        ("LN10", Value::Num(std::f64::consts::LN_10)),
        ("SQRT2", Value::Num(std::f64::consts::SQRT_2)),
        ("sin", native(m_sin, Value::Undef)),
        ("cos", native(m_cos, Value::Undef)),
        ("tan", native(m_tan, Value::Undef)),
        ("asin", native(m_asin, Value::Undef)),
        ("acos", native(m_acos, Value::Undef)),
        ("atan", native(m_atan, Value::Undef)),
        ("atan2", native(m_atan2, Value::Undef)),
        ("sqrt", native(m_sqrt, Value::Undef)),
        ("pow", native(m_pow, Value::Undef)),
        ("exp", native(m_exp, Value::Undef)),
        ("log", native(m_log, Value::Undef)),
        ("abs", native(m_abs, Value::Undef)),
        ("floor", native(m_floor, Value::Undef)),
        ("ceil", native(m_ceil, Value::Undef)),
        ("round", native(m_round, Value::Undef)),
        ("sign", native(m_sign, Value::Undef)),
        ("trunc", native(m_trunc, Value::Undef)),
        ("min", native(m_min, Value::Undef)),
        ("max", native(m_max, Value::Undef)),
        ("random", native(m_random, Value::Undef)),
    ]);
    define(&g, "Math", math);
    let funcs: &[(&str, NativeFn)] = &[
        ("$bm_sum", bm_sum),
        ("sum", bm_sum),
        ("add", bm_sum),
        ("$bm_sub", bm_sub),
        ("sub", bm_sub),
        ("$bm_mul", bm_mul),
        ("mul", bm_mul),
        ("$bm_div", bm_div),
        ("div", bm_div),
        ("$bm_mod", bm_mod),
        ("mod", bm_mod),
        ("$bm_neg", bm_neg),
        ("$bm_isInstanceOfArray", bm_is_array),
        ("clamp", g_clamp),
        ("linear", g_linear),
        ("ease", g_ease),
        ("easeIn", g_ease_in),
        ("easeOut", g_ease_out),
        ("length", g_length),
        ("normalize", g_normalize),
        ("dot", g_dot),
        ("random", g_random),
        ("gaussRandom", g_gauss_random),
        ("seedRandom", g_seed_random),
        ("degreesToRadians", m_deg2rad),
        ("radiansToDegrees", m_rad2deg),
        ("parseFloat", g_parse_float),
        ("parseInt", g_parse_int),
        ("isNaN", g_is_nan),
        ("Number", g_number),
        ("String", g_string),
        ("Array", g_array),
    ];
    for (name, f) in funcs {
        define(&g, name, native(*f, Value::Undef));
    }
    host::install_globals(&g);
    g
}

/// Новая область вычисления поверх общих встроенных.
pub fn eval_scope() -> Env {
    GLOBALS.with(|g| Scope::new(Some(g.clone()), true))
}
