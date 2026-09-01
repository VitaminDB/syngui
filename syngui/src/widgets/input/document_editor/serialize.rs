//! `DocModel` → markdown.
//!
//! Выход детерминированный и нормализованный: ATX-заголовки, `- ` маркеры,
//! последовательная нумерация, fenced-код, пустая строка между блоками
//! (кроме соседних элементов списка), атрибуты хвостом `{k=v}`. Гарантия
//! round-trip: `serialize(parse(serialize(parse(x)))) == serialize(parse(x))`
//! — проверяется корпусом фикстур в tests/document_roundtrip.rs.

use super::attrs::serialize_attrs;
use super::model::*;

pub fn serialize_document(model: &DocModel) -> String {
    let lines = blocks_to_lines(&model.blocks);
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Список блоков → строки. Между соседними элементами списка пустой строки
/// нет (иначе pulldown делает список loose), между остальными блоками — есть.
fn blocks_to_lines(blocks: &[DocBlock]) -> Vec<String> {
    let mut out = Vec::new();
    let mut prev_is_item = false;
    for (i, block) in blocks.iter().enumerate() {
        let is_item = block.kind.is_list_item();
        if i > 0 && !(prev_is_item && is_item) {
            out.push(String::new());
        }
        out.extend(block_lines(block));
        prev_is_item = is_item;
    }
    out
}

fn block_lines(block: &DocBlock) -> Vec<String> {
    match &block.kind {
        BlockKind::Paragraph(text) => inline_to_md(text)
            .split('\n')
            .map(|l| escape_line_start(l.to_string()))
            .collect(),
        BlockKind::Heading { level, text } => {
            let hashes = "#".repeat((*level).clamp(1, 6) as usize);
            let mut line = format!("{hashes} {}", inline_to_md(text).replace('\n', " "));
            if !block.attrs.is_empty() {
                line.push(' ');
                line.push_str(&serialize_attrs(&block.attrs));
            }
            vec![line]
        }
        BlockKind::Bullet { text, children } => list_item_lines("- ", 2, text, children),
        BlockKind::Todo { checked, text, children } => {
            let marker = if *checked { "- [x] " } else { "- [ ] " };
            list_item_lines(marker, 2, text, children)
        }
        BlockKind::Numbered { number, text, children } => {
            let marker = format!("{number}. ");
            let indent = marker.len();
            list_item_lines(&marker, indent, text, children)
        }
        BlockKind::Quote(children) => quote_lines(None, children),
        BlockKind::Callout { kind, title, children } => {
            let first = callout_first_line(kind, &block.attrs, title);
            quote_lines(Some(first), children)
        }
        BlockKind::Toggle { summary, children, collapsed } => {
            let mut attrs = block.attrs.clone();
            if !*collapsed {
                attrs.set("open", "");
            }
            let first = callout_first_line("toggle", &attrs, summary);
            quote_lines(Some(first), children)
        }
        BlockKind::CodeBlock { language, code } => {
            let longest = longest_run(code, '`');
            let fence = "`".repeat((longest + 1).max(3));
            let mut lines = vec![format!("{fence}{}", language.as_deref().unwrap_or(""))];
            lines.extend(code.split('\n').map(str::to_string));
            lines.push(fence);
            lines
        }
        BlockKind::Table { headers, rows, aligns } => table_lines(headers, rows, aligns),
        BlockKind::Divider => vec!["---".to_string()],
        BlockKind::Media { url, alt, .. } => {
            let alt = alt.replace('\n', " ").replace('[', "\\[").replace(']', "\\]");
            let mut line = format!("![{alt}]({})", format_url(url));
            line.push_str(&serialize_attrs(&block.attrs));
            vec![line]
        }
        BlockKind::Embed { target } => {
            let mut line = format!("![[{target}]]");
            line.push_str(&serialize_attrs(&block.attrs));
            vec![line]
        }
    }
}

/// Элемент списка: маркер + текст (hard break продолжает строку с отступом
/// до контента) + дети с отступом до контента (nested-список без пустой
/// строки, прочие блоки — после неё, иначе lazy continuation склеит текст).
fn list_item_lines(
    marker: &str,
    content_indent: usize,
    text: &InlineText,
    children: &[DocBlock],
) -> Vec<String> {
    let indent = " ".repeat(content_indent);
    let text_md = inline_to_md(text);
    let mut lines = Vec::new();
    for (i, l) in text_md.split('\n').enumerate() {
        if i == 0 {
            lines.push(format!("{marker}{l}"));
        } else {
            lines.push(format!("{indent}{l}"));
        }
    }
    if !children.is_empty() {
        if !children[0].kind.is_list_item() {
            lines.push(String::new());
        }
        for l in blocks_to_lines(children) {
            if l.is_empty() {
                lines.push(String::new());
            } else {
                lines.push(format!("{indent}{l}"));
            }
        }
    }
    lines
}

/// Цитата/callout: строки детей с префиксом `> `.
fn quote_lines(first: Option<String>, children: &[DocBlock]) -> Vec<String> {
    let mut lines = Vec::new();
    let has_first = first.is_some();
    if let Some(f) = first {
        lines.push(f);
    }
    for (i, l) in blocks_to_lines(children).into_iter().enumerate() {
        // Между маркерной строкой callout и телом нужна пустая quote-строка,
        // иначе первый параграф тела склеится с заголовком (soft break).
        if i == 0 && has_first {
            lines.push(">".to_string());
        }
        if l.is_empty() {
            lines.push(">".to_string());
        } else {
            lines.push(format!("> {l}"));
        }
    }
    lines
}

fn callout_first_line(kind: &str, attrs: &Attrs, title: &InlineText) -> String {
    let mut line = format!("> [!{kind}]");
    line.push_str(&serialize_attrs(attrs));
    let title_md = inline_to_md(title).replace('\n', " ");
    if !title_md.is_empty() {
        line.push(' ');
        line.push_str(&title_md);
    }
    line
}

fn table_lines(headers: &[InlineText], rows: &[Vec<InlineText>], aligns: &[DocAlign]) -> Vec<String> {
    let cols = headers.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0)).max(1);
    let cell = |t: Option<&InlineText>| -> String {
        let md = t.map(inline_to_md).unwrap_or_default();
        md.replace("\\\n", " ").replace('\n', " ").replace('|', "\\|")
    };
    let mut lines = Vec::new();
    let mut header_line = String::from("|");
    for i in 0..cols {
        header_line.push_str(&format!(" {} |", cell(headers.get(i))));
    }
    lines.push(header_line);
    let mut sep = String::from("|");
    for i in 0..cols {
        let marker = match aligns.get(i).copied().unwrap_or_default() {
            DocAlign::Left => "---",
            DocAlign::Center => ":-:",
            DocAlign::Right => "--:",
        };
        sep.push_str(&format!(" {marker} |"));
    }
    lines.push(sep);
    for row in rows {
        let mut line = String::from("|");
        for i in 0..cols {
            line.push_str(&format!(" {} |", cell(row.get(i))));
        }
        lines.push(line);
    }
    lines
}

