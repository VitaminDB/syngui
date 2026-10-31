//! Внешние субтитры: поиск файлов рядом с роликом, кодировка и разбор
//! SRT, WebVTT, ASS/SSA.
//!
//! Подходящие файлы: в папке ролика — `<имя ролика>.srt` и
//! `<имя ролика>.<что-угодно>.srt` («Фильм.rus.srt», «Фильм.en.forced.srt»);
//! во вложенных папках (`Subs/`, `RUS Sub/`, `Subs/<имя ролика>/`) — так же,
//! плюс всё из папки с именем ролика. Язык и пометки (forced, SDH) берутся
//! из хвоста имени и из имени папки.

use std::path::{Path, PathBuf};

use super::ass::{self, AssHeader};
use super::{SubText, SubtitleSource, SubtitleTrackInfo};

pub const EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt"];

fn is_sub_file(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

/// Найденный файл и что о нём известно по имени.
#[derive(Clone, Debug)]
pub struct Found {
    pub path: PathBuf,
    pub info: SubtitleTrackInfo,
}

/// Код языка по слову из имени файла или папки («rus», «RU», «Russian»,
/// «Русские»…).
fn language_of(word: &str) -> Option<&'static str> {
    let w = word.trim().to_lowercase();
    let w = w.trim_matches(|c: char| !c.is_alphanumeric());
    Some(match w {
        "ru" | "rus" | "russian" | "русский" | "русские" | "рус" => "rus",
        "en" | "eng" | "english" | "английский" | "англ" => "eng",
        "uk" | "ua" | "ukr" | "ukrainian" | "украинский" => "ukr",
        "be" | "bel" | "belarusian" => "bel",
        "kk" | "kaz" | "kazakh" => "kaz",
        "de" | "ger" | "deu" | "german" => "ger",
        "fr" | "fre" | "fra" | "french" => "fre",
        "es" | "spa" | "spanish" => "spa",
        "it" | "ita" | "italian" => "ita",
        "pt" | "por" | "portuguese" => "por",
        "pl" | "pol" | "polish" => "pol",
        "ja" | "jp" | "jpn" | "japanese" => "jpn",
        "ko" | "kor" | "korean" => "kor",
        "zh" | "chi" | "zho" | "chinese" => "chi",
        "tr" | "tur" | "turkish" => "tur",
        _ => return None,
    })
}

/// Сведения о дорожке по имени файла: язык, название, forced/SDH.
/// `stem` — имя ролика без расширения (хвост после него разбирается).
pub fn describe(path: &Path, stem: Option<&str>) -> SubtitleTrackInfo {
    let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let tail = match stem {
        Some(s) if file_stem.len() >= s.len() && file_stem[..s.len()].eq_ignore_ascii_case(s) => &file_stem[s.len()..],
        _ => file_stem,
    };
    let mut language = None;
    let mut forced = false;
    let mut sdh = false;
    let mut title_words: Vec<&str> = Vec::new();
    for word in tail.split(|c: char| c == '.' || c == '_' || c == '[' || c == ']' || c == '(' || c == ')').filter(|w| !w.trim().is_empty()) {
        let lw = word.trim().to_lowercase();
        if lw == "forced" || lw == "надписи" {
            forced = true;
            if lw == "надписи" {
                title_words.push(word.trim());
            }
        } else if matches!(lw.as_str(), "sdh" | "cc" | "hi") {
            sdh = true;
        } else if language.is_none() && language_of(&lw).is_some() {
            language = language_of(&lw);
        } else if stem.is_some() || !title_words.is_empty() || tail != file_stem {
            title_words.push(word.trim());
        } else {
            title_words.push(word.trim());
        }
    }
    // Язык из имени папки: «Subs/RUS/…», «Russian/…».
    if language.is_none() {
        if let Some(dir) = path.parent().and_then(|d| d.file_name()).and_then(|d| d.to_str()) {
            language = dir.split(|c: char| !c.is_alphanumeric()).find_map(language_of);
        }
    }
    let title = Some(title_words.join(" ")).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let codec = match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("ass") => "ASS",
        Some("ssa") => "SSA",
        Some("vtt") => "WebVTT",
        _ => "SRT",
    };
    SubtitleTrackInfo {
        source: SubtitleSource::External(path.to_path_buf()),
        language: language.map(String::from),
        title,
        codec: codec.into(),
        bitmap: false,
        default: false,
        forced,
        hearing_impaired: sdh,
    }
}

