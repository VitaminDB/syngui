//! Запись markdown с сохранением исходной разметки (файлы с диска).
//!
//! [`serialize_document`] выдаёт нормализованный текст: `*курсив*`, `- `
//! маркеры, ATX-заголовки. Для заметок это удобно, но файл пользователя
//! после правки одного слова переписывался бы целиком. Здесь исходник
//! документа ([`DocModel::source`]) режется на куски верхнего уровня —
//! блоки, а у списков каждый пункт отдельно, — и кусок, который разбирается
//! в те же блоки, что и сейчас в модели, пишется дословно, вместе с
//! пустыми строками и отступами между нетронутыми соседями. Изменённые и
//! новые блоки пишутся нормализатором, но символами, принятыми в документе
//! ([`MdStyle`]: `_курсив_`, `__жирный__`, `* ` маркеры, `1)`, `~~~`).
//!
//! Сопоставление — по содержимому (markdown блока при одном и том же
//! стиле), по порядку, с поиском ближайшей точки совпадения после вставок
//! и удалений; id блоков для этого не нужны.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use super::free;
use super::model::*;
use super::parse::parse_document;
use super::serialize::{block_markdown, serialize_document, with_style, MdStyle};

/// Насколько далеко искать точку совпадения после правки (блоков + кусков).
const RESYNC: usize = 96;

/// Кусок исходника и блоки, в которые он разбирается.
struct Chunk {
    range: Range<usize>,
    canon: Vec<String>,
    first_item: bool,
    last_item: bool,
}

/// Markdown документа с сохранением исходной разметки нетронутых блоков.
/// Без исходника (документ создан не разбором) или со свободной
/// раскладкой — обычная нормализованная запись.
pub fn serialize_preserving(model: &DocModel) -> String {
    let Some(src) = model.source.clone() else {
        return serialize_document(model);
    };
    if free::serialize_geometry(&model.blocks).is_some() {
        return serialize_document(model);
    }
    let (ranges, style, defs) = scan(&src);
    // Служебный блок свободной раскладки в исходнике — запись целиком
    // нормализатором (он же пишет и раскладку).
    let fence = format!("```{}", free::LAYOUT_FENCE);
    if ranges.iter().any(|r| src[r.clone()].trim_start().starts_with(&fence)) {
        return serialize_document(model);
    }
    with_style(style, || {
        let chunks: Vec<Chunk> = ranges
            .into_iter()
            .filter_map(|range| {
                // Ссылки по меткам (`[текст][метка]`) разбираются только с
                // определениями меток — они дописываются к куску.
                let blocks = if defs.is_empty() {
                    parse_document(&src[range.clone()]).blocks
                } else {
                    parse_document(&format!("{}\n\n{defs}", &src[range.clone()])).blocks
                };
                let first = blocks.first()?.kind.is_list_item();
                let last = blocks.last()?.kind.is_list_item();
                Some(Chunk { canon: blocks.iter().map(canon).collect(), range, first_item: first, last_item: last })
            })
            .collect();
        let md: Vec<String> = model.blocks.iter().map(block_markdown).collect();
        let keys: Vec<String> = model.blocks.iter().map(canon).collect();
        emit(&src, &chunks, &model.blocks, &md, &keys)
    })
}

/// Ключ сравнения блока: его markdown без номера пункта — модель нумерует
/// пункты подряд, а в исходнике бывает `1. 1. 1.` или `3. 5.` (CommonMark
/// берёт только номер первого пункта, так что дословный кусок верен).
fn canon(b: &DocBlock) -> String {
    match &b.kind {
        BlockKind::Numbered { number, .. } if *number != 1 => {
            let mut c = b.clone();
            if let BlockKind::Numbered { number, .. } = &mut c.kind {
                *number = 1;
            }
            block_markdown(&c)
        }
        _ => block_markdown(b),
    }
}

