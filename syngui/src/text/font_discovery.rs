use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{Properties, Weight, Style};
use font_kit::source::SystemSource;

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
