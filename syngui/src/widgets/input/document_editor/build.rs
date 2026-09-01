//! Сборка виджетов из блоков модели (read-only рендер).
//!
//! Каждый `DocBlock` превращается в лист из [`super::rows`] либо в
//! [`Chrome`]-контейнер с детьми. Врезки и не-картиночные медиа пока
//! рендерятся карточками-плейсхолдерами — живые плееры и врезки приходят
//! на этапах S7–S9.

use std::sync::Arc;

use crate::core::Color;
use crate::widget::Widget;

use super::chrome::Chrome;
use super::model::{Attrs, BlockKind, DocBlock, InlineText, MediaKind};
use super::state::GeomMap;
use super::rows::{
    CodeBlockView, DividerView, EmbedCard, MediaCard, MediaGlyph, RowDecor, TableBlockView,
    TextRow,
};
use super::style::DocStyle;

pub fn block_widget(block: &DocBlock, style: &Arc<DocStyle>, geom: &GeomMap) -> Box<dyn Widget> {
    match &block.kind {
        BlockKind::Paragraph(text) => text_row(block, text, style, geom, RowDecor::None, 0.0, None),
        BlockKind::Heading { level, text } => {
            let idx = (*level as usize).saturating_sub(1).min(5);
            let mut row = base_row(block, text, style, geom);
            row.font_size = style.heading_sizes[idx];
            row.bold = true;
            row.color = style.heading_color;
            Box::new(row)
        }
        BlockKind::Bullet { text, children } => {
            item_widget(block, text, children, style, geom, RowDecor::Bullet)
        }
        BlockKind::Numbered { number, text, children } => {
            item_widget(block, text, children, style, geom, RowDecor::Number(*number))
        }
        BlockKind::Todo { checked, text, children } => {
            item_widget(block, text, children, style, geom, RowDecor::Checkbox { checked: *checked })
        }
        BlockKind::Toggle { summary, children, collapsed } => {
            let row = text_row(
                block,
                summary,
                style,
                geom,
                RowDecor::Toggle { collapsed: *collapsed },
                style.indent,
                None,
            );
            if *collapsed || children.is_empty() {
                row
            } else {
                Box::new(
                    Chrome::new()
                        .gap(style.child_spacing)
                        .child(row)
                        .child(children_column(children, style, geom)),
                )
            }
        }
        BlockKind::Quote(children) => Box::new(
            Chrome::new()
                .gap(style.child_spacing)
                .padding(style.quote_border_width + style.quote_padding_left, 4.0, 4.0, 4.0)
                .border_left(style.quote_border_width, style.quote_border_color)
                .children(children.iter().map(|b| block_widget(b, style, geom))),
        ),
        BlockKind::Callout { kind, title, children } => {
            let accent = attr_color(&block.attrs).unwrap_or_else(|| style.callout_color(kind));
            let mut chrome = Chrome::new()
                .gap(style.child_spacing)
                .padding(
                    style.callout_padding + 3.0,
                    style.callout_padding * 0.75,
                    style.callout_padding,
                    style.callout_padding * 0.75,
                )
                .bg(accent.with_alpha(style.callout_bg_alpha))
                .radius(style.callout_radius)
                .border_left(3.0, accent);
            if !title.is_empty() {
                let mut row = base_row(block, title, style, geom);
                row.bold = true;
                row.color = accent;
                chrome = chrome.child(Box::new(row));
            }
            chrome = chrome.children(children.iter().map(|b| block_widget(b, style, geom)));
            Box::new(chrome)
        }
        BlockKind::CodeBlock { language, code } => Box::new(CodeBlockView {
            block_id: block.id,
            language: language.clone(),
            code: code.clone(),
            style: style.clone(),
        }),
        BlockKind::Table { headers, rows, .. } => Box::new(TableBlockView {
            block_id: block.id,
            headers: headers.clone(),
            rows: rows.clone(),
            style: style.clone(),
        }),
        BlockKind::Divider => Box::new(DividerView { style: style.clone() }),
        BlockKind::Media { media, url, alt } => media_widget(block, *media, url, alt, style),
        BlockKind::Embed { target } => Box::new(EmbedCard {
            block_id: block.id,
            target: target.clone(),
            style: style.clone(),
        }),
    }
}

/// Пункт списка: строка с маркером + (опционально) колонка детей с отступом.
fn item_widget(
    block: &DocBlock,
    text: &InlineText,
    children: &[DocBlock],
    style: &Arc<DocStyle>,
    geom: &GeomMap,
    decor: RowDecor,
) -> Box<dyn Widget> {
    let row = text_row(block, text, style, geom, decor, style.indent, None);
    if children.is_empty() {
        return row;
    }
    Box::new(
        Chrome::new()
            .gap(style.child_spacing)
            .child(row)
            .child(children_column(children, style, geom)),
    )
}

/// Колонка детей блока с отступом под гаттер родителя.
fn children_column(children: &[DocBlock], style: &Arc<DocStyle>, geom: &GeomMap) -> Box<dyn Widget> {
    Box::new(
        Chrome::new()
            .gap(style.child_spacing)
            .padding(style.indent, 0.0, 0.0, 0.0)
            .children(children.iter().map(|b| block_widget(b, style, geom))),
    )
}

fn base_row(block: &DocBlock, text: &InlineText, style: &Arc<DocStyle>, geom: &GeomMap) -> TextRow {
    TextRow {
        block_id: block.id,
        text: text.clone(),
        font_size: style.text_size,
        bold: false,
        color: style.text_color,
        decor: RowDecor::None,
        gutter: 0.0,
        style: style.clone(),
        geom: Some(geom.clone()),
    }
}

fn text_row(
    block: &DocBlock,
    text: &InlineText,
    style: &Arc<DocStyle>,
    geom: &GeomMap,
    decor: RowDecor,
    gutter: f32,
    color: Option<Color>,
) -> Box<dyn Widget> {
    let mut row = base_row(block, text, style, geom);
    row.decor = decor;
    row.gutter = gutter;
    if let Some(c) = color {
        row.color = c;
    }
    Box::new(row)
}

fn media_widget(
    block: &DocBlock,
    media: MediaKind,
    url: &str,
    alt: &str,
    style: &Arc<DocStyle>,
) -> Box<dyn Widget> {
    // Картинки с обычным путём/URL показываем сразу через ImageStore;
    // blob:-ссылки резолвятся хостом на этапе S7 (DocMediaResolver).
    if media == MediaKind::Image && !url.starts_with("blob:") {
        #[cfg(feature = "image")]
        {
            use crate::widgets::visual::image::{Image, ImageFit};
            let img = if url.starts_with("http://") || url.starts_with("https://") {
                Image::from_url(url)
            } else {
                Image::new(url)
            };
            return Box::new(img.fit(ImageFit::Contain));
        }
    }
    let glyph = match media {
        MediaKind::Video => MediaGlyph::Video,
        MediaKind::Audio => MediaGlyph::Audio,
        MediaKind::Image => MediaGlyph::Image,
        MediaKind::File => MediaGlyph::File,
    };
    let title = if alt.is_empty() { url.to_string() } else { alt.to_string() };
    Box::new(MediaCard {
        block_id: block.id,
        glyph,
        title,
        subtitle: url.to_string(),
        style: style.clone(),
    })
}

fn attr_color(attrs: &Attrs) -> Option<Color> {
    attrs.get("color").map(Color::from_hex)
}