/// Куски верхнего уровня и символы разметки, которых в документе больше.
fn scan(src: &str) -> (Vec<Range<usize>>, MdStyle, String) {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let mut ranges = Vec::new();
    let mut depth = 0usize;
    let mut top_list = false;
    // Счётчики вариантов: [*, _], [*, _], [-, *, +], [., )], [`, ~].
    let (mut em, mut strong, mut bullet, mut delim, mut fence) = ([0; 2], [0; 2], [0; 3], [0; 2], [0; 2]);
    let mut ordered: Vec<bool> = Vec::new();
    let byte = |i: usize| src.as_bytes().get(i).copied().unwrap_or(0);
    let mut parser = Parser::new_ext(src, opts).into_offset_iter();
    let mut defs = String::new();
    let mut defs_taken = false;
    loop {
        let Some((ev, r)) = parser.next() else { break };
        if !defs_taken {
            // Определения меток известны после первого события.
            defs_taken = true;
            for (_, d) in parser.reference_definitions().iter() {
                defs.push_str(&src[d.span.clone()]);
                defs.push('\n');
            }
        }
        match ev {
            Event::Start(tag) => {
                match &tag {
                    Tag::Emphasis => em[(byte(r.start) == b'_') as usize] += 1,
                    Tag::Strong => strong[(byte(r.start) == b'_') as usize] += 1,
                    Tag::List(start) => ordered.push(start.is_some()),
                    Tag::Item => {
                        let s = src[r.start..].trim_start_matches([' ', '\t']);
                        if ordered.last() == Some(&true) {
                            match s.trim_start_matches(|c: char| c.is_ascii_digit()).bytes().next() {
                                Some(b')') => delim[1] += 1,
                                Some(b'.') => delim[0] += 1,
                                _ => {}
                            }
                        } else {
                            match s.bytes().next() {
                                Some(b'-') => bullet[0] += 1,
                                Some(b'*') => bullet[1] += 1,
                                Some(b'+') => bullet[2] += 1,
                                _ => {}
                            }
                        }
                    }
                    Tag::CodeBlock(CodeBlockKind::Fenced(_)) => {
                        match src[r.start..].trim_start_matches([' ', '\t']).bytes().next() {
                            Some(b'~') => fence[1] += 1,
                            Some(b'`') => fence[0] += 1,
                            _ => {}
                        }
                    }
                    _ => {}
                }
                if depth == 0 {
                    if matches!(tag, Tag::List(_)) {
                        top_list = true;
                    } else {
                        ranges.push(trimmed(src, r.clone()));
                    }
                } else if depth == 1 && top_list && matches!(tag, Tag::Item) {
                    ranges.push(trimmed(src, r.clone()));
                }
                depth += 1;
            }
            Event::End(end) => {
                depth = depth.saturating_sub(1);
                if let TagEnd::List(_) = end {
                    ordered.pop();
                    if depth == 0 {
                        top_list = false;
                    }
                }
            }
            _ if depth == 0 => ranges.push(trimmed(src, r)),
            _ => {}
        }
    }
    ranges.retain(|r| r.start < r.end);
    let pick = |c: &[i32], v: &[char]| -> char {
        let mut best = 0;
        for i in 1..c.len() {
            if c[i] > c[best] {
                best = i;
            }
        }
        v[best]
    };
    let style = MdStyle {
        em: pick(&em, &['*', '_']),
        strong: pick(&strong, &['*', '_']),
        bullet: pick(&bullet, &['-', '*', '+']),
        delim: pick(&delim, &['.', ')']),
        fence: pick(&fence, &['`', '~']),
    };
    (ranges, style, defs)
}

/// От начала строки (отступ кода и пунктов — часть куска), без хвостовых
/// пробелов и переводов строк — они уходят в промежуток.
fn trimmed(src: &str, r: Range<usize>) -> Range<usize> {
    let line = src[..r.start].rfind('\n').map_or(0, |i| i + 1);
    let start = if src[line..r.start].trim().is_empty() { line } else { r.start };
    let end = r.start + src[r.clone()].trim_end_matches(['\n', '\r']).len();
    start..end.max(start)
}

