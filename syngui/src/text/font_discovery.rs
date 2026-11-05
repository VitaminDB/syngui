use crate::text::script::Script;
use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Style, Weight};
use font_kit::source::SystemSource;

#[cfg(target_os = "windows")]
const FALLBACK_FAMILIES: &[&str] = &[
    "Segoe UI",
    "Arial",
    "Tahoma",
    "Verdana",
    "Microsoft Sans Serif",
    "Calibri",
    "Trebuchet MS",
    "Lucida Sans Unicode",
];

#[cfg(target_vendor = "apple")]
const FALLBACK_FAMILIES: &[&str] = &[
    "Helvetica Neue",
    "Helvetica",
    "Arial",
    "Lucida Grande",
    "SF Pro",
    "San Francisco",
];

#[cfg(not(any(target_os = "windows", target_vendor = "apple")))]
const FALLBACK_FAMILIES: &[&str] = &[
    "DejaVu Sans",
    "Liberation Sans",
    "Noto Sans",
    "Ubuntu",
    "Cantarell",
    "Droid Sans",
    "FreeSans",
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

/// Начертание заданного веса (500 Medium, 600 SemiBold…). Подбор font-kit
/// идёт по правилам CSS: нет точного веса — вернётся ближайший (для 500 —
/// обычный, для 600 — жирный); атлас такое совпадение распознаёт сам.
pub fn discover_weight_font(preferred_family: Option<&str>, weight: u16) -> (Vec<u8>, u32) {
    let families = build_families(preferred_family);
    let props = Properties {
        weight: Weight(weight as f32),
        style: Style::Normal,
        ..Properties::default()
    };
    load_font_bytes(&families, &props).unwrap_or_default()
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

/// Системные шрифты доступны: на Linux fontconfig подгружается через dlopen — его может не быть (сервер,
/// контейнер, минимальная система); тогда программа работает без системных шрифтов, а не падает в font-kit.
pub fn system_fonts_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        static WARNED: std::sync::Once = std::sync::Once::new();
        let ok = fontconfig_sys::statics::LIB_RESULT.is_ok();
        if !ok {
            WARNED.call_once(|| log::warn!("[font_discovery] libfontconfig.so.1 not found — no system fonts"));
        }
        ok
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

#[cfg(not(target_os = "ios"))]
fn load_font_bytes(families: &[FamilyName], props: &Properties) -> Option<(Vec<u8>, u32)> {
    if !system_fonts_available() {
        return None;
    }
    let source = SystemSource::new();
    let handle = source.select_best_match(families, props).ok()?;
    handle_to_bytes(handle)
}

/// iOS: `select_best_match` font-kit читает у каждого начертания признаки
/// CoreText (`Font::properties`), а у части системных шрифтов iOS 26 нет
/// наклона — `core-text` падает на `unwrap()`. Здесь семейство только
/// перечисляется (`select_family_by_name` даёт байты файлов), вес и наклон
/// читаются из самого шрифта (swash). Паника в font-kit/core-text не роняет
/// приложение: шрифт просто считается ненайденным.
#[cfg(target_os = "ios")]
fn load_font_bytes(families: &[FamilyName], props: &Properties) -> Option<(Vec<u8>, u32)> {
    let families = families.to_vec();
    let props = *props;
    std::panic::catch_unwind(move || ios_best_match(&families, &props))
        .unwrap_or_else(|_| {
            log::warn!("[font_discovery] подбор шрифта CoreText упал — шрифт пропущен");
            None
        })
}

#[cfg(target_os = "ios")]
fn ios_best_match(families: &[FamilyName], props: &Properties) -> Option<(Vec<u8>, u32)> {
    let source = SystemSource::new();
    let target = props.weight.0;
    let want_italic = props.style != Style::Normal;
    for family in families {
        let name = match family {
            FamilyName::Title(n) => n.as_str(),
            FamilyName::SansSerif => "Helvetica Neue",
            FamilyName::Serif => "Times New Roman",
            FamilyName::Monospace => "Menlo",
            _ => continue,
        };
        let Ok(handles) = source.select_family_by_name(name) else {
            continue;
        };
        let mut best: Option<(f32, &Handle)> = None;
        for handle in handles.fonts() {
            let Handle::Memory { bytes, font_index } = handle else {
                continue;
            };
            let Some(font) = swash::FontRef::from_index(bytes, *font_index as usize) else {
                continue;
            };
            let attrs = font.attributes();
            let weight = attrs.weight().0 as f32;
            let italic = attrs.style() != swash::Style::Normal;
            // CSS-подбор упрощённо: сначала наклон, потом ближайший вес; при
            // равенстве для жирных целей — тяжелее, для обычных — легче.
            let lean = if (weight > target) == (target >= 500.0) { 0.0 } else { 0.5 };
            let score = if italic != want_italic { 10_000.0 } else { 0.0 } + (weight - target).abs() + lean;
            if best.is_none_or(|(b, _)| score < b) {
                best = Some((score, handle));
            }
        }
        if let Some((_, handle)) = best {
            return handle_to_bytes(handle.clone());
        }
    }
    None
}

fn try_families_individually(
    families: &[FamilyName],
    props: &Properties,
) -> Option<(Vec<u8>, u32)> {
    families
        .iter()
        .find_map(|family| load_font_bytes(std::slice::from_ref(family), props))
}

fn handle_to_bytes(handle: Handle) -> Option<(Vec<u8>, u32)> {
    match handle {
        Handle::Path {
            ref path,
            font_index,
        } => std::fs::read(path).ok().map(|data| (data, font_index)),
        Handle::Memory { bytes, font_index } => Some(((*bytes).clone(), font_index)),
    }
}

pub fn list_monospace_families() -> &'static [String] {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            // iOS: перебор всех семейств через select_best_match упирается в
            // ту же поломку признаков CoreText, что и load_font_bytes.
            if cfg!(target_os = "ios") || !system_fonts_available() {
                return Vec::new();
            }
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
    (
        Script::Han,
        &["Microsoft YaHei", "Yu Gothic", "Malgun Gothic", "SimSun"],
    ),
    (
        Script::Kana,
        &["Yu Gothic", "Meiryo", "MS Gothic", "Microsoft YaHei"],
    ),
    (
        Script::Hangul,
        &["Malgun Gothic", "Gulim", "Microsoft YaHei"],
    ),
];

#[cfg(target_vendor = "apple")]
const FALLBACK_BY_SCRIPT: &[(Script, &[&str])] = &[
    (
        Script::Han,
        &[
            "PingFang SC",
            "Hiragino Sans GB",
            "Hiragino Sans",
            "Apple SD Gothic Neo",
        ],
    ),
    (
        Script::Kana,
        &["Hiragino Sans", "Hiragino Kaku Gothic ProN", "PingFang SC"],
    ),
    (Script::Hangul, &["Apple SD Gothic Neo", "PingFang SC"]),
];

#[cfg(not(any(target_os = "windows", target_vendor = "apple")))]
const FALLBACK_BY_SCRIPT: &[(Script, &[&str])] = &[
    (
        Script::Han,
        &[
            "Noto Sans CJK SC",
            "Noto Sans CJK JP",
            "Noto Sans CJK KR",
            "Noto Sans CJK TC",
            "Source Han Sans",
            "WenQuanYi Zen Hei",
            "WenQuanYi Micro Hei",
            "Droid Sans Fallback",
            "AR PL UMing CN",
            "AR PL New Sung",
        ],
    ),
    (
        Script::Kana,
        &[
            "Noto Sans CJK JP",
            "Noto Sans CJK SC",
            "Source Han Sans",
            "Droid Sans Fallback",
        ],
    ),
    (
        Script::Hangul,
        &[
            "Noto Sans CJK KR",
            "Noto Sans CJK SC",
            "NanumGothic",
            "Droid Sans Fallback",
        ],
    ),
];

/// Запасные семейства для символов вне основного шрифта (≤, →, ✓, ∑, рамки…),
/// по убыванию охвата. Перебираются по одному, пока не найдётся глиф.
pub const SYMBOL_FAMILIES: &[&str] = &[
    "DejaVu Sans",
    "Noto Sans Math",
    "Noto Sans Symbols 2",
    "Noto Sans Symbols",
    "FreeSerif",
    "Symbola",
];

/// Шрифт семейства с нужной насыщенностью и наклоном (без подмены на
/// «похожее» семейство): переменный шрифт отдаётся тем же файлом — ось
/// `wght` выставляет тот, кто растеризует.
pub fn discover_styled(name: &str, weight: u16, italic: bool) -> Option<(Vec<u8>, u32)> {
    let props = Properties {
        weight: Weight(weight as f32),
        style: if italic { Style::Italic } else { Style::Normal },
        ..Properties::default()
    };
    try_families_individually(&[FamilyName::Title(name.to_string())], &props)
}

/// Все семейства шрифтов системы, по алфавиту.
pub fn list_families() -> Vec<String> {
    if !system_fonts_available() {
        return Vec::new();
    }
    let mut v = SystemSource::new().all_families().unwrap_or_default();
    v.sort_by_key(|a| a.to_lowercase());
    v.dedup();
    v
}

/// Шрифт одного семейства (без подмены на «похожее»).
pub fn discover_family_font(name: &str) -> Option<(Vec<u8>, u32)> {
    try_families_individually(&[FamilyName::Title(name.to_string())], &Properties::default())
}

fn fallback_families(script: Script) -> &'static [&'static str] {
    FALLBACK_BY_SCRIPT
        .iter()
        .find(|(s, _)| *s == script)
        .map(|(_, families)| *families)
        .unwrap_or(&[])
}

