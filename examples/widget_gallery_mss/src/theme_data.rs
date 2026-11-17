//! Оформления галереи. Каждое — не перекраска, а свой вид оболочки и
//! виджетов (`styles/designs/*.mss`: панели, форма пунктов, рамки, тени,
//! шрифт) и пара палитр, светлая и тёмная.
//!
//! - **Studio** — тёмная рамка (заголовок + боковая панель одной поверхностью),
//!   вкладки перетекают в скруглённую карточку содержимого; спокойный интерфейс.
//! - **Aurora** — стекло: плавающие полупрозрачные панели с размытием подложки
//!   над живым северным сиянием, пилюли, градиенты, свечение; шрифт Outfit.
//! - **Brutal** — необрутализм: толстые чёрные рамки, жёсткие тени без
//!   размытия, плоские яркие цвета, моноширинные подписи; Space Grotesk.

/// Палитра: значения переменных `:root` (без `--`).
pub struct GalleryTheme {
    pub id: &'static str,
    pub is_dark: bool,
    pub vars: &'static [(&'static str, &'static str)],
}

impl GalleryTheme {
    /// Блок `:root` со всеми переменными; `--bg` — по нему окно и снимок
    /// выбирают светлые или тёмные запасные цвета виджетов.
    pub fn to_mss(&self) -> String {
        let mut s = String::from(":root {\n");
        for (k, v) in self.vars {
            s.push_str(&format!("    --{k}: {v};\n"));
        }
        if let Some((_, bg)) = self.vars.iter().find(|(k, _)| *k == "bg-base") {
            s.push_str(&format!("    --bg: {bg};\n"));
        }
        s.push('}');
        s
    }
}

/// Оформление галереи.
pub struct Design {
    pub id: &'static str,
    /// Название для переключателя (одинаково на всех языках — имя стиля).
    pub name: &'static str,
    pub light: GalleryTheme,
    pub dark: GalleryTheme,
    /// Свои стили оболочки и виджетов поверх общих компонентных.
    pub mss: &'static str,
}

impl Design {
    pub fn palette(&self, dark: bool) -> &GalleryTheme {
        if dark { &self.dark } else { &self.light }
    }

    /// Полная таблица стилей: палитра, общие компоненты, стили оформления.
    pub fn full_mss(&self, dark: bool) -> String {
        format!("{}\n{}\n{}", self.palette(dark).to_mss(), crate::styles::component_styles(), self.mss)
    }
}

pub const DESIGNS: &[Design] = &[STUDIO, AURORA, BRUTAL];

pub fn find_design(id: &str) -> &'static Design {
    DESIGNS.iter().find(|d| d.id == id).unwrap_or(&DESIGNS[0])
}

/// Светлая палитра оформления по умолчанию (страницы, которым нужна таблица стилей сами по себе).
pub fn default_light() -> &'static GalleryTheme {
    &DESIGNS[0].light
}

// ─── Studio ─────────────────────────────────────────────────────────────