#[derive(Clone, Copy, PartialEq)]
enum Prev {
    None,
    Chunk(usize),
    /// Блок нормализатора; `replaced` — последний исходный кусок, который
    /// он заменил (тогда промежуток после него берётся из исходника).
    Block { item: bool, replaced: Option<usize> },
}

fn emit(src: &str, chunks: &[Chunk], blocks: &[DocBlock], md: &[String], keys: &[String]) -> String {
    let matches = |i: usize, k: usize| -> bool {
        let c = &chunks[k].canon;
        i + c.len() <= keys.len() && keys[i..i + c.len()] == c[..]
    };
    // Исходный промежуток перед куском `k`.
    let gap = |k: usize| -> &str {
        if k == 0 {
            &src[..chunks[0].range.start]
        } else {
            &src[chunks[k - 1].range.end..chunks[k].range.start]
        }
    };
    let sep = |prev: Prev, first_item: bool| -> &'static str {
        let last_item = match prev {
            Prev::None => return "",
            Prev::Block { item, .. } => item,
            Prev::Chunk(k) => chunks[k].last_item,
        };
        if last_item && first_item {
            "\n"
        } else {
            "\n\n"
        }
    };
    let mut out = String::new();
    let mut prev = Prev::None;
    let put_chunk = |out: &mut String, prev: &mut Prev, k: usize| {
        let c = &chunks[k];
        match *prev {
            Prev::Chunk(p) if p + 1 == k => out.push_str(gap(k)),
            Prev::Block { replaced: Some(p), .. } if p + 1 == k => out.push_str(gap(k)),
            Prev::None if k == 0 => out.push_str(gap(0)),
            p => out.push_str(sep(p, c.first_item)),
        }
        out.push_str(&src[c.range.clone()]);
        *prev = Prev::Chunk(k);
    };
    // Блоки `i..i+b` на месте кусков `k..k+a`.
    let put_blocks = |out: &mut String, prev: &mut Prev, i: usize, b: usize, k: usize, a: usize| {
        for n in i..i + b {
            let item = blocks[n].kind.is_list_item();
            let first = n == i;
            match *prev {
                Prev::Chunk(p) if first && a > 0 && p + 1 == k => out.push_str(gap(k)),
                Prev::None if first && a > 0 && k == 0 => out.push_str(gap(0)),
                p => out.push_str(sep(p, item)),
            }
            out.push_str(md[n].trim_end_matches('\n'));
            let replaced = (n + 1 == i + b && a > 0).then(|| k + a - 1);
            *prev = Prev::Block { item, replaced };
        }
    };
    let (mut i, mut k) = (0, 0);
    while i < md.len() {
        if k < chunks.len() && matches(i, k) {
            put_chunk(&mut out, &mut prev, k);
            i += chunks[k].canon.len();
            k += 1;
            continue;
        }
        // Ближайшая точка, где блоки снова совпадают с кусками: `b` новых
        // или изменённых блоков, `a` удалённых или изменённых кусков.
        let mut found = None;
        'search: for d in 1..=RESYNC {
            for a in 0..=d {
                let b = d - a;
                if k + a < chunks.len() && i + b < md.len() && matches(i + b, k + a) {
                    found = Some((a, b));
                    break 'search;
                }
            }
        }
        let (a, b) = found.unwrap_or((chunks.len().saturating_sub(k), md.len() - i));
        put_blocks(&mut out, &mut prev, i, b, k, a);
        i += b;
        k += a;
    }
    let last = chunks.len().checked_sub(1);
    match prev {
        // Хвост файла (перевод строки в конце или его отсутствие) — как был.
        Prev::Chunk(p) | Prev::Block { replaced: Some(p), .. } if Some(p) == last => {
            out.push_str(&src[chunks[p].range.end..]);
            if !matches!(prev, Prev::Chunk(_)) && !out.ends_with('\n') && src.ends_with('\n') {
                out.push('\n');
            }
        }
        Prev::None => {}
        _ => out.push('\n'),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::input::document_editor::model::{BlockKind, InlineRun, InlineText};

    fn edit_text(model: &mut DocModel, idx: usize, f: impl FnOnce(&mut InlineText)) {
        match &mut model.blocks[idx].kind {
            BlockKind::Paragraph(t) | BlockKind::Heading { text: t, .. } | BlockKind::Bullet { text: t, .. } | BlockKind::Numbered { text: t, .. } => f(t),
            _ => panic!("не текстовый блок"),
        }
    }

    fn append(t: &mut InlineText, s: &str) {
        t.0.push(InlineRun { text: s.to_string(), style: Default::default() });
    }

    const SRC: &str = "Title\n=====\n\nЭто _курсив_ и __жирный__ текст.\n\n* раз\n* два\n\n1) один\n2) два\n   * вложенный\n\n~~~rust\nfn main() {}\n~~~\n\n***\n\n\nКонец   \n";

    #[test]
    fn untouched_document_is_byte_identical() {
        let m = parse_document(SRC);
        assert_eq!(serialize_preserving(&m), SRC);
        let no_nl = "*a*\n\n- b";
        assert_eq!(serialize_preserving(&parse_document(no_nl)), no_nl);
    }

    #[test]
    fn edit_keeps_other_blocks_and_document_style() {
        let mut m = parse_document(SRC);
        // Правка второго пункта: остальное — дословно, пункт — маркером `*`.
        let idx = m.blocks.iter().position(|b| matches!(&b.kind, BlockKind::Bullet { text, .. } if text.0.iter().any(|r| r.text == "два"))).unwrap();
        edit_text(&mut m, idx, |t| append(t, " ab"));
        let out = serialize_preserving(&m);
        assert_eq!(out, SRC.replace("* два\n", "* два ab\n"));
    }

    #[test]
    fn edited_paragraph_uses_document_emphasis() {
        let mut m = parse_document(SRC);
        edit_text(&mut m, 1, |t| append(t, "!"));
        let out = serialize_preserving(&m);
        assert!(out.contains("Это _курсив_ и __жирный__ текст.!"), "{out}");
        assert!(out.starts_with("Title\n=====\n\n"), "{out}");
    }

    #[test]
    fn inserted_and_deleted_blocks_resync() {
        let mut m = parse_document(SRC);
        let p = m.blocks[1].clone();
        let mut extra = p.clone();
        extra.id = m.alloc_id();
        m.blocks.insert(2, extra);
        let out = serialize_preserving(&m);
        assert_eq!(out.matches("Это _курсив_").count(), 2, "{out}");
        assert!(out.contains("~~~rust\nfn main() {}\n~~~\n\n***\n\n\nКонец   \n"), "{out}");
        // Удаление блока кода — соседние остаются как были.
        let mut m = parse_document(SRC);
        m.blocks.retain(|b| !matches!(b.kind, BlockKind::CodeBlock { .. }));
        let out = serialize_preserving(&m);
        assert!(!out.contains("fn main"), "{out}");
        assert!(out.contains("   * вложенный\n\n***\n\n\nКонец   \n"), "{out}");
    }

    #[test]
    fn underscore_falls_back_inside_words() {
        let src = "_a_ x\n\nслово\n";
        let mut m = parse_document(src);
        // Курсив внутри слова: `_` там не работает, нужен `*`.
        edit_text(&mut m, 1, |t| {
            t.0 = vec![
                InlineRun { text: "сло".into(), style: Default::default() },
                InlineRun { text: "во".into(), style: crate::widgets::input::document_editor::model::InlineStyle { italic: true, ..Default::default() } },
            ]
        });
        let out = serialize_preserving(&m);
        assert!(out.contains("сло*во*"), "{out}");
        assert_eq!(parse_document(&out).blocks.len(), 2);
    }

    #[test]
    fn plain_serialize_stays_normalized() {
        assert_eq!(serialize_document(&parse_document("_a_\n")), "*a*\n");
    }
}

