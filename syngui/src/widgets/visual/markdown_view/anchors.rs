use super::model::{MdBlock, MdInline, MdListItem, MdTaskItem, MdTableCell};

pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_dash = true;
    for ch in text.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn inlines_to_plain(inlines: &[MdInline]) -> String {
    let mut s = String::new();
    fn visit(inlines: &[MdInline], out: &mut String) {
        for i in inlines {
            match i {
                MdInline::Text(t) | MdInline::Code(t) => out.push_str(t),
                MdInline::Bold(c)
                | MdInline::Italic(c)
                | MdInline::Strikethrough(c)
                | MdInline::Link { children: c, .. } => visit(c, out),
                MdInline::Image { alt, .. } => out.push_str(alt),
                MdInline::SoftBreak | MdInline::HardBreak => out.push(' '),
                MdInline::FootnoteRef(_) => {}
            }
        }
    }
    visit(inlines, &mut s);
    s
}

pub fn assign_heading_ids(blocks: &mut [MdBlock]) {
    let mut seen: hashbrown::HashMap<String, u32> = hashbrown::HashMap::new();
    walk_assign(blocks, &mut seen);
}

fn walk_assign(blocks: &mut [MdBlock], seen: &mut hashbrown::HashMap<String, u32>) {
    for b in blocks {
        match b {
            MdBlock::Heading { inlines, id, .. } => {
                if id.is_some() {
                    continue;
                }
                let base = slugify(&inlines_to_plain(inlines));
                if base.is_empty() {
                    continue;
                }
                let counter = seen.entry(base.clone()).or_insert(0);
                *counter += 1;
                let final_id = if *counter == 1 {
                    base
                } else {
                    format!("{base}-{counter}")
                };
                *id = Some(final_id);
            }
            MdBlock::BlockQuote { blocks } => walk_assign(blocks, seen),
            MdBlock::UnorderedList { items } | MdBlock::OrderedList { items, .. } => {
                for item in items.iter_mut() {
                    walk_assign(&mut item.blocks, seen);
                }
            }
            MdBlock::FootnoteDefinition { blocks, .. } => walk_assign(blocks, seen),
            _ => {}
        }
    }
}

pub fn find_autolinks(text: &str) -> Vec<(usize, usize, String)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut last_end: usize = 0;

    for (i, _ch) in text.char_indices() {
        if i < last_end {
            continue;
        }
        let rest = &text[i..];
        let scheme_len = if rest.starts_with("https://") {
            8
        } else if rest.starts_with("http://") {
            7
        } else {
            continue;
        };
        if i > 0 {
            if let Some((_, prev_ch)) = text[..i].char_indices().next_back() {
                if prev_ch.is_alphanumeric() {
                    continue;
                }
            }
        }
        // на не-ASCII или пробеле, что и обеспечивает UTF-8 safety.
        let start = i;
        let mut j = i + scheme_len;
        while j < bytes.len() && is_url_byte(bytes[j]) {
            j += 1;
        }
        while j > start + scheme_len {
            let last = bytes[j - 1];
            if matches!(last, b'.' | b',' | b';' | b':' | b'!' | b'?' | b')' | b']' | b'>' | b'\'' | b'"') {
                j -= 1;
            } else {
                break;
            }
        }
        if j > start + scheme_len {
            let url = text[start..j].to_string();
            out.push((start, j, url));
            last_end = j;
        }
    }
    out
}

fn is_url_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'.' | b'_' | b'~' | b':' | b'/' | b'?' | b'#' | b'[' | b']'
                | b'@' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+'
                | b',' | b';' | b'=' | b'%'
        )
}

pub fn apply_autolinks_to_blocks(blocks: &mut Vec<MdBlock>) {
    for b in blocks.iter_mut() {
        apply_to_block(b);
    }
}

fn apply_to_block(block: &mut MdBlock) {
    match block {
        MdBlock::Paragraph { inlines } | MdBlock::Heading { inlines, .. } => {
            *inlines = expand_inlines(std::mem::take(inlines));
        }
        MdBlock::BlockQuote { blocks } => apply_autolinks_to_blocks(blocks),
        MdBlock::UnorderedList { items } | MdBlock::OrderedList { items, .. } => {
            for it in items.iter_mut() {
                apply_to_list_item(it);
            }
        }
        MdBlock::TaskList { items } => {
            for it in items.iter_mut() {
                apply_to_task_item(it);
            }
        }
        MdBlock::Table { headers, rows, .. } => {
            for c in headers.iter_mut() {
                apply_to_cell(c);
            }
            for row in rows.iter_mut() {
                for c in row.iter_mut() {
                    apply_to_cell(c);
                }
            }
        }
        MdBlock::FootnoteDefinition { blocks, .. } => apply_autolinks_to_blocks(blocks),
        MdBlock::CodeBlock { .. } | MdBlock::HorizontalRule => {}
    }
}

fn apply_to_list_item(it: &mut MdListItem) {
    apply_autolinks_to_blocks(&mut it.blocks);
}

fn apply_to_task_item(it: &mut MdTaskItem) {
    it.inlines = expand_inlines(std::mem::take(&mut it.inlines));
}

fn apply_to_cell(c: &mut MdTableCell) {
    c.inlines = expand_inlines(std::mem::take(&mut c.inlines));
}

fn expand_inlines(input: Vec<MdInline>) -> Vec<MdInline> {
    let mut out = Vec::with_capacity(input.len());
    for i in input {
        match i {
            MdInline::Text(t) => {
                let links = find_autolinks(&t);
                if links.is_empty() {
                    out.push(MdInline::Text(t));
                } else {
                    let mut cursor = 0;
                    for (s, e, url) in links {
                        if s > cursor {
                            out.push(MdInline::Text(t[cursor..s].to_string()));
                        }
                        out.push(MdInline::Link {
                            children: vec![MdInline::Text(url.clone())],
                            url,
                        });
                        cursor = e;
                    }
                    if cursor < t.len() {
                        out.push(MdInline::Text(t[cursor..].to_string()));
                    }
                }
            }
            MdInline::Bold(c) => out.push(MdInline::Bold(expand_inlines(c))),
            MdInline::Italic(c) => out.push(MdInline::Italic(expand_inlines(c))),
            MdInline::Strikethrough(c) => out.push(MdInline::Strikethrough(expand_inlines(c))),
            MdInline::Link { children, url } => out.push(MdInline::Link {
                children: expand_inlines(children),
                url,
            }),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("  Multiple   spaces  "), "multiple-spaces");
        assert_eq!(slugify("Code blocks: rust + json!"), "code-blocks-rust-json");
        assert_eq!(slugify(""), "");
        assert_eq!(slugify("---"), "");
        assert_eq!(slugify("SYNGUI v0.1"), "syngui-v0-1");
    }

    #[test]
    fn autolinks_single() {
        let r = find_autolinks("see https://example.com for details");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].2, "https://example.com");
    }

    #[test]
    fn autolinks_punct_trim() {
        let r = find_autolinks("(see https://example.com).");
        assert_eq!(r[0].2, "https://example.com");
    }

    #[test]
    fn autolinks_inside_word_skipped() {
        let r = find_autolinks("blahhttps://x.io");
        assert!(r.is_empty());
    }

    #[test]
    fn autolinks_two() {
        let r = find_autolinks("a http://a.io and https://b.io done");
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn autolinks_multibyte_safe() {
        let r = find_autolinks("works[^1] — see definitions at the bottom.");
        assert!(r.is_empty());
        let r2 = find_autolinks("foo — https://example.com — bar");
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].2, "https://example.com");
    }
}
