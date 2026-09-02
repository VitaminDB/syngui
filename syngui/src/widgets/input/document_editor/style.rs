//! Визуальные параметры DocumentEditor.
//!
//! Все цвета/размеры собраны в [`DocStyle`]; дефолты — нейтральная тёмная
//! палитра. Приложение переопределяет их через MSS-переменные `--doc-*`
//! на элементе `document-editor` (паттерн `--md-*` у MarkdownView).

use crate::animation::transition::mss_color_to_core;
use crate::core::Color;
use crate::mss::ComputedStyle;

#[derive(Clone, Debug, PartialEq)]
pub struct DocStyle {
    // Текст.
    pub text_color: Color,
    pub muted_color: Color,
    pub text_size: f32,
    /// Множитель высоты строки.
    pub line_height: f32,
    pub heading_color: Color,
    pub heading_sizes: [f32; 6],
    pub link_color: Color,
    /// Цвет wiki-ссылки на несуществующую страницу.
    pub link_missing_color: Color,
    pub caret_color: Color,
    pub selection_color: Color,
    /// Всплывающие меню редактора (slash-меню, автокомплит ссылок).
    pub menu_bg: Color,
    pub menu_border: Color,
    pub menu_sel_bg: Color,

    // Инлайн-код и код-блоки.
    pub code_color: Color,
    pub code_bg: Color,
    pub code_font_size: f32,
    pub code_radius: f32,
    pub code_padding_h: f32,
    pub code_block_bg: Color,
    pub code_block_color: Color,
    pub code_block_radius: f32,
    pub code_block_padding: f32,

    // Цитаты и callout'ы.
    pub quote_border_color: Color,
    pub quote_border_width: f32,
    pub quote_padding_left: f32,
    pub callout_bg_alpha: f32,
    pub callout_radius: f32,
    pub callout_padding: f32,

    // Маркеры списков.
    pub bullet_color: Color,
    pub bullet_radius: f32,
    pub number_color: Color,
    pub checkbox_color: Color,
    pub checkbox_check_color: Color,
    pub checkbox_size: f32,
    /// Зазор между чекбоксом и текстом задачи.
    pub checkbox_gap: f32,
    pub toggle_chevron_color: Color,

    // Прочие блоки.
    pub divider_color: Color,
    /// Цвет фон-сетки свободной раскладки.
    pub grid_color: Color,
    /// Подсветка габаритов блока под курсором.
    pub block_hover_color: Color,
    /// Рамка текущего блока (панель свойств работает над ним).
    pub block_selected_color: Color,
    pub divider_thickness: f32,
    pub media_bg: Color,
    pub media_radius: f32,
    pub media_placeholder_height: f32,
    /// Обводка векторного примитива по умолчанию (см. [`super::shape`]).
    pub shape_stroke_color: Color,
    /// Цвет хваталок концов линии и углов фигуры.
    pub shape_handle_color: Color,
    pub embed_border_color: Color,
    pub embed_bg: Color,
    pub table_border_color: Color,
    pub table_header_bg: Color,
    pub table_cell_padding_h: f32,
    pub table_cell_padding_v: f32,

    // Геометрия документа.
    pub block_spacing: f32,
    /// Вертикальный зазор между элементами списка и детьми блока.
    pub child_spacing: f32,
    /// Отступ детей блока и ширина гаттера маркера.
    pub indent: f32,
    pub doc_padding: f32,
    /// Ограничение ширины контента (колонка как в Notion); None — на всю ширину.
    pub max_content_width: Option<f32>,
}

impl Default for DocStyle {
    fn default() -> Self {
        Self {
            text_color: Color::from_hex("#e6e9ef"),
            muted_color: Color::from_hex("#9aa3b2"),
            text_size: 15.0,
            line_height: 1.55,
            heading_color: Color::from_hex("#f2f4f8"),
            heading_sizes: [28.0, 23.0, 19.0, 17.0, 15.5, 14.5],
            link_color: Color::from_hex("#6ea8ff"),
            link_missing_color: Color::from_hex("#c76a6a"),
            caret_color: Color::from_hex("#6ea8ff"),
            selection_color: Color::from_hex("#6ea8ff").with_alpha(0.28),
            menu_bg: Color::from_hex("#232833"),
            menu_border: Color::from_hex("#ffffff").with_alpha(0.12),
            menu_sel_bg: Color::from_hex("#ffffff").with_alpha(0.10),

            code_color: Color::from_hex("#e8b3f0"),
            code_bg: Color::from_hex("#ffffff").with_alpha(0.08),
            code_font_size: 13.5,
            code_radius: 4.0,
            code_padding_h: 4.0,
            code_block_bg: Color::from_hex("#000000").with_alpha(0.25),
            code_block_color: Color::from_hex("#dde3ec"),
            code_block_radius: 8.0,
            code_block_padding: 12.0,

            quote_border_color: Color::from_hex("#5a6372"),
            quote_border_width: 3.0,
            quote_padding_left: 14.0,
            callout_bg_alpha: 0.10,
            callout_radius: 8.0,
            callout_padding: 12.0,

            bullet_color: Color::from_hex("#9aa3b2"),
            bullet_radius: 2.5,
            number_color: Color::from_hex("#9aa3b2"),
            checkbox_color: Color::from_hex("#7d8797"),
            checkbox_check_color: Color::from_hex("#6ea8ff"),
            checkbox_size: 15.0,
            checkbox_gap: 8.0,
            toggle_chevron_color: Color::from_hex("#9aa3b2"),

            divider_color: Color::from_hex("#ffffff").with_alpha(0.12),
            grid_color: Color::from_hex("#ffffff").with_alpha(0.09),
            block_hover_color: Color::from_hex("#ffffff").with_alpha(0.06),
            block_selected_color: Color::from_hex("#6ea8ff").with_alpha(0.55),
            divider_thickness: 1.0,
            media_bg: Color::from_hex("#ffffff").with_alpha(0.05),
            media_radius: 8.0,
            media_placeholder_height: 72.0,
            shape_stroke_color: Color::from_hex("#9aa3b2"),
            shape_handle_color: Color::from_hex("#6ea8ff"),
            embed_border_color: Color::from_hex("#ffffff").with_alpha(0.14),
            embed_bg: Color::from_hex("#ffffff").with_alpha(0.04),
            table_border_color: Color::from_hex("#ffffff").with_alpha(0.12),
            table_header_bg: Color::from_hex("#ffffff").with_alpha(0.06),
            table_cell_padding_h: 10.0,
            table_cell_padding_v: 6.0,

            block_spacing: 10.0,
            child_spacing: 6.0,
            indent: 26.0,
            doc_padding: 16.0,
            max_content_width: Some(760.0),
        }
    }
}

