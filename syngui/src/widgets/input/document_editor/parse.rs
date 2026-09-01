//! Markdown → `DocModel`.
//!
//! Двухфазная схема: сначала pulldown-cmark (те же опции, что у MarkdownView,
//! минус footnotes) собирается во внутреннюю модель, затем линейные
//! пост-проходы добавляют «наш» синтаксис: `[[wiki-ссылки]]`, `![[врезки]]`,
//! callout/toggle из blockquote, инлайн-атрибуты `{k=v}`, медиа-блоки.
//!
//! Осознанные нормализации (см. docs тесты round-trip):
//! - инлайн-картинка посреди параграфа раскалывает его на
//!   Paragraph + Media + Paragraph (в Notion-модели медиа — всегда блок);
//! - soft break сворачивается в пробел, hard break живёт как `\n` в ране;
//! - нумерация Numbered-блоков всегда последовательна от start списка;
//! - сырой HTML не интерпретируется и сохраняется как текстовый параграф.

use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};

use super::attrs::{split_leading_attrs, split_trailing_attrs};
use super::model::*;

pub fn parse_document(source: &str) -> DocModel {
    let opts =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(source, opts);
    let mut b = Builder::new();
    for event in parser {
        b.process(event);
    }
    let (mut blocks, next_id) = b.finish();
    post_process(&mut blocks);
    DocModel::with_blocks(blocks, next_id)
}

// ─── Фаза 1: сборка событий pulldown ────────────────────────────────────────

struct Builder {
    next_id: u64,
    stack: Vec<Frame>,
    root: Vec<DocBlock>,
    style: StyleState,
    /// Только что эмитирован Media-блок: следующий Text может начинаться
    /// с его блока атрибутов `{...}`.
    media_pending: bool,
}

#[derive(Default)]
struct StyleState {
    bold: u32,
    italic: u32,
    strike: u32,
    links: Vec<LinkTarget>,
}

impl StyleState {
    fn current(&self) -> InlineStyle {
        InlineStyle {
            bold: self.bold > 0,
            italic: self.italic > 0,
            strike: self.strike > 0,
            code: false,
            link: self.links.last().cloned(),
        }
    }
}

enum Frame {
    Para { parts: Vec<DocBlock>, text: InlineText },
    Heading { level: u8, text: InlineText },
    Code { lang: Option<String>, code: String },
    Quote { blocks: Vec<DocBlock> },
    List { ordered: bool, next_number: u64, items: Vec<DocBlock> },
    Item { text: InlineText, blocks: Vec<DocBlock>, todo: Option<bool>, ordered: bool, number: u64 },
    Table { aligns: Vec<DocAlign>, headers: Vec<InlineText>, rows: Vec<Vec<InlineText>>, in_head: bool },
    Row { cells: Vec<InlineText> },
    Cell { text: InlineText },
    /// Захват alt-текста картинки: пока фрейм на вершине, инлайн-события
    /// уходят в `alt`.
    Image { url: String, alt: String },
    Html { text: String },
}

impl Builder {
    fn new() -> Self {
        Self {
            next_id: 0,
            stack: Vec::new(),
            root: Vec::new(),
            style: StyleState::default(),
            media_pending: false,
        }
    }

    fn alloc(&mut self) -> BlockId {
        let id = BlockId(self.next_id);
        self.next_id += 1;
        id
    }

    fn block(&mut self, kind: BlockKind) -> DocBlock {
        DocBlock::new(self.alloc(), kind)
    }

