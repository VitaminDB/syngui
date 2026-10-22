//! Толерантный разбор MSS без потерь — для инструментов (подсветка ошибок,
//! дополнение, форматирование), а не для применения стилей.
//!
//! - [`tokenize`] режет текст на токены, включая пробелы и комментарии:
//!   склейка всех токенов по порядку даёт исходный текст байт в байт.
//! - [`parse`] строит дерево [`Stylesheet`]: правила с селекторами,
//!   объявления в исходном порядке, `:root`-переменные, вложенные правила,
//!   `@keyframes`, неизвестные @-правила. На ошибке разбор не
//!   останавливается: ошибка уходит в [`Stylesheet::errors`], разбор
//!   продолжается с ближайшего понятного места.
//!
//! Правила разбора повторяют основной парсер syngui (`syngui::mss::MssParser`):
//! что он отвергает, то здесь — ошибка с местом. Например, вложенное
//! правило распознаётся по первому символу (`.`, `&`, `>`, `+`, `~`, `*` или
//! заглавная буква), как в основном парсере.
//!
//! Позиции — байтовые диапазоны [`Span`]; перевод в строку и колонку
//! (символы Unicode, с нуля) — [`LineIndex`].

use std::ops::Range;

/// Диапазон байт `[start, end)` в исходном тексте.
pub type Span = Range<usize>;

// ---------------------------------------------------------------------------
// Строки и колонки
// ---------------------------------------------------------------------------

/// Перевод байтовых смещений в строку/колонку и обратно. Строки и колонки —
/// с нуля, колонка — в символах Unicode (как у `CodeEditor`).
#[derive(Debug, Clone)]
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    /// Число строк (пустой текст — одна строка).
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Байт начала строки `line` (за последней строкой — длина текста не
    /// известна индексу, поэтому `None`).
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.line_starts.get(line).copied()
    }

    /// Строка и колонка смещения `offset` (за концом — конец текста, внутри
    /// символа — его начало).
    pub fn position(&self, text: &str, offset: usize) -> (usize, usize) {
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = match self.line_starts.binary_search(&offset) {
            Ok(l) => l,
            Err(l) => l - 1,
        };
        let col = text[self.line_starts[line]..offset].chars().count();
        (line, col)
    }

    /// Смещение строки `line` и колонки `col`. Колонка за концом строки —
    /// конец строки (без перевода строки), строка за концом текста — конец
    /// текста.
    pub fn offset(&self, text: &str, line: usize, col: usize) -> usize {
        let Some(&start) = self.line_starts.get(line) else {
            return text.len();
        };
        let end = self
            .line_starts
            .get(line + 1)
            .map_or(text.len(), |&s| s - 1);
        let line_text = &text[start..end];
        let line_text = line_text.strip_suffix('\r').unwrap_or(line_text);
        match line_text.char_indices().nth(col) {
            Some((i, _)) => start + i,
            None => start + line_text.len(),
        }
    }
}

// ---------------------------------------------------------------------------
// Токены
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Пробелы и переводы строк.
    Whitespace,
    /// `/* … */` (незакрытый — до конца текста, с ошибкой).
    Comment,
    /// Имя: буквы, цифры, `-`, `_`; `--переменная` тоже имя.
    Ident,
    /// `@keyframes`, `@media` — вместе с `@`.
    AtKeyword,
    /// `#abc`, `#id` — вместе с `#`.
    Hash,
    /// `12`, `-0.5`, `.5`.
    Number,
    /// `50%`.
    Percentage,
    /// Число с единицей: `12px`, `200ms`, `45deg`.
    Dimension,
    /// `"…"` или `'…'`.
    String,
    Colon,
    Semicolon,
    Comma,
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    /// Прочий одиночный символ: `.`, `>`, `+`, `~`, `*`, `&`, `!`, `/`…
    Delim,
}

impl TokenKind {
    /// Пробел или комментарий.
    pub fn is_trivia(self) -> bool {
        matches!(self, TokenKind::Whitespace | TokenKind::Comment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.span.clone()]
    }
}

