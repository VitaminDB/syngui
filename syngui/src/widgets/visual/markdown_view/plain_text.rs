use super::model::{MdBlock, MdInline, MdListItem, MdTableCell, MdTaskItem};
use super::parser::parse_markdown;

pub fn linearize(blocks: &[MdBlock]) -> String {
    let mut buf = String::new();
    write_blocks(&mut buf, blocks, "");
    buf
}

pub fn linearize_markdown_source(source: &str) -> String {
    let blocks = parse_markdown(source);
    linearize(&blocks)
}

fn write_blocks(buf: &mut String, blocks: &[MdBlock], line_prefix: &str) {
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            buf.push('\n');
            push_line_prefix(buf, line_prefix);
            buf.push('\n');
        }
        write_block(buf, block, line_prefix);
    }
}

fn write_block(buf: &mut String, block: &MdBlock, line_prefix: &str) {
    match block {
        MdBlock::Heading { inlines, .. } => {
            push_line_prefix(buf, line_prefix);
            write_inlines(buf, inlines, line_prefix);
        }
        MdBlock::Paragraph { inlines } => {
            push_line_prefix(buf, line_prefix);
            write_inlines(buf, inlines, line_prefix);
        }
        MdBlock::CodeBlock { code, .. } => {
            push_line_prefix(buf, line_prefix);
            let mut first = true;
            for line in code.split('\n') {
                if !first {
                    buf.push('\n');
                    push_line_prefix(buf, line_prefix);
                }
                buf.push_str(line);
                first = false;
            }
        }
        MdBlock::BlockQuote { blocks } => {
            let nested_prefix = format!("{}> ", line_prefix);
            write_blocks(buf, blocks, &nested_prefix);
        }
        MdBlock::UnorderedList { items } => {
            write_list_items(buf, items, line_prefix, |_idx| "- ".to_string());
        }
        MdBlock::OrderedList { start, items } => {
            write_list_items(buf, items, line_prefix, |idx| {
                format!("{}. ", *start + idx as u64)
            });
        }
        MdBlock::TaskList { items } => {
            write_task_items(buf, items, line_prefix);
        }
        MdBlock::Table { headers, rows, .. } => {
            write_table(buf, headers, rows, line_prefix);
        }
        MdBlock::HorizontalRule => {
            push_line_prefix(buf, line_prefix);
        }
        MdBlock::FootnoteDefinition { label, blocks } => {
            push_line_prefix(buf, line_prefix);
            buf.push_str("[^");
            buf.push_str(label);
            buf.push_str("]: ");
            write_blocks_inline_first(buf, blocks, line_prefix);
        }
    }
}

fn write_blocks_inline_first(buf: &mut String, blocks: &[MdBlock], line_prefix: &str) {
    for (i, block) in blocks.iter().enumerate() {
        if i == 0 {
            write_block_no_first_prefix(buf, block, line_prefix);
        } else {
            buf.push('\n');
            push_line_prefix(buf, line_prefix);
            buf.push('\n');
            write_block(buf, block, line_prefix);
        }
    }
}

fn write_block_no_first_prefix(buf: &mut String, block: &MdBlock, line_prefix: &str) {
    match block {
        MdBlock::Heading { inlines, .. } | MdBlock::Paragraph { inlines } => {
            write_inlines(buf, inlines, line_prefix);
        }
        MdBlock::CodeBlock { code, .. } => {
            let mut first = true;
            for line in code.split('\n') {
                if !first {
                    buf.push('\n');
                    push_line_prefix(buf, line_prefix);
                }
                buf.push_str(line);
                first = false;
            }
        }
        _ => write_block(buf, block, line_prefix),
    }
}

fn write_list_items<F: Fn(usize) -> String>(
    buf: &mut String,
    items: &[MdListItem],
    line_prefix: &str,
    marker: F,
) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            buf.push('\n');
        }
        let mark = marker(i);
        push_line_prefix(buf, line_prefix);
        buf.push_str(&mark);
        let continuation = format!("{}{}", line_prefix, " ".repeat(mark.chars().count()));
        write_blocks_inline_first(buf, &item.blocks, &continuation);
    }
}

fn write_task_items(buf: &mut String, items: &[MdTaskItem], line_prefix: &str) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            buf.push('\n');
        }
        push_line_prefix(buf, line_prefix);
        buf.push_str(if item.checked { "[x] " } else { "[ ] " });
        write_inlines(buf, &item.inlines, line_prefix);
    }
}

