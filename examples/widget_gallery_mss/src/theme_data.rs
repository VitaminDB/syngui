/// Built-in gallery themes (5 light + 5 dark).

pub struct GalleryTheme {
    pub id: &'static str,
    pub name: &'static str,
    pub is_dark: bool,

    // Core backgrounds
    pub bg_base: &'static str,
    pub bg_surface: &'static str,
    pub bg_overlay: &'static str,
    pub bg_elevated: &'static str,

    // Text
    pub text: &'static str,
    pub text_subtle: &'static str,
    pub text_muted: &'static str,

    // Accent
    pub accent: &'static str,
    pub accent_hover: &'static str,
    pub success: &'static str,
    pub warning: &'static str,
    pub error: &'static str,
    pub info: &'static str,

    // Border & shadow
    pub border: &'static str,
    pub shadow_color: &'static str,

    // Layout
    pub header_bg: &'static str,
    pub header_text: &'static str,
    pub sidebar_bg: &'static str,
    pub input_bg: &'static str,
    pub section_hover: &'static str,
    pub button_bg: &'static str,
    pub button_hover_text: &'static str,
    pub button_pressed: &'static str,
    pub focus_ring: &'static str,

    // Palette (vivid)
    pub purple: &'static str,
    pub pink: &'static str,
    pub orange: &'static str,
    pub teal: &'static str,
    pub indigo: &'static str,

    // Palette (muted backgrounds)
    pub blue_muted: &'static str,
    pub green_muted: &'static str,
    pub amber_muted: &'static str,
    pub purple_muted: &'static str,
    pub red_muted: &'static str,

    // Charts
    pub chart_grid: &'static str,
    pub tooltip_bg: &'static str,
    pub tooltip_border: &'static str,
    pub chart_shadow: &'static str,

    // Glass
    pub glass_bg: &'static str,
    pub glass_border: &'static str,
    pub glass_dark_bg: &'static str,
    pub glass_dark_border: &'static str,

    // Effects
    pub fx_shadow: &'static str,
    pub fx_shadow_md: &'static str,
    pub fx_shadow_lg: &'static str,
}

impl GalleryTheme {
    /// Generate MSS :root block with all CSS variables.
    pub fn to_mss(&self) -> String {
        format!(
            r#":root {{
    --bg-base: {bg_base};
    --bg-surface: {bg_surface};
    --bg-overlay: {bg_overlay};
    --bg-elevated: {bg_elevated};

    --text: {text};
    --text-subtle: {text_subtle};
    --text-muted: {text_muted};

    --accent: {accent};
    --accent-hover: {accent_hover};
    --success: {success};
    --warning: {warning};
    --error: {error};
    --info: {info};

    --border: {border};
    --shadow-color: {shadow_color};

    --header-bg: {header_bg};
    --header-text: {header_text};
    --sidebar-bg: {sidebar_bg};
    --input-bg: {input_bg};
    --section-hover: {section_hover};
    --button-bg: {button_bg};
    --button-hover-text: {button_hover_text};
    --button-pressed: {button_pressed};
    --focus-ring: {focus_ring};

    --purple: {purple};
    --pink: {pink};
    --orange: {orange};
    --teal: {teal};
    --indigo: {indigo};

    --blue-muted: {blue_muted};
    --green-muted: {green_muted};
    --amber-muted: {amber_muted};
    --purple-muted: {purple_muted};
    --red-muted: {red_muted};

    --chart-grid: {chart_grid};
    --tooltip-bg: {tooltip_bg};
    --tooltip-border: {tooltip_border};
    --chart-shadow: {chart_shadow};

    --glass-bg: {glass_bg};
    --glass-border: {glass_border};
    --glass-dark-bg: {glass_dark_bg};
    --glass-dark-border: {glass_dark_border};

    --fx-shadow: {fx_shadow};
    --fx-shadow-md: {fx_shadow_md};
    --fx-shadow-lg: {fx_shadow_lg};
}}"#,
            bg_base = self.bg_base,
            bg_surface = self.bg_surface,
            bg_overlay = self.bg_overlay,
            bg_elevated = self.bg_elevated,
            text = self.text,
            text_subtle = self.text_subtle,
            text_muted = self.text_muted,
            accent = self.accent,
            accent_hover = self.accent_hover,
            success = self.success,
            warning = self.warning,
            error = self.error,
            info = self.info,
            border = self.border,
            shadow_color = self.shadow_color,
            header_bg = self.header_bg,
            header_text = self.header_text,
            sidebar_bg = self.sidebar_bg,
            input_bg = self.input_bg,
            section_hover = self.section_hover,
            button_bg = self.button_bg,
            button_hover_text = self.button_hover_text,
            button_pressed = self.button_pressed,
            focus_ring = self.focus_ring,
            purple = self.purple,
            pink = self.pink,
            orange = self.orange,
            teal = self.teal,
            indigo = self.indigo,
            blue_muted = self.blue_muted,
            green_muted = self.green_muted,
            amber_muted = self.amber_muted,
            purple_muted = self.purple_muted,
            red_muted = self.red_muted,
            chart_grid = self.chart_grid,
            tooltip_bg = self.tooltip_bg,
            tooltip_border = self.tooltip_border,
            chart_shadow = self.chart_shadow,
            glass_bg = self.glass_bg,
            glass_border = self.glass_border,
            glass_dark_bg = self.glass_dark_bg,
            glass_dark_border = self.glass_dark_border,
            fx_shadow = self.fx_shadow,
            fx_shadow_md = self.fx_shadow_md,
            fx_shadow_lg = self.fx_shadow_lg,
        )
    }
}