// ─── Инлайны ────────────────────────────────────────────────────────────────

/// Раны → md. Обёртки группируются по «измерениям» стиля снаружи внутрь:
/// bold → italic → strike → link → code, так что соседние раны с общим
/// жирным дают `**a*b***`, а не мусор из смежных маркеров.
fn inline_to_md(text: &InlineText) -> String {
    group_runs(&text.0, 0)
}

fn group_runs(runs: &[InlineRun], dim: usize) -> String {
    if runs.is_empty() {
        return String::new();
    }
    if dim == 4 {
        // Листовой уровень: code-спаны и обычный текст.
        let mut out = String::new();
        for run in runs {
            if run.style.code {
                out.push_str(&code_span(&run.text));
            } else {
                out.push_str(&escape_md(&run.text));
            }
        }
        return out;
    }
    let mut out = String::new();
    let mut i = 0;
    while i < runs.len() {
        let mut j = i + 1;
        while j < runs.len() && dim_eq(&runs[i], &runs[j], dim) {
            j += 1;
        }
        let group = &runs[i..j];
        match dim {
            0 if runs[i].style.bold => {
                out.push_str("**");
                out.push_str(&group_runs(group, 1));
                out.push_str("**");
            }
            1 if runs[i].style.italic => {
                out.push('*');
                out.push_str(&group_runs(group, 2));
                out.push('*');
            }
            2 if runs[i].style.strike => {
                out.push_str("~~");
                out.push_str(&group_runs(group, 3));
                out.push_str("~~");
            }
            3 => match &runs[i].style.link {
                Some(LinkTarget::Url(url)) => {
                    out.push('[');
                    out.push_str(&group_runs(group, 4));
                    out.push_str(&format!("]({})", format_url(url)));
                }
                Some(LinkTarget::Wiki { target }) => {
                    // Wiki-ссылка пишется сырым текстом, без md-экранирования.
                    let display: String = group.iter().map(|r| r.text.as_str()).collect();
                    if display == *target {
                        out.push_str(&format!("[[{target}]]"));
                    } else {
                        out.push_str(&format!("[[{target}|{display}]]"));
                    }
                }
                None => out.push_str(&group_runs(group, 4)),
            },
            _ => out.push_str(&group_runs(group, dim + 1)),
        }
        i = j;
    }
    out
}

