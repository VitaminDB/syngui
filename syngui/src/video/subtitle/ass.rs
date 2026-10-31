//! Разбор ASS/SSA: заголовок (разрешение сценария, стили), строки
//! `Dialogue` и разметка текста (`{\i1}`, `{\an8}`, `{\pos(…)}`, `\N`…).
//!
//! Разметку понимают и все остальные текстовые форматы: декодеры ffmpeg
//! отдают SRT, WebVTT и mov_text строками ASS, а внешний SRT переводится в
//! неё здесь же ([`srt_to_ass`]). Из оформления файла берётся смысловое —
//! курсив, жирный, подчёркивание, зачёркивание, цвет, выравнивание и
//! позиция; шрифт, кегль и контур задаёт оформление пользователя.

use std::collections::HashMap;

use super::{Span, SubText};

/// Стиль ASS — только то, что переносится в показ.
#[derive(Clone, Debug, PartialEq)]
pub struct AssStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Цвет текста стиля (PrimaryColour).
    pub color: Option<[u8; 3]>,
    pub align: u8,
}

impl Default for AssStyle {
    fn default() -> Self {
        Self { bold: false, italic: false, underline: false, strike: false, color: None, align: 2 }
    }
}

/// Заголовок сценария: разрешение (для `\pos`) и стили по имени.
#[derive(Clone, Debug, PartialEq)]
pub struct AssHeader {
    pub play_res: (f32, f32),
    pub styles: HashMap<String, AssStyle>,
    /// Порядок полей строк `Dialogue` (раздел [Events], строка `Format:`).
    pub event_format: Vec<String>,
    /// SSA (v4): старая нумерация выравнивания.
    pub legacy: bool,
}

impl Default for AssHeader {
    fn default() -> Self {
        Self {
            play_res: (384.0, 288.0),
            styles: HashMap::new(),
            event_format: ["Layer", "Start", "End", "Style", "Name", "MarginL", "MarginR", "MarginV", "Effect", "Text"]
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .collect(),
            legacy: false,
        }
    }
}

/// Цвет ASS `&HAABBGGRR&` / `&HBBGGRR&` (или десятичный) → RGB.
fn parse_color(s: &str) -> Option<[u8; 3]> {
    let t = s.trim().trim_start_matches('&').trim_end_matches('&');
    let v = if let Some(h) = t.strip_prefix('H').or_else(|| t.strip_prefix('h')) {
        u32::from_str_radix(h.trim_end_matches('&'), 16).ok()?
    } else {
        t.parse::<i64>().ok()? as u32
    };
    Some([(v & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, ((v >> 16) & 0xFF) as u8])
}

/// SSA: 1–3 снизу, +4 — сверху (5–7), +8 — посередине (9–11) → цифровой блок.
fn legacy_align(a: u8) -> u8 {
    match a {
        1..=3 => a,
        5..=7 => a + 2,
        9..=11 => a - 5,
        _ => 2,
    }
}

impl AssHeader {
    /// Разобрать заголовок (`[Script Info]`, `[V4+ Styles]`, `Format:` у
    /// `[Events]`). Строки событий пропускаются.
    pub fn parse(text: &str) -> Self {
        let mut h = Self::default();
        let mut section = String::new();
        let mut style_format: Vec<String> = Vec::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].to_ascii_lowercase();
                if section == "v4 styles" {
                    h.legacy = true;
                }
                continue;
            }
            let Some((key, value)) = line.split_once(':') else { continue };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match section.as_str() {
                "script info" => match key.as_str() {
                    "playresx" => h.play_res.0 = value.parse().unwrap_or(h.play_res.0),
                    "playresy" => h.play_res.1 = value.parse().unwrap_or(h.play_res.1),
                    "scripttype" => h.legacy |= value.eq_ignore_ascii_case("v4.00"),
                    _ => {}
                },
                "v4+ styles" | "v4 styles" => match key.as_str() {
                    "format" => style_format = value.split(',').map(|f| f.trim().to_ascii_lowercase()).collect(),
                    "style" => {
                        let fields: Vec<&str> = value.splitn(style_format.len().max(1), ',').map(str::trim).collect();
                        let get = |name: &str| style_format.iter().position(|f| f == name).and_then(|i| fields.get(i).copied());
                        let flag = |name: &str| get(name).and_then(|v| v.parse::<i32>().ok()).is_some_and(|v| v != 0);
                        let Some(name) = get("name") else { continue };
                        let mut align = get("alignment").and_then(|v| v.parse::<u8>().ok()).unwrap_or(2);
                        if h.legacy {
                            align = legacy_align(align);
                        }
                        h.styles.insert(
                            name.trim_start_matches('*').to_string(),
                            AssStyle {
                                bold: flag("bold"),
                                italic: flag("italic"),
                                underline: flag("underline"),
                                strike: flag("strikeout"),
                                color: get("primarycolour").and_then(parse_color),
                                align: align.clamp(1, 9),
                            },
                        );
                    }
                    _ => {}
                },
                "events" if key == "format" => {
                    h.event_format = value.split(',').map(|f| f.trim().to_ascii_lowercase()).collect();
                }
                _ => {}
            }
        }
        h
    }

    fn style(&self, name: &str) -> AssStyle {
        self.styles
            .get(name.trim_start_matches('*'))
            .or_else(|| self.styles.get("Default"))
            .cloned()
            .unwrap_or_default()
    }
}

