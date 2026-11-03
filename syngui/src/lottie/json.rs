//! Минимальный JSON-парсер для Lottie: без зависимостей, объекты хранятся
//! как список пар (порядок ключей сохраняется, поиск линейный — объекты
//! Lottie маленькие). Числа — `f64`, строки — с полной обработкой
//! экранирования (включая суррогатные пары `😀`).

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

#[derive(Debug, Clone)]
pub struct JsonError {
    pub offset: usize,
    pub message: &'static str,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSON: {} (смещение {})", self.message, self.offset)
    }
}

impl std::error::Error for JsonError {}

impl Json {
    /// Поле объекта по ключу (`None` для не-объектов и отсутствующих ключей).
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            // Некоторые экспортёры пишут числа строками ("0.5").
            Json::Str(s) => s.trim().parse().ok(),
            Json::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        self.as_f64().map(|v| v as f32)
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.as_f64().map(|v| v.round() as i64)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            Json::Num(n) => Some(*n != 0.0),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a.as_slice()),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Json::Obj(_))
    }

    /// Число по ключу или значение по умолчанию.
    pub fn f32_or(&self, key: &str, default: f32) -> f32 {
        self.get(key).and_then(Json::as_f32).unwrap_or(default)
    }

    pub fn i64_or(&self, key: &str, default: i64) -> i64 {
        self.get(key).and_then(Json::as_i64).unwrap_or(default)
    }

    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        self.get(key).and_then(Json::as_bool).unwrap_or(default)
    }

    pub fn str_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.get(key).and_then(Json::as_str).unwrap_or(default)
    }

    /// Массив чисел (нечисловые элементы → 0).
    pub fn num_array(&self) -> Vec<f32> {
        match self {
            Json::Arr(a) => a.iter().map(|v| v.as_f32().unwrap_or(0.0)).collect(),
            Json::Num(n) => vec![*n as f32],
            _ => Vec::new(),
        }
    }
}

/// Разобрать JSON-документ.
pub fn parse(text: &str) -> Result<Json, JsonError> {
    let mut p = Parser {
        bytes: text.as_bytes(),
        pos: 0,
        depth: 0,
    };
    p.skip_ws();
    let v = p.value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(p.err("лишние данные после документа"));
    }
    Ok(v)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, message: &'static str) -> JsonError {
        JsonError {
            offset: self.pos,
            message,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\n' || b == b'\r' || b == b'\t' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, lit: &[u8]) -> Result<(), JsonError> {
        if self.bytes[self.pos..].starts_with(lit) {
            self.pos += lit.len();
            Ok(())
        } else {
            Err(self.err("неожиданный токен"))
        }
    }

    fn value(&mut self) -> Result<Json, JsonError> {
        match self.peek() {
            None => Err(self.err("неожиданный конец")),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Json::Str),
            Some(b't') => self.expect(b"true").map(|_| Json::Bool(true)),
            Some(b'f') => self.expect(b"false").map(|_| Json::Bool(false)),
            Some(b'n') => self.expect(b"null").map(|_| Json::Null),
            Some(b'-') | Some(b'0'..=b'9') => self.number(),
            // Нестандартные, но встречающиеся в экспорте значения.
            Some(b'N') => self.expect(b"NaN").map(|_| Json::Num(0.0)),
            Some(b'I') => self.expect(b"Infinity").map(|_| Json::Num(f64::MAX)),
            Some(_) => Err(self.err("недопустимый символ")),
        }
    }

    fn enter(&mut self) -> Result<(), JsonError> {
        self.depth += 1;
        if self.depth > 512 {
            return Err(self.err("слишком глубокая вложенность"));
        }
        Ok(())
    }

    fn object(&mut self) -> Result<Json, JsonError> {
        self.enter()?;
        self.pos += 1; // {
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            self.depth -= 1;
            return Ok(Json::Obj(pairs));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(self.err("ожидался ключ объекта"));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(self.err("ожидалось ':'"));
            }
            self.pos += 1;
            self.skip_ws();
            let v = self.value()?;
            pairs.push((key, v));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("ожидалось ',' или '}'")),
            }
        }
        self.depth -= 1;
        Ok(Json::Obj(pairs))
    }

    fn array(&mut self) -> Result<Json, JsonError> {
        self.enter()?;
        self.pos += 1; // [
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            self.depth -= 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("ожидалось ',' или ']'")),
            }
        }
        self.depth -= 1;
        Ok(Json::Arr(items))
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'E' || b == b'+' || b == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        // Безопасно: границы токена лежат на ASCII-символах.
        let s = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("");
        s.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| self.err("неверное число"))
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        if self.pos + 4 > self.bytes.len() {
            return Err(self.err("обрыв \\u-последовательности"));
        }
        let s = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
            .map_err(|_| self.err("неверная \\u-последовательность"))?;
        let v = u32::from_str_radix(s, 16).map_err(|_| self.err("неверная \\u-последовательность"))?;
        self.pos += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.pos += 1; // "
        let mut out: Vec<u8> = Vec::new();
        loop {
            let start = self.pos;
            // Быстрый прогон до спецсимвола.
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' {
                    break;
                }
                self.pos += 1;
            }
            out.extend_from_slice(&self.bytes[start..self.pos]);
            match self.peek() {
                None => return Err(self.err("незакрытая строка")),
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(_) => {
                    // '\\'
                    self.pos += 1;
                    let Some(esc) = self.peek() else {
                        return Err(self.err("обрыв экранирования"));
                    };
                    self.pos += 1;
                    match esc {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let mut cp = self.hex4()?;
                            if (0xD800..0xDC00).contains(&cp) {
                                // Старший суррогат: ждём \uXXXX младшего.
                                if self.bytes[self.pos..].starts_with(b"\\u") {
                                    self.pos += 2;
                                    let lo = self.hex4()?;
                                    if (0xDC00..0xE000).contains(&lo) {
                                        cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                    } else {
                                        cp = 0xFFFD;
                                    }
                                } else {
                                    cp = 0xFFFD;
                                }
                            } else if (0xDC00..0xE000).contains(&cp) {
                                cp = 0xFFFD;
                            }
                            let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return Err(self.err("неизвестное экранирование")),
                    }
                }
            }
        }
        String::from_utf8(out).map_err(|_| self.err("строка не в UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested() {
        let v = parse(r#"{"a":[1,2.5,-3e2,true,null,"x\nA😀"],"b":{}}"#).unwrap();
        let a = v.get("a").unwrap().as_array().unwrap();
        assert_eq!(a[0].as_f64(), Some(1.0));
        assert_eq!(a[1].as_f64(), Some(2.5));
        assert_eq!(a[2].as_f64(), Some(-300.0));
        assert_eq!(a[3].as_bool(), Some(true));
        assert!(a[4].is_null());
        assert_eq!(a[5].as_str(), Some("x\nA😀"));
        assert!(v.get("b").unwrap().is_object());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("{\"a\":}").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse("[1] x").is_err());
    }
}
