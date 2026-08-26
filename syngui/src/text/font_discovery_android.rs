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
    (Script::Han, &[
        ("/system/fonts/NotoSansCJK-Regular.ttc", 2),
        ("/system/fonts/NotoSansSC-Regular.otf", 0),
        ("/system/fonts/DroidSansFallback.ttf", 0),
    ]),
    (Script::Kana, &[
        ("/system/fonts/NotoSansCJK-Regular.ttc", 0),
        ("/system/fonts/NotoSansJP-Regular.otf", 0),
        ("/system/fonts/DroidSansFallback.ttf", 0),
    ]),
    (Script::Hangul, &[
        ("/system/fonts/NotoSansCJK-Regular.ttc", 1),
        ("/system/fonts/NotoSansKR-Regular.otf", 0),
        ("/system/fonts/DroidSansFallback.ttf", 0),
    ]),
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
pub fn discover_fallback_font(script: Script, prefer_japanese: bool, prefer_korean: bool) -> Option<(Vec<u8>, u32)> {
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
    log::warn!("No Android fallback font for {:?} — its characters will not render", script);
    None
}