/// Корпус: `MD_CORPUS=список.txt cargo test … corpus -- --ignored` —
/// каждый файл из списка без правок пишется байт в байт.
#[cfg(test)]
#[test]
#[ignore]
fn corpus_roundtrip_is_identity() {
    let list = std::env::var("MD_CORPUS").expect("MD_CORPUS");
    let mut bad = Vec::new();
    let mut n = 0;
    for path in std::fs::read_to_string(list).unwrap().lines() {
        let Ok(src) = std::fs::read_to_string(path) else { continue };
        n += 1;
        let out = serialize_preserving(&parse_document(&src));
        if out != src {
            if bad.len() < 12 {
                let at = src.bytes().zip(out.bytes()).position(|(a, b)| a != b).unwrap_or(src.len().min(out.len()));
                let lo = src.floor_char_boundary(at.saturating_sub(80));
                let lo2 = out.floor_char_boundary(at.saturating_sub(80));
                eprintln!("== {path} @{at}\n-- src: {:?}\n-- out: {:?}", &src[lo..src.floor_char_boundary((at + 80).min(src.len()))], &out[lo2..out.floor_char_boundary((at + 80).min(out.len()))]);
            }
            bad.push(path.to_string());
        }
    }
    assert!(bad.is_empty(), "{} из {n}: {:#?}", bad.len(), &bad[..bad.len().min(30)]);
}

