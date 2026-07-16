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
