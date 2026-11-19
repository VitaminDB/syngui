//! Построчная подсветка Pascal (Delphi, Free Pascal; `.pas`, `.dpr`, `.dpk`, `.lpr`, текстовые формы `.dfm`/`.lfm`).
//!
//! Регистр не важен. Комментарии `//`, `{ … }`, `(* … *)` (многострочные — состоянием), директивы
//! компилятора `{$…}`/`(*$…*)`, строки `'…'` (с `''` внутри) и символы `#13`, `#$0A`, числа (`$FF`, `1.5e3`),
//! ключевые слова, слова управления (`begin`/`end`, `if`, `try` …), встроенные типы, имена типов по
//! соглашению Delphi (`TForm`, `IShapeVisitor`, `EShapeError`) и вызовы (`Имя(`).

use super::capture_names::TokenClass;
use super::highlight_cache::{LineSpans, Span};
use super::highlighter::LineHighlighter;

const NORMAL: u32 = 0;
const BRACE_COMMENT: u32 = 1;
const PAREN_COMMENT: u32 = 2;
const BRACE_DIRECTIVE: u32 = 3;

pub struct PascalHighlighter;

const KEYWORDS: &[&str] = &[
    "absolute", "abstract", "and", "array", "as", "asm", "assembler", "automated", "cdecl", "class", "const", "constructor", "default", "deprecated", "destructor", "dispid", "dispinterface", "div",
    "dynamic", "export", "exports", "external", "far", "file", "final", "forward", "function", "helper", "implementation", "implements", "in", "index", "inherited", "initialization", "finalization",
    "inline", "interface", "is", "label", "library", "message", "mod", "name", "near", "nil", "nodefault", "not", "object", "of", "on", "operator", "or", "out", "overload", "override", "package", "packed",
    "pascal", "platform", "private", "procedure", "program", "property", "protected", "public", "published", "read", "readonly", "record", "register", "reintroduce", "requires", "resident",
    "resourcestring", "safecall", "sealed", "set", "shl", "shr", "static", "stdcall", "stored", "strict", "string", "threadvar", "type", "unit", "uses", "var", "virtual", "write", "writeonly", "xor",
    "contains", "self", "result", "true", "false", "item",
];

const CONTROL: &[&str] = &[
    "begin", "end", "if", "then", "else", "case", "while", "do", "repeat", "until", "for", "to", "downto", "try", "except", "finally", "raise", "exit", "break", "continue", "with", "goto", "halt",
];

const BUILTIN_TYPES: &[&str] = &[
    "integer", "cardinal", "shortint", "smallint", "longint", "int64", "uint64", "byte", "word", "longword", "dword", "nativeint", "nativeuint", "boolean", "bytebool", "wordbool", "longbool", "char",
    "ansichar", "widechar", "ansistring", "widestring", "unicodestring", "shortstring", "rawbytestring", "utf8string", "pchar", "pansichar", "pwidechar", "pointer", "real", "single", "double",
    "extended", "currency", "comp", "variant", "olevariant", "text", "textfile", "tdatetime",
];

fn ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 0x80
}

fn ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

/// Имя типа по соглашению Delphi: `T`/`I`/`E`/`P` + заглавная (`TForm`, `IUnknown`, `EAbort`, `PChar`).
fn delphi_type_name(w: &str) -> bool {
    let b = w.as_bytes();
    b.len() >= 2 && matches!(b[0], b'T' | b'I' | b'E' | b'P') && b[1].is_ascii_uppercase()
}