/// Строка события ASS: поля по порядку `Format:`.
#[derive(Clone, Debug, PartialEq)]
pub struct AssDialogue<'a> {
    pub layer: i32,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub style: &'a str,
    pub text: &'a str,
}

/// Время ASS `h:mm:ss.cc` → секунды.
pub fn parse_time(s: &str) -> Option<f64> {
    let mut parts = s.trim().split(':');
    let h: f64 = parts.next()?.trim().parse().ok()?;
    let m: f64 = parts.next()?.trim().parse().ok()?;
    let sec: f64 = parts.next()?.trim().replace(',', ".").parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Разобрать строку события: либо `Dialogue: …` из файла (поля по
/// `format`), либо строку декодера ffmpeg `ReadOrder,Layer,Style,Name,
/// MarginL,MarginR,MarginV,Effect,Text` (без времени — оно у пакета).
pub fn parse_dialogue<'a>(line: &'a str, format: &[String]) -> Option<AssDialogue<'a>> {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(rest) = line.strip_prefix("Dialogue:") {
        let n = format.len().max(1);
        let fields: Vec<&str> = rest.trim_start().splitn(n, ',').collect();
        let get = |name: &str| format.iter().position(|f| f == name).and_then(|i| fields.get(i).copied());
        return Some(AssDialogue {
            layer: get("layer").and_then(|v| v.trim().parse().ok()).unwrap_or(0),
            start: get("start").and_then(parse_time),
            end: get("end").and_then(parse_time),
            style: get("style").map(str::trim).unwrap_or("Default"),
            text: get("text").unwrap_or(""),
        });
    }
    // ffmpeg: ReadOrder,Layer,Style,Name,MarginL,MarginR,MarginV,Effect,Text
    let fields: Vec<&str> = line.splitn(9, ',').collect();
    if fields.len() < 9 {
        return None;
    }
    Some(AssDialogue {
        layer: fields[1].trim().parse().unwrap_or(0),
        start: None,
        end: None,
        style: fields[2].trim(),
        text: fields[8],
    })
}

/// Текущее оформление при разборе разметки.
#[derive(Clone)]
struct State {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<[u8; 3]>,
}

impl State {
    fn of(style: &AssStyle) -> Self {
        Self { bold: style.bold, italic: style.italic, underline: style.underline, strike: style.strike, color: style.color }
    }

    fn span(&self, text: String) -> Span {
        Span { text, bold: self.bold, italic: self.italic, underline: self.underline, strike: self.strike, color: self.color }
    }
}