fn write_table(
    buf: &mut String,
    headers: &[MdTableCell],
    rows: &[Vec<MdTableCell>],
    line_prefix: &str,
) {
    if !headers.is_empty() {
        push_line_prefix(buf, line_prefix);
        write_table_row(buf, headers, line_prefix);
    }
    for row in rows {
        if !buf.ends_with('\n') {
            buf.push('\n');
        }
        push_line_prefix(buf, line_prefix);
        write_table_row(buf, row, line_prefix);
    }
}

fn write_table_row(buf: &mut String, cells: &[MdTableCell], line_prefix: &str) {
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            buf.push('\t');
        }
        write_inlines(buf, &cell.inlines, line_prefix);
    }
}

fn write_inlines(buf: &mut String, inlines: &[MdInline], line_prefix: &str) {
    for inline in inlines {
        match inline {
            MdInline::Text(s) => buf.push_str(s),
            MdInline::Bold(children)
            | MdInline::Italic(children)
            | MdInline::Strikethrough(children) => {
                write_inlines(buf, children, line_prefix);
            }
            MdInline::Code(s) => buf.push_str(s),
            MdInline::Link { children, .. } => {
                write_inlines(buf, children, line_prefix);
            }
            MdInline::Image { .. } => {
            }
            MdInline::SoftBreak => buf.push(' '),
            MdInline::HardBreak => {
                buf.push('\n');
                push_line_prefix(buf, line_prefix);
            }
            MdInline::FootnoteRef(label) => {
                buf.push_str("[^");
                buf.push_str(label);
                buf.push(']');
            }
        }
    }
}