    fn process(&mut self, event: Event) {
        // Любое событие, кроме Text, снимает ожидание атрибутов медиа.
        let was_media_pending = self.media_pending;
        if !matches!(event, Event::Text(_)) {
            self.media_pending = false;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),

            Event::Text(text) => {
                let mut text = text.to_string();
                if was_media_pending {
                    text = self.try_attach_media_attrs(text);
                    self.media_pending = false;
                }
                self.push_text(&text, false);
            }
            Event::Code(code) => self.push_text(&code, true),
            Event::SoftBreak => self.push_text(" ", false),
            Event::HardBreak => self.push_text("\n", false),

            Event::Html(html) | Event::InlineHtml(html) => {
                // Сырой HTML не интерпретируем — сохраняем как текст.
                if let Some(Frame::Html { text }) = self.stack.last_mut() {
                    text.push_str(&html);
                } else {
                    self.push_text(&html, false);
                }
            }

            Event::Rule => {
                let b = self.block(BlockKind::Divider);
                self.emit_block(b);
            }
            Event::TaskListMarker(checked) => {
                for frame in self.stack.iter_mut().rev() {
                    if let Frame::Item { todo, .. } = frame {
                        *todo = Some(checked);
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                self.stack.push(Frame::Para { parts: Vec::new(), text: InlineText::default() });
            }
            Tag::Heading { level, .. } => {
                self.stack.push(Frame::Heading { level: level as u8, text: InlineText::default() });
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(s) if !s.is_empty() => {
                        Some(s.to_string())
                    }
                    _ => None,
                };
                self.stack.push(Frame::Code { lang, code: String::new() });
            }
            Tag::BlockQuote(_) => {
                self.stack.push(Frame::Quote { blocks: Vec::new() });
            }
            Tag::List(first) => {
                let (ordered, start) = match first {
                    Some(n) => (true, n),
                    None => (false, 1),
                };
                self.stack.push(Frame::List { ordered, next_number: start, items: Vec::new() });
            }
            Tag::Item => {
                let (ordered, number) = match self.stack.last_mut() {
                    Some(Frame::List { ordered, next_number, .. }) => {
                        let n = *next_number;
                        *next_number += 1;
                        (*ordered, n)
                    }
                    _ => (false, 1),
                };
                self.stack.push(Frame::Item {
                    text: InlineText::default(),
                    blocks: Vec::new(),
                    todo: None,
                    ordered,
                    number,
                });
            }
            Tag::Table(aligns) => {
                let aligns = aligns
                    .iter()
                    .map(|a| match a {
                        Alignment::None | Alignment::Left => DocAlign::Left,
                        Alignment::Center => DocAlign::Center,
                        Alignment::Right => DocAlign::Right,
                    })
                    .collect();
                self.stack.push(Frame::Table {
                    aligns,
                    headers: Vec::new(),
                    rows: Vec::new(),
                    in_head: false,
                });
            }
            Tag::TableHead => {
                if let Some(Frame::Table { in_head, .. }) = self.stack.last_mut() {
                    *in_head = true;
                }
                self.stack.push(Frame::Row { cells: Vec::new() });
            }
            Tag::TableRow => self.stack.push(Frame::Row { cells: Vec::new() }),
            Tag::TableCell => self.stack.push(Frame::Cell { text: InlineText::default() }),

            Tag::Emphasis => self.style.italic += 1,
            Tag::Strong => self.style.bold += 1,
            Tag::Strikethrough => self.style.strike += 1,
            Tag::Link { dest_url, .. } => {
                self.style.links.push(LinkTarget::Url(dest_url.to_string()));
            }
            Tag::Image { dest_url, .. } => {
                self.stack.push(Frame::Image { url: dest_url.to_string(), alt: String::new() });
            }
            Tag::HtmlBlock => self.stack.push(Frame::Html { text: String::new() }),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if let Some(Frame::Para { mut parts, mut text }) = self.stack.pop() {
                    text.normalize();
                    if !parts.is_empty() {
                        // Хвост расколотого параграфа: пробел после картинки
                        // не несёт смысла и ломает идемпотентность.
                        trim_start_ws(&mut text);
                    }
                    if !text.is_empty() {
                        let b = self.block(BlockKind::Paragraph(text));
                        parts.push(b);
                    }
                    for part in parts {
                        self.emit_block(part);
                    }
                }
            }
            TagEnd::Heading(_) => {
                if let Some(Frame::Heading { level, mut text }) = self.stack.pop() {
                    text.normalize();
                    // Хвостовые атрибуты заголовка: `## Текст {icon=🚀}`.
                    let mut attrs = None;
                    if let Some(last) = text.0.last_mut() {
                        let (rest, found) = split_trailing_attrs(&last.text);
                        if found.is_some() {
                            last.text = rest;
                            attrs = found;
                        }
                    }
                    text.normalize();
                    let mut b = self.block(BlockKind::Heading { level, text });
                    if let Some(a) = attrs {
                        b.attrs = a;
                    }
                    self.emit_block(b);
                }
            }
            TagEnd::CodeBlock => {
                if let Some(Frame::Code { lang, mut code }) = self.stack.pop() {
                    if code.ends_with('\n') {
                        code.pop();
                    }
                    let b = self.block(BlockKind::CodeBlock { language: lang, code });
                    self.emit_block(b);
                }
            }
            TagEnd::BlockQuote(_) => {
                if let Some(Frame::Quote { blocks }) = self.stack.pop() {
                    let b = self.finish_quote(blocks);
                    self.emit_block(b);
                }
            }
            TagEnd::List(_) => {
                if let Some(Frame::List { items, .. }) = self.stack.pop() {
                    for item in items {
                        self.emit_block(item);
                    }
                }
            }
            TagEnd::Item => {
                if let Some(Frame::Item { mut text, mut blocks, todo, ordered, number }) =
                    self.stack.pop()
                {
                    text.normalize();
                    // Loose-список: текст пункта пришёл параграфом.
                    if text.is_empty() && !blocks.is_empty() {
                        if let BlockKind::Paragraph(_) = &blocks[0].kind {
                            if blocks[0].attrs.is_empty() {
                                if let BlockKind::Paragraph(t) = blocks.remove(0).kind {
                                    text = t;
                                }
                            }
                        }
                    }
                    let kind = match todo {
                        Some(checked) => BlockKind::Todo { checked, text, children: blocks },
                        None if ordered => BlockKind::Numbered { number, text, children: blocks },
                        None => BlockKind::Bullet { text, children: blocks },
                    };
                    let b = self.block(kind);
                    if let Some(Frame::List { items, .. }) = self.stack.last_mut() {
                        items.push(b);
                    } else {
                        self.emit_block(b);
                    }
                }
            }
            TagEnd::Table => {
                if let Some(Frame::Table { aligns, headers, rows, .. }) = self.stack.pop() {
                    let b = self.block(BlockKind::Table { headers, rows, aligns });
                    self.emit_block(b);
                }
            }
            TagEnd::TableHead => {
                if let Some(Frame::Row { cells }) = self.stack.pop() {
                    if let Some(Frame::Table { headers, in_head, .. }) = self.stack.last_mut() {
                        *headers = cells;
                        *in_head = false;
                    }
                }
            }
            TagEnd::TableRow => {
                if let Some(Frame::Row { cells }) = self.stack.pop() {
                    if let Some(Frame::Table { rows, .. }) = self.stack.last_mut() {
                        rows.push(cells);
                    }
                }
            }
            TagEnd::TableCell => {
                if let Some(Frame::Cell { mut text }) = self.stack.pop() {
                    text.normalize();
                    if let Some(Frame::Row { cells }) = self.stack.last_mut() {
                        cells.push(text);
                    }
                }
            }

            TagEnd::Emphasis => self.style.italic = self.style.italic.saturating_sub(1),
            TagEnd::Strong => self.style.bold = self.style.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.style.strike = self.style.strike.saturating_sub(1),
            TagEnd::Link => {
                self.style.links.pop();
            }
            TagEnd::Image => {
                if let Some(Frame::Image { url, alt }) = self.stack.pop() {
                    self.emit_media(url, alt);
                }
            }
            TagEnd::HtmlBlock => {
                if let Some(Frame::Html { text }) = self.stack.pop() {
                    let trimmed = text.trim_end_matches('\n');
                    if !trimmed.is_empty() {
                        let b =
                            self.block(BlockKind::Paragraph(InlineText::plain(trimmed)));
                        self.emit_block(b);
                    }
                }
            }
            _ => {}
        }
    }

    /// Инлайн-текст уходит в ближайший принимающий фрейм.
    fn push_text(&mut self, text: &str, code: bool) {
        let mut style = self.style.current();
        style.code = code;
        for frame in self.stack.iter_mut().rev() {
            match frame {
                Frame::Image { alt, .. } => {
                    alt.push_str(text);
                    return;
                }
                Frame::Para { text: t, .. }
                | Frame::Heading { text: t, .. }
                | Frame::Cell { text: t }
                | Frame::Item { text: t, .. } => {
                    t.push_run(text, style);
                    return;
                }
                Frame::Code { code: c, .. } => {
                    c.push_str(text);
                    return;
                }
                _ => {}
            }
        }
        // Голый текст на корне (не должен случаться у pulldown) — в параграф.
        let b = self.block(BlockKind::Paragraph(InlineText::plain(text)));
        self.root.push(b);
    }

    /// Медиа — всегда блок: параграф раскалывается, картинка в пункте списка
    /// уходит в его детей, в заголовке/ячейке от неё остаётся alt-текст.
    fn emit_media(&mut self, url: String, alt: String) {
        enum Slot {
            Para(usize),
            Item(usize),
            TextOnly(usize),
        }
        let mut slot = None;
        for (i, frame) in self.stack.iter().enumerate().rev() {
            match frame {
                Frame::Para { .. } => {
                    slot = Some(Slot::Para(i));
                    break;
                }
                Frame::Item { .. } => {
                    slot = Some(Slot::Item(i));
                    break;
                }
                Frame::Heading { .. } | Frame::Cell { .. } => {
                    slot = Some(Slot::TextOnly(i));
                    break;
                }
                _ => {}
            }
        }
        let media = MediaKind::detect(&url, &Attrs::default());
        match slot {
            Some(Slot::Para(i)) => {
                // Откалываем накопленный текст параграфа отдельным блоком.
                let pending_text = match &mut self.stack[i] {
                    Frame::Para { text, .. } => {
                        text.normalize();
                        trim_end_ws(text);
                        if text.is_empty() { None } else { Some(std::mem::take(text)) }
                    }
                    _ => None,
                };
                let para = pending_text.map(|t| self.block(BlockKind::Paragraph(t)));
                let media_block = self.block(BlockKind::Media { media, url, alt });
                if let Frame::Para { parts, .. } = &mut self.stack[i] {
                    if let Some(p) = para {
                        parts.push(p);
                    }
                    parts.push(media_block);
                }
                self.media_pending = true;
            }
            Some(Slot::Item(i)) => {
                let b = self.block(BlockKind::Media { media, url, alt });
                if let Frame::Item { blocks, .. } = &mut self.stack[i] {
                    blocks.push(b);
                }
                self.media_pending = true;
            }
            Some(Slot::TextOnly(i)) => {
                let style = self.style.current();
                match &mut self.stack[i] {
                    Frame::Heading { text, .. } | Frame::Cell { text } => {
                        text.push_run(alt, style);
                    }
                    _ => {}
                }
            }
            None => {
                let b = self.block(BlockKind::Media { media, url, alt });
                self.root.push(b);
                self.media_pending = true;
            }
        }
    }

    /// Текст сразу после Media-блока: пробуем снять с него `{attrs}`.
    fn try_attach_media_attrs(&mut self, text: String) -> String {
        let (attrs, rest) = split_leading_attrs(&text);
        let Some(attrs) = attrs else { return text };
        // Ищем последний эмитированный Media-блок.
        let slot: Option<&mut DocBlock> = {
            let mut found = None;
            for frame in self.stack.iter_mut().rev() {
                match frame {
                    Frame::Para { parts, .. } => {
                        found = parts.last_mut();
                        break;
                    }
                    Frame::Item { blocks, .. } => {
                        found = blocks.last_mut();
                        break;
                    }
                    _ => {}
                }
            }
            if found.is_none() {
                found = self.root.last_mut();
            }
            found
        };
        if let Some(block) = slot {
            if let BlockKind::Media { media, url, .. } = &mut block.kind {
                *media = MediaKind::detect(url, &attrs);
                block.attrs = attrs;
                return rest;
            }
        }
        text
    }

    /// BlockQuote → Quote / Callout / Toggle.
    fn finish_quote(&mut self, mut blocks: Vec<DocBlock>) -> DocBlock {
        let marker = parse_callout_marker(&mut blocks);
        match marker {
            Some((kind, mut attrs, title)) if kind == "toggle" => {
                let collapsed = !attrs.flag("open");
                attrs.remove("open");
                let mut b = self.block(BlockKind::Toggle {
                    summary: title,
                    children: blocks,
                    collapsed,
                });
                b.attrs = attrs;
                b
            }
            Some((kind, attrs, title)) => {
                let mut b = self.block(BlockKind::Callout { kind, title, children: blocks });
                b.attrs = attrs;
                b
            }
            None => self.block(BlockKind::Quote(blocks)),
        }
    }

    /// Блок закончен — кладём его в ближайший контейнер или в корень.
    fn emit_block(&mut self, block: DocBlock) {
        for frame in self.stack.iter_mut().rev() {
            match frame {
                Frame::Quote { blocks } | Frame::Item { blocks, .. } => {
                    blocks.push(block);
                    return;
                }
                _ => {}
            }
        }
        self.root.push(block);
    }

    fn finish(mut self) -> (Vec<DocBlock>, u64) {
        // pulldown всегда балансирует теги; на всякий случай доносим хвосты.
        while let Some(frame) = self.stack.pop() {
            if let Frame::Quote { blocks } | Frame::Item { blocks, .. } = frame {
                for b in blocks {
                    self.root.push(b);
                }
            }
        }
        (self.root, self.next_id)
    }
}

