//! Лексер и парсер подмножества JavaScript, на котором написаны выражения
//! After Effects в экспорте bodymovin: `var`, функции, `if`/`switch`/циклы,
//! `try`/`catch`, все обычные операторы (включая запятую, тернарный,
//! составное присваивание, `++`/`--`), литералы массивов и объектов.

use std::sync::Arc as Rc;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(Rc<str>),
    Ident(Rc<str>),
    Punct(&'static str),
    Eof,
}

const PUNCTS: &[&str] = &[
    ">>>=", "===", "!==", "**=", ">>>", "<<=", ">>=", "...", "==", "!=", "<=", ">=", "&&", "||",
    "??", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "**", "=>", "<<", ">>", "+",
    "-", "*", "/", "%", "=", "<", ">", "!", "?", ":", ",", ".", ";", "(", ")", "[", "]", "{", "}",
    "&", "|", "^", "~",
];

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            if c == b'0' && matches!(b.get(i + 1), Some(b'x' | b'X')) {
                i += 2;
                while i < b.len() && b[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let v = i64::from_str_radix(&src[start + 2..i], 16).map_err(|e| e.to_string())?;
                out.push(Tok::Num(v as f64));
                continue;
            }
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                i += 1;
                if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
                    i += 1;
                }
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let v: f64 = src[start..i]
                .parse()
                .map_err(|_| format!("число `{}`", &src[start..i]))?;
            out.push(Tok::Num(v));
            continue;
        }
        if c == b'"' || c == b'\'' {
            let q = c;
            i += 1;
            let mut s = String::new();
            let mut chars = src[i..].char_indices();
            let mut end = None;
            while let Some((k, ch)) = chars.next() {
                if ch as u32 == q as u32 {
                    end = Some(i + k + 1);
                    break;
                }
                if ch == '\\' {
                    let Some((_, e)) = chars.next() else { break };
                    s.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '0' => '\0',
                        other => other,
                    });
                } else {
                    s.push(ch);
                }
            }
            i = end.ok_or("незакрытая строка")?;
            out.push(Tok::Str(s.into()));
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80 {
            let start = i;
            while i < b.len()
                && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$' || b[i] >= 0x80)
            {
                i += 1;
            }
            out.push(Tok::Ident(src[start..i].into()));
            continue;
        }
        let rest = &src[i..];
        match PUNCTS.iter().find(|p| rest.starts_with(**p)) {
            Some(p) => {
                out.push(Tok::Punct(p));
                i += p.len();
            }
            None => return Err(format!("неожиданный символ `{}`", c as char)),
        }
    }
    out.push(Tok::Eof);
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    StrictEq,
    StrictNe,
    Lt,
    Le,
    Gt,
    Ge,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Plus,
    Not,
    BitNot,
    Typeof,
}

#[derive(Debug)]
pub struct FuncDef {
    pub name: Option<Rc<str>>,
    pub params: Vec<Rc<str>>,
    pub body: Vec<Stmt>,
}

#[derive(Debug)]
pub enum Expr {
    Num(f64),
    Str(Rc<str>),
    Bool(bool),
    Null,
    Undef,
    Ident(Rc<str>),
    This,
    Array(Vec<Expr>),
    Object(Vec<(Rc<str>, Expr)>),
    Member(Box<Expr>, Rc<str>),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    New(Box<Expr>, Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    /// `++`/`--`: шаг ±1, префиксная форма.
    Update(f64, bool, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Nullish(Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Assign(Option<BinOp>, Box<Expr>, Box<Expr>),
    Seq(Vec<Expr>),
    Func(Rc<FuncDef>),
}

#[derive(Debug)]
pub enum Stmt {
    Expr(Expr),
    Var(Vec<(Rc<str>, Option<Expr>)>),
    Func(Rc<FuncDef>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    Block(Vec<Stmt>),
    Return(Option<Expr>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    Break,
    Continue,
    Throw(Expr),
    Try(
        Vec<Stmt>,
        Option<(Option<Rc<str>>, Vec<Stmt>)>,
        Option<Vec<Stmt>>,
    ),
    Switch(Expr, Vec<(Option<Expr>, Vec<Stmt>)>),
    Empty,
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

type PResult<T> = Result<T, String>;

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos]
    }

    fn peek_at(&self, k: usize) -> &Tok {
        self.toks.get(self.pos + k).unwrap_or(&Tok::Eof)
    }

    fn next(&mut self) -> Tok {
        let t = self.toks[self.pos].clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::Punct(q) if *q == p)
    }

    fn is_kw(&self, k: &str) -> bool {
        matches!(self.peek(), Tok::Ident(q) if &**q == k)
    }

    fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.next();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, k: &str) -> bool {
        if self.is_kw(k) {
            self.next();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, p: &str) -> PResult<()> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(format!("ожидалось `{p}`, а не {:?}", self.peek()))
        }
    }