impl DocStyle {
    /// Цвет callout'а по его типу (переопределяется атрибутом `color=`).
    pub fn callout_color(&self, kind: &str) -> Color {
        match kind {
            "warning" | "caution" => Color::from_hex("#e0a030"),
            "danger" | "error" | "bug" => Color::from_hex("#e06060"),
            "tip" | "hint" | "success" | "done" => Color::from_hex("#4fbf7a"),
            "question" | "help" => Color::from_hex("#c08fe8"),
            "quote" | "cite" => Color::from_hex("#8b95a6"),
            // note / info / прочие.
            _ => Color::from_hex("#6ea8ff"),
        }
    }

    /// Высота строки блока с базовым размером шрифта `font_size`.
    pub fn line_h(&self, font_size: f32) -> f32 {
        font_size * self.line_height
    }

    /// Переопределения из MSS (`--doc-*` на `document-editor`).
    pub fn apply(&mut self, style: &ComputedStyle) {
        if let Some(c) = style.color() {
            self.text_color = mss_color_to_core(c);
        }
        let fs = style.font_size();
        if fs != 16.0 {
            self.text_size = fs;
        }
        let color = |name: &str| style.get(name).and_then(|v| v.as_color()).map(mss_color_to_core);
        let px = |name: &str| style.get(name).and_then(|v| v.as_px());

        macro_rules! set_color {
            ($($name:literal => $field:ident),+ $(,)?) => {
                $(if let Some(c) = color($name) { self.$field = c; })+
            };
        }
        macro_rules! set_px {
            ($($name:literal => $field:ident),+ $(,)?) => {
                $(if let Some(v) = px($name) { self.$field = v; })+
            };
        }

        set_color! {
            "--doc-text-color" => text_color,
            "--doc-muted-color" => muted_color,
            "--doc-heading-color" => heading_color,
            "--doc-link-color" => link_color,
            "--doc-link-missing-color" => link_missing_color,
            "--doc-caret-color" => caret_color,
            "--doc-selection-color" => selection_color,
            "--doc-menu-bg" => menu_bg,
            "--doc-menu-border" => menu_border,
            "--doc-menu-sel-bg" => menu_sel_bg,
            "--doc-code-color" => code_color,
            "--doc-code-bg" => code_bg,
            "--doc-code-block-bg" => code_block_bg,
            "--doc-code-block-color" => code_block_color,
            "--doc-quote-border-color" => quote_border_color,
            "--doc-bullet-color" => bullet_color,
            "--doc-number-color" => number_color,
            "--doc-checkbox-color" => checkbox_color,
            "--doc-checkbox-check-color" => checkbox_check_color,
            "--doc-toggle-chevron-color" => toggle_chevron_color,
            "--doc-divider-color" => divider_color,
            "--doc-grid-color" => grid_color,
            "--doc-block-hover-color" => block_hover_color,
            "--doc-block-selected-color" => block_selected_color,
            "--doc-media-bg" => media_bg,
            "--doc-shape-stroke-color" => shape_stroke_color,
            "--doc-shape-handle-color" => shape_handle_color,
            "--doc-embed-border-color" => embed_border_color,
            "--doc-embed-bg" => embed_bg,
            "--doc-table-border-color" => table_border_color,
            "--doc-table-header-bg" => table_header_bg,
        }
        set_px! {
            "--doc-text-size" => text_size,
            "--doc-code-font-size" => code_font_size,
            "--doc-block-spacing" => block_spacing,
            "--doc-child-spacing" => child_spacing,
            "--doc-indent" => indent,
            "--doc-padding" => doc_padding,
            "--doc-code-block-padding" => code_block_padding,
            "--doc-callout-padding" => callout_padding,
            "--doc-checkbox-gap" => checkbox_gap,
        }
        if let Some(lh) = style.get("--doc-line-height").and_then(|v| v.as_px()) {
            self.line_height = lh;
        }
        for (i, name) in [
            "--doc-h1-size",
            "--doc-h2-size",
            "--doc-h3-size",
            "--doc-h4-size",
            "--doc-h5-size",
            "--doc-h6-size",
        ]
        .iter()
        .enumerate()
        {
            if let Some(v) = px(name) {
                self.heading_sizes[i] = v;
            }
        }
        if let Some(v) = px("--doc-max-content-width") {
            self.max_content_width = if v > 0.0 { Some(v) } else { None };
        }
    }
}