const STUDIO: Design = Design {
    id: "studio",
    name: "Studio",
    mss: include_str!("../styles/designs/studio.mss"),
    light: GalleryTheme {
        id: "studio-light",
        is_dark: false,
        vars: &[
            ("bg-base", "#f4f5f9"), ("bg-surface", "#ffffff"), ("bg-overlay", "#eceef5"), ("bg-elevated", "#e1e4ee"),
            ("text", "#151926"), ("text-subtle", "#4a5168"), ("text-muted", "#8a91a6"),
            ("accent", "#4f5bff"), ("accent-hover", "#3d48e8"), ("on-accent", "#ffffff"),
            ("success", "#12b886"), ("warning", "#f59f00"), ("error", "#f03e3e"), ("info", "#1c9fd8"),
            ("border", "#e1e4ec"), ("shadow-color", "rgba(21, 25, 38, 0.07)"),
            ("header-bg", "#161a28"), ("header-text", "#e7eaf4"), ("header-fill", "rgba(255, 255, 255, 0.08)"), ("header-line", "rgba(255, 255, 255, 0.14)"),
            ("sidebar-bg", "#161a28"), ("input-bg", "#ffffff"), ("section-hover", "#eff1f7"),
            ("button-bg", "#eceef5"), ("button-hover-text", "#ffffff"), ("button-pressed", "#dde1ec"),
            ("focus-ring", "rgba(79, 91, 255, 0.22)"),
            ("purple", "#8b5cf6"), ("pink", "#ec4899"), ("orange", "#fb7a1c"), ("teal", "#14b8a6"), ("indigo", "#6366f1"),
            ("blue-muted", "#e4e7ff"), ("green-muted", "#d8f6ea"), ("amber-muted", "#fff0cc"), ("purple-muted", "#eee6ff"), ("red-muted", "#ffe2e2"),
            ("chart-grid", "#e7eaf1"), ("tooltip-bg", "#161a28"), ("tooltip-border", "#2a3044"), ("chart-shadow", "rgba(21, 25, 38, 0.08)"),
            ("glass-bg", "rgba(255, 255, 255, 0.6)"), ("glass-border", "rgba(255, 255, 255, 0.8)"),
            ("glass-dark-bg", "rgba(22, 26, 40, 0.5)"), ("glass-dark-border", "rgba(255, 255, 255, 0.1)"),
            ("fx-shadow", "rgba(21, 25, 38, 0.12)"), ("fx-shadow-md", "rgba(21, 25, 38, 0.18)"), ("fx-shadow-lg", "rgba(21, 25, 38, 0.26)"),
            ("glow-1", "#4f5bff"), ("glow-2", "#ec4899"), ("glow-3", "#14b8a6"),
            ("ink", "#151926"), ("hard-shadow", "#151926"),
        ],
    },
    dark: GalleryTheme {
        id: "studio-dark",
        is_dark: true,
        vars: &[
            ("bg-base", "#14161d"), ("bg-surface", "#1b1e27"), ("bg-overlay", "#252935"), ("bg-elevated", "#2f3442"),
            ("text", "#e8eaf2"), ("text-subtle", "#a9aec0"), ("text-muted", "#6c7286"),
            ("accent", "#7b86ff"), ("accent-hover", "#959dff"), ("on-accent", "#0b0c11"),
            ("success", "#38d9a9"), ("warning", "#fcc419"), ("error", "#ff6b6b"), ("info", "#4dabf7"),
            ("border", "#2a2e3a"), ("shadow-color", "rgba(0, 0, 0, 0.45)"),
            ("header-bg", "#0b0c11"), ("header-text", "#d7dbe8"), ("header-fill", "rgba(255, 255, 255, 0.07)"), ("header-line", "rgba(255, 255, 255, 0.12)"),
            ("sidebar-bg", "#0b0c11"), ("input-bg", "#12141a"), ("section-hover", "#222631"),
            ("button-bg", "#252935"), ("button-hover-text", "#0b0c11"), ("button-pressed", "#343a4a"),
            ("focus-ring", "rgba(123, 134, 255, 0.3)"),
            ("purple", "#b197fc"), ("pink", "#f783ac"), ("orange", "#ffa94d"), ("teal", "#63e6be"), ("indigo", "#91a7ff"),
            ("blue-muted", "#1f2550"), ("green-muted", "#0f3a2e"), ("amber-muted", "#3d2e0a"), ("purple-muted", "#2b1f4d"), ("red-muted", "#40191c"),
            ("chart-grid", "rgba(255, 255, 255, 0.07)"), ("tooltip-bg", "#0b0c11"), ("tooltip-border", "#2a2e3a"), ("chart-shadow", "rgba(0, 0, 0, 0.3)"),
            ("glass-bg", "rgba(255, 255, 255, 0.06)"), ("glass-border", "rgba(255, 255, 255, 0.1)"),
            ("glass-dark-bg", "rgba(0, 0, 0, 0.35)"), ("glass-dark-border", "rgba(255, 255, 255, 0.06)"),
            ("fx-shadow", "rgba(0, 0, 0, 0.4)"), ("fx-shadow-md", "rgba(0, 0, 0, 0.5)"), ("fx-shadow-lg", "rgba(0, 0, 0, 0.6)"),
            ("glow-1", "#7b86ff"), ("glow-2", "#f783ac"), ("glow-3", "#63e6be"),
            ("ink", "#e8eaf2"), ("hard-shadow", "#000000"),
        ],
    },
};