impl LineHighlighter for PascalHighlighter {
    fn highlight_line(&self, line: &str, state: u32, out: &mut LineSpans) -> u32 {
        let b = line.as_bytes();
        let n = b.len();
        let mut i = 0usize;
        // продолжение многострочного комментария или директивы
        match state {
            BRACE_COMMENT | BRACE_DIRECTIVE => {
                let class = if state == BRACE_DIRECTIVE { TokenClass::Attribute } else { TokenClass::Comment };
                match line.find('}') {
                    Some(e) => {
                        out.push(Span::new(0, (e + 1) as u32, class));
                        i = e + 1;
                    }
                    None => {
                        out.push(Span::new(0, n as u32, class));
                        return state;
                    }
                }
            }
            PAREN_COMMENT => match line.find("*)") {
                Some(e) => {
                    out.push(Span::new(0, (e + 2) as u32, TokenClass::Comment));
                    i = e + 2;
                }
                None => {
                    out.push(Span::new(0, n as u32, TokenClass::Comment));
                    return PAREN_COMMENT;
                }
            },
            _ => {}
        }
        while i < n {
            let c = b[i];
            // `//` до конца строки
            if c == b'/' && b.get(i + 1) == Some(&b'/') {
                out.push(Span::new(i as u32, n as u32, TokenClass::Comment));
                return NORMAL;
            }
            // `{ … }` и директива `{$ … }`
            if c == b'{' {
                let directive = b.get(i + 1) == Some(&b'$');
                let class = if directive { TokenClass::Attribute } else { TokenClass::Comment };
                match line[i..].find('}') {
                    Some(e) => {
                        out.push(Span::new(i as u32, (i + e + 1) as u32, class));
                        i += e + 1;
                        continue;
                    }
                    None => {
                        out.push(Span::new(i as u32, n as u32, class));
                        return if directive { BRACE_DIRECTIVE } else { BRACE_COMMENT };
                    }
                }
            }
            // `(* … *)`
            if c == b'(' && b.get(i + 1) == Some(&b'*') {
                match line[i + 2..].find("*)") {
                    Some(e) => {
                        out.push(Span::new(i as u32, (i + 2 + e + 2) as u32, TokenClass::Comment));
                        i += e + 4;
                        continue;
                    }
                    None => {
                        out.push(Span::new(i as u32, n as u32, TokenClass::Comment));
                        return PAREN_COMMENT;
                    }
                }
            }
            // строка `'…'` (`''` — кавычка внутри); незакрытая — до конца строки
            if c == b'\'' {
                let mut j = i + 1;
                while j < n {
                    if b[j] == b'\'' {
                        if b.get(j + 1) == Some(&b'\'') {
                            j += 2;
                            continue;
                        }
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                out.push(Span::new(i as u32, j.min(n) as u32, TokenClass::String));
                i = j;
                continue;
            }
            // символ `#13`, `#$0A`
            if c == b'#' && b.get(i + 1).is_some_and(|x| x.is_ascii_digit() || *x == b'$') {
                let mut j = i + 1;
                if b[j] == b'$' {
                    j += 1;
                    while j < n && b[j].is_ascii_hexdigit() {
                        j += 1;
                    }
                } else {
                    while j < n && b[j].is_ascii_digit() {
                        j += 1;
                    }
                }
                out.push(Span::new(i as u32, j as u32, TokenClass::StringSpecial));
                i = j;
                continue;
            }
            // числа: `$FF`, `123`, `1.5e-3` (не `1..5` — диапазон)
            if c == b'$' && b.get(i + 1).is_some_and(|x| x.is_ascii_hexdigit()) {
                let mut j = i + 1;
                while j < n && b[j].is_ascii_hexdigit() {
                    j += 1;
                }
                out.push(Span::new(i as u32, j as u32, TokenClass::Number));
                i = j;
                continue;
            }
            if c.is_ascii_digit() {
                let mut j = i;
                while j < n && b[j].is_ascii_digit() {
                    j += 1;
                }
                if j + 1 < n && b[j] == b'.' && b[j + 1].is_ascii_digit() {
                    j += 1;
                    while j < n && b[j].is_ascii_digit() {
                        j += 1;
                    }
                }
                if j < n && (b[j] == b'e' || b[j] == b'E') && b.get(j + 1).is_some_and(|x| x.is_ascii_digit() || *x == b'-' || *x == b'+') {
                    j += 2;
                    while j < n && b[j].is_ascii_digit() {
                        j += 1;
                    }
                }
                out.push(Span::new(i as u32, j as u32, TokenClass::Number));
                i = j;
                continue;
            }
            if ident_start(c) {
                let mut j = i;
                while j < n && ident_char(b[j]) {
                    j += 1;
                }
                let w = &line[i..j];
                let l = w.to_ascii_lowercase();
                let class = if CONTROL.contains(&l.as_str()) {
                    Some(TokenClass::KeywordControl)
                } else if KEYWORDS.contains(&l.as_str()) {
                    Some(TokenClass::Keyword)
                } else if BUILTIN_TYPES.contains(&l.as_str()) {
                    Some(TokenClass::TypeBuiltin)
                } else if delphi_type_name(w) {
                    Some(TokenClass::Type)
                } else {
                    // вызов: `Имя(`
                    let mut k = j;
                    while k < n && b[k] == b' ' {
                        k += 1;
                    }
                    (k < n && b[k] == b'(').then_some(TokenClass::Function)
                };
                if let Some(cl) = class {
                    out.push(Span::new(i as u32, j as u32, cl));
                }
                i = j;
                continue;
            }
            if matches!(c, b':' | b'=' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'@' | b'^') {
                out.push(Span::new(i as u32, (i + 1) as u32, TokenClass::Operator));
            } else if matches!(c, b';' | b',' | b'.' | b'(' | b')' | b'[' | b']') {
                out.push(Span::new(i as u32, (i + 1) as u32, TokenClass::Punctuation));
            }
            i += 1;
        }
        NORMAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(line: &str, state: u32) -> (Vec<(String, TokenClass)>, u32) {
        let mut ls = LineSpans::new();
        let st = PascalHighlighter.highlight_line(line, state, &mut ls);
        (ls.spans.iter().map(|s| (line[s.byte_start as usize..s.byte_end as usize].to_string(), s.class)).collect(), st)
    }

    #[test]
    fn keywords_strings_comments() {
        let (v, st) = classes("procedure TForm1.Button1Click(Sender: TObject); // обработчик", NORMAL);
        assert_eq!(st, NORMAL);
        assert!(v.contains(&("procedure".into(), TokenClass::Keyword)));
        assert!(v.contains(&("TForm1".into(), TokenClass::Type)));
        assert!(v.contains(&("TObject".into(), TokenClass::Type)));
        assert!(v.iter().any(|(t, c)| t.starts_with("//") && *c == TokenClass::Comment));
        let (v, _) = classes("  S := 'it''s' + #13#10 + IntToStr($FF);", NORMAL);
        assert!(v.contains(&("'it''s'".into(), TokenClass::String)));
        assert!(v.contains(&("#13".into(), TokenClass::StringSpecial)));
        assert!(v.contains(&("$FF".into(), TokenClass::Number)));
        assert!(v.contains(&("IntToStr".into(), TokenClass::Function)));
        let (v, _) = classes("BEGIN Result := 0 END;", NORMAL);
        assert!(v.contains(&("BEGIN".into(), TokenClass::KeywordControl)));
        assert!(v.contains(&("Result".into(), TokenClass::Keyword)));
    }

    #[test]
    fn multiline_comments_and_directives() {
        let (_, st) = classes("x := 1; { начало", NORMAL);
        assert_eq!(st, BRACE_COMMENT);
        let (v, st) = classes("конец } y := 2;", st);
        assert_eq!(st, NORMAL);
        assert_eq!(v[0].1, TokenClass::Comment);
        let (v, _) = classes("{$R *.dfm}", NORMAL);
        assert_eq!(v[0], ("{$R *.dfm}".into(), TokenClass::Attribute));
        let (_, st) = classes("(* многострочный", NORMAL);
        assert_eq!(st, PAREN_COMMENT);
    }
}