/// Корпус: правка абзаца посередине — результат разбирается в ту же
/// модель, а строк исходника теряется не больше, чем в правленом абзаце.
#[cfg(test)]
#[test]
#[ignore]
fn corpus_edit_is_faithful() {
    use super::model::{InlineRun, InlineStyle};
    let list = std::env::var("MD_CORPUS").expect("MD_CORPUS");
    let mut bad = Vec::new();
    let mut n = 0;
    for path in std::fs::read_to_string(list).unwrap().lines() {
        let Ok(src) = std::fs::read_to_string(path) else { continue };
        let mut m = parse_document(&src);
        if free::serialize_geometry(&m.blocks).is_some() {
            continue;
        }
        let paras: Vec<usize> = (0..m.blocks.len()).filter(|&i| matches!(m.blocks[i].kind, BlockKind::Paragraph(_))).collect();
        let Some(&idx) = paras.get(paras.len() / 2) else { continue };
        n += 1;
        if let BlockKind::Paragraph(t) = &mut m.blocks[idx].kind {
            t.0.push(InlineRun { text: " ПРАВКА".into(), style: InlineStyle::default() });
        }
        let out = serialize_preserving(&m);
        let same_model = serialize_document(&parse_document(&out)) == serialize_document(&m);
        let src_lines: std::collections::HashSet<&str> = out.lines().collect();
        let para = block_markdown(&m.blocks[idx]);
        // Строки исходника, которых нет в выводе и которые не из правленого
        // абзаца (его мягкие переносы нормализатор склеивает).
        let lost: Vec<&str> = src
            .lines()
            .filter(|l| !src_lines.contains(l))
            .filter(|l| !l.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).all(|w| para.contains(w)))
            .collect();
        let lost_n = lost.len();
        if !same_model || lost_n > 0 {
            if bad.len() < 8 {
                eprintln!("   потеряно: {:?}", &lost[..lost.len().min(4)]);
            }
            if bad.len() < 8 {
                eprintln!("== {path}: модель {same_model}, потеряно строк {lost_n}");
            }
            bad.push(path.to_string());
        }
    }
    assert!(bad.is_empty(), "{} из {n}: {:#?}", bad.len(), &bad[..bad.len().min(30)]);
}