/// Ошибка разбора: место и сообщение по-русски.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub span: Span,
    pub message: String,
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// Токены текста без потерь (склейка спанов — исходный текст) и ошибки
/// токенизации (незакрытые комментарии и строки).
pub fn tokenize(text: &str) -> (Vec<Token>, Vec<SyntaxError>) {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let bytes_len = text.len();
    let mut pos = 0usize;
    let char_at = |p: usize| text[p..].chars().next();
    let ident_end = |mut p: usize| {
        while let Some(c) = text[p..].chars().next() {
            if is_ident_char(c) {
                p += c.len_utf8();
            } else {
                break;
            }
        }
        p
    };
    let digits_end = |mut p: usize| {
        while p < bytes_len && text.as_bytes()[p].is_ascii_digit() {
            p += 1;
        }
        p
    };

    while pos < bytes_len {
        let c = char_at(pos).unwrap();
        let next = char_at(pos + c.len_utf8());
        let start = pos;
        let kind;
        if c.is_whitespace() {
            while let Some(c) = char_at(pos) {
                if c.is_whitespace() {
                    pos += c.len_utf8();
                } else {
                    break;
                }
            }
            kind = TokenKind::Whitespace;
        } else if c == '/' && next == Some('*') {
            match text[pos + 2..].find("*/") {
                Some(i) => pos = pos + 2 + i + 2,
                None => {
                    pos = bytes_len;
                    errors.push(SyntaxError {
                        span: start..start + 2,
                        message: "комментарий не закрыт: нет «*/»".into(),
                    });
                }
            }
            kind = TokenKind::Comment;
        } else if c == '"' || c == '\'' {
            pos += 1;
            let mut closed = false;
            while let Some(ch) = char_at(pos) {
                if ch == '\\' {
                    pos += 1;
                    if let Some(e) = char_at(pos) {
                        pos += e.len_utf8();
                    }
                    continue;
                }
                if ch == '\n' {
                    break;
                }
                pos += ch.len_utf8();
                if ch == c {
                    closed = true;
                    break;
                }
            }
            if !closed {
                errors.push(SyntaxError {
                    span: start..pos,
                    message: format!("строка не закрыта: нет «{c}»"),
                });
            }
            kind = TokenKind::String;
        } else if c == '@' && next.is_some_and(|n| is_ident_start(n) || n == '-') {
            pos = ident_end(pos + 1);
            kind = TokenKind::AtKeyword;
        } else if c == '#' && next.is_some_and(is_ident_char) {
            pos = ident_end(pos + 1);
            kind = TokenKind::Hash;
        } else if starts_number(text, pos) {
            if c == '-' || c == '+' {
                pos += 1;
            }
            pos = digits_end(pos);
            if text[pos..].starts_with('.')
                && text.as_bytes().get(pos + 1).is_some_and(u8::is_ascii_digit)
            {
                pos = digits_end(pos + 1);
            }
            if text[pos..].starts_with('%') {
                pos += 1;
                kind = TokenKind::Percentage;
            } else if char_at(pos).is_some_and(|c| c.is_alphabetic()) {
                pos = ident_end(pos);
                kind = TokenKind::Dimension;
            } else {
                kind = TokenKind::Number;
            }
        } else if is_ident_start(c)
            || (c == '-' && next.is_some_and(|n| is_ident_start(n) || n == '-'))
        {
            pos = ident_end(pos);
            kind = TokenKind::Ident;
        } else {
            pos += c.len_utf8();
            kind = match c {
                ':' => TokenKind::Colon,
                ';' => TokenKind::Semicolon,
                ',' => TokenKind::Comma,
                '{' => TokenKind::LBrace,
                '}' => TokenKind::RBrace,
                '(' => TokenKind::LParen,
                ')' => TokenKind::RParen,
                '[' => TokenKind::LBracket,
                ']' => TokenKind::RBracket,
                _ => TokenKind::Delim,
            };
        }
        tokens.push(Token {
            kind,
            span: start..pos,
        });
    }
    (tokens, errors)
}