/// Аргумент тега: `\pos(1,2)` → «1,2»; `\i1` → «1».
fn tag_args(rest: &str) -> &str {
    rest.trim().trim_start_matches('(').trim_end_matches(')')
}

/// Разметка события → строки и куски. `style` — стиль строки, `header` —
/// для разрешения сценария (`\pos`) и сброса `\r<стиль>`. Пустой текст
/// (рисунки `\p1`, одни пробелы) — `None`.
pub fn parse_text(text: &str, style_name: &str, header: &AssHeader, layer: i32) -> Option<SubText> {
    let style = header.style(style_name);
    let mut st = State::of(&style);
    let mut align = style.align;
    let mut pos: Option<(f32, f32)> = None;
    let mut drawing = false;
    let mut lines: Vec<Vec<Span>> = vec![Vec::new()];
    let mut cur = String::new();

    let flush = |cur: &mut String, st: &State, lines: &mut Vec<Vec<Span>>| {
        if !cur.is_empty() {
            let line = lines.last_mut().expect("строка есть всегда");
            // соседний кусок с тем же оформлением — продолжить его
            match line.last_mut() {
                Some(last)
                    if last.bold == st.bold
                        && last.italic == st.italic
                        && last.underline == st.underline
                        && last.strike == st.strike
                        && last.color == st.color =>
                {
                    last.text.push_str(cur)
                }
                _ => line.push(st.span(cur.clone())),
            }
            cur.clear();
        }
    };

    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '{' => {
                let Some(close) = text[i..].find('}') else {
                    if !drawing {
                        cur.push(c);
                    }
                    continue;
                };
                let block = &text[i + 1..i + close];
                // перейти за «}»
                while let Some(&(j, _)) = chars.peek() {
                    if j > i + close {
                        break;
                    }
                    chars.next();
                }
                flush(&mut cur, &st, &mut lines);
                for tag in block.split('\\').skip(1) {
                    let tag = tag.trim();
                    let num = |s: &str| s.trim().parse::<f32>().ok();
                    if let Some(r) = tag.strip_prefix("pos") {
                        let a: Vec<f32> = tag_args(r).split(',').filter_map(num).collect();
                        if a.len() >= 2 {
                            pos = Some((a[0] / header.play_res.0.max(1.0), a[1] / header.play_res.1.max(1.0)));
                        }
                    } else if let Some(r) = tag.strip_prefix("move") {
                        let a: Vec<f32> = tag_args(r).split(',').filter_map(num).collect();
                        if a.len() >= 2 {
                            pos = Some((a[0] / header.play_res.0.max(1.0), a[1] / header.play_res.1.max(1.0)));
                        }
                    } else if let Some(r) = tag.strip_prefix("an") {
                        if let Some(v) = r.trim().parse::<u8>().ok().filter(|v| (1..=9).contains(v)) {
                            align = v;
                        }
                    } else if let Some(r) = tag.strip_prefix('a').filter(|r| r.chars().next().is_some_and(|c| c.is_ascii_digit())) {
                        if let Ok(v) = r.trim().parse::<u8>() {
                            align = legacy_align(v);
                        }
                    } else if let Some(r) = tag.strip_prefix('p').filter(|r| r.chars().all(|c| c.is_ascii_digit()) && !r.is_empty()) {
                        drawing = r.parse::<u32>().unwrap_or(0) > 0;
                    } else if let Some(r) = tag.strip_prefix("1c").or_else(|| tag.strip_prefix('c').filter(|r| r.starts_with('&') || r.is_empty())) {
                        st.color = if r.trim().is_empty() { style.color } else { parse_color(r).or(st.color) };
                    } else if let Some(r) = tag.strip_prefix('r') {
                        let name = r.trim();
                        st = State::of(&if name.is_empty() { style.clone() } else { header.style(name) });
                    } else if let Some(r) = tag.strip_prefix('b').filter(|r| r.chars().all(|c| c.is_ascii_digit()) && !r.is_empty()) {
                        let v: u32 = r.parse().unwrap_or(0);
                        st.bold = v == 1 || v >= 600;
                    } else if let Some(r) = tag.strip_prefix('i').filter(|r| matches!(*r, "0" | "1" | "")) {
                        st.italic = r != "0";
                    } else if let Some(r) = tag.strip_prefix('u').filter(|r| matches!(*r, "0" | "1" | "")) {
                        st.underline = r != "0";
                    } else if let Some(r) = tag.strip_prefix('s').filter(|r| matches!(*r, "0" | "1" | "")) {
                        st.strike = r != "0";
                    }
                }
            }
            '\\' => match chars.peek().map(|&(_, n)| n) {
                Some('N') => {
                    chars.next();
                    flush(&mut cur, &st, &mut lines);
                    lines.push(Vec::new());
                }
                Some('n') => {
                    chars.next();
                    if !drawing {
                        cur.push(' ');
                    }
                }
                Some('h') => {
                    chars.next();
                    if !drawing {
                        cur.push('\u{00A0}');
                    }
                }
                _ => {
                    if !drawing {
                        cur.push(c);
                    }
                }
            },
            '\n' => {
                flush(&mut cur, &st, &mut lines);
                lines.push(Vec::new());
            }
            '\r' => {}
            _ => {
                if !drawing {
                    cur.push(c);
                }
            }
        }
    }
    flush(&mut cur, &st, &mut lines);

    // Пробелы по краям строк и пустые строки — прочь.
    let mut out: Vec<Vec<Span>> = Vec::new();
    for mut line in lines {
        if let Some(first) = line.first_mut() {
            first.text = first.text.trim_start().to_string();
        }
        if let Some(last) = line.last_mut() {
            last.text = last.text.trim_end().to_string();
        }
        line.retain(|s| !s.text.is_empty());
        if !line.is_empty() {
            out.push(line);
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(SubText { lines: out, align, pos, layer })
}

/// Разметка SRT (HTML-подобные теги) → разметка ASS: `<i>`, `<b>`, `<u>`,
/// `<s>`, `<font color="#rrggbb">`; переводы строк → `\N`. Теги ASS
/// (`{\an8}`), которые часто встречаются в SRT, остаются как есть.
pub fn srt_to_ass(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt].replace("\r\n", "\\N").replace('\n', "\\N"));
        let after = &rest[lt..];
        let Some(gt) = after.find('>') else {
            out.push_str(after);
            rest = "";
            break;
        };
        let tag = after[1..gt].trim().to_ascii_lowercase();
        let (closing, name) = match tag.strip_prefix('/') {
            Some(n) => (true, n.trim().to_string()),
            None => (false, tag.split_whitespace().next().unwrap_or("").to_string()),
        };
        let v = if closing { "0" } else { "1" };
        match name.as_str() {
            "i" | "em" => out.push_str(&format!("{{\\i{v}}}")),
            "b" | "strong" => out.push_str(&format!("{{\\b{v}}}")),
            "u" => out.push_str(&format!("{{\\u{v}}}")),
            "s" => out.push_str(&format!("{{\\s{v}}}")),
            "font" if closing => out.push_str("{\\c}"),
            "font" => {
                let raw = &after[1..gt];
                if let Some(rgb) = html_color(raw) {
                    out.push_str(&format!("{{\\c&H{:02X}{:02X}{:02X}&}}", rgb[2], rgb[1], rgb[0]));
                }
            }
            "br" => out.push_str("\\N"),
            // прочие теги (<c.yellow>, <v Имя>, <ruby>…) — без разметки
            _ => {}
        }
        rest = &after[gt + 1..];
    }
    out.push_str(&rest.replace("\r\n", "\\N").replace('\n', "\\N"));
    out
}