pub fn builtin_themes() -> Vec<GalleryTheme> {
    vec![
        clean_modern(),
        ocean_blue(),
        rose_garden(),
        emerald_mint(),
        warm_amber(),
        catppuccin_macchiato(),
        nord(),
        dracula(),
        monokai(),
        tokyo_night(),
    ]
}

pub fn find_theme(id: &str) -> Option<GalleryTheme> {
    builtin_themes().into_iter().find(|t| t.id == id)
}

pub fn find_theme_by_name(name: &str) -> Option<GalleryTheme> {
    builtin_themes().into_iter().find(|t| t.name == name)
}

pub fn default_light() -> GalleryTheme {
    clean_modern()
}

pub fn default_dark() -> GalleryTheme {
    catppuccin_macchiato()
}

// ─── Light themes ─────────────────────────────────────────────

fn clean_modern() -> GalleryTheme {
    GalleryTheme {
        id: "clean_modern",
        name: "Clean Modern",
        is_dark: false,
        bg_base: "#f8fafc",
        bg_surface: "#ffffff",
        bg_overlay: "#f1f5f9",
        bg_elevated: "#e2e8f0",
        text: "#1e293b",
        text_subtle: "#64748b",
        text_muted: "#94a3b8",
        accent: "#3b82f6",
        accent_hover: "#2563eb",
        success: "#22c55e",
        warning: "#f59e0b",
        error: "#ef4444",
        info: "#06b6d4",
        border: "#e2e8f0",
        shadow_color: "rgba(0, 0, 0, 0.08)",
        header_bg: "#1e293b",
        header_text: "#ffffff",
        sidebar_bg: "#ffffff",
        input_bg: "#ffffff",
        section_hover: "#f8fafc",
        button_bg: "#ffffff",
        button_hover_text: "#ffffff",
        button_pressed: "#2563eb",
        focus_ring: "rgba(59, 130, 246, 0.15)",
        purple: "#8b5cf6",
        pink: "#ec4899",
        orange: "#f97316",
        teal: "#14b8a6",
        indigo: "#6366f1",
        blue_muted: "#dbeafe",
        green_muted: "#d1fae5",
        amber_muted: "#fef3c7",
        purple_muted: "#ede9fe",
        red_muted: "#fee2e2",
        chart_grid: "#e2e8f0",
        tooltip_bg: "#1e293b",
        tooltip_border: "#334155",
        chart_shadow: "rgba(0, 0, 0, 0.06)",
        glass_bg: "rgba(255, 255, 255, 0.15)",
        glass_border: "rgba(255, 255, 255, 0.25)",
        glass_dark_bg: "rgba(0, 0, 0, 0.2)",
        glass_dark_border: "rgba(255, 255, 255, 0.1)",
        fx_shadow: "rgba(0, 0, 0, 0.12)",
        fx_shadow_md: "rgba(0, 0, 0, 0.18)",
        fx_shadow_lg: "rgba(0, 0, 0, 0.22)",
    }
}