/// Файлы субтитров к ролику `video` (см. описание модуля), по имени.
pub fn find(video: &Path) -> Vec<Found> {
    let Some(dir) = video.parent() else { return Vec::new() };
    let Some(stem) = video.file_stem().and_then(|s| s.to_str()) else { return Vec::new() };
    let matches_stem = |p: &Path| {
        p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| {
            s.len() >= stem.len()
                && s[..stem.len()].eq_ignore_ascii_case(stem)
                && (s.len() == stem.len() || matches!(s.as_bytes()[stem.len()], b'.' | b'_' | b' ' | b'-' | b'['))
        })
    };
    let mut out: Vec<Found> = Vec::new();
    let push = |p: PathBuf, out: &mut Vec<Found>| {
        if !out.iter().any(|f| f.path == p) {
            let info = describe(&p, Some(stem));
            out.push(Found { path: p, info });
        }
    };
    let read = |d: &Path| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        v.sort();
        v
    };
    for p in read(dir) {
        if p.is_file() && is_sub_file(&p) && matches_stem(&p) {
            push(p, &mut out);
        }
    }
    // Вложенные папки: «Subs», «RUS Subs», «Subtitles», «Субтитры» и папка
    // с именем ролика внутри них. Глубже двух уровней не идём.
    let subdir = |d: &Path| {
        d.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            let n = n.to_lowercase();
            n.contains("sub") || n.contains("субт") || n.contains("сабы")
        })
    };
    for d in read(dir).into_iter().filter(|d| d.is_dir() && subdir(d)) {
        for p in read(&d) {
            if p.is_file() && is_sub_file(&p) && matches_stem(&p) {
                push(p, &mut out);
            } else if p.is_dir() {
                let named_as_video = p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(stem));
                for q in read(&p) {
                    if q.is_file() && is_sub_file(&q) && (named_as_video || matches_stem(&q)) {
                        push(q, &mut out);
                    }
                }
            }
        }
    }
    out
}

/// Файл → текст: BOM UTF-8/UTF-16, иначе UTF-8, если он корректен; иначе
/// однобайтовая кодировка — Windows-1251, если высоких байтов много (так
/// пишется кириллица), иначе Windows-1252 (западные языки).
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if bytes.len() >= 2 && (bytes[..2] == [0xFF, 0xFE] || bytes[..2] == [0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let high = bytes.iter().filter(|&&b| b >= 0xC0).count();
    let letters = bytes.iter().filter(|&&b| b.is_ascii_alphabetic() || b >= 0xC0).count().max(1);
    if high * 3 >= letters {
        bytes.iter().map(|&b| cp1251(b)).collect()
    } else {
        // Windows-1252 ≈ Latin-1 для букв
        bytes.iter().map(|&b| cp1252(b)).collect()
    }
}

fn cp1251(b: u8) -> char {
    const HI: [u16; 64] = [
        0x0402, 0x0403, 0x201A, 0x0453, 0x201E, 0x2026, 0x2020, 0x2021, 0x20AC, 0x2030, 0x0409, 0x2039, 0x040A, 0x040C, 0x040B, 0x040F, //
        0x0452, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0xFFFD, 0x2122, 0x0459, 0x203A, 0x045A, 0x045C, 0x045B, 0x045F, //
        0x00A0, 0x040E, 0x045E, 0x0408, 0x00A4, 0x0490, 0x00A6, 0x00A7, 0x0401, 0x00A9, 0x0404, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x0407, //
        0x00B0, 0x00B1, 0x0406, 0x0456, 0x0491, 0x00B5, 0x00B6, 0x00B7, 0x0451, 0x2116, 0x0454, 0x00BB, 0x0458, 0x0405, 0x0455, 0x0457,
    ];
    match b {
        0..=0x7F => b as char,
        0x80..=0xBF => char::from_u32(HI[(b - 0x80) as usize] as u32).unwrap_or('\u{FFFD}'),
        _ => char::from_u32(0x0410 + (b - 0xC0) as u32).unwrap_or('\u{FFFD}'),
    }
}

fn cp1252(b: u8) -> char {
    const HI: [u16; 32] = [
        0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0xFFFD, 0x017D, 0xFFFD, //
        0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0xFFFD, 0x017E, 0x0178,
    ];
    match b {
        0x80..=0x9F => char::from_u32(HI[(b - 0x80) as usize] as u32).unwrap_or('\u{FFFD}'),
        _ => b as char,
    }
}

/// Разобранный файл: события (начало, конец, текст) от начала ролика.
pub struct Parsed {
    pub format: &'static str,
    pub events: Vec<(f64, f64, SubText)>,
}

/// Прочитать и разобрать файл субтитров (формат — по содержимому, затем по
/// расширению).
pub fn load(path: &Path) -> Result<Parsed, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = decode_text(&bytes);
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let head = text.trim_start_matches('\u{FEFF}').trim_start();
    let parsed = if head.starts_with("WEBVTT") || ext == "vtt" {
        Parsed { format: "WebVTT", events: parse_vtt(&text) }
    } else if head.starts_with("[Script Info]") || ext == "ass" || ext == "ssa" {
        Parsed { format: if ext == "ssa" { "SSA" } else { "ASS" }, events: parse_ass(&text) }
    } else {
        Parsed { format: "SRT", events: parse_srt(&text) }
    };
    if parsed.events.is_empty() {
        return Err("в файле нет ни одной строки субтитров".into());
    }
    Ok(parsed)
}

