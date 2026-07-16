use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, Alignment};

use super::anchors::{apply_autolinks_to_blocks, assign_heading_ids};
use super::model::*;

pub fn parse_markdown(source: &str) -> Vec<MdBlock> {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES;
    let parser = Parser::new_ext(source, opts);
    let mut ctx = ParseContext::new();
    for event in parser {
        ctx.process(event);
    }
    let mut blocks = ctx.finish();
    apply_autolinks_to_blocks(&mut blocks);
    assign_heading_ids(&mut blocks);
    blocks
}

struct ParseContext {
    stack: Vec<Frame>,
    root_blocks: Vec<MdBlock>,
}

enum Frame {
    BlockQuote { blocks: Vec<MdBlock> },
    List { ordered: bool, start: u64, items: Vec<MdListItem>, task_items: Vec<MdTaskItem>, is_task: bool },
    ListItem { blocks: Vec<MdBlock> },
    Heading { level: u8, inlines: Vec<MdInline> },
    Paragraph { inlines: Vec<MdInline> },
    Emphasis { inlines: Vec<MdInline> },
    Strong { inlines: Vec<MdInline> },
    Strikethrough { inlines: Vec<MdInline> },
    Link { url: String, inlines: Vec<MdInline> },
    Table { alignments: Vec<MdAlign>, headers: Vec<MdTableCell>, rows: Vec<Vec<MdTableCell>>, in_head: bool },
    TableRow { cells: Vec<MdTableCell> },
    TableCell { inlines: Vec<MdInline> },
    TaskListItem { checked: bool, inlines: Vec<MdInline> },
    FootnoteDefinition { label: String, blocks: Vec<MdBlock> },
}

impl ParseContext {
    fn new() -> Self {
        Self { stack: Vec::new(), root_blocks: Vec::new() }
    }

    fn process(&mut self, event: Event) {
        match event {
            Event::Start(Tag::BlockQuote(_)) => {
                self.stack.push(Frame::BlockQuote { blocks: Vec::new() });
            }
            Event::Start(Tag::List(first)) => {
                let (ordered, start) = match first {
                    Some(n) => (true, n),
                    None => (false, 1),
                };
                self.stack.push(Frame::List {
                    ordered, start,
                    items: Vec::new(),
                    task_items: Vec::new(),
                    is_task: false,
                });
            }
            Event::Start(Tag::Item) => {
                self.stack.push(Frame::ListItem { blocks: Vec::new() });
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl = match level {
                    pulldown_cmark::HeadingLevel::H1 => 1,
                    pulldown_cmark::HeadingLevel::H2 => 2,
                    pulldown_cmark::HeadingLevel::H3 => 3,
                    pulldown_cmark::HeadingLevel::H4 => 4,
                    pulldown_cmark::HeadingLevel::H5 => 5,
                    pulldown_cmark::HeadingLevel::H6 => 6,
                };
                self.stack.push(Frame::Heading { level: lvl, inlines: Vec::new() });
            }
            Event::Start(Tag::Paragraph) => {
                self.stack.push(Frame::Paragraph { inlines: Vec::new() });
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(s) => {
                        let s = s.to_string();
                        if s.is_empty() { None } else { Some(s) }
                    }
                    pulldown_cmark::CodeBlockKind::Indented => None,
                };
                self.stack.push(Frame::Paragraph { inlines: vec![MdInline::Text(lang.unwrap_or_default())] });
            }

            Event::Start(Tag::Emphasis) => {
                self.stack.push(Frame::Emphasis { inlines: Vec::new() });
            }
            Event::Start(Tag::Strong) => {
                self.stack.push(Frame::Strong { inlines: Vec::new() });
            }
            Event::Start(Tag::Strikethrough) => {
                self.stack.push(Frame::Strikethrough { inlines: Vec::new() });
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.stack.push(Frame::Link { url: dest_url.to_string(), inlines: Vec::new() });
            }
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                self.stack.push(Frame::Link { url: dest_url.to_string(), inlines: vec![MdInline::Text(title.to_string())] });
            }