fn fallback_search_order(
    script: Script,
    prefer_japanese: bool,
    prefer_korean: bool,
) -> Vec<&'static str> {
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
pub fn discover_fallback_font(
    script: Script,
    prefer_japanese: bool,
    prefer_korean: bool,
) -> Option<(Vec<u8>, u32)> {
    let families: Vec<FamilyName> = fallback_search_order(script, prefer_japanese, prefer_korean)
        .into_iter()
        .map(|name| FamilyName::Title(name.to_string()))
        .collect();
    let found = try_families_individually(&families, &Properties::default());
    if found.is_none() {
        log::warn!(
            "No fallback font for {:?} — its characters will not render",
            script
        );
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
        assert_eq!(
            jp.len(),
            kana.len() + plain.iter().filter(|n| !kana.contains(n)).count()
        );

        let ko = fallback_search_order(Script::Han, false, true);
        assert_eq!(ko[0], fallback_families(Script::Hangul)[0]);

        assert_eq!(
            fallback_search_order(Script::Kana, true, true),
            fallback_families(Script::Kana)
        );
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
            eprintln!(
                "no Han fallback family installed; discovery returned {}",
                if found.is_some() { "Some" } else { "None" }
            );
            return;
        }
        let (data, face_index) =
            found.unwrap_or_else(|| panic!("{installed:?} installed but discovery returned None"));
        let font = swash::FontRef::from_index(&data, face_index as usize).expect("loadable face");
        eprintln!(
            "Han fallback: {} bytes, face {face_index}, candidates {installed:?}",
            data.len()
        );
        assert_ne!(
            font.charmap().map('日'),
            0,
            "face {face_index} does not cover U+65E5"
        );
    }
}

#[cfg(test)]
mod weight_tests {
    /// На машине с системным шрифтом, где есть Medium/SemiBold, подбор по весу
    /// даёт отдельные файлы; где их нет — совпадёт с обычным/жирным (атлас это
    /// распознаёт). Печатает размеры, не падает без шрифтов.
    #[test]
    fn weight_faces_resolve() {
        let (reg, _) = super::discover_font(None);
        let (bold, _) = super::discover_bold_font(None);
        let (w500, _) = super::discover_weight_font(None, 500);
        let (w600, _) = super::discover_weight_font(None, 600);
        eprintln!(
            "regular {} bold {} 500 {} 600 {}",
            reg.len(),
            bold.len(),
            w500.len(),
            w600.len()
        );
        if !reg.is_empty() && !w600.is_empty() {
            assert!(w600 != reg, "600 не должен падать на обычное начертание");
        }
    }
}