fn ocean_blue() -> GalleryTheme {
    GalleryTheme {
        id: "ocean_blue",
        name: "Ocean Blue",
        is_dark: false,
        bg_base: "#f0f7ff",
        bg_surface: "#ffffff",
        bg_overlay: "#e0ecf8",
        bg_elevated: "#c7d9ef",
        text: "#1a2b42",
        text_subtle: "#4a6580",
        text_muted: "#8aa4be",
        accent: "#0077b6",
        accent_hover: "#005f8f",
        success: "#06d6a0",
        warning: "#ffd166",
        error: "#ef476f",
        info: "#118ab2",
        border: "#c7d9ef",
        shadow_color: "rgba(0, 40, 80, 0.08)",
        header_bg: "#023e73",
        header_text: "#ffffff",
        sidebar_bg: "#ffffff",
        input_bg: "#ffffff",
        section_hover: "#f0f7ff",
        button_bg: "#ffffff",
        button_hover_text: "#ffffff",
        button_pressed: "#005f8f",
        focus_ring: "rgba(0, 119, 182, 0.15)",
        purple: "#7b68ee",
        pink: "#ff6b9d",
        orange: "#ff8a5c",
        teal: "#06d6a0",
        indigo: "#5c6bc0",
        blue_muted: "#d0e8f7",
        green_muted: "#c8f7e5",
        amber_muted: "#fff3cd",
        purple_muted: "#e8e0ff",
        red_muted: "#fde2e8",
        chart_grid: "#c7d9ef",
        tooltip_bg: "#023e73",
        tooltip_border: "#0a5c99",
        chart_shadow: "rgba(0, 40, 80, 0.06)",
        glass_bg: "rgba(255, 255, 255, 0.15)",
        glass_border: "rgba(255, 255, 255, 0.25)",
        glass_dark_bg: "rgba(0, 0, 0, 0.2)",
        glass_dark_border: "rgba(255, 255, 255, 0.1)",
        fx_shadow: "rgba(0, 0, 0, 0.12)",
        fx_shadow_md: "rgba(0, 0, 0, 0.18)",
        fx_shadow_lg: "rgba(0, 0, 0, 0.22)",
    }
}

fn rose_garden() -> GalleryTheme {
    GalleryTheme {
        id: "rose_garden",
        name: "Rose Garden",
        is_dark: false,
        bg_base: "#fdf2f8",
        bg_surface: "#ffffff",
        bg_overlay: "#fce7f3",
        bg_elevated: "#f9a8d4",
        text: "#3b0a2a",
        text_subtle: "#9d4c7e",
        text_muted: "#c084a8",
        accent: "#db2777",
        accent_hover: "#be185d",
        success: "#34d399",
        warning: "#fbbf24",
        error: "#f43f5e",
        info: "#38bdf8",
        border: "#fce7f3",
        shadow_color: "rgba(80, 0, 40, 0.08)",
        header_bg: "#831843",
        header_text: "#ffffff",
        sidebar_bg: "#ffffff",
        input_bg: "#ffffff",
        section_hover: "#fdf2f8",
        button_bg: "#ffffff",
        button_hover_text: "#ffffff",
        button_pressed: "#be185d",
        focus_ring: "rgba(219, 39, 119, 0.15)",
        purple: "#a855f7",
        pink: "#ec4899",
        orange: "#fb923c",
        teal: "#2dd4bf",
        indigo: "#818cf8",
        blue_muted: "#ede9fe",
        green_muted: "#d1fae5",
        amber_muted: "#fef9c3",
        purple_muted: "#f3e8ff",
        red_muted: "#fce4ec",
        chart_grid: "#fce7f3",
        tooltip_bg: "#831843",
        tooltip_border: "#9d174d",
        chart_shadow: "rgba(80, 0, 40, 0.06)",
        glass_bg: "rgba(255, 255, 255, 0.15)",
        glass_border: "rgba(255, 255, 255, 0.25)",
        glass_dark_bg: "rgba(0, 0, 0, 0.2)",
        glass_dark_border: "rgba(255, 255, 255, 0.1)",
        fx_shadow: "rgba(0, 0, 0, 0.12)",
        fx_shadow_md: "rgba(0, 0, 0, 0.18)",
        fx_shadow_lg: "rgba(0, 0, 0, 0.22)",
    }
}