            Event::Start(Tag::Table(aligns)) => {
                let alignments = aligns.iter().map(|a| match a {
                    Alignment::None | Alignment::Left => MdAlign::Left,
                    Alignment::Center => MdAlign::Center,
                    Alignment::Right => MdAlign::Right,
                }).collect();
                self.stack.push(Frame::Table {
                    alignments, headers: Vec::new(), rows: Vec::new(), in_head: false,
                });
            }
            Event::Start(Tag::TableHead) => {
                if let Some(Frame::Table { in_head, .. }) = self.stack.last_mut() {
                    *in_head = true;
                }
                self.stack.push(Frame::TableRow { cells: Vec::new() });
            }
            Event::Start(Tag::TableRow) => {
                self.stack.push(Frame::TableRow { cells: Vec::new() });
            }
            Event::Start(Tag::TableCell) => {
                self.stack.push(Frame::TableCell { inlines: Vec::new() });
            }

            Event::Start(Tag::FootnoteDefinition(label)) => {
                self.stack.push(Frame::FootnoteDefinition {
                    label: label.to_string(),
                    blocks: Vec::new(),
                });
            }
            Event::FootnoteReference(label) => {
                self.push_inline(MdInline::FootnoteRef(label.to_string()));
            }

            Event::Text(text) => {
                self.push_inline(MdInline::Text(text.to_string()));
            }
            Event::Code(code) => {
                self.push_inline(MdInline::Code(code.to_string()));
            }
            Event::SoftBreak => {
                self.push_inline(MdInline::SoftBreak);
            }
            Event::HardBreak => {
                self.push_inline(MdInline::HardBreak);
            }

            Event::TaskListMarker(checked) => {
                if let Some(Frame::ListItem { blocks }) = self.stack.last_mut() {
                    let inlines = Vec::new();
                    let blocks = std::mem::take(blocks);
                    *self.stack.last_mut().unwrap() = Frame::TaskListItem { checked, inlines };
                    for frame in self.stack.iter_mut().rev().skip(1) {
                        if let Frame::List { is_task, .. } = frame {
                            *is_task = true;
                            break;
                        }
                    }
                    let _ = blocks;
                }
            }

            Event::Rule => {
                self.emit_block(MdBlock::HorizontalRule);
            }

            Event::End(tag_end) => self.process_end(tag_end),