/// `color="#ff8800"` / `color=red` из тега `<font …>`.
fn html_color(tag: &str) -> Option<[u8; 3]> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find("color")?;
    let v = lower[at + 5..].trim_start().strip_prefix('=')?.trim_start();
    let v = v.trim_start_matches(['"', '\'']);
    let end = v.find(|c: char| c == '"' || c == '\'' || c.is_whitespace() || c == '>').unwrap_or(v.len());
    let v = &v[..end];
    if let Some(hex) = v.strip_prefix('#') {
        let n = u32::from_str_radix(hex, 16).ok()?;
        return match hex.len() {
            6 => Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]),
            3 => Some([((n >> 8) & 0xF) as u8 * 17, ((n >> 4) & 0xF) as u8 * 17, (n & 0xF) as u8 * 17]),
            _ => None,
        };
    }
    Some(match v {
        "white" => [255, 255, 255],
        "yellow" => [255, 255, 0],
        "red" => [255, 0, 0],
        "green" | "lime" => [0, 255, 0],
        "blue" => [0, 0, 255],
        "cyan" | "aqua" => [0, 255, 255],
        "magenta" | "fuchsia" => [255, 0, 255],
        "black" => [0, 0, 0],
        "gray" | "grey" => [128, 128, 128],
        "orange" => [255, 165, 0],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "[Script Info]\nPlayResX: 1920\nPlayResY: 1080\n\n[V4+ Styles]\n\
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,1,2,10,10,40,1\n\
Style: Top,Arial,48,&H0000FFFF,&H000000FF,&H00000000,&H00000000,-1,1,0,0,100,100,0,0,1,2,1,8,10,10,40,1\n\n\
[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

    #[test]
    fn header_styles_and_res() {
        let h = AssHeader::parse(HEADER);
        assert_eq!(h.play_res, (1920.0, 1080.0));
        let top = &h.styles["Top"];
        assert_eq!(top.align, 8);
        assert!(top.bold && top.italic);
        assert_eq!(top.color, Some([255, 255, 0]));
    }

    #[test]
    fn dialogue_from_file_and_ffmpeg() {
        let h = AssHeader::parse(HEADER);
        let d = parse_dialogue("Dialogue: 0,0:00:01.50,0:00:04.00,Top,,0,0,0,,Привет, мир", &h.event_format).unwrap();
        assert_eq!(d.start, Some(1.5));
        assert_eq!(d.end, Some(4.0));
        assert_eq!(d.style, "Top");
        assert_eq!(d.text, "Привет, мир");
        let d = parse_dialogue("3,0,Default,,0,0,0,,Строка, с запятой", &h.event_format).unwrap();
        assert_eq!(d.style, "Default");
        assert_eq!(d.text, "Строка, с запятой");
    }

    #[test]
    fn markup_spans_lines_and_tags() {
        let h = AssHeader::parse(HEADER);
        let t = parse_text("{\\an8}Обычный {\\i1}курсив{\\i0}\\Nвторая {\\c&H0000FF&}красная", "Default", &h, 0).unwrap();
        assert_eq!(t.align, 8);
        assert_eq!(t.lines.len(), 2);
        assert_eq!(t.lines[0][0].text, "Обычный ");
        assert!(t.lines[0][1].italic);
        assert_eq!(t.lines[1][1].color, Some([255, 0, 0]));
        let t = parse_text("{\\pos(960,540)}Знак", "Default", &h, 0).unwrap();
        assert_eq!(t.pos, Some((0.5, 0.5)));
        // рисунок — пусто
        assert!(parse_text("{\\p1}m 0 0 l 100 0 100 100{\\p0}", "Default", &h, 0).is_none());
        assert_eq!(parse_text("а\\hб", "Default", &h, 0).unwrap().plain(), "а\u{a0}б");
    }

    #[test]
    fn srt_tags_to_ass() {
        let a = srt_to_ass("<i>Курсив</i>\n<font color=\"#FF0000\">Красный</font>");
        let h = AssHeader::default();
        let t = parse_text(&a, "Default", &h, 0).unwrap();
        assert!(t.lines[0][0].italic);
        assert_eq!(t.lines[1][0].color, Some([255, 0, 0]));
        assert_eq!(t.plain(), "Курсив\nКрасный");
    }
}