/// Срезает пробелы в начале первого рана.
fn trim_start_ws(text: &mut InlineText) {
    if let Some(first) = text.0.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    text.normalize();
}

/// Срезает пробелы в конце последнего рана.
fn trim_end_ws(text: &mut InlineText) {
    if let Some(last) = text.0.last_mut() {
        last.text = last.text.trim_end().to_string();
    }
    text.normalize();
}

/// Снимает маркер `[!kind]{attrs} Заголовок` с первого параграфа цитаты.
fn parse_callout_marker(blocks: &mut Vec<DocBlock>) -> Option<(String, Attrs, InlineText)> {
    let first = blocks.first_mut()?;
    let BlockKind::Paragraph(text) = &mut first.kind else { return None };
    let first_run = text.0.first()?;
    if !first_run.style.plain() {
        return None;
    }
    let run_text = first_run.text.clone();
    let inner = run_text.strip_prefix("[!")?;
    let close = inner.find(']')?;
    let kind: String = inner[..close].to_string();
    if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let after = &inner[close + 1..];
    let (attrs, after) = split_leading_attrs(after);
    // Забираем заголовок: остаток первого рана + все прочие раны параграфа.
    let mut title = InlineText::default();
    let after = after.strip_prefix(' ').unwrap_or(&after).to_string();
    if !after.is_empty() {
        let style = text.0[0].style.clone();
        title.push_run(after, style);
    }
    for run in text.0.iter().skip(1) {
        title.push_run(run.text.clone(), run.style.clone());
    }
    title.normalize();
    blocks.remove(0);
    Some((kind.to_ascii_lowercase(), attrs.unwrap_or_default(), title))
}