fn emerald_mint() -> GalleryTheme {
    GalleryTheme {
        id: "emerald_mint",
        name: "Emerald Mint",
        is_dark: false,
        bg_base: "#f0fdf4",
        bg_surface: "#ffffff",
        bg_overlay: "#dcfce7",
        bg_elevated: "#bbf7d0",
        text: "#14532d",
        text_subtle: "#4d7c5f",
        text_muted: "#86b898",
        accent: "#059669",
        accent_hover: "#047857",
        success: "#22c55e",
        warning: "#eab308",
        error: "#ef4444",
        info: "#0891b2",
        border: "#bbf7d0",
        shadow_color: "rgba(0, 60, 30, 0.08)",
        header_bg: "#064e3b",
        header_text: "#ffffff",
        sidebar_bg: "#ffffff",
        input_bg: "#ffffff",
        section_hover: "#f0fdf4",
        button_bg: "#ffffff",
        button_hover_text: "#ffffff",
        button_pressed: "#047857",
        focus_ring: "rgba(5, 150, 105, 0.15)",
        purple: "#8b5cf6",
        pink: "#f472b6",
        orange: "#fb923c",
        teal: "#14b8a6",
        indigo: "#6366f1",
        blue_muted: "#dbeafe",
        green_muted: "#d1fae5",
        amber_muted: "#fef3c7",
        purple_muted: "#ede9fe",
        red_muted: "#fee2e2",
        chart_grid: "#bbf7d0",
        tooltip_bg: "#064e3b",
        tooltip_border: "#065f46",
        chart_shadow: "rgba(0, 60, 30, 0.06)",
        glass_bg: "rgba(255, 255, 255, 0.15)",
        glass_border: "rgba(255, 255, 255, 0.25)",
        glass_dark_bg: "rgba(0, 0, 0, 0.2)",
        glass_dark_border: "rgba(255, 255, 255, 0.1)",
        fx_shadow: "rgba(0, 0, 0, 0.12)",
        fx_shadow_md: "rgba(0, 0, 0, 0.18)",
        fx_shadow_lg: "rgba(0, 0, 0, 0.22)",
    }
}