            _ => {}
        }
    }

    fn process_end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::BlockQuote(_) => {
                if let Some(Frame::BlockQuote { blocks }) = self.stack.pop() {
                    self.emit_block(MdBlock::BlockQuote { blocks });
                }
            }
            TagEnd::List(_) => {
                if let Some(Frame::List { ordered, start, items, task_items, is_task }) = self.stack.pop() {
                    if is_task {
                        self.emit_block(MdBlock::TaskList { items: task_items });
                    } else if ordered {
                        self.emit_block(MdBlock::OrderedList { start, items });
                    } else {
                        self.emit_block(MdBlock::UnorderedList { items });
                    }
                }
            }
            TagEnd::Item => {
                match self.stack.pop() {
                    Some(Frame::ListItem { blocks }) => {
                        if let Some(Frame::List { items, .. }) = self.stack.last_mut() {
                            items.push(MdListItem { blocks });
                        }
                    }
                    Some(Frame::TaskListItem { checked, inlines }) => {
                        if let Some(Frame::List { task_items, .. }) = self.stack.last_mut() {
                            task_items.push(MdTaskItem { checked, inlines });
                        }
                    }
                    _ => {}
                }
            }
            TagEnd::Heading(_) => {
                if let Some(Frame::Heading { level, inlines }) = self.stack.pop() {
                    self.emit_block(MdBlock::Heading { level, inlines, id: None });
                }
            }
            TagEnd::Paragraph => {
                if let Some(Frame::Paragraph { inlines }) = self.stack.pop() {
                    self.emit_block(MdBlock::Paragraph { inlines });
                }
            }
            TagEnd::CodeBlock => {
                if let Some(Frame::Paragraph { inlines }) = self.stack.pop() {
                    let lang = if let Some(MdInline::Text(s)) = inlines.first() {
                        if s.is_empty() { None } else { Some(s.clone()) }
                    } else {
                        None
                    };
                    let code: String = inlines.iter().skip(1).filter_map(|i| {
                        if let MdInline::Text(t) = i { Some(t.as_str()) } else { None }
                    }).collect();
                    self.emit_block(MdBlock::CodeBlock { language: lang, code });
                }
            }
            TagEnd::Emphasis => {
                if let Some(Frame::Emphasis { inlines }) = self.stack.pop() {
                    self.push_inline(MdInline::Italic(inlines));
                }
            }
            TagEnd::Strong => {
                if let Some(Frame::Strong { inlines }) = self.stack.pop() {
                    self.push_inline(MdInline::Bold(inlines));
                }
            }
            TagEnd::Strikethrough => {
                if let Some(Frame::Strikethrough { inlines }) = self.stack.pop() {
                    self.push_inline(MdInline::Strikethrough(inlines));
                }
            }
            TagEnd::Link => {
                if let Some(Frame::Link { url, inlines }) = self.stack.pop() {
                    self.push_inline(MdInline::Link { children: inlines, url });
                }
            }
            TagEnd::Image => {
                if let Some(Frame::Link { url, inlines }) = self.stack.pop() {
                    let alt = inlines_to_plain_text(&inlines);
                    self.push_inline(MdInline::Image { alt, url });
                }
            }
            TagEnd::Table => {
                if let Some(Frame::Table { alignments, headers, rows, .. }) = self.stack.pop() {
                    self.emit_block(MdBlock::Table { headers, rows, alignments });
                }
            }
            TagEnd::TableHead => {
                if let Some(Frame::TableRow { cells }) = self.stack.pop() {
                    if let Some(Frame::Table { headers, in_head, .. }) = self.stack.last_mut() {
                        *headers = cells;
                        *in_head = false;
                    }
                }
            }
            TagEnd::TableRow => {
                if let Some(Frame::TableRow { cells }) = self.stack.pop() {
                    if let Some(Frame::Table { rows, .. }) = self.stack.last_mut() {
                        rows.push(cells);
                    }
                }
            }
            TagEnd::TableCell => {
                if let Some(Frame::TableCell { inlines }) = self.stack.pop() {
                    if let Some(Frame::TableRow { cells }) = self.stack.last_mut() {
                        cells.push(MdTableCell { inlines });
                    }
                }
            }
            TagEnd::FootnoteDefinition => {
                if let Some(Frame::FootnoteDefinition { label, blocks }) = self.stack.pop() {
                    self.emit_block(MdBlock::FootnoteDefinition { label, blocks });
                }
            }
            _ => {}
        }
    }

    fn push_inline(&mut self, inline: MdInline) {
        for frame in self.stack.iter_mut().rev() {
            match frame {
                Frame::Heading { inlines, .. }
                | Frame::Paragraph { inlines }
                | Frame::Emphasis { inlines }
                | Frame::Strong { inlines }
                | Frame::Strikethrough { inlines }
                | Frame::Link { inlines, .. }
                | Frame::TaskListItem { inlines, .. }
                | Frame::TableCell { inlines } => {
                    inlines.push(inline);
                    return;
                }
                Frame::ListItem { blocks } | Frame::BlockQuote { blocks } => {
                    if let Some(MdBlock::Paragraph { inlines }) = blocks.last_mut() {
                        inlines.push(inline);
                    } else {
                        blocks.push(MdBlock::Paragraph { inlines: vec![inline] });
                    }
                    return;
                }
                _ => {}
            }
        }
        if let Some(MdBlock::Paragraph { inlines }) = self.root_blocks.last_mut() {
            inlines.push(inline);
        } else {
            self.root_blocks.push(MdBlock::Paragraph { inlines: vec![inline] });
        }
    }

    fn emit_block(&mut self, block: MdBlock) {
        for frame in self.stack.iter_mut().rev() {
            match frame {
                Frame::BlockQuote { blocks }
                | Frame::ListItem { blocks }
                | Frame::FootnoteDefinition { blocks, .. } => {
                    blocks.push(block);
                    return;
                }
                _ => {}
            }
        }
        self.root_blocks.push(block);
    }

    fn finish(self) -> Vec<MdBlock> {
        self.root_blocks
    }
}

fn inlines_to_plain_text(inlines: &[MdInline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            MdInline::Text(t) => out.push_str(t),
            MdInline::Bold(children) | MdInline::Italic(children)
            | MdInline::Strikethrough(children) | MdInline::Link { children, .. } => {
                out.push_str(&inlines_to_plain_text(children));
            }
            MdInline::Code(c) => out.push_str(c),
            MdInline::SoftBreak => out.push(' '),
            MdInline::HardBreak => out.push('\n'),
            MdInline::Image { alt, .. } => out.push_str(alt),
            MdInline::FootnoteRef(_) => {}
        }
    }
    out
}