// ─── Фаза 2: пост-проходы ───────────────────────────────────────────────────

fn post_process(blocks: &mut Vec<DocBlock>) {
    for block in blocks.iter_mut() {
        // Параграф из одного `![[Имя]]` → Embed-блок.
        if let BlockKind::Paragraph(text) = &mut block.kind {
            text.normalize();
            if let Some((target, attrs)) = parse_embed_paragraph(text) {
                block.kind = BlockKind::Embed { target };
                block.attrs = attrs;
                continue;
            }
        }
        // Wiki-ссылки во всех текстах блока.
        match &mut block.kind {
            BlockKind::Paragraph(t)
            | BlockKind::Heading { text: t, .. }
            | BlockKind::Bullet { text: t, .. }
            | BlockKind::Numbered { text: t, .. }
            | BlockKind::Todo { text: t, .. }
            | BlockKind::Toggle { summary: t, .. }
            | BlockKind::Callout { title: t, .. } => wikify(t),
            BlockKind::Table { headers, rows, .. } => {
                for cell in headers.iter_mut() {
                    wikify(cell);
                }
                for row in rows.iter_mut() {
                    for cell in row.iter_mut() {
                        wikify(cell);
                    }
                }
            }
            _ => {}
        }
        if let Some(children) = block.kind.children_mut() {
            post_process(children);
        }
    }
}