fn starts_number(text: &str, pos: usize) -> bool {
    let b = text.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    match b[pos] {
        b'0'..=b'9' => true,
        b'.' => digit(pos + 1),
        b'-' | b'+' => digit(pos + 1) || (b.get(pos + 1) == Some(&b'.') && digit(pos + 2)),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Дерево
// ---------------------------------------------------------------------------

/// Разобранный документ.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    /// Все токены текста (без потерь).
    pub tokens: Vec<Token>,
    /// Узлы верхнего уровня в исходном порядке.
    pub items: Vec<Item>,
    /// Ошибки по порядку в тексте.
    pub errors: Vec<SyntaxError>,
}

#[derive(Debug, Clone)]
pub enum Item {
    /// Правило с селекторами (в том числе `:root`, см. [`Rule::is_root`]).
    Rule(Rule),
    Keyframes(Keyframes),
    /// Неизвестное @-правило (`@media`, `@import`…): основной парсер его не
    /// знает, узел — чтобы инструменты видели его границы.
    AtRule(AtRule),
}

#[derive(Debug, Clone)]
pub struct Rule {
    /// От начала селектора до `}` включительно (или до конца разобранного).
    pub span: Span,
    /// Весь список селекторов без краевых пробелов.
    pub selector_span: Span,
    /// Селекторы через запятую.
    pub selectors: Vec<Selector>,
    /// `:root { --x: … }` — блок переменных.
    pub is_root: bool,
    /// Блок `{ … }`; `None` — после селектора не нашлось `{`.
    pub block: Option<Block>,
}

#[derive(Debug, Clone)]
pub struct Selector {
    pub span: Span,
    pub text: String,
    pub parts: Vec<SelectorPart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorPartKind {
    /// `Button`
    Element,
    /// `.class`
    Class,
    /// `#id`
    Id,
    /// `*`
    Universal,
    /// `:hover`
    Pseudo,
    /// `&`
    Nesting,
    /// `>`, `+`, `~` или `' '` (потомок — пробел между частями).
    Combinator(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorPart {
    pub kind: SelectorPartKind,
    /// Имя без `.`/`#`/`:` (у комбинатора и `&`/`*` — сам символ).
    pub name: String,
    /// Вместе с `.`/`#`/`:`.
    pub span: Span,
    /// Диапазон только имени.
    pub name_span: Span,
}

#[derive(Debug, Clone)]
pub struct Block {
    /// От `{` до `}` включительно (или до конца разобранного).
    pub span: Span,
    pub open: Span,
    /// `None` — блок не закрыт.
    pub close: Option<Span>,
    pub items: Vec<BlockItem>,
}

impl Block {
    /// Объявления блока по порядку (без вложенных правил).
    pub fn declarations(&self) -> impl Iterator<Item = &Declaration> {
        self.items.iter().filter_map(|i| match i {
            BlockItem::Declaration(d) => Some(d),
            BlockItem::Rule(_) => None,
        })
    }

    /// Вложенные правила блока.
    pub fn rules(&self) -> impl Iterator<Item = &Rule> {
        self.items.iter().filter_map(|i| match i {
            BlockItem::Rule(r) => Some(r),
            BlockItem::Declaration(_) => None,
        })
    }
}

#[derive(Debug, Clone)]
pub enum BlockItem {
    Declaration(Declaration),
    Rule(Rule),
}

#[derive(Debug, Clone)]
pub struct Declaration {
    /// Имя свойства или переменной (`--x`).
    pub name: String,
    pub name_span: Span,
    /// `:` — `None`, если его нет (имя недописано).
    pub colon: Option<Span>,
    /// Значение без краевых пробелов (как его видит основной парсер,
    /// комментарии внутри остаются).
    pub value: String,
    /// Диапазон значения; пустое значение — пустой диапазон после `:`.
    pub value_span: Span,
    /// Индексы токенов значения в [`Stylesheet::tokens`].
    pub value_tokens: Range<usize>,
    /// `;` в конце, если есть.
    pub semicolon: Option<Span>,
    /// От имени до `;` включительно (или до конца значения).
    pub span: Span,
}

impl Declaration {
    /// Переменная (`--x: …`).
    pub fn is_variable(&self) -> bool {
        self.name.starts_with("--")
    }
}

#[derive(Debug, Clone)]
pub struct Keyframes {
    pub span: Span,
    /// `@keyframes`
    pub keyword_span: Span,
    /// Имя анимации и его место.
    pub name: Option<(String, Span)>,
    pub open: Option<Span>,
    pub close: Option<Span>,
    pub steps: Vec<KeyframeStep>,
}

#[derive(Debug, Clone)]
pub struct KeyframeStep {
    pub span: Span,
    /// `from`, `to`, `50%` — по одному на позицию в списке через запятую.
    pub selectors: Vec<(String, Span)>,
    pub block: Option<Block>,
}

#[derive(Debug, Clone)]
pub struct AtRule {
    /// Имя без `@`.
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub block: Option<Span>,
}

impl Stylesheet {
    /// Все правила документа с вложенными, в порядке обхода (родитель
    /// раньше детей). Второй элемент — цепочка родителей.
    pub fn walk_rules(&self) -> Vec<(&Rule, Vec<&Rule>)> {
        fn go<'a>(
            r: &'a Rule,
            parents: &mut Vec<&'a Rule>,
            out: &mut Vec<(&'a Rule, Vec<&'a Rule>)>,
        ) {
            out.push((r, parents.clone()));
            if let Some(b) = &r.block {
                parents.push(r);
                for child in b.rules() {
                    go(child, parents, out);
                }
                parents.pop();
            }
        }
        let mut out = Vec::new();
        for item in &self.items {
            if let Item::Rule(r) = item {
                go(r, &mut Vec::new(), &mut out);
            }
        }
        out
    }

    /// Все объявления документа: правил (с вложенными), `:root` и шагов
    /// `@keyframes`.
    pub fn all_declarations(&self) -> Vec<&Declaration> {
        let mut out = Vec::new();
        for (r, _) in self.walk_rules() {
            if let Some(b) = &r.block {
                out.extend(b.declarations());
            }
        }
        for item in &self.items {
            if let Item::Keyframes(k) = item {
                for s in &k.steps {
                    if let Some(b) = &s.block {
                        out.extend(b.declarations());
                    }
                }
            }
        }
        out.sort_by_key(|d| d.span.start);
        out
    }

    /// Переменные из `:root` (последнее объявление имени побеждает, как в
    /// основном парсере; порядок — по первому появлению).
    pub fn root_variables(&self) -> Vec<&Declaration> {
        let mut out: Vec<&Declaration> = Vec::new();
        for item in &self.items {
            let Item::Rule(r) = item else { continue };
            if !r.is_root {
                continue;
            }
            let Some(b) = &r.block else { continue };
            for d in b.declarations().filter(|d| d.is_variable()) {
                match out.iter_mut().find(|o| o.name == d.name) {
                    Some(o) => *o = d,
                    None => out.push(d),
                }
            }
        }
        out
    }

    /// Имена `@keyframes` документа.
    pub fn keyframes_names(&self) -> Vec<(&str, Span)> {
        self.items
            .iter()
            .filter_map(|i| match i {
                Item::Keyframes(k) => k.name.as_ref().map(|(n, s)| (n.as_str(), s.clone())),
                _ => None,
            })
            .collect()
    }

    /// Индекс токена, содержащего смещение `offset` (на границе — токен,
    /// который начинается в `offset`; в конце текста — последний).
    pub fn token_at(&self, offset: usize) -> Option<usize> {
        if self.tokens.is_empty() {
            return None;
        }
        let i = self.tokens.partition_point(|t| t.span.end <= offset);
        Some(i.min(self.tokens.len() - 1))
    }
}

// ---------------------------------------------------------------------------
// Разбор
// ---------------------------------------------------------------------------

/// Разобрать документ. Не паникует на любом тексте.
pub fn parse(text: &str) -> Stylesheet {
    let (tokens, mut errors) = tokenize(text);
    let mut p = Parser {
        src: text,
        toks: &tokens,
        i: 0,
        errors: Vec::new(),
    };
    let items = p.parse_top();
    errors.append(&mut p.errors);
    errors.sort_by_key(|e| e.span.start);
    Stylesheet {
        tokens,
        items,
        errors,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum BlockKind {
    /// Блок правила: объявления и вложенные правила.
    Rule,
    /// `:root`: только переменные.
    Root,
    /// Шаг `@keyframes`: только объявления.
    Step,
}

struct Parser<'a> {
    src: &'a str,
    toks: &'a [Token],
    i: usize,
    errors: Vec<SyntaxError>,
}

impl<'a> Parser<'a> {
    fn err(&mut self, span: Span, message: impl Into<String>) {
        self.errors.push(SyntaxError {
            span,
            message: message.into(),
        });
    }

    fn peek(&self) -> Option<&'a Token> {
        self.toks.get(self.i)
    }

    fn kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind)
    }

    fn text(&self, t: &Token) -> &'a str {
        &self.src[t.span.clone()]
    }

    fn skip_trivia(&mut self) {
        while self.kind().is_some_and(TokenKind::is_trivia) {
            self.i += 1;
        }
    }

    /// Место для ошибки «здесь»: текущий токен или пустой диапазон в конце.
    fn here(&self) -> Span {
        match self.peek() {
            Some(t) => t.span.clone(),
            None => self.src.len()..self.src.len(),
        }
    }

    fn describe_here(&self) -> String {
        match self.peek() {
            None => "конец текста".into(),
            Some(t) if t.kind == TokenKind::Whitespace => "перевод строки".into(),
            Some(t) => format!("«{}»", self.text(t)),
        }
    }

    /// Конец последнего не-пробельного токена перед `self.i`, но не раньше `min`.
    fn trimmed_end(&self, from: usize, min: usize) -> usize {
        let mut j = self.i;
        while j > from && self.toks[j - 1].kind.is_trivia() {
            j -= 1;
        }
        if j > from {
            self.toks[j - 1].span.end.max(min)
        } else {
            min
        }
    }

    fn parse_top(&mut self) -> Vec<Item> {
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            let Some(t) = self.peek() else { break };
            match t.kind {
                TokenKind::AtKeyword if self.text(t) == "@keyframes" => {
                    items.push(Item::Keyframes(self.parse_keyframes()));
                }
                TokenKind::AtKeyword => items.push(Item::AtRule(self.parse_at_rule())),
                TokenKind::RBrace => {
                    self.err(t.span.clone(), "лишняя «}»");
                    self.i += 1;
                }
                TokenKind::Semicolon => {
                    self.err(t.span.clone(), "лишняя «;» вне правила");
                    self.i += 1;
                }
                _ => {
                    if let Some(r) = self.parse_rule(false) {
                        items.push(Item::Rule(r));
                    }
                }
            }
        }
        items
    }

    /// Правило: селекторы до `{`, затем блок. `nested` — внутри блока
    /// (тогда `}` закрывает родителя и не съедается).
    fn parse_rule(&mut self, nested: bool) -> Option<Rule> {
        let start_i = self.i;
        let start = self.toks[start_i].span.start;
        let mut depth = 0i32;
        while let Some(t) = self.peek() {
            match t.kind {
                TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RParen | TokenKind::RBracket => depth -= 1,
                TokenKind::LBrace if depth <= 0 => break,
                TokenKind::Semicolon | TokenKind::RBrace if depth <= 0 => break,
                _ => {}
            }
            self.i += 1;
        }
        let sel_end = self.trimmed_end(start_i, start);
        let selector_span = start..sel_end;
        let selectors = self.parse_selectors(start_i, self.i);
        let sel_text = self.src[selector_span.clone()].trim();
        let is_root = !nested && sel_text == ":root";

        match self.kind() {
            Some(TokenKind::LBrace) => {}
            other => {
                // После селектора нет блока: `;`, `}` или конец текста.
                self.err(
                    selector_span.clone(),
                    format!("после селектора «{sel_text}» ожидалась «{{»"),
                );
                let mut end = sel_end;
                if other == Some(TokenKind::Semicolon)
                    || (other == Some(TokenKind::RBrace) && !nested)
                {
                    end = self.toks[self.i].span.end;
                    self.i += 1;
                }
                if self.i == start_i {
                    // Защита от зацикливания.
                    self.i += 1;
                }
                return Some(Rule {
                    span: start..end,
                    selector_span,
                    selectors,
                    is_root,
                    block: None,
                });
            }
        }
        if sel_text.is_empty() {
            self.err(self.here(), "пустой селектор перед «{»");
        }
        let kind = if is_root {
            BlockKind::Root
        } else {
            BlockKind::Rule
        };
        let block = self.parse_block(kind, &selector_span, sel_text);
        Some(Rule {
            span: start..block.span.end,
            selector_span,
            selectors,
            is_root,
            block: Some(block),
        })
    }

    /// Блок с текущей `{`. `owner_span`/`owner` — для ошибки «не закрыт».
    fn parse_block(&mut self, kind: BlockKind, owner_span: &Span, owner: &str) -> Block {
        let open = self.toks[self.i].span.clone();
        self.i += 1;
        let mut items = Vec::new();
        let close;
        loop {
            self.skip_trivia();
            let Some(t) = self.peek() else {
                let what = if owner.is_empty() {
                    "{".to_string()
                } else {
                    owner.to_string()
                };
                self.err(
                    if owner_span.is_empty() {
                        open.clone()
                    } else {
                        owner_span.clone()
                    },
                    format!("блок «{what}» не закрыт: нет «}}»"),
                );
                close = None;
                break;
            };
            match t.kind {
                TokenKind::RBrace => {
                    close = Some(t.span.clone());
                    self.i += 1;
                    break;
                }
                TokenKind::Semicolon => {
                    self.err(t.span.clone(), "лишняя «;»: ожидалось имя свойства");
                    self.i += 1;
                }
                _ if kind == BlockKind::Rule && self.is_nested_rule_start(t) => {
                    if let Some(r) = self.parse_rule(true) {
                        items.push(BlockItem::Rule(r));
                    }
                }
                _ => {
                    if let Some(d) = self.parse_declaration(kind) {
                        items.push(BlockItem::Declaration(d));
                    }
                }
            }
        }
        let end = match &close {
            Some(c) => c.end,
            None => self.src.len(),
        };
        Block {
            span: open.start..end,
            open,
            close,
            items,
        }
    }

    /// Вложенное правило — по первому символу, как в основном парсере.
    fn is_nested_rule_start(&self, t: &Token) -> bool {
        let first = self.text(t).chars().next().unwrap_or(' ');
        matches!(first, '.' | '&' | '>' | '+' | '~' | '*') || first.is_ascii_uppercase()
    }

    /// Пропустить до `;` (съедается) или `}` (остаётся) на глубине 0.
    fn recover_to_decl_end(&mut self) {
        let mut depth = 0i32;
        while let Some(t) = self.peek() {
            match t.kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
                TokenKind::RParen | TokenKind::RBracket if depth > 0 => depth -= 1,
                TokenKind::RBrace if depth > 0 => depth -= 1,
                TokenKind::RBrace => return,
                TokenKind::Semicolon if depth == 0 => {
                    self.i += 1;
                    return;
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    fn parse_declaration(&mut self, kind: BlockKind) -> Option<Declaration> {
        let t = self.peek()?;
        if t.kind != TokenKind::Ident {
            let what = match kind {
                BlockKind::Root => "имя переменной --имя",
                _ => "имя свойства",
            };
            let msg = format!("ожидалось {what}, а встречено {}", self.describe_here());
            self.err(t.span.clone(), msg);
            let before = self.i;
            self.recover_to_decl_end();
            if self.i == before {
                self.i += 1;
            }
            return None;
        }
        let name = self.text(t).to_string();
        let name_span = t.span.clone();
        self.i += 1;
        if kind == BlockKind::Root && !name.starts_with("--") {
            self.err(
                name_span.clone(),
                format!("в :root допустимы только переменные --имя, а не «{name}»"),
            );
        }
        self.skip_trivia();
        if self.kind() != Some(TokenKind::Colon) {
            let msg = format!(
                "ожидалось «:» после «{name}», а встречено {}",
                self.describe_here()
            );
            self.err(self.here(), msg);
            let empty = name_span.end..name_span.end;
            self.recover_to_decl_end();
            let semicolon = (self.i > 0
                && self
                    .toks
                    .get(self.i - 1)
                    .is_some_and(|t| t.kind == TokenKind::Semicolon)
                && self.toks[self.i - 1].span.start >= name_span.end)
                .then(|| self.toks[self.i - 1].span.clone());
            let end = semicolon.as_ref().map_or(name_span.end, |s| s.end);
            return Some(Declaration {
                name,
                span: name_span.start..end,
                name_span,
                colon: None,
                value: String::new(),
                value_span: empty,
                value_tokens: self.i..self.i,
                semicolon,
            });
        }
        let colon = self.toks[self.i].span.clone();
        self.i += 1;
        self.skip_trivia();
        let v_start_i = self.i;
        let v_start = self.peek().map_or(self.src.len(), |t| t.span.start);
        let mut depth = 0i32;
        while let Some(t) = self.peek() {
            match t.kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
                TokenKind::RParen | TokenKind::RBracket if depth > 0 => depth -= 1,
                TokenKind::RBrace if depth > 0 => depth -= 1,
                TokenKind::Semicolon | TokenKind::RBrace if depth <= 0 => break,
                _ => {}
            }
            self.i += 1;
        }
        let v_end = self.trimmed_end(v_start_i, v_start);
        let mut v_end_i = self.i;
        while v_end_i > v_start_i && self.toks[v_end_i - 1].kind.is_trivia() {
            v_end_i -= 1;
        }
        let value_span = v_start..v_end;
        let value = self.src[value_span.clone()].to_string();
        if value.is_empty() {
            let at = if v_start_i < self.toks.len() {
                self.here()
            } else {
                colon.clone()
            };
            self.err(at, format!("пустое значение у «{name}»"));
        } else {
            self.check_value(v_start_i, v_end_i);
        }
        let semicolon = if self.kind() == Some(TokenKind::Semicolon) {
            let s = self.toks[self.i].span.clone();
            self.i += 1;
            Some(s)
        } else {
            None
        };
        let end = semicolon.as_ref().map_or(v_end.max(colon.end), |s| s.end);
        Some(Declaration {
            name,
            span: name_span.start..end,
            name_span,
            colon: Some(colon),
            value,
            value_span,
            value_tokens: v_start_i..v_end_i,
            semicolon,
        })
    }

    /// Проверки значения, которые основной парсер делает при разборе:
    /// `var()` в начале значения должен быть единственным, имя переменной —
    /// с `--`, значение по умолчанию — не пустое; скобки — парные.
    fn check_value(&mut self, from: usize, to: usize) {
        let toks = &self.toks[from..to];
        // Парность скобок.
        let mut stack: Vec<&Token> = Vec::new();
        for t in toks {
            match t.kind {
                TokenKind::LParen | TokenKind::LBracket => stack.push(t),
                TokenKind::RParen | TokenKind::RBracket if stack.pop().is_none() => {
                    self.err(t.span.clone(), format!("лишняя «{}»", self.text(t)));
                }
                _ => {}
            }
        }
        if let Some(t) = stack.first() {
            self.err(t.span.clone(), "скобка не закрыта");
        }
        let first = &toks[0];
        if !(first.kind == TokenKind::Ident
            && self.text(first) == "var"
            && toks.get(1).is_some_and(|t| t.kind == TokenKind::LParen))
        {
            return;
        }
        // Внутри var( … ).
        let mut j = 2;
        while toks.get(j).is_some_and(|t| t.kind.is_trivia()) {
            j += 1;
        }
        match toks.get(j) {
            Some(t) if t.kind == TokenKind::Ident && self.text(t).starts_with("--") => {}
            Some(t) => {
                let span = t.span.clone();
                self.err(span, "в var() ожидалось имя переменной вида --имя");
                return;
            }
            None => return,
        }
        // Конец var(): парная «)».
        let mut depth = 0i32;
        let mut close = None;
        let mut comma = None;
        for (k, t) in toks.iter().enumerate().skip(1) {
            match t.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(k);
                        break;
                    }
                }
                TokenKind::Comma if depth == 1 && comma.is_none() => comma = Some(k),
                _ => {}
            }
        }
        let Some(close) = close else { return };
        if let Some(c) = comma {
            if toks[c + 1..close].iter().all(|t| t.kind.is_trivia()) {
                let span = toks[c].span.clone();
                self.err(span, "в var() пустое значение по умолчанию после «,»");
            }
        }
        if let Some(rest) = toks[close + 1..].iter().find(|t| !t.kind.is_trivia()) {
            let span = rest.span.start..toks[toks.len() - 1].span.end;
            self.err(
                span,
                "после var(…) в начале значения ничего быть не может: такое значение \
                 не разбирается, и правило пропускается целиком",
            );
        }
    }

    fn parse_keyframes(&mut self) -> Keyframes {
        let keyword_span = self.toks[self.i].span.clone();
        self.i += 1;
        self.skip_trivia();
        let name = match self.peek() {
            Some(t) if t.kind == TokenKind::Ident => {
                self.i += 1;
                Some((self.text(t).to_string(), t.span.clone()))
            }
            _ => {
                let msg = format!(
                    "ожидалось имя @keyframes, а встречено {}",
                    self.describe_here()
                );
                self.err(self.here(), msg);
                None
            }
        };
        self.skip_trivia();
        let header_end = name.as_ref().map_or(keyword_span.end, |n| n.1.end);
        let header = keyword_span.start..header_end;
        let title = match &name {
            Some((n, _)) => format!("@keyframes {n}"),
            None => "@keyframes".into(),
        };
        if self.kind() != Some(TokenKind::LBrace) {
            let msg = format!(
                "ожидалась «{{» после {title}, а встречено {}",
                self.describe_here()
            );
            self.err(self.here(), msg);
            // Пропуск до конца строки-заголовка: до `{`, `}` или `;`.
            while let Some(t) = self.peek() {
                match t.kind {
                    TokenKind::LBrace => break,
                    TokenKind::Semicolon | TokenKind::RBrace => {
                        self.i += 1;
                        return Keyframes {
                            span: keyword_span.start..t.span.end,
                            keyword_span,
                            name,
                            open: None,
                            close: None,
                            steps: Vec::new(),
                        };
                    }
                    _ => self.i += 1,
                }
            }
            if self.peek().is_none() {
                return Keyframes {
                    span: keyword_span.start..self.src.len(),
                    keyword_span,
                    name,
                    open: None,
                    close: None,
                    steps: Vec::new(),
                };
            }
        }
        let open = self.toks[self.i].span.clone();
        self.i += 1;
        let mut steps = Vec::new();
        let close;
        loop {
            self.skip_trivia();
            let Some(t) = self.peek() else {
                self.err(
                    header.clone(),
                    format!("блок «{title}» не закрыт: нет «}}»"),
                );
                close = None;
                break;
            };
            if t.kind == TokenKind::RBrace {
                close = Some(t.span.clone());
                self.i += 1;
                break;
            }
            steps.push(self.parse_keyframe_step());
        }
        let end = close.as_ref().map_or(self.src.len(), |c| c.end);
        Keyframes {
            span: keyword_span.start..end,
            keyword_span,
            name,
            open: Some(open),
            close,
            steps,
        }
    }

    fn parse_keyframe_step(&mut self) -> KeyframeStep {
        let start_i = self.i;
        let start = self.toks[start_i].span.start;
        let mut selectors = Vec::new();
        let mut expect_pos = true;
        while let Some(t) = self.peek() {
            match t.kind {
                TokenKind::LBrace | TokenKind::RBrace => break,
                k if k.is_trivia() => {}
                TokenKind::Comma => {
                    if expect_pos {
                        self.err(t.span.clone(), "лишняя «,» в позициях шага");
                    }
                    expect_pos = true;
                }
                TokenKind::Ident if matches!(self.text(t), "from" | "to") => {
                    selectors.push((self.text(t).to_string(), t.span.clone()));
                    expect_pos = false;
                }
                TokenKind::Percentage => {
                    selectors.push((self.text(t).to_string(), t.span.clone()));
                    expect_pos = false;
                }
                _ => {
                    self.err(
                        t.span.clone(),
                        format!(
                            "позиция шага @keyframes — процент, from или to, а не «{}»",
                            self.text(t)
                        ),
                    );
                    expect_pos = false;
                }
            }
            self.i += 1;
        }
        let sel_end = self.trimmed_end(start_i, start);
        if self.kind() != Some(TokenKind::LBrace) {
            self.err(
                start..sel_end.max(start + 1).min(self.src.len()),
                "ожидалась «{» после позиции шага @keyframes",
            );
            if self.i == start_i {
                self.i += 1;
            }
            return KeyframeStep {
                span: start..sel_end,
                selectors,
                block: None,
            };
        }
        let owner = self.src[start..sel_end].to_string();
        let block = self.parse_block(BlockKind::Step, &(start..sel_end), &owner);
        KeyframeStep {
            span: start..block.span.end,
            selectors,
            block: Some(block),
        }
    }

    fn parse_at_rule(&mut self) -> AtRule {
        let t = &self.toks[self.i];
        let name = self.text(t)[1..].to_string();
        let name_span = t.span.clone();
        self.err(
            name_span.clone(),
            format!("@{name} не поддерживается: MSS знает только @keyframes"),
        );
        self.i += 1;
        let start = name_span.start;
        let mut block = None;
        let mut end = self.src.len();
        while let Some(t) = self.peek() {
            match t.kind {
                TokenKind::Semicolon => {
                    end = t.span.end;
                    self.i += 1;
                    break;
                }
                TokenKind::LBrace => {
                    let open = t.span.start;
                    let mut depth = 0i32;
                    while let Some(t) = self.peek() {
                        self.i += 1;
                        match t.kind {
                            TokenKind::LBrace => depth += 1,
                            TokenKind::RBrace => {
                                depth -= 1;
                                if depth == 0 {
                                    end = t.span.end;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    block = Some(open..end);
                    break;
                }
                TokenKind::RBrace => {
                    end = t.span.start;
                    break;
                }
                _ => self.i += 1,
            }
        }
        AtRule {
            name,
            name_span,
            span: start..end,
            block,
        }
    }

    /// Селекторы из токенов `[from, to)` (список через запятую).
    fn parse_selectors(&mut self, from: usize, to: usize) -> Vec<Selector> {
        let mut out = Vec::new();
        let mut j = from;
        while j < to {
            // Один селектор — до запятой верхнего уровня.
            let s_from = j;
            let mut depth = 0i32;
            while j < to {
                match self.toks[j].kind {
                    TokenKind::LParen => depth += 1,
                    TokenKind::RParen => depth -= 1,
                    TokenKind::Comma if depth <= 0 => break,
                    _ => {}
                }
                j += 1;
            }
            if let Some(sel) = self.parse_one_selector(s_from, j) {
                out.push(sel);
            } else if j < to {
                let span = self.toks[j].span.clone();
                self.err(span, "пустой селектор перед «,»");
            }
            if j < to {
                j += 1; // запятая
                if j >= to || self.toks[j..to].iter().all(|t| t.kind.is_trivia()) {
                    let span = self.toks[j - 1].span.clone();
                    self.err(span, "после «,» ожидался селектор");
                }
            }
        }
        out
    }

    fn parse_one_selector(&mut self, from: usize, to: usize) -> Option<Selector> {
        let mut a = from;
        let mut b = to;
        while a < b && self.toks[a].kind.is_trivia() {
            a += 1;
        }
        while b > a && self.toks[b - 1].kind.is_trivia() {
            b -= 1;
        }
        if a == b {
            return None;
        }
        let span = self.toks[a].span.start..self.toks[b - 1].span.end;
        let mut parts: Vec<SelectorPart> = Vec::new();
        let mut j = a;
        let mut gap = false;
        let part = |kind, name: &str, span: Span, name_span: Span| SelectorPart {
            kind,
            name: name.to_string(),
            span,
            name_span,
        };
        while j < b {
            let t = &self.toks[j];
            let text = self.text(t);
            let compound_part = !matches!(t.kind, TokenKind::Whitespace | TokenKind::Comment)
                && !(t.kind == TokenKind::Delim && matches!(text, ">" | "+" | "~"))
                && t.kind != TokenKind::Colon;
            if gap
                && compound_part
                && parts
                    .last()
                    .is_some_and(|p| !matches!(p.kind, SelectorPartKind::Combinator(_)))
            {
                let at = t.span.start;
                parts.push(part(SelectorPartKind::Combinator(' '), " ", at..at, at..at));
            }
            gap = false;
            match t.kind {
                TokenKind::Whitespace => gap = true,
                TokenKind::Comment => {}
                TokenKind::Delim if text == "." => match self.toks.get(j + 1) {
                    Some(n) if j + 1 < b && n.kind == TokenKind::Ident => {
                        parts.push(part(
                            SelectorPartKind::Class,
                            self.text(n),
                            t.span.start..n.span.end,
                            n.span.clone(),
                        ));
                        j += 1;
                    }
                    _ => self.err(t.span.clone(), "после «.» ожидалось имя класса"),
                },
                TokenKind::Number if text.starts_with('.') => {
                    // `.5x` — имя класса не может начинаться с цифры.
                    self.err(t.span.clone(), "имя класса не может начинаться с цифры");
                }
                TokenKind::Hash => parts.push(part(
                    SelectorPartKind::Id,
                    &text[1..],
                    t.span.clone(),
                    t.span.start + 1..t.span.end,
                )),
                TokenKind::Ident => parts.push(part(
                    SelectorPartKind::Element,
                    text,
                    t.span.clone(),
                    t.span.clone(),
                )),
                TokenKind::Colon => match self.toks.get(j + 1) {
                    Some(n) if j + 1 < b && n.kind == TokenKind::Ident => {
                        parts.push(part(
                            SelectorPartKind::Pseudo,
                            self.text(n),
                            t.span.start..n.span.end,
                            n.span.clone(),
                        ));
                        j += 1;
                    }
                    _ => self.err(t.span.clone(), "после «:» ожидалось имя псевдокласса"),
                },
                TokenKind::Delim if text == "*" => parts.push(part(
                    SelectorPartKind::Universal,
                    "*",
                    t.span.clone(),
                    t.span.clone(),
                )),
                TokenKind::Delim if text == "&" => parts.push(part(
                    SelectorPartKind::Nesting,
                    "&",
                    t.span.clone(),
                    t.span.clone(),
                )),
                TokenKind::Delim if matches!(text, ">" | "+" | "~") => {
                    if matches!(
                        parts.last().map(|p| &p.kind),
                        Some(SelectorPartKind::Combinator(' '))
                    ) {
                        parts.pop();
                    }
                    let c = text.chars().next().unwrap();
                    parts.push(part(
                        SelectorPartKind::Combinator(c),
                        text,
                        t.span.clone(),
                        t.span.clone(),
                    ));
                }
                _ => {
                    self.err(t.span.clone(), format!("непонятное «{text}» в селекторе"));
                }
            }
            j += 1;
        }
        Some(Selector {
            text: self.src[span.clone()].to_string(),
            span,
            parts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_lossless() {
        let src = ".a > Б:hover { color: #fff; /* к */ margin: -2px .5em 10%; }\n@keyframes x{from{opacity:0}}\"s";
        let (toks, errs) = tokenize(src);
        let joined: String = toks.iter().map(|t| t.text(src)).collect();
        assert_eq!(joined, src);
        assert_eq!(errs.len(), 1, "незакрытая строка");
        let kinds: Vec<_> = toks
            .iter()
            .filter(|t| !t.kind.is_trivia())
            .map(|t| (t.kind, t.text(src)))
            .take(6)
            .collect();
        assert_eq!(
            kinds,
            vec![
                (TokenKind::Delim, "."),
                (TokenKind::Ident, "a"),
                (TokenKind::Delim, ">"),
                (TokenKind::Ident, "Б"),
                (TokenKind::Colon, ":"),
                (TokenKind::Ident, "hover"),
            ]
        );
        assert!(toks
            .iter()
            .any(|t| t.kind == TokenKind::Dimension && t.text(src) == "-2px"));
        assert!(toks
            .iter()
            .any(|t| t.kind == TokenKind::Dimension && t.text(src) == ".5em"));
        assert!(toks
            .iter()
            .any(|t| t.kind == TokenKind::Percentage && t.text(src) == "10%"));
        assert!(toks
            .iter()
            .any(|t| t.kind == TokenKind::Hash && t.text(src) == "#fff"));
    }

    #[test]
    fn rule_with_declarations_and_spans() {
        let src = ".card, Button:hover {\n  color: red;\n  margin: 1px 2px\n}";
        let s = parse(src);
        assert!(s.errors.is_empty(), "{:?}", s.errors);
        let Item::Rule(r) = &s.items[0] else { panic!() };
        assert_eq!(&src[r.selector_span.clone()], ".card, Button:hover");
        assert_eq!(r.selectors.len(), 2);
        assert_eq!(r.selectors[1].parts[1].kind, SelectorPartKind::Pseudo);
        assert_eq!(r.selectors[1].parts[1].name, "hover");
        let decls: Vec<_> = r.block.as_ref().unwrap().declarations().collect();
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0].name, "color");
        assert_eq!(&src[decls[0].span.clone()], "color: red;");
        assert_eq!(&src[decls[1].value_span.clone()], "1px 2px");
        assert!(decls[1].semicolon.is_none());
        assert_eq!(r.span, 0..src.len());
    }

    #[test]
    fn root_nested_keyframes_and_unknown_at() {
        let src = ":root { --a: #fff; --b: 4px; }\n.p { gap: 1px; &:hover { color: red; } .c { x: 1; } }\n@keyframes spin { from, 50% { opacity: 0; } to { opacity: 1 } }\n@media screen { .x { y: 1 } }\n.z { w: 1 }";
        let s = parse(src);
        assert_eq!(s.errors.len(), 1, "{:?}", s.errors);
        assert!(s.errors[0].message.contains("@media"));
        let vars: Vec<_> = s.root_variables().iter().map(|d| d.name.clone()).collect();
        assert_eq!(vars, ["--a", "--b"]);
        let rules = s.walk_rules();
        assert_eq!(rules.len(), 5); // :root, .p, &:hover, .c, .z
        assert_eq!(rules[2].1.len(), 1);
        assert_eq!(s.keyframes_names()[0].0, "spin");
        let Item::Keyframes(k) = &s.items[2] else {
            panic!()
        };
        assert_eq!(k.steps.len(), 2);
        assert_eq!(k.steps[0].selectors.len(), 2);
        assert!(matches!(s.items[3], Item::AtRule(_)));
        assert!(matches!(&s.items[4], Item::Rule(r) if r.selectors[0].text == ".z"));
    }

    #[test]
    fn recovery_after_errors() {
        let src = ".a { color red; margin: 1px; }\n.b { : 1; }\n.c { col";
        let s = parse(src);
        let msgs: Vec<_> = s.errors.iter().map(|e| e.message.as_str()).collect();
        assert!(
            msgs[0].starts_with("ожидалось «:» после «color»"),
            "{msgs:?}"
        );
        assert!(msgs.iter().any(|m| m.contains("ожидалось имя свойства")));
        assert!(msgs.iter().any(|m| m.contains("не закрыт")));
        let Item::Rule(a) = &s.items[0] else { panic!() };
        let names: Vec<_> = a
            .block
            .as_ref()
            .unwrap()
            .declarations()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(names, ["color", "margin"]);
        // Недописанное имя — объявление без «:».
        let Item::Rule(c) = &s.items[2] else { panic!() };
        let d = c.block.as_ref().unwrap().declarations().next().unwrap();
        assert_eq!(d.name, "col");
        assert!(d.colon.is_none());
    }

    #[test]
    fn var_prefix_value_must_be_alone() {
        let s = parse(".a { padding: var(--x) var(--y); color: var(x); b: var(--z, ); }");
        let msgs: Vec<_> = s.errors.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(msgs.len(), 3, "{msgs:?}");
        assert!(msgs[0].contains("после var(…)"));
        assert!(msgs[1].contains("--имя"));
        assert!(msgs[2].contains("пустое значение по умолчанию"));
    }

    #[test]
    fn never_panics_on_garbage() {
        let samples = [
            "",
            "{",
            "}",
            ":",
            ";",
            "@",
            "@keyframes",
            "@keyframes x",
            "@keyframes x {",
            "@keyframes x { 5 }",
            ".a { b: (; }",
            "/*",
            "\"",
            ".a{&",
            ":root{",
            ":root{a:1}",
            "a,{}",
            ",",
            ".",
            "#",
            "-",
            "--",
            ".a { b: c } } }",
            "ё{ж:з}",
            "\u{feff}.a{}",
            "@media{{{",
            ".a{.b{.c{",
            ".a{b:var(}",
            "\r\n.a\r\n{\r\n}",
        ];
        for s in samples {
            let p = parse(s);
            let joined: String = p.tokens.iter().map(|t| t.text(s)).collect();
            assert_eq!(joined, s);
            for e in &p.errors {
                assert!(
                    e.span.end <= s.len() && e.span.start <= e.span.end,
                    "{s:?}: {e:?}"
                );
            }
        }
    }

    #[test]
    fn line_index_roundtrip() {
        let src = "аб\r\nвгд\n\nx";
        let li = LineIndex::new(src);
        assert_eq!(li.line_count(), 4);
        for off in [0, 2, 4, 6, 13, src.len()] {
            let (l, c) = li.position(src, off);
            assert_eq!(li.offset(src, l, c), off, "{off}");
        }
        assert_eq!(li.position(src, src.find('в').unwrap()), (1, 0));
        assert_eq!(li.offset(src, 0, 99), "аб".len());
        assert_eq!(li.offset(src, 99, 0), src.len());
    }
}