fn dim_eq(a: &InlineRun, b: &InlineRun, dim: usize) -> bool {
    match dim {
        0 => a.style.bold == b.style.bold,
        1 => a.style.italic == b.style.italic,
        2 => a.style.strike == b.style.strike,
        3 => a.style.link == b.style.link,
        _ => true,
    }
}

/// Code-спан: столько backtick'ов, чтобы содержимое не закрыло его раньше
/// времени; пробельная прокладка по craft-правилу CommonMark.
fn code_span(text: &str) -> String {
    let content = text.replace('\n', " ");
    let fence = "`".repeat(longest_run(&content, '`') + 1);
    if content.starts_with('`') || content.ends_with('`') || content.is_empty() {
        format!("{fence} {content} {fence}")
    } else {
        format!("{fence}{content}{fence}")
    }
}

fn longest_run(s: &str, ch: char) -> usize {
    let mut max = 0;
    let mut cur = 0;
    for c in s.chars() {
        if c == ch {
            cur += 1;
            max = max.max(cur);
        } else {
            cur = 0;
        }
    }
    max
}

/// Экранирование инлайн-текста. `\n` в ране — hard break (`\` + перевод
/// строки). `{` сознательно не экранируем: см. известную нормализацию
/// в docs (литеральный `{k=v}` в конце заголовка станет атрибутами).
fn escape_md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '*' | '_' | '~' | '[' | ']' | '`' | '<' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\\n"),
            _ => out.push(c),
        }
    }
    out
}

/// Экранирование начала строки параграфа, чтобы текст не стал блоком:
/// `#`, `>`, маркеры списков, `1.`, `---`.
fn escape_line_start(line: String) -> String {
    let bytes = line.as_bytes();
    if bytes.is_empty() {
        return line;
    }
    match bytes[0] {
        b'#' => {
            let hashes = bytes.iter().take_while(|&&b| b == b'#').count();
            if hashes <= 6 && matches!(bytes.get(hashes), None | Some(b' ')) {
                return format!("\\{line}");
            }
        }
        b'>' => return format!("\\{line}"),
        b'-' | b'+' => {
            if matches!(bytes.get(1), None | Some(b' ') | Some(b'-')) {
                return format!("\\{line}");
            }
        }
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if digits <= 9 {
                if let Some(&marker @ (b'.' | b')')) = bytes.get(digits) {
                    if matches!(bytes.get(digits + 1), None | Some(b' ')) {
                        let mut out = line[..digits].to_string();
                        out.push('\\');
                        out.push(marker as char);
                        out.push_str(&line[digits + 1..]);
                        return out;
                    }
                }
            }
        }
        _ => {}
    }
    line
}

/// URL в круглых скобках: с пробелами/скобками — в угловые скобки.
fn format_url(url: &str) -> String {
    if url.is_empty() || url.chars().any(|c| c.is_whitespace() || c == '(' || c == ')') {
        format!("<{url}>")
    } else {
        url.to_string()
    }
}
