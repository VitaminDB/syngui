use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Weight, Style};
use font_kit::source::SystemSource;
use crate::text::script::Script;

#[cfg(target_os = "windows")]
const FALLBACK_FAMILIES: &[&str] = &[
    "Segoe UI", "Arial", "Tahoma", "Verdana", "Microsoft Sans Serif",
    "Calibri", "Trebuchet MS", "Lucida Sans Unicode",
];

#[cfg(target_os = "macos")]
const FALLBACK_FAMILIES: &[&str] = &[
    "Helvetica Neue", "Helvetica", "Arial", "Lucida Grande",
    "SF Pro", "San Francisco",
];

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const FALLBACK_FAMILIES: &[&str] = &[
    "DejaVu Sans", "Liberation Sans", "Noto Sans", "Ubuntu",
    "Cantarell", "Droid Sans", "FreeSans",
];

fn build_families(preferred: Option<&str>) -> Vec<FamilyName> {
    let mut families = Vec::new();
    if let Some(family) = preferred {
        families.push(FamilyName::Title(family.to_string()));
    }
    families.push(FamilyName::SansSerif);
    families.push(FamilyName::Serif);
    for name in FALLBACK_FAMILIES {
        families.push(FamilyName::Title(name.to_string()));
    }
    families
}

pub fn discover_font(preferred_family: Option<&str>) -> (Vec<u8>, u32) {
    let families = build_families(preferred_family);
    let props = Properties {
        weight: Weight::NORMAL,
        style: Style::Normal,
        ..Properties::default()
    };

    match load_font_bytes(&families, &props) {
        Some((data, face_index)) => (data, face_index),
        None => {
            log::warn!("No system font found");
            (Vec::new(), 0)
        }
    }
}

pub fn discover_bold_font(preferred_family: Option<&str>) -> (Vec<u8>, u32) {
    let families = build_families(preferred_family);
    let props = Properties {
        weight: Weight::BOLD,
        style: Style::Normal,
        ..Properties::default()
    };

    match load_font_bytes(&families, &props) {
        Some((data, face_index)) => (data, face_index),
        None => {
            log::warn!("No bold font found — bold text will use regular font");
            (Vec::new(), 0)
        }
    }
}

pub fn discover_emoji_font() -> (Vec<u8>, u32) {
    let families = [
        FamilyName::Title("Segoe UI Emoji".to_string()),
        FamilyName::Title("Apple Color Emoji".to_string()),
        FamilyName::Title("Noto Color Emoji".to_string()),
        FamilyName::Title("Noto Emoji".to_string()),
        FamilyName::Title("Twemoji Mozilla".to_string()),
    ];

    let props = Properties::default();

    if let Some((data, face_index)) = load_font_bytes(&families, &props) {
        return (data, face_index);
    }

    if let Some((data, face_index)) = try_families_individually(&families, &props) {
        return (data, face_index);
    }

    log::warn!("No emoji font found — emoji will not render");
    (Vec::new(), 0)
}

fn load_font_bytes(families: &[FamilyName], props: &Properties) -> Option<(Vec<u8>, u32)> {
    let source = SystemSource::new();
    let handle = source.select_best_match(families, props).ok()?;
    handle_to_bytes(handle)
}

fn try_families_individually(families: &[FamilyName], props: &Properties) -> Option<(Vec<u8>, u32)> {
    let source = SystemSource::new();
    for family in families {
        if let Ok(handle) = source.select_best_match(std::slice::from_ref(family), props) {
            if let Some(result) = handle_to_bytes(handle) {
                return Some(result);
            }
        }
    }
    None
}

fn handle_to_bytes(handle: Handle) -> Option<(Vec<u8>, u32)> {
    match handle {
        Handle::Path { ref path, font_index } => {
            std::fs::read(path).ok().map(|data| (data, font_index))
        }
        Handle::Memory { bytes, font_index } => Some(((*bytes).clone(), font_index)),
    }
}

pub fn list_monospace_families() -> &'static [String] {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let source = SystemSource::new();
            let all = match source.all_families() {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("[font_discovery] all_families failed: {e}");
                    return Vec::new();
                }
            };
            let props = Properties::default();
            let mut out: Vec<String> = all
                .into_iter()
                .filter(|name| {
                    let families = [FamilyName::Title(name.clone())];
                    let handle = match source.select_best_match(&families, &props) {
                        Ok(h) => h,
                        Err(_) => return false,
                    };
                    let font = match handle.load() {
                        Ok(f) => f,
                        Err(_) => return false,
                    };
                    font.is_monospace()
                })
                .collect();
            out.sort_unstable();
            out.dedup();
            out
        })
        .as_slice()
    }