/// Время SRT/VTT: `01:02:03,456`, `01:02:03.456`, `02:03.456`.
fn parse_clock(s: &str) -> Option<f64> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(':').collect();
    let sec: f64 = parts.last()?.trim().replace(',', ".").parse().ok()?;
    let (h, m) = match parts.len() {
        3 => (parts[0].trim().parse::<f64>().ok()?, parts[1].trim().parse::<f64>().ok()?),
        2 => (0.0, parts[0].trim().parse::<f64>().ok()?),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Строка времени «A --> B [настройки]» → (A, B, настройки).
fn parse_timing(line: &str) -> Option<(f64, f64, &str)> {
    let (a, rest) = line.split_once("-->")?;
    let rest = rest.trim_start();
    let end_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let start = parse_clock(a)?;
    let end = parse_clock(&rest[..end_len])?;
    Some((start, end, rest[end_len..].trim()))
}

pub fn parse_srt(text: &str) -> Vec<(f64, f64, SubText)> {
    let header = AssHeader::default();
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let Some((start, end, _)) = parse_timing(lines[i]) else {
            i += 1;
            continue;
        };
        i += 1;
        let mut body: Vec<&str> = Vec::new();
        while i < lines.len() && !lines[i].trim().is_empty() {
            // Следующая реплика без пустой строки между ними: номер и время.
            if i + 1 < lines.len() && lines[i].trim().chars().all(|c| c.is_ascii_digit()) && parse_timing(lines[i + 1]).is_some() {
                break;
            }
            body.push(lines[i]);
            i += 1;
        }
        let markup = ass::srt_to_ass(&body.join("\n"));
        if let Some(t) = ass::parse_text(&markup, "Default", &header, 0) {
            if end > start {
                out.push((start, end, t));
            }
        }
    }
    out
}

pub fn parse_vtt(text: &str) -> Vec<(f64, f64, SubText)> {
    let header = AssHeader::default();
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        // Блоки NOTE / STYLE / REGION — до пустой строки.
        if line.starts_with("NOTE") || line == "STYLE" || line == "REGION" {
            while i < lines.len() && !lines[i].trim().is_empty() {
                i += 1;
            }
            continue;
        }
        let Some((start, end, settings)) = parse_timing(lines[i]) else {
            i += 1;
            continue;
        };
        i += 1;
        let mut body: Vec<&str> = Vec::new();
        while i < lines.len() && !lines[i].trim().is_empty() {
            body.push(lines[i]);
            i += 1;
        }
        // Внутренние метки времени караоке <00:00:01.000> — прочь.
        let joined = body.join("\n");
        let mut clean = String::with_capacity(joined.len());
        let mut rest = joined.as_str();
        while let Some(lt) = rest.find('<') {
            clean.push_str(&rest[..lt]);
            let after = &rest[lt..];
            match after.find('>') {
                Some(gt) if after[1..gt].chars().next().is_some_and(|c| c.is_ascii_digit()) => rest = &after[gt + 1..],
                Some(gt) => {
                    clean.push_str(&after[..=gt]);
                    rest = &after[gt + 1..];
                }
                None => {
                    clean.push_str(after);
                    rest = "";
                }
            }
        }
        clean.push_str(rest);
        let clean = clean.replace("&amp;", "&").replace("&lt;", "‹").replace("&gt;", "›").replace("&nbsp;", "\u{a0}");
        let markup = ass::srt_to_ass(&clean);
        if let Some(mut t) = ass::parse_text(&markup, "Default", &header, 0) {
            // «line:0» / «line:10%» — у верхнего края
            for s in settings.split_whitespace() {
                if let Some(v) = s.strip_prefix("line:") {
                    let v = v.split(',').next().unwrap_or("");
                    let top = match v.strip_suffix('%') {
                        Some(p) => p.parse::<f32>().is_ok_and(|p| p < 50.0),
                        None => v.parse::<i32>().is_ok_and(|n| n >= 0 && n < 4),
                    };
                    if top {
                        t.align = 8;
                    }
                }
            }
            if end > start {
                out.push((start, end, t));
            }
        }
    }
    out
}