fn warm_amber() -> GalleryTheme {
    GalleryTheme {
        id: "warm_amber",
        name: "Warm Amber",
        is_dark: false,
        bg_base: "#fffbeb",
        bg_surface: "#ffffff",
        bg_overlay: "#fef3c7",
        bg_elevated: "#fde68a",
        text: "#451a03",
        text_subtle: "#92400e",
        text_muted: "#b45309",
        accent: "#d97706",
        accent_hover: "#b45309",
        success: "#22c55e",
        warning: "#f59e0b",
        error: "#ef4444",
        info: "#0ea5e9",
        border: "#fde68a",
        shadow_color: "rgba(80, 50, 0, 0.08)",
        header_bg: "#78350f",
        header_text: "#ffffff",
        sidebar_bg: "#ffffff",
        input_bg: "#ffffff",
        section_hover: "#fffbeb",
        button_bg: "#ffffff",
        button_hover_text: "#ffffff",
        button_pressed: "#b45309",
        focus_ring: "rgba(217, 119, 6, 0.15)",
        purple: "#8b5cf6",
        pink: "#ec4899",
        orange: "#f97316",
        teal: "#14b8a6",
        indigo: "#6366f1",
        blue_muted: "#dbeafe",
        green_muted: "#d1fae5",
        amber_muted: "#fef3c7",
        purple_muted: "#ede9fe",
        red_muted: "#fee2e2",
        chart_grid: "#fde68a",
        tooltip_bg: "#78350f",
        tooltip_border: "#92400e",
        chart_shadow: "rgba(80, 50, 0, 0.06)",
        glass_bg: "rgba(255, 255, 255, 0.15)",
        glass_border: "rgba(255, 255, 255, 0.25)",
        glass_dark_bg: "rgba(0, 0, 0, 0.2)",
        glass_dark_border: "rgba(255, 255, 255, 0.1)",
        fx_shadow: "rgba(0, 0, 0, 0.12)",
        fx_shadow_md: "rgba(0, 0, 0, 0.18)",
        fx_shadow_lg: "rgba(0, 0, 0, 0.22)",
    }
}

// ─── Dark themes ──────────────────────────────────────────────

fn catppuccin_macchiato() -> GalleryTheme {
    GalleryTheme {
        id: "catppuccin_macchiato",
        name: "Catppuccin Macchiato",
        is_dark: true,
        bg_base: "#1e2030",
        bg_surface: "#24273a",
        bg_overlay: "#363a4f",
        bg_elevated: "#494d64",
        text: "#cad3f5",
        text_subtle: "#a5adcb",
        text_muted: "#6e738d",
        accent: "#8aadf4",
        accent_hover: "#7dc4e4",
        success: "#a6da95",
        warning: "#eed49f",
        error: "#ed8796",
        info: "#91d7e3",
        border: "#363a4f",
        shadow_color: "rgba(0, 0, 0, 0.5)",
        header_bg: "#181926",
        header_text: "#cad3f5",
        sidebar_bg: "#1e2030",
        input_bg: "#1e2030",
        section_hover: "#2a2d42",
        button_bg: "#363a4f",
        button_hover_text: "#181926",
        button_pressed: "#739df2",
        focus_ring: "rgba(138, 173, 244, 0.3)",
        purple: "#c6a0f6",
        pink: "#f5bde6",
        orange: "#f5a97f",
        teal: "#8bd5ca",
        indigo: "#b7bdf8",
        blue_muted: "#1e3a5f",
        green_muted: "#064e3b",
        amber_muted: "#78350f",
        purple_muted: "#2e1065",
        red_muted: "#3b1219",
        chart_grid: "rgba(255, 255, 255, 0.08)",
        tooltip_bg: "#0f172a",
        tooltip_border: "#1e293b",
        chart_shadow: "rgba(0, 0, 0, 0.2)",
        glass_bg: "rgba(255, 255, 255, 0.08)",
        glass_border: "rgba(255, 255, 255, 0.12)",
        glass_dark_bg: "rgba(0, 0, 0, 0.3)",
        glass_dark_border: "rgba(255, 255, 255, 0.06)",
        fx_shadow: "rgba(0, 0, 0, 0.4)",
        fx_shadow_md: "rgba(0, 0, 0, 0.5)",
        fx_shadow_lg: "rgba(0, 0, 0, 0.6)",
    }
}

