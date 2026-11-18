//! Построчная подсветка ассемблера: GNU as (AT&T и `.intel_syntax`), NASM/MASM.
//!
//! Строка разбирается сама по себе (многострочное состояние — только `/* … */`): метки (`имя:`,
//! `.L_3016:`), директивы (`.section`, `.byte`), мнемоника (первое слово команды, префиксы
//! `lock`/`rep` и псевдопрефиксы `{disp8}` — отдельно), регистры (`%rax`, `rax`, `x0`, `w1`),
//! непосредственные (`$0x10`, `#4`), числа, строки и символы, комментарии (`#`, `;`, `//`, `@`
//! у ARM — только в начале операндов не трогается).

use super::capture_names::TokenClass;
use super::highlight_cache::{LineSpans, Span};
use super::highlighter::LineHighlighter;

const NORMAL: u32 = 0;
const BLOCK_COMMENT: u32 = 1;

const COMMENT: TokenClass = TokenClass::Comment;
const LABEL: TokenClass = TokenClass::Function;
const DIRECTIVE: TokenClass = TokenClass::Attribute;
const MNEMONIC: TokenClass = TokenClass::Keyword;
const PREFIX: TokenClass = TokenClass::KeywordControl;
const REGISTER: TokenClass = TokenClass::Variable;
const NUMBER: TokenClass = TokenClass::Number;
const STRING: TokenClass = TokenClass::String;
const PUNCT: TokenClass = TokenClass::Punctuation;

pub struct AsmHighlighter;

/// Имя регистра без `%` (x86, ARM/AArch64, RISC-V, MIPS — самые частые).
fn is_register(w: &str) -> bool {
    let l = w.to_ascii_lowercase();
    let l = l.as_str();
    const X86: &[&str] = &[
        "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "rip", "eax", "ebx", "ecx", "edx", "esi", "edi", "ebp", "esp", "eip", "ax", "bx", "cx", "dx", "si", "di", "bp", "sp", "al", "bl", "cl", "dl", "ah",
        "bh", "ch", "dh", "sil", "dil", "bpl", "spl", "cs", "ds", "es", "fs", "gs", "ss", "st", "eflags", "rflags",
    ];
    if X86.contains(&l) {
        return true;
    }
    let num = |p: &str| -> Option<u32> { l.strip_prefix(p).and_then(|r| r.trim_end_matches(['d', 'w', 'b']).parse::<u32>().ok()) };
    // r8..r15 (+d/w/b), xmm/ymm/zmm0..31, k0..7, st(0) пишется отдельно, mm0..7, cr/dr
    if num("r").is_some_and(|n| n < 32) || ["xmm", "ymm", "zmm", "mm", "cr", "dr", "k"].iter().any(|p| num(p).is_some_and(|n| n < 32)) {
        return true;
    }
    // AArch64: x0..x30, w0..w30, v/q/d/s/h/b0..31, xzr, wzr, sp, lr, fp, pc
    if matches!(l, "xzr" | "wzr" | "lr" | "fp" | "pc" | "sp") {
        return true;
    }
    ["x", "w", "v", "q", "d", "s", "h", "b", "a", "t"].iter().any(|p| l.strip_prefix(p).and_then(|r| r.parse::<u32>().ok()).is_some_and(|n| n < 32))
}

fn ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'$' | b'@')
}

