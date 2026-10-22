#[cfg(feature = "material-icons")]
pub mod material {
    pub const FONT_DATA: &[u8] = include_bytes!("../../assets/fonts/MaterialIcons-Regular.ttf");

    pub const FAMILY_NAME: &str = "Material Icons";
}

#[cfg(feature = "font-awesome")]
pub mod awesome {
    pub const SOLID_FONT_DATA: &[u8] = include_bytes!("../../assets/fonts/FontAwesome6-Solid.otf");

    pub const REGULAR_FONT_DATA: &[u8] =
        include_bytes!("../../assets/fonts/FontAwesome6-Regular.otf");

    pub const BRANDS_FONT_DATA: &[u8] =
        include_bytes!("../../assets/fonts/FontAwesome6-Brands.otf");

    pub const SOLID_FAMILY_NAME: &str = "Font Awesome 6 Free Solid";
    pub const REGULAR_FAMILY_NAME: &str = "Font Awesome 6 Free";
    pub const BRANDS_FAMILY_NAME: &str = "Font Awesome 6 Brands";
}

/// Font Awesome рядом с Material Icons: у обоих значки в одной области PUA
/// (U+E000…F8FF), поэтому значки FA адресуются в дополнительных областях —
/// Solid: `U+F0000 + код`, Regular: `U+100000 + код`, Brands: `U+100000 + (код − 0xE000)`.
/// Атлас переводит их обратно и берёт глиф из нужного начертания FA
/// (`App::with_font_awesome()`). Пример: `fa::solid(0xF015)` — «дом».
pub mod fa {
    pub const SOLID_BASE: u32 = 0xF0000;
    pub const REGULAR_BASE: u32 = 0x100000;
    pub const BRANDS_BASE: u32 = 0x100000;

    fn ch(cp: u32) -> String {
        char::from_u32(cp).map(String::from).unwrap_or_default()
    }

    /// Значок FA Solid по коду из каталога Font Awesome (`0xF015`).
    pub fn solid(code: u32) -> String {
        ch(SOLID_BASE + code)
    }

    /// Значок FA Regular (контурный).
    pub fn regular(code: u32) -> String {
        ch(REGULAR_BASE + code)
    }

    /// Значок FA Brands (логотипы: github 0xF09B, linux 0xF17C…).
    pub fn brands(code: u32) -> String {
        ch(BRANDS_BASE + code.saturating_sub(0xE000))
    }

    /// Начертание и исходный код FA для символа атласа: 0 — Solid, 1 — Regular, 2 — Brands.
    pub fn decode(ch: char) -> Option<(usize, char)> {
        let c = ch as u32;
        let (style, code) = match c {
            0xF0000..=0xFFFFD => (0, c - SOLID_BASE),
            0x10E000..=0x10F8FF => (1, c - REGULAR_BASE),
            0x100000..=0x1018FF => (2, c - BRANDS_BASE + 0xE000),
            _ => return None,
        };
        char::from_u32(code).map(|ch| (style, ch))
    }
}