/// `![[Имя]]` (+ опциональные хвостовые атрибуты) единственным содержимым
/// параграфа.
fn parse_embed_paragraph(text: &InlineText) -> Option<(String, Attrs)> {
    if text.0.len() != 1 || !text.0[0].style.plain() {
        return None;
    }
    let raw = text.0[0].text.trim();
    let (body, attrs) = split_trailing_attrs(raw);
    let inner = body.trim().strip_prefix("![[")?.strip_suffix("]]")?;
    if inner.is_empty() || inner.contains('[') || inner.contains(']') || inner.contains('\n') {
        return None;
    }
    Some((inner.trim().to_string(), attrs.unwrap_or_default()))
}

/// Превращает `[[target|алиас]]` в текстовых ранах в wiki-ссылки.
/// Инлайновый `![[x]]` посреди текста нормализуется в обычную wiki-ссылку.
fn wikify(text: &mut InlineText) {
    text.normalize();
    let runs = std::mem::take(&mut text.0);
    for run in runs {
        if run.style.code || run.style.link.is_some() {
            text.0.push(run);
            continue;
        }
        split_wiki_run(&run, &mut text.0);
    }
    text.normalize();
}

fn split_wiki_run(run: &InlineRun, out: &mut Vec<InlineRun>) {
    let s = &run.text;
    let mut cursor = 0;
    while let Some(open_rel) = s[cursor..].find("[[") {
        let mut open = cursor + open_rel;
        let inner_start = open + 2;
        let Some(close_rel) = s[inner_start..].find("]]") else { break };
        let close = inner_start + close_rel;
        let inner = &s[inner_start..close];
        let valid = !inner.is_empty()
            && !inner.contains('[')
            && !inner.contains(']')
            && !inner.contains('\n')
            && !inner.trim().is_empty();
        if !valid {
            cursor = inner_start;
            continue;
        }
        // Инлайновый `![[…]]` — забираем и `!`.
        if s[..open].ends_with('!') {
            open -= 1;
        }
        let (target, alias) = match inner.split_once('|') {
            Some((t, a)) => (t.trim(), Some(a.trim())),
            None => (inner.trim(), None),
        };
        if target.is_empty() {
            cursor = close + 2;
            continue;
        }
        if open > 0 {
            out.push(InlineRun { text: s[..open].to_string(), style: run.style.clone() });
        }
        let display = alias.filter(|a| !a.is_empty()).unwrap_or(target);
        let mut style = run.style.clone();
        style.link = Some(LinkTarget::Wiki { target: target.to_string() });
        out.push(InlineRun { text: display.to_string(), style });
        // Остаток обрабатываем рекурсивно тем же способом.
        let rest = InlineRun { text: s[close + 2..].to_string(), style: run.style.clone() };
        split_wiki_run(&rest, out);
        return;
    }
    if !s.is_empty() {
        out.push(run.clone());
    }
}