fn nord() -> GalleryTheme {
    GalleryTheme {
        id: "nord",
        name: "Nord",
        is_dark: true,
        bg_base: "#2e3440",
        bg_surface: "#3b4252",
        bg_overlay: "#434c5e",
        bg_elevated: "#4c566a",
        text: "#eceff4",
        text_subtle: "#d8dee9",
        text_muted: "#616e88",
        accent: "#88c0d0",
        accent_hover: "#5e81ac",
        success: "#a3be8c",
        warning: "#ebcb8b",
        error: "#bf616a",
        info: "#81a1c1",
        border: "#434c5e",
        shadow_color: "rgba(0, 0, 0, 0.4)",
        header_bg: "#242933",
        header_text: "#eceff4",
        sidebar_bg: "#2e3440",
        input_bg: "#2e3440",
        section_hover: "#3e4658",
        button_bg: "#434c5e",
        button_hover_text: "#2e3440",
        button_pressed: "#4c8eaa",
        focus_ring: "rgba(136, 192, 208, 0.3)",
        purple: "#b48ead",
        pink: "#d08770",
        orange: "#d08770",
        teal: "#8fbcbb",
        indigo: "#81a1c1",
        blue_muted: "#2e3c4a",
        green_muted: "#2e3e33",
        amber_muted: "#3e3828",
        purple_muted: "#3a2e46",
        red_muted: "#3e2a2d",
        chart_grid: "rgba(255, 255, 255, 0.08)",
        tooltip_bg: "#242933",
        tooltip_border: "#3b4252",
        chart_shadow: "rgba(0, 0, 0, 0.2)",
        glass_bg: "rgba(255, 255, 255, 0.06)",
        glass_border: "rgba(255, 255, 255, 0.1)",
        glass_dark_bg: "rgba(0, 0, 0, 0.25)",
        glass_dark_border: "rgba(255, 255, 255, 0.05)",
        fx_shadow: "rgba(0, 0, 0, 0.35)",
        fx_shadow_md: "rgba(0, 0, 0, 0.45)",
        fx_shadow_lg: "rgba(0, 0, 0, 0.55)",
    }
}

fn dracula() -> GalleryTheme {
    GalleryTheme {
        id: "dracula",
        name: "Dracula",
        is_dark: true,
        bg_base: "#282a36",
        bg_surface: "#343746",
        bg_overlay: "#44475a",
        bg_elevated: "#565969",
        text: "#f8f8f2",
        text_subtle: "#cac8db",
        text_muted: "#6272a4",
        accent: "#bd93f9",
        accent_hover: "#ff79c6",
        success: "#50fa7b",
        warning: "#f1fa8c",
        error: "#ff5555",
        info: "#8be9fd",
        border: "#44475a",
        shadow_color: "rgba(0, 0, 0, 0.5)",
        header_bg: "#1e1f29",
        header_text: "#f8f8f2",
        sidebar_bg: "#282a36",
        input_bg: "#282a36",
        section_hover: "#383a4e",
        button_bg: "#44475a",
        button_hover_text: "#282a36",
        button_pressed: "#a070e0",
        focus_ring: "rgba(189, 147, 249, 0.3)",
        purple: "#bd93f9",
        pink: "#ff79c6",
        orange: "#ffb86c",
        teal: "#8be9fd",
        indigo: "#6272a4",
        blue_muted: "#2a2556",
        green_muted: "#1a3a1e",
        amber_muted: "#3a3520",
        purple_muted: "#2e1f4a",
        red_muted: "#3e1a1a",
        chart_grid: "rgba(255, 255, 255, 0.08)",
        tooltip_bg: "#1e1f29",
        tooltip_border: "#343746",
        chart_shadow: "rgba(0, 0, 0, 0.2)",
        glass_bg: "rgba(255, 255, 255, 0.08)",
        glass_border: "rgba(255, 255, 255, 0.12)",
        glass_dark_bg: "rgba(0, 0, 0, 0.3)",
        glass_dark_border: "rgba(255, 255, 255, 0.06)",
        fx_shadow: "rgba(0, 0, 0, 0.4)",
        fx_shadow_md: "rgba(0, 0, 0, 0.5)",
        fx_shadow_lg: "rgba(0, 0, 0, 0.6)",
    }
}

