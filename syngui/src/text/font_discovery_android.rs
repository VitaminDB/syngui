use crate::text::script::Script;

const FONT_PATHS: &[&str] = &[
    "/system/fonts/Roboto-Regular.ttf",
    "/system/fonts/DroidSans.ttf",
    "/system/fonts/NotoSans-Regular.ttf",
];

const BOLD_FONT_PATHS: &[&str] = &[
    "/system/fonts/Roboto-Bold.ttf",
    "/system/fonts/DroidSans-Bold.ttf",
    "/system/fonts/NotoSans-Bold.ttf",
];

const EMOJI_FONT_PATHS: &[&str] = &[
    "/system/fonts/NotoColorEmoji.ttf",
    "/system/fonts/NotoColorEmoji-compat.ttf",
];

pub fn discover_font(_preferred_family: Option<&str>) -> (Vec<u8>, u32) {
    for path in FONT_PATHS {
        if let Ok(data) = std::fs::read(path) {
            return (data, 0);
        }
    }
    log::warn!("No Android system font found");
    (Vec::new(), 0)
}

pub fn discover_bold_font(_preferred_family: Option<&str>) -> (Vec<u8>, u32) {
    for path in BOLD_FONT_PATHS {
        if let Ok(data) = std::fs::read(path) {
            return (data, 0);
        }
    }
    log::warn!("No Android bold font found");
    (Vec::new(), 0)
}

pub fn discover_emoji_font() -> (Vec<u8>, u32) {
    for path in EMOJI_FONT_PATHS {
        if let Ok(data) = std::fs::read(path) {
            return (data, 0);
        }
    }
    log::warn!("No Android emoji font found");
    (Vec::new(), 0)
}

const FALLBACK_PATHS_BY_SCRIPT: &[(Script, &[(&str, u32)])] = &[
    (
        Script::Han,
        &[
            ("/system/fonts/NotoSansCJK-Regular.ttc", 2),
            ("/system/fonts/NotoSansSC-Regular.otf", 0),
            ("/system/fonts/DroidSansFallback.ttf", 0),
        ],
    ),
    (
        Script::Kana,
        &[
            ("/system/fonts/NotoSansCJK-Regular.ttc", 0),
            ("/system/fonts/NotoSansJP-Regular.otf", 0),
            ("/system/fonts/DroidSansFallback.ttf", 0),
        ],
    ),
    (
        Script::Hangul,
        &[
            ("/system/fonts/NotoSansCJK-Regular.ttc", 1),
            ("/system/fonts/NotoSansKR-Regular.otf", 0),
            ("/system/fonts/DroidSansFallback.ttf", 0),
        ],
    ),
];

fn fallback_paths(script: Script) -> &'static [(&'static str, u32)] {
    FALLBACK_PATHS_BY_SCRIPT
        .iter()
        .find(|(s, _)| *s == script)
        .map(|(_, paths)| *paths)
        .unwrap_or(&[])
}

/// Loads the first present system font for `script`. The Noto CJK collection
/// ships faces in the order JP=0, KR=1, SC=2, TC=3; Han uses SC unless the UI
/// prefers Japanese or Korean, in which case that script's list goes first.
pub fn discover_fallback_font(
    script: Script,
    prefer_japanese: bool,
    prefer_korean: bool,
) -> Option<(Vec<u8>, u32)> {
    let mut order: Vec<(&'static str, u32)> = Vec::new();
    let mut push_all = |paths: &'static [(&'static str, u32)]| {
        for entry in paths {
            if !order.contains(entry) {
                order.push(*entry);
            }
        }
    };
    if script == Script::Han {
        if prefer_japanese {
            push_all(fallback_paths(Script::Kana));
        }
        if prefer_korean {
            push_all(fallback_paths(Script::Hangul));
        }
    }
    push_all(fallback_paths(script));
    for (path, face_index) in order {
        if let Ok(data) = std::fs::read(path) {
            return Some((data, face_index));
        }
    }
    log::warn!(
        "No Android fallback font for {:?} — its characters will not render",
        script
    );
    None
}

// ─── Семейства системы (субтитры, выбор шрифта) ─────────────────────────────

/// Шрифт из `/system/fonts`: семейство и начертание — по имени файла
/// («NotoSerif-BoldItalic.ttf» → «Noto Serif», 700, курсив; «-VF» —
/// переменный, насыщенность ставит ось `wght`). Читать сотни файлов ради
/// таблицы `name` дорого (≈130 МБ), имена файлов Android устойчивы.
struct Entry {
    family: String,
    path: std::path::PathBuf,
    weight: u16,
    italic: bool,
}