impl LineHighlighter for AsmHighlighter {
    fn highlight_line(&self, line: &str, state: u32, out: &mut LineSpans) -> u32 {
        let b = line.as_bytes();
        let n = b.len();
        let mut i = 0usize;
        // продолжение `/* … */`
        if state == BLOCK_COMMENT {
            match line.find("*/") {
                Some(e) => {
                    out.push(Span::new(0, (e + 2) as u32, COMMENT));
                    i = e + 2;
                }
                None => {
                    out.push(Span::new(0, n as u32, COMMENT));
                    return BLOCK_COMMENT;
                }
            }
        }
        // первое слово команды на этой строке ещё не встречалось
        let mut need_mnemonic = true;
        while i < n {
            let c = b[i];
            if c == b' ' || c == b'\t' {
                i += 1;
                continue;
            }
            // комментарии до конца строки: `#` (GNU as x86), `;` (NASM/MASM, кроме разделителя у as —
            // считаем комментарием), `//`; `/*` — блочный
            if c == b'#' && (need_mnemonic || i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') && !b.get(i + 1).is_some_and(|d| d.is_ascii_digit() || *d == b'-') {
                out.push(Span::new(i as u32, n as u32, COMMENT));
                return NORMAL;
            }
            if c == b';' || (c == b'/' && b.get(i + 1) == Some(&b'/')) {
                out.push(Span::new(i as u32, n as u32, COMMENT));
                return NORMAL;
            }
            if c == b'/' && b.get(i + 1) == Some(&b'*') {
                match line[i + 2..].find("*/") {
                    Some(e) => {
                        let end = i + 2 + e + 2;
                        out.push(Span::new(i as u32, end as u32, COMMENT));
                        i = end;
                        continue;
                    }
                    None => {
                        out.push(Span::new(i as u32, n as u32, COMMENT));
                        return BLOCK_COMMENT;
                    }
                }
            }
            // строки и символы
            if c == b'"' || c == b'\'' {
                let mut j = i + 1;
                while j < n && b[j] != c {
                    if b[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                let end = (j + 1).min(n);
                out.push(Span::new(i as u32, end as u32, STRING));
                i = end;
                continue;
            }
            // псевдопрефикс GNU as: {disp8}, {load}, {vex3}
            if c == b'{' {
                if let Some(e) = line[i..].find('}') {
                    out.push(Span::new(i as u32, (i + e + 1) as u32, PREFIX));
                    i += e + 1;
                    continue;
                }
            }
            // регистр AT&T: %rax, %xmm0, %st(1)
            if c == b'%' {
                let mut j = i + 1;
                while j < n && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                out.push(Span::new(i as u32, j as u32, REGISTER));
                i = j;
                continue;
            }
            // непосредственное значение: $0x10, $-8, #4 (ARM); `$метка` — тоже
            if (c == b'$' || c == b'#') && b.get(i + 1).is_some_and(|d| d.is_ascii_alphanumeric() || matches!(*d, b'-' | b'_' | b'.')) {
                let mut j = i + 1;
                if b.get(j) == Some(&b'-') {
                    j += 1;
                }
                let digit = b.get(j).is_some_and(|d| d.is_ascii_digit());
                while j < n && ident_char(b[j]) {
                    j += 1;
                }
                out.push(Span::new(i as u32, j as u32, if digit { NUMBER } else { REGISTER }));
                i = j;
                continue;
            }
            // число: 0x1f, 123, 1f/1b (локальные метки), -4
            if c.is_ascii_digit() || (c == b'-' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
                let mut j = i + 1;
                while j < n && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                // `1:` — числовая метка в начале строки
                if need_mnemonic && b.get(j) == Some(&b':') {
                    out.push(Span::new(i as u32, (j + 1) as u32, LABEL));
                    i = j + 1;
                    continue;
                }
                out.push(Span::new(i as u32, j as u32, NUMBER));
                i = j;
                continue;
            }
            // слово: метка, директива, мнемоника, регистр (Intel), символ
            if ident_char(c) {
                let mut j = i;
                while j < n && ident_char(b[j]) {
                    j += 1;
                }
                let w = &line[i..j];
                if need_mnemonic && b.get(j) == Some(&b':') {
                    out.push(Span::new(i as u32, (j + 1) as u32, LABEL));
                    i = j + 1;
                    continue;
                }
                if need_mnemonic && w.starts_with('.') {
                    out.push(Span::new(i as u32, j as u32, DIRECTIVE));
                    need_mnemonic = false;
                } else if need_mnemonic {
                    // `имя = выражение` (символ), а не команда
                    let rest = line[j..].trim_start();
                    if rest.starts_with('=') && !rest.starts_with("==") {
                        out.push(Span::new(i as u32, j as u32, LABEL));
                    } else {
                        let lw = w.to_ascii_lowercase();
                        let prefix = matches!(lw.as_str(), "lock" | "rep" | "repe" | "repz" | "repne" | "repnz" | "notrack" | "data16" | "addr32" | "rex" | "rex.w" | "bnd" | "xacquire" | "xrelease");
                        out.push(Span::new(i as u32, j as u32, if prefix { PREFIX } else { MNEMONIC }));
                        if !prefix {
                            need_mnemonic = false;
                        }
                    }
                } else if is_register(w) {
                    out.push(Span::new(i as u32, j as u32, REGISTER));
                } else if matches!(w.to_ascii_lowercase().as_str(), "byte" | "word" | "dword" | "qword" | "xmmword" | "ymmword" | "zmmword" | "tbyte" | "ptr" | "offset" | "short" | "near" | "far") {
                    out.push(Span::new(i as u32, j as u32, PREFIX));
                }
                i = j;
                continue;
            }
            // знаки: ( ) [ ] , + - * :
            if matches!(c, b'(' | b')' | b'[' | b']' | b',' | b'+' | b'*' | b':' | b'!') {
                out.push(Span::new(i as u32, (i + 1) as u32, PUNCT));
            }
            i += 1;
        }
        NORMAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(line: &str) -> Vec<(String, TokenClass)> {
        let mut out = LineSpans::default();
        AsmHighlighter.highlight_line(line, NORMAL, &mut out);
        out.spans.iter().map(|s| (line[s.byte_start as usize..s.byte_end as usize].to_string(), s.class)).collect()
    }

    #[test]
    fn att_instruction_with_comment() {
        let c = classes("\tmovq __gmon_start__@GOTPCREL(%rip),%rax    # 3008");
        assert_eq!(c[0], ("movq".into(), MNEMONIC));
        assert!(c.contains(&("%rip".into(), REGISTER)));
        assert!(c.contains(&("%rax".into(), REGISTER)));
        assert_eq!(c.last().unwrap(), &("# 3008".into(), COMMENT));
    }

    #[test]
    fn labels_directives_immediates() {
        assert_eq!(classes(".L_3016:")[0], (".L_3016:".into(), LABEL));
        let d = classes("\t.section .init,\"ax\",@progbits");
        assert_eq!(d[0], (".section".into(), DIRECTIVE));
        assert!(d.contains(&("\"ax\"".into(), STRING)));
        let s = classes("\tsubq $8,%rsp");
        assert!(s.contains(&("$8".into(), NUMBER)));
        let p = classes("\t{disp8} lock addl $0xfffffff0,4(%esp)");
        assert_eq!(p[0], ("{disp8}".into(), PREFIX));
        assert_eq!(p[1], ("lock".into(), PREFIX));
        assert_eq!(p[2], ("addl".into(), MNEMONIC));
        assert!(p.contains(&("4".into(), NUMBER)));
    }

    #[test]
    fn intel_and_block_comment() {
        let c = classes("mov eax, dword ptr [ebx+4] ; x");
        assert_eq!(c[0], ("mov".into(), MNEMONIC));
        assert!(c.contains(&("eax".into(), REGISTER)));
        assert!(c.contains(&("dword".into(), PREFIX)));
        assert!(c.contains(&("ebx".into(), REGISTER)));
        let mut out = LineSpans::default();
        assert_eq!(AsmHighlighter.highlight_line("nop /* начало", NORMAL, &mut out), BLOCK_COMMENT);
        let mut out = LineSpans::default();
        assert_eq!(AsmHighlighter.highlight_line("конец */ ret", BLOCK_COMMENT, &mut out), NORMAL);
        assert_eq!(out.spans[1].class, MNEMONIC);
    }
}