fn monokai() -> GalleryTheme {
    GalleryTheme {
        id: "monokai",
        name: "Monokai",
        is_dark: true,
        bg_base: "#272822",
        bg_surface: "#32332c",
        bg_overlay: "#49483e",
        bg_elevated: "#5b5a50",
        text: "#f8f8f2",
        text_subtle: "#cfcfc2",
        text_muted: "#75715e",
        accent: "#a6e22e",
        accent_hover: "#e6db74",
        success: "#a6e22e",
        warning: "#e6db74",
        error: "#f92672",
        info: "#66d9ef",
        border: "#49483e",
        shadow_color: "rgba(0, 0, 0, 0.5)",
        header_bg: "#1e1f1a",
        header_text: "#f8f8f2",
        sidebar_bg: "#272822",
        input_bg: "#272822",
        section_hover: "#363730",
        button_bg: "#49483e",
        button_hover_text: "#272822",
        button_pressed: "#8cc420",
        focus_ring: "rgba(166, 226, 46, 0.3)",
        purple: "#ae81ff",
        pink: "#f92672",
        orange: "#fd971f",
        teal: "#66d9ef",
        indigo: "#7d7dbd",
        blue_muted: "#1e2a3a",
        green_muted: "#1e3a1e",
        amber_muted: "#3a3018",
        purple_muted: "#2a1e3e",
        red_muted: "#3a1a22",
        chart_grid: "rgba(255, 255, 255, 0.08)",
        tooltip_bg: "#1e1f1a",
        tooltip_border: "#32332c",
        chart_shadow: "rgba(0, 0, 0, 0.2)",
        glass_bg: "rgba(255, 255, 255, 0.08)",
        glass_border: "rgba(255, 255, 255, 0.12)",
        glass_dark_bg: "rgba(0, 0, 0, 0.3)",
        glass_dark_border: "rgba(255, 255, 255, 0.06)",
        fx_shadow: "rgba(0, 0, 0, 0.4)",
        fx_shadow_md: "rgba(0, 0, 0, 0.5)",
        fx_shadow_lg: "rgba(0, 0, 0, 0.6)",
    }
}

fn tokyo_night() -> GalleryTheme {
    GalleryTheme {
        id: "tokyo_night",
        name: "Tokyo Night",
        is_dark: true,
        bg_base: "#1a1b26",
        bg_surface: "#24283b",
        bg_overlay: "#343b58",
        bg_elevated: "#414868",
        text: "#c0caf5",
        text_subtle: "#a9b1d6",
        text_muted: "#565f89",
        accent: "#7aa2f7",
        accent_hover: "#2ac3de",
        success: "#9ece6a",
        warning: "#e0af68",
        error: "#f7768e",
        info: "#2ac3de",
        border: "#343b58",
        shadow_color: "rgba(0, 0, 0, 0.5)",
        header_bg: "#16161e",
        header_text: "#c0caf5",
        sidebar_bg: "#1a1b26",
        input_bg: "#1a1b26",
        section_hover: "#292e42",
        button_bg: "#343b58",
        button_hover_text: "#1a1b26",
        button_pressed: "#5d8ae0",
        focus_ring: "rgba(122, 162, 247, 0.3)",
        purple: "#bb9af7",
        pink: "#ff007c",
        orange: "#ff9e64",
        teal: "#73daca",
        indigo: "#7dcfff",
        blue_muted: "#1a2a4a",
        green_muted: "#1a3a28",
        amber_muted: "#3a3018",
        purple_muted: "#2a1e44",
        red_muted: "#3a1a24",
        chart_grid: "rgba(255, 255, 255, 0.08)",
        tooltip_bg: "#16161e",
        tooltip_border: "#24283b",
        chart_shadow: "rgba(0, 0, 0, 0.2)",
        glass_bg: "rgba(255, 255, 255, 0.08)",
        glass_border: "rgba(255, 255, 255, 0.12)",
        glass_dark_bg: "rgba(0, 0, 0, 0.3)",
        glass_dark_border: "rgba(255, 255, 255, 0.06)",
        fx_shadow: "rgba(0, 0, 0, 0.4)",
        fx_shadow_md: "rgba(0, 0, 0, 0.5)",
        fx_shadow_lg: "rgba(0, 0, 0, 0.6)",
    }
}