fn push_line_prefix(buf: &mut String, line_prefix: &str) {
    if !line_prefix.is_empty() {
        buf.push_str(line_prefix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::model::{MdAlign, MdBlock, MdInline, MdListItem, MdTableCell, MdTaskItem};

    fn text(s: &str) -> MdInline {
        MdInline::Text(s.to_string())
    }

    fn para(s: &str) -> MdBlock {
        MdBlock::Paragraph {
            inlines: vec![text(s)],
        }
    }

    #[test]
    fn empty_document_is_empty_string() {
        assert_eq!(linearize(&[]), "");
    }

    #[test]
    fn heading_strips_hash_marker() {
        let blocks = vec![MdBlock::Heading {
            level: 2,
            inlines: vec![text("Раздел")],
            id: None,
        }];
        assert_eq!(linearize(&blocks), "Раздел");
    }

    #[test]
    fn heading_then_paragraph_separated_by_blank_line() {
        let blocks = vec![
            MdBlock::Heading {
                level: 1,
                inlines: vec![text("Title")],
                id: None,
            },
            para("Body content"),
        ];
        assert_eq!(linearize(&blocks), "Title\n\nBody content");
    }

    #[test]
    fn bold_italic_code_link_strip_markup() {
        let blocks = vec![MdBlock::Paragraph {
            inlines: vec![
                MdInline::Bold(vec![text("жирный")]),
                text(" "),
                MdInline::Italic(vec![text("курсив")]),
                text(" "),
                MdInline::Code("code".to_string()),
                text(" "),
                MdInline::Link {
                    children: vec![text("link")],
                    url: "https://x".to_string(),
                },
            ],
        }];
        assert_eq!(linearize(&blocks), "жирный курсив code link");
    }

    #[test]
    fn code_block_keeps_internal_newlines() {
        let blocks = vec![MdBlock::CodeBlock {
            language: Some("rust".to_string()),
            code: "fn main() {\n    println!(\"hi\");\n}".to_string(),
        }];
        assert_eq!(
            linearize(&blocks),
            "fn main() {\n    println!(\"hi\");\n}"
        );
    }

    #[test]
    fn unordered_list_uses_dash_marker() {
        let blocks = vec![MdBlock::UnorderedList {
            items: vec![
                MdListItem {
                    blocks: vec![para("один")],
                },
                MdListItem {
                    blocks: vec![para("два")],
                },
            ],
        }];
        assert_eq!(linearize(&blocks), "- один\n- два");
    }

    #[test]
    fn ordered_list_uses_numeric_marker_from_start() {
        let blocks = vec![MdBlock::OrderedList {
            start: 3,
            items: vec![
                MdListItem {
                    blocks: vec![para("alpha")],
                },
                MdListItem {
                    blocks: vec![para("beta")],
                },
            ],
        }];
        assert_eq!(linearize(&blocks), "3. alpha\n4. beta");
    }

    #[test]
    fn task_list_marker_reflects_checked_state() {
        let blocks = vec![MdBlock::TaskList {
            items: vec![
                MdTaskItem {
                    checked: true,
                    inlines: vec![text("done")],
                },
                MdTaskItem {
                    checked: false,
                    inlines: vec![text("todo")],
                },
            ],
        }];
        assert_eq!(linearize(&blocks), "[x] done\n[ ] todo");
    }

    #[test]
    fn nested_unordered_list_indents_continuation() {
        let blocks = vec![MdBlock::UnorderedList {
            items: vec![MdListItem {
                blocks: vec![
                    para("outer"),
                    MdBlock::UnorderedList {
                        items: vec![MdListItem {
                            blocks: vec![para("inner")],
                        }],
                    },
                ],
            }],
        }];
        assert_eq!(linearize(&blocks), "- outer\n  \n  - inner");
    }

    #[test]
    fn block_quote_prefixes_each_line() {
        let blocks = vec![MdBlock::BlockQuote {
            blocks: vec![
                para("первая строка"),
                para("вторая строка"),
            ],
        }];
        assert_eq!(
            linearize(&blocks),
            "> первая строка\n> \n> вторая строка"
        );
    }

    #[test]
    fn images_are_skipped_keeping_paragraph_intact() {
        let blocks = vec![MdBlock::Paragraph {
            inlines: vec![
                text("Слово "),
                MdInline::Image {
                    alt: "alt".to_string(),
                    url: "u".to_string(),
                },
                text(" ещё слово"),
            ],
        }];
        assert_eq!(linearize(&blocks), "Слово  ещё слово");
    }

    #[test]
    fn soft_break_becomes_space_hard_break_becomes_newline() {
        let blocks = vec![MdBlock::Paragraph {
            inlines: vec![
                text("a"),
                MdInline::SoftBreak,
                text("b"),
                MdInline::HardBreak,
                text("c"),
            ],
        }];
        assert_eq!(linearize(&blocks), "a b\nc");
    }

    #[test]
    fn footnote_ref_keeps_visible_marker() {
        let blocks = vec![MdBlock::Paragraph {
            inlines: vec![
                text("текст"),
                MdInline::FootnoteRef("note".to_string()),
            ],
        }];
        assert_eq!(linearize(&blocks), "текст[^note]");
    }

    #[test]
    fn footnote_definition_renders_label_prefix() {
        let blocks = vec![MdBlock::FootnoteDefinition {
            label: "1".to_string(),
            blocks: vec![para("пояснение")],
        }];
        assert_eq!(linearize(&blocks), "[^1]: пояснение");
    }

    #[test]
    fn table_uses_tabs_and_newlines_no_pipes() {
        let blocks = vec![MdBlock::Table {
            headers: vec![
                MdTableCell {
                    inlines: vec![text("A")],
                },
                MdTableCell {
                    inlines: vec![text("B")],
                },
            ],
            rows: vec![vec![
                MdTableCell {
                    inlines: vec![text("x")],
                },
                MdTableCell {
                    inlines: vec![text("y")],
                },
            ]],
            alignments: vec![MdAlign::Left, MdAlign::Left],
        }];
        assert_eq!(linearize(&blocks), "A\tB\nx\ty");
    }

    #[test]
    fn utf8_byte_boundaries_are_preserved() {
        let blocks = vec![para("Привет, мир! 🌍")];
        let plain = linearize(&blocks);
        assert_eq!(plain, "Привет, мир! 🌍");
        assert_eq!(plain.len(), "Привет, мир! 🌍".len());
    }

    #[test]
    fn horizontal_rule_collapses_to_blank_line_between_blocks() {
        let blocks = vec![para("before"), MdBlock::HorizontalRule, para("after")];
        assert_eq!(linearize(&blocks), "before\n\n\n\nafter");
    }

    #[test]
    fn linearize_markdown_source_parses_and_linearizes() {
        let s = "# Заголовок\n\nПервый абзац.\n\n- one\n- two\n";
        let plain = linearize_markdown_source(s);
        assert_eq!(plain, "Заголовок\n\nПервый абзац.\n\n- one\n- two");
    }
}