// ─── Aurora ─────────────────────────────────────────────────────────────

const AURORA: Design = Design {
    id: "aurora",
    name: "Aurora",
    mss: include_str!("../styles/designs/aurora.mss"),
    light: GalleryTheme {
        id: "aurora-light",
        is_dark: false,
        vars: &[
            ("bg-base", "#f3f0ff"), ("bg-surface", "#ffffff"), ("bg-overlay", "rgba(124, 58, 237, 0.08)"), ("bg-elevated", "rgba(124, 58, 237, 0.15)"),
            ("text", "#1d1340"), ("text-subtle", "#564c80"), ("text-muted", "#958dba"),
            ("accent", "#7c3aed"), ("accent-hover", "#6d28d9"), ("on-accent", "#ffffff"),
            ("success", "#059669"), ("warning", "#d97706"), ("error", "#e11d48"), ("info", "#0891b2"),
            ("border", "rgba(60, 30, 140, 0.12)"), ("shadow-color", "rgba(76, 29, 149, 0.10)"),
            ("header-bg", "rgba(255, 255, 255, 0.5)"), ("header-text", "#1d1340"), ("header-fill", "rgba(255, 255, 255, 0.6)"), ("header-line", "rgba(60, 30, 140, 0.12)"),
            ("sidebar-bg", "rgba(255, 255, 255, 0.45)"), ("input-bg", "rgba(255, 255, 255, 0.75)"), ("section-hover", "rgba(124, 58, 237, 0.06)"),
            ("button-bg", "rgba(255, 255, 255, 0.7)"), ("button-hover-text", "#ffffff"), ("button-pressed", "rgba(124, 58, 237, 0.22)"),
            ("focus-ring", "rgba(124, 58, 237, 0.25)"),
            ("purple", "#8b5cf6"), ("pink", "#ec4899"), ("orange", "#f97316"), ("teal", "#14b8a6"), ("indigo", "#6366f1"),
            ("blue-muted", "#dbeafe"), ("green-muted", "#d1fae5"), ("amber-muted", "#fef3c7"), ("purple-muted", "#ede9fe"), ("red-muted", "#ffe4e6"),
            ("chart-grid", "rgba(60, 30, 140, 0.08)"), ("tooltip-bg", "#1d1340"), ("tooltip-border", "#3b2d6b"), ("chart-shadow", "rgba(76, 29, 149, 0.1)"),
            ("glass-bg", "rgba(255, 255, 255, 0.55)"), ("glass-border", "rgba(255, 255, 255, 0.85)"),
            ("glass-dark-bg", "rgba(29, 19, 64, 0.35)"), ("glass-dark-border", "rgba(255, 255, 255, 0.2)"),
            ("fx-shadow", "rgba(76, 29, 149, 0.14)"), ("fx-shadow-md", "rgba(76, 29, 149, 0.2)"), ("fx-shadow-lg", "rgba(76, 29, 149, 0.28)"),
            ("glow-1", "#c4b5fd"), ("glow-2", "#f9a8d4"), ("glow-3", "#7dd3fc"),
            ("ink", "#1d1340"), ("hard-shadow", "#1d1340"),
        ],
    },
    dark: GalleryTheme {
        id: "aurora-dark",
        is_dark: true,
        vars: &[
            ("bg-base", "#0d0a20"), ("bg-surface", "#1a1538"), ("bg-overlay", "rgba(255, 255, 255, 0.08)"), ("bg-elevated", "rgba(255, 255, 255, 0.14)"),
            ("text", "#f2eeff"), ("text-subtle", "#bdb4e8"), ("text-muted", "#7f76ad"),
            ("accent", "#a78bfa"), ("accent-hover", "#c4b5fd"), ("on-accent", "#ffffff"),
            ("success", "#34d399"), ("warning", "#fbbf24"), ("error", "#fb7185"), ("info", "#22d3ee"),
            ("border", "rgba(255, 255, 255, 0.12)"), ("shadow-color", "rgba(5, 2, 20, 0.45)"),
            ("header-bg", "rgba(255, 255, 255, 0.06)"), ("header-text", "#f2eeff"), ("header-fill", "rgba(255, 255, 255, 0.08)"), ("header-line", "rgba(255, 255, 255, 0.14)"),
            ("sidebar-bg", "rgba(255, 255, 255, 0.05)"), ("input-bg", "rgba(255, 255, 255, 0.06)"), ("section-hover", "rgba(255, 255, 255, 0.08)"),
            ("button-bg", "rgba(255, 255, 255, 0.09)"), ("button-hover-text", "#ffffff"), ("button-pressed", "rgba(139, 92, 246, 0.5)"),
            ("focus-ring", "rgba(167, 139, 250, 0.35)"),
            ("purple", "#a78bfa"), ("pink", "#f472b6"), ("orange", "#fb923c"), ("teal", "#2dd4bf"), ("indigo", "#818cf8"),
            ("blue-muted", "rgba(59, 130, 246, 0.2)"), ("green-muted", "rgba(16, 185, 129, 0.2)"), ("amber-muted", "rgba(245, 158, 11, 0.2)"), ("purple-muted", "rgba(139, 92, 246, 0.22)"), ("red-muted", "rgba(244, 63, 94, 0.2)"),
            ("chart-grid", "rgba(255, 255, 255, 0.08)"), ("tooltip-bg", "rgba(20, 14, 48, 0.94)"), ("tooltip-border", "rgba(255, 255, 255, 0.14)"), ("chart-shadow", "rgba(0, 0, 0, 0.3)"),
            ("glass-bg", "rgba(255, 255, 255, 0.07)"), ("glass-border", "rgba(255, 255, 255, 0.14)"),
            ("glass-dark-bg", "rgba(0, 0, 0, 0.25)"), ("glass-dark-border", "rgba(255, 255, 255, 0.08)"),
            ("fx-shadow", "rgba(10, 4, 40, 0.4)"), ("fx-shadow-md", "rgba(10, 4, 40, 0.5)"), ("fx-shadow-lg", "rgba(10, 4, 40, 0.65)"),
            ("glow-1", "#7c3aed"), ("glow-2", "#db2777"), ("glow-3", "#0891b2"),
            ("ink", "#f2eeff"), ("hard-shadow", "#000000"),
        ],
    },
};