pub fn parse_ass(text: &str) -> Vec<(f64, f64, SubText)> {
    let header = AssHeader::parse(text);
    let mut out = Vec::new();
    for line in text.lines() {
        if !line.starts_with("Dialogue:") {
            continue;
        }
        let Some(d) = ass::parse_dialogue(line, &header.event_format) else { continue };
        let (Some(start), Some(end)) = (d.start, d.end) else { continue };
        if let Some(t) = ass::parse_text(d.text, d.style, &header, d.layer) {
            if end > start {
                out.push((start, end, t));
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_basic_and_glued_cues() {
        let s = "1\r\n00:00:01,000 --> 00:00:03,500\r\n<i>Привет</i>\r\nмир\r\n\r\n2\r\n00:00:04,000 --> 00:00:05,000\r\nВторая\r\n3\r\n00:00:06,000 --> 00:00:07,000\r\nТретья\r\n";
        let ev = parse_srt(s);
        assert_eq!(ev.len(), 3);
        assert_eq!(ev[0].0, 1.0);
        assert_eq!(ev[0].1, 3.5);
        assert_eq!(ev[0].2.plain(), "Привет\nмир");
        assert!(ev[0].2.lines[0][0].italic);
        assert_eq!(ev[2].2.plain(), "Третья");
    }

    #[test]
    fn vtt_cues_settings_and_tags() {
        let s = "WEBVTT\n\nNOTE комментарий\nещё\n\nid1\n00:01.000 --> 00:02.500 line:0\n<v Анна>Сверху</v>\n\n00:00:03.000 --> 00:00:04.000\nКара<00:00:03.500>оке &amp; всё\n";
        let ev = parse_vtt(s);
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].2.align, 8);
        assert_eq!(ev[0].2.plain(), "Сверху");
        assert_eq!(ev[1].2.plain(), "Караоке & всё");
    }

    #[test]
    fn cp1251_is_detected() {
        // «Привет, мир» в Windows-1251
        let b: Vec<u8> = vec![0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2, 0x2C, 0x20, 0xEC, 0xE8, 0xF0];
        assert_eq!(decode_text(&b), "Привет, мир");
        // «café» в Windows-1252
        assert_eq!(decode_text(&[b'c', b'a', b'f', 0xE9]), "café");
        // UTF-16LE с BOM
        assert_eq!(decode_text(&[0xFF, 0xFE, 0x1F, 0x04, 0x40, 0x04]), "Пр");
    }

    #[test]
    fn names_give_language_and_flags() {
        let i = describe(Path::new("/v/Фильм.rus.forced.srt"), Some("Фильм"));
        assert_eq!(i.language.as_deref(), Some("rus"));
        assert!(i.forced);
        let i = describe(Path::new("/v/Subs/English/Фильм.srt"), Some("Фильм"));
        assert_eq!(i.language.as_deref(), Some("eng"));
        let i = describe(Path::new("/v/Фильм.Кубик в кубе.ass"), Some("Фильм"));
        assert_eq!(i.title.as_deref(), Some("Кубик в кубе"));
        assert_eq!(i.codec, "ASS");
    }

    #[test]
    fn finds_files_near_video() {
        let dir = std::env::temp_dir().join(format!("syngui-subs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Subs/RUS")).unwrap();
        std::fs::create_dir_all(dir.join("Subs/Серия 1")).unwrap();
        for f in ["Серия 1.mkv", "Серия 1.srt", "Серия 1.en.srt", "Серия 10.srt", "Subs/RUS/Серия 1.ass", "Subs/Серия 1/2_English.srt", "Subs/RUS/Серия 2.ass"] {
            std::fs::write(dir.join(f), "1\n00:00:01,000 --> 00:00:02,000\nx\n").unwrap();
        }
        let found = find(&dir.join("Серия 1.mkv"));
        let names: Vec<String> = found.iter().map(|f| f.path.strip_prefix(&dir).unwrap().display().to_string()).collect();
        assert_eq!(names, vec!["Серия 1.en.srt", "Серия 1.srt", "Subs/RUS/Серия 1.ass", "Subs/Серия 1/2_English.srt"]);
        assert_eq!(found[2].info.language.as_deref(), Some("rus"));
        assert_eq!(found[0].info.language.as_deref(), Some("eng"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