const FONT_DIRS: &[&str] = &["/system/fonts", "/product/fonts"];

/// «NotoSansCJK» → «Noto Sans CJK», «DroidSans» → «Droid Sans».
fn spaced(stem: &str) -> String {
    let chars: Vec<char> = stem.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        let boundary = c.is_uppercase()
            && prev.is_some_and(|p| p.is_lowercase() || (p.is_uppercase() && next.is_some_and(|n| n.is_lowercase())));
        if boundary {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

fn style_of(style: &str) -> (u16, bool) {
    let s = style.to_ascii_lowercase();
    let italic = s.contains("italic");
    let weight = if s.contains("thin") || s.contains("hairline") {
        100
    } else if s.contains("extralight") || s.contains("ultralight") {
        200
    } else if s.contains("light") {
        300
    } else if s.contains("medium") {
        500
    } else if s.contains("semibold") || s.contains("demibold") {
        600
    } else if s.contains("extrabold") || s.contains("ultrabold") {
        800
    } else if s.contains("bold") {
        700
    } else if s.contains("black") || s.contains("heavy") {
        900
    } else {
        400
    };
    (weight, italic)
}

fn catalog() -> &'static Vec<Entry> {
    static C: std::sync::OnceLock<Vec<Entry>> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let mut v = Vec::new();
        for dir in FONT_DIRS {
            let Ok(rd) = std::fs::read_dir(dir) else { continue };
            for e in rd.flatten() {
                let path = e.path();
                let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                if !matches!(ext.as_str(), "ttf" | "otf") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
                let (fam, style) = stem.split_once('-').unwrap_or((stem, "Regular"));
                let (weight, italic) = style_of(style);
                v.push(Entry { family: spaced(fam), path, weight, italic });
            }
        }
        v
    })
}

fn norm(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != '-' && *c != '_').flat_map(char::to_lowercase).collect()
}

/// Семейства для выбора шрифта: без письменностей отдельных языков (Noto Sans
/// Bengali и т. п.), эмодзи и служебных — то, чем пишут латиницу и кириллицу.
pub fn list_families() -> Vec<String> {
    let mut v: Vec<String> = catalog()
        .iter()
        .map(|e| e.family.clone())
        .filter(|f| {
            let noto_script = (f.starts_with("Noto Sans ") || f.starts_with("Noto Serif ") || f.starts_with("Noto Naskh"))
                && !matches!(f.as_str(), "Noto Sans Mono" | "Noto Serif Display");
            !noto_script && !f.contains("Emoji") && !f.starts_with("Android") && !f.ends_with(" UI")
        })
        .collect();
    v.sort_by_key(|a| a.to_lowercase());
    v.dedup();
    v
}

/// Начертание семейства `name`, ближайшее к насыщенности `weight`; курсив —
/// если есть (иначе его наклонят при отрисовке).
pub fn discover_styled(name: &str, weight: u16, italic: bool) -> Option<(Vec<u8>, u32)> {
    let want = norm(name);
    // «sans-serif» и пустое — системный шрифт интерфейса.
    let want = if want.is_empty() || want == "sansserif" { "roboto".to_string() } else { want };
    let best = catalog()
        .iter()
        .filter(|e| norm(&e.family) == want)
        .min_by_key(|e| (e.italic != italic) as u32 * 10_000 + (e.weight as i32 - weight as i32).unsigned_abs())?;
    std::fs::read(&best.path).ok().map(|d| (d, 0))
}

/// Системный шрифт интерфейса нужной насыщенности.
pub fn discover_weight_font(family: Option<&str>, weight: u16) -> (Vec<u8>, u32) {
    family
        .and_then(|f| discover_styled(f, weight, false))
        .or_else(|| discover_styled("Roboto", weight, false))
        .unwrap_or_else(|| if weight >= 600 { discover_bold_font(None) } else { discover_font(None) })
}

/// Одно семейство без подмены на «похожее».
pub fn discover_family_font(name: &str) -> Option<(Vec<u8>, u32)> {
    discover_styled(name, 400, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_names_from_files() {
        assert_eq!(spaced("NotoSansCJK"), "Noto Sans CJK");
        assert_eq!(spaced("DroidSans"), "Droid Sans");
        assert_eq!(spaced("Roboto"), "Roboto");
        assert_eq!(style_of("BoldItalic"), (700, true));
        assert_eq!(style_of("VF"), (400, false));
        assert_eq!(style_of("SemiBold"), (600, false));
    }
}