    fn ident(&mut self) -> PResult<Rc<str>> {
        match self.next() {
            Tok::Ident(s) => Ok(s),
            t => Err(format!("ожидалось имя, а не {t:?}")),
        }
    }

    fn semi(&mut self) {
        self.eat(";");
    }

    fn program(&mut self) -> PResult<Vec<Stmt>> {
        let mut out = Vec::new();
        while *self.peek() != Tok::Eof {
            out.push(self.stmt()?);
        }
        Ok(out)
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect("{")?;
        let mut out = Vec::new();
        while !self.is("}") {
            if *self.peek() == Tok::Eof {
                return Err("незакрытый блок".into());
            }
            out.push(self.stmt()?);
        }
        self.next();
        Ok(out)
    }

    fn var_decl(&mut self) -> PResult<Stmt> {
        let mut decls = Vec::new();
        loop {
            let name = self.ident()?;
            let init = if self.eat("=") {
                Some(self.assign()?)
            } else {
                None
            };
            decls.push((name, init));
            if !self.eat(",") {
                break;
            }
        }
        Ok(Stmt::Var(decls))
    }

    fn function(&mut self) -> PResult<Rc<FuncDef>> {
        let name = match self.peek() {
            Tok::Ident(_) => Some(self.ident()?),
            _ => None,
        };
        self.expect("(")?;
        let mut params = Vec::new();
        while !self.is(")") {
            params.push(self.ident()?);
            // Значения по умолчанию не поддерживаем — пропускаем выражение.
            if self.eat("=") {
                self.assign()?;
            }
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        let body = self.block()?;
        Ok(Rc::new(FuncDef { name, params, body }))
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        if self.is("{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.eat(";") {
            return Ok(Stmt::Empty);
        }
        if let Tok::Ident(k) = self.peek().clone() {
            match &*k {
                "var" | "let" | "const" => {
                    self.next();
                    let s = self.var_decl()?;
                    self.semi();
                    return Ok(s);
                }
                "function" if matches!(self.peek_at(1), Tok::Ident(_)) => {
                    self.next();
                    return Ok(Stmt::Func(self.function()?));
                }
                "if" => {
                    self.next();
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    let then = Box::new(self.stmt()?);
                    let els = if self.eat_kw("else") {
                        Some(Box::new(self.stmt()?))
                    } else {
                        None
                    };
                    return Ok(Stmt::If(c, then, els));
                }
                "return" => {
                    self.next();
                    let v = if self.is(";") || self.is("}") || *self.peek() == Tok::Eof {
                        None
                    } else {
                        Some(self.expr()?)
                    };
                    self.semi();
                    return Ok(Stmt::Return(v));
                }
                "while" => {
                    self.next();
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    return Ok(Stmt::While(c, Box::new(self.stmt()?)));
                }
                "do" => {
                    self.next();
                    let body = Box::new(self.stmt()?);
                    if !self.eat_kw("while") {
                        return Err("ожидалось `while`".into());
                    }
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    self.semi();
                    return Ok(Stmt::DoWhile(body, c));
                }
                "for" => {
                    self.next();
                    self.expect("(")?;
                    let init = if self.is(";") {
                        None
                    } else if self.is_kw("var") || self.is_kw("let") || self.is_kw("const") {
                        self.next();
                        Some(Box::new(self.var_decl()?))
                    } else {
                        Some(Box::new(Stmt::Expr(self.expr()?)))
                    };
                    self.expect(";")?;
                    let cond = if self.is(";") {
                        None
                    } else {
                        Some(self.expr()?)
                    };
                    self.expect(";")?;
                    let upd = if self.is(")") {
                        None
                    } else {
                        Some(self.expr()?)
                    };
                    self.expect(")")?;
                    return Ok(Stmt::For(init, cond, upd, Box::new(self.stmt()?)));
                }
                "break" => {
                    self.next();
                    self.semi();
                    return Ok(Stmt::Break);
                }
                "continue" => {
                    self.next();
                    self.semi();
                    return Ok(Stmt::Continue);
                }
                "throw" => {
                    self.next();
                    let e = self.expr()?;
                    self.semi();
                    return Ok(Stmt::Throw(e));
                }
                "try" => {
                    self.next();
                    let body = self.block()?;
                    let catch = if self.eat_kw("catch") {
                        let name = if self.eat("(") {
                            let n = self.ident()?;
                            self.expect(")")?;
                            Some(n)
                        } else {
                            None
                        };
                        Some((name, self.block()?))
                    } else {
                        None
                    };
                    let fin = if self.eat_kw("finally") {
                        Some(self.block()?)
                    } else {
                        None
                    };
                    return Ok(Stmt::Try(body, catch, fin));
                }
                "switch" => {
                    self.next();
                    self.expect("(")?;
                    let d = self.expr()?;
                    self.expect(")")?;
                    self.expect("{")?;
                    let mut cases = Vec::new();
                    while !self.eat("}") {
                        let test = if self.eat_kw("case") {
                            Some(self.expr()?)
                        } else if self.eat_kw("default") {
                            None
                        } else {
                            return Err("ожидалось `case`".into());
                        };
                        self.expect(":")?;
                        let mut body = Vec::new();
                        while !self.is_kw("case") && !self.is_kw("default") && !self.is("}") {
                            body.push(self.stmt()?);
                        }
                        cases.push((test, body));
                    }
                    return Ok(Stmt::Switch(d, cases));
                }
                _ => {}
            }
        }
        let e = self.expr()?;
        self.semi();
        Ok(Stmt::Expr(e))
    }

    fn expr(&mut self) -> PResult<Expr> {
        let first = self.assign()?;
        if !self.is(",") {
            return Ok(first);
        }
        let mut v = vec![first];
        while self.eat(",") {
            v.push(self.assign()?);
        }
        Ok(Expr::Seq(v))
    }

    fn assign(&mut self) -> PResult<Expr> {
        let lhs = self.cond()?;
        let op = match self.peek() {
            Tok::Punct("=") => None,
            Tok::Punct("+=") => Some(BinOp::Add),
            Tok::Punct("-=") => Some(BinOp::Sub),
            Tok::Punct("*=") => Some(BinOp::Mul),
            Tok::Punct("/=") => Some(BinOp::Div),
            Tok::Punct("%=") => Some(BinOp::Mod),
            Tok::Punct("**=") => Some(BinOp::Pow),
            Tok::Punct("&=") => Some(BinOp::BitAnd),
            Tok::Punct("|=") => Some(BinOp::BitOr),
            Tok::Punct("^=") => Some(BinOp::BitXor),
            Tok::Punct("<<=") => Some(BinOp::Shl),
            Tok::Punct(">>=") => Some(BinOp::Shr),
            Tok::Punct(">>>=") => Some(BinOp::UShr),
            _ => return Ok(lhs),
        };
        if !matches!(lhs, Expr::Ident(_) | Expr::Member(..) | Expr::Index(..)) {
            return Err("присваивание не переменной".into());
        }
        self.next();
        let rhs = self.assign()?;
        Ok(Expr::Assign(op, Box::new(lhs), Box::new(rhs)))
    }

    fn cond(&mut self) -> PResult<Expr> {
        let c = self.binary(0)?;
        if self.eat("?") {
            let a = self.assign()?;
            self.expect(":")?;
            let b = self.assign()?;
            return Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }

    /// Бинарные операторы по приоритету (возрастающему).
    fn binary(&mut self, min: u8) -> PResult<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let (prec, op) = match self.peek() {
                Tok::Punct("??") => (1, None),
                Tok::Punct("||") => (2, None),
                Tok::Punct("&&") => (3, None),
                Tok::Punct("|") => (4, Some(BinOp::BitOr)),
                Tok::Punct("^") => (5, Some(BinOp::BitXor)),
                Tok::Punct("&") => (6, Some(BinOp::BitAnd)),
                Tok::Punct("==") => (7, Some(BinOp::Eq)),
                Tok::Punct("!=") => (7, Some(BinOp::Ne)),
                Tok::Punct("===") => (7, Some(BinOp::StrictEq)),
                Tok::Punct("!==") => (7, Some(BinOp::StrictNe)),
                Tok::Punct("<") => (8, Some(BinOp::Lt)),
                Tok::Punct("<=") => (8, Some(BinOp::Le)),
                Tok::Punct(">") => (8, Some(BinOp::Gt)),
                Tok::Punct(">=") => (8, Some(BinOp::Ge)),
                Tok::Punct("<<") => (9, Some(BinOp::Shl)),
                Tok::Punct(">>") => (9, Some(BinOp::Shr)),
                Tok::Punct(">>>") => (9, Some(BinOp::UShr)),
                Tok::Punct("+") => (10, Some(BinOp::Add)),
                Tok::Punct("-") => (10, Some(BinOp::Sub)),
                Tok::Punct("*") => (11, Some(BinOp::Mul)),
                Tok::Punct("/") => (11, Some(BinOp::Div)),
                Tok::Punct("%") => (11, Some(BinOp::Mod)),
                Tok::Punct("**") => (12, Some(BinOp::Pow)),
                _ => break,
            };
            if prec < min {
                break;
            }
            let tok = self.next();
            // `**` — правоассоциативный.
            let next_min = if prec == 12 { prec } else { prec + 1 };
            let rhs = self.binary(next_min)?;
            lhs = match (op, tok) {
                (Some(op), _) => Expr::Binary(op, Box::new(lhs), Box::new(rhs)),
                (None, Tok::Punct("&&")) => Expr::And(Box::new(lhs), Box::new(rhs)),
                (None, Tok::Punct("||")) => Expr::Or(Box::new(lhs), Box::new(rhs)),
                (None, _) => Expr::Nullish(Box::new(lhs), Box::new(rhs)),
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let op = match self.peek() {
            Tok::Punct("-") => Some(UnOp::Neg),
            Tok::Punct("+") => Some(UnOp::Plus),
            Tok::Punct("!") => Some(UnOp::Not),
            Tok::Punct("~") => Some(UnOp::BitNot),
            Tok::Ident(k) if &**k == "typeof" => Some(UnOp::Typeof),
            _ => None,
        };
        if let Some(op) = op {
            self.next();
            return Ok(Expr::Unary(op, Box::new(self.unary()?)));
        }
        if self.is("++") || self.is("--") {
            let step = if self.is("++") { 1.0 } else { -1.0 };
            self.next();
            let t = self.unary()?;
            return Ok(Expr::Update(step, true, Box::new(t)));
        }
        if self.is_kw("void") || self.is_kw("delete") {
            self.next();
            self.unary()?;
            return Ok(Expr::Undef);
        }
        let mut e = self.postfix()?;
        if self.is("++") || self.is("--") {
            let step = if self.is("++") { 1.0 } else { -1.0 };
            self.next();
            e = Expr::Update(step, false, Box::new(e));
        }
        Ok(e)
    }

    fn args(&mut self) -> PResult<Vec<Expr>> {
        let mut a = Vec::new();
        while !self.is(")") {
            a.push(self.assign()?);
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(a)
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = if self.eat_kw("new") {
            let callee = self.primary()?;
            let args = if self.eat("(") {
                self.args()?
            } else {
                Vec::new()
            };
            Expr::New(Box::new(callee), args)
        } else {
            self.primary()?
        };
        loop {
            if self.eat(".") {
                let name = self.ident()?;
                e = Expr::Member(Box::new(e), name);
            } else if self.eat("[") {
                let i = self.expr()?;
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(i));
            } else if self.eat("(") {
                let a = self.args()?;
                e = Expr::Call(Box::new(e), a);
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> PResult<Expr> {
        match self.next() {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Punct("(") => {
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Punct("[") => {
                let mut items = Vec::new();
                while !self.is("]") {
                    items.push(self.assign()?);
                    if !self.eat(",") {
                        break;
                    }
                }
                self.expect("]")?;
                Ok(Expr::Array(items))
            }
            Tok::Punct("{") => {
                let mut fields = Vec::new();
                while !self.is("}") {
                    let key: Rc<str> = match self.next() {
                        Tok::Ident(s) | Tok::Str(s) => s,
                        Tok::Num(n) => format_num(n).into(),
                        t => return Err(format!("ключ объекта {t:?}")),
                    };
                    self.expect(":")?;
                    fields.push((key, self.assign()?));
                    if !self.eat(",") {
                        break;
                    }
                }
                self.expect("}")?;
                Ok(Expr::Object(fields))
            }
            Tok::Ident(s) => Ok(match &*s {
                "true" => Expr::Bool(true),
                "false" => Expr::Bool(false),
                "null" => Expr::Null,
                "undefined" => Expr::Undef,
                "this" => Expr::This,
                "function" => Expr::Func(self.function()?),
                _ => Expr::Ident(s),
            }),
            t => Err(format!("неожиданный {t:?}")),
        }
    }
}

/// Число в строку как в JS (целые — без дробной части).
pub fn format_num(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else if n.is_nan() {
        "NaN".into()
    } else {
        format!("{n}")
    }
}

/// Разобрать текст программы.
pub fn parse(src: &str) -> Result<Vec<Stmt>, String> {
    let toks = lex(src)?;
    Parser { toks, pos: 0 }.program()
}