#[cfg(target_os = "windows")]
const FALLBACK_BY_SCRIPT: &[(Script, &[&str])] = &[
    (Script::Han, &["Microsoft YaHei", "Yu Gothic", "Malgun Gothic", "SimSun"]),
    (Script::Kana, &["Yu Gothic", "Meiryo", "MS Gothic", "Microsoft YaHei"]),
    (Script::Hangul, &["Malgun Gothic", "Gulim", "Microsoft YaHei"]),
];

#[cfg(target_os = "macos")]
const FALLBACK_BY_SCRIPT: &[(Script, &[&str])] = &[
    (Script::Han, &["PingFang SC", "Hiragino Sans GB", "Hiragino Sans", "Apple SD Gothic Neo"]),
    (Script::Kana, &["Hiragino Sans", "Hiragino Kaku Gothic ProN", "PingFang SC"]),
    (Script::Hangul, &["Apple SD Gothic Neo", "PingFang SC"]),
];

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const FALLBACK_BY_SCRIPT: &[(Script, &[&str])] = &[
    (Script::Han, &[
        "Noto Sans CJK SC", "Noto Sans CJK JP", "Noto Sans CJK KR", "Noto Sans CJK TC",
        "Source Han Sans", "WenQuanYi Zen Hei", "WenQuanYi Micro Hei", "Droid Sans Fallback",
        "AR PL UMing CN", "AR PL New Sung",
    ]),
    (Script::Kana, &["Noto Sans CJK JP", "Noto Sans CJK SC", "Source Han Sans", "Droid Sans Fallback"]),
    (Script::Hangul, &["Noto Sans CJK KR", "Noto Sans CJK SC", "NanumGothic", "Droid Sans Fallback"]),
];

fn fallback_families(script: Script) -> &'static [&'static str] {
    FALLBACK_BY_SCRIPT
        .iter()
        .find(|(s, _)| *s == script)
        .map(|(_, families)| *families)
        .unwrap_or(&[])
}

fn fallback_search_order(script: Script, prefer_japanese: bool, prefer_korean: bool) -> Vec<&'static str> {
    let mut order: Vec<&'static str> = Vec::new();
    let mut push_all = |names: &'static [&'static str]| {
        for name in names {
            if !order.contains(name) {
                order.push(name);
            }
        }
    };
    if script == Script::Han {
        if prefer_japanese {
            push_all(fallback_families(Script::Kana));
        }
        if prefer_korean {
            push_all(fallback_families(Script::Hangul));
        }
    }
    push_all(fallback_families(script));
    order
}

/// Loads a system font covering `script`, trying families one at a time so a
/// missing family never resolves to the default sans. For Han, the Japanese-
/// or Korean-first families go ahead when the UI prefers those glyph shapes.
/// Returns the file bytes and the face index inside them (non-zero for `.ttc`).
pub fn discover_fallback_font(script: Script, prefer_japanese: bool, prefer_korean: bool) -> Option<(Vec<u8>, u32)> {
    let families: Vec<FamilyName> = fallback_search_order(script, prefer_japanese, prefer_korean)
        .into_iter()
        .map(|name| FamilyName::Title(name.to_string()))
        .collect();
    let found = try_families_individually(&families, &Properties::default());
    if found.is_none() {
        log::warn!("No fallback font for {:?} — its characters will not render", script);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn han_search_order_honours_preference() {
        let plain = fallback_search_order(Script::Han, false, false);
        assert_eq!(plain, fallback_families(Script::Han));

        let jp = fallback_search_order(Script::Han, true, false);
        let kana = fallback_families(Script::Kana);
        assert_eq!(&jp[..kana.len()], kana);
        assert_eq!(jp.len(), kana.len() + plain.iter().filter(|n| !kana.contains(n)).count());

        let ko = fallback_search_order(Script::Han, false, true);
        assert_eq!(ko[0], fallback_families(Script::Hangul)[0]);

        assert_eq!(fallback_search_order(Script::Kana, true, true), fallback_families(Script::Kana));
    }

    #[test]
    fn han_fallback_discovery_matches_installed_families() {
        let source = SystemSource::new();
        let installed: Vec<&str> = fallback_families(Script::Han)
            .iter()
            .copied()
            .filter(|name| source.select_family_by_name(name).is_ok())
            .collect();
        let found = discover_fallback_font(Script::Han, false, false);
        if installed.is_empty() {
            eprintln!("no Han fallback family installed; discovery returned {}", if found.is_some() { "Some" } else { "None" });
            return;
        }
        let (data, face_index) = found.unwrap_or_else(|| panic!("{installed:?} installed but discovery returned None"));
        let font = swash::FontRef::from_index(&data, face_index as usize).expect("loadable face");
        eprintln!("Han fallback: {} bytes, face {face_index}, candidates {installed:?}", data.len());
        assert_ne!(font.charmap().map('日'), 0, "face {face_index} does not cover U+65E5");
    }
}