// ─── Brutal ─────────────────────────────────────────────────────────────

const BRUTAL: Design = Design {
    id: "brutal",
    name: "Brutal",
    mss: include_str!("../styles/designs/brutal.mss"),
    light: GalleryTheme {
        id: "brutal-light",
        is_dark: false,
        vars: &[
            ("bg-base", "#fff3d6"), ("bg-surface", "#ffffff"), ("bg-overlay", "#ffeaa8"), ("bg-elevated", "#ffd84d"),
            ("text", "#0f0f0f"), ("text-subtle", "#2b2b2b"), ("text-muted", "#6b6b6b"),
            ("accent", "#2f5bff"), ("accent-hover", "#1f45e0"), ("on-accent", "#ffffff"),
            ("success", "#00c27a"), ("warning", "#ffb800"), ("error", "#ff3b30"), ("info", "#00a8f0"),
            ("border", "#0f0f0f"), ("shadow-color", "rgba(15, 15, 15, 0.0)"),
            ("header-bg", "#0f0f0f"), ("header-text", "#fff3d6"), ("header-fill", "#fff3d6"), ("header-line", "#fff3d6"),
            ("sidebar-bg", "#ffd23f"), ("input-bg", "#ffffff"), ("section-hover", "#fff8e6"),
            ("button-bg", "#ffffff"), ("button-hover-text", "#0f0f0f"), ("button-pressed", "#ffd23f"),
            ("focus-ring", "rgba(47, 91, 255, 0.35)"),
            ("purple", "#9b5cff"), ("pink", "#ff6fb5"), ("orange", "#ff8a00"), ("teal", "#00c2a8"), ("indigo", "#4c4cff"),
            ("blue-muted", "#cfdcff"), ("green-muted", "#c6f7e2"), ("amber-muted", "#ffe7a3"), ("purple-muted", "#e6d8ff"), ("red-muted", "#ffd4d0"),
            ("chart-grid", "rgba(15, 15, 15, 0.15)"), ("tooltip-bg", "#0f0f0f"), ("tooltip-border", "#0f0f0f"), ("chart-shadow", "rgba(0, 0, 0, 0)"),
            ("glass-bg", "rgba(255, 255, 255, 0.75)"), ("glass-border", "#0f0f0f"),
            ("glass-dark-bg", "rgba(15, 15, 15, 0.6)"), ("glass-dark-border", "#0f0f0f"),
            ("fx-shadow", "rgba(15, 15, 15, 0.5)"), ("fx-shadow-md", "rgba(15, 15, 15, 0.7)"), ("fx-shadow-lg", "rgba(15, 15, 15, 0.9)"),
            ("glow-1", "#ffd23f"), ("glow-2", "#ff6fb5"), ("glow-3", "#2f5bff"),
            ("ink", "#0f0f0f"), ("hard-shadow", "#0f0f0f"),
        ],
    },
    dark: GalleryTheme {
        id: "brutal-dark",
        is_dark: true,
        vars: &[
            ("bg-base", "#151515"), ("bg-surface", "#202020"), ("bg-overlay", "#2b2b2b"), ("bg-elevated", "#3a3a3a"),
            ("text", "#f4f1e8"), ("text-subtle", "#d2cdc0"), ("text-muted", "#8f8a80"),
            ("accent", "#b8ff2c"), ("accent-hover", "#d0ff6e"), ("on-accent", "#111111"),
            ("success", "#3dff9a"), ("warning", "#ffd23f"), ("error", "#ff5470"), ("info", "#4dd2ff"),
            ("border", "#f4f1e8"), ("shadow-color", "rgba(0, 0, 0, 0.0)"),
            ("header-bg", "#b8ff2c"), ("header-text", "#111111"), ("header-fill", "#111111"), ("header-line", "#111111"),
            ("sidebar-bg", "#202020"), ("input-bg", "#151515"), ("section-hover", "#262626"),
            ("button-bg", "#202020"), ("button-hover-text", "#111111"), ("button-pressed", "#b8ff2c"),
            ("focus-ring", "rgba(184, 255, 44, 0.35)"),
            ("purple", "#b388ff"), ("pink", "#ff7ac6"), ("orange", "#ff9f1a"), ("teal", "#2ee6c5"), ("indigo", "#8c9eff"),
            ("blue-muted", "#1d2b55"), ("green-muted", "#133d2a"), ("amber-muted", "#3d3210"), ("purple-muted", "#2d1f4a"), ("red-muted", "#4a1820"),
            ("chart-grid", "rgba(244, 241, 232, 0.15)"), ("tooltip-bg", "#111111"), ("tooltip-border", "#f4f1e8"), ("chart-shadow", "rgba(0, 0, 0, 0)"),
            ("glass-bg", "rgba(255, 255, 255, 0.08)"), ("glass-border", "#f4f1e8"),
            ("glass-dark-bg", "rgba(0, 0, 0, 0.5)"), ("glass-dark-border", "#f4f1e8"),
            ("fx-shadow", "rgba(0, 0, 0, 0.6)"), ("fx-shadow-md", "rgba(0, 0, 0, 0.7)"), ("fx-shadow-lg", "rgba(0, 0, 0, 0.85)"),
            ("glow-1", "#b8ff2c"), ("glow-2", "#ff7ac6"), ("glow-3", "#4dd2ff"),
            ("ink", "#f4f1e8"), ("hard-shadow", "#b8ff2c"),
        ],
    },
};
