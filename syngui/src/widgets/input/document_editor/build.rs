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
use super::links::{DocLinkProvider, DocMediaResolver, EmbedCtx, EmbedFactory};
use super::model::{Attrs, BlockKind, DocBlock, InlineText, MediaKind};
use super::state::{GeomMap, TableGeomMap};
use super::rows::{
    CodeBlockView, DividerView, EmbedCard, MediaCard, MediaGlyph, RowDecor, TableBlockView,
    TextRow,
};
use super::style::DocStyle;

/// Окружение сборки блоков: стиль, реестр геометрии, инъекции хоста.
pub struct BuildEnv {
    pub style: Arc<DocStyle>,
    pub geom: GeomMap,
    pub tables: TableGeomMap,
    pub links: Option<Arc<dyn DocLinkProvider>>,
    pub media: Option<Arc<dyn DocMediaResolver>>,
    pub embeds: Option<Arc<dyn EmbedFactory>>,
    pub embed_ctx: EmbedCtx,
}

pub fn block_widget(block: &DocBlock, env: &BuildEnv) -> Box<dyn Widget> {
    let style = &env.style;
    let geom = &env.geom;
    let _ = geom;
    match &block.kind {
        BlockKind::Paragraph(text) => text_row(block, text, env, RowDecor::None, 0.0, None),
        BlockKind::Heading { level, text } => {
            let idx = (*level as usize).saturating_sub(1).min(5);
            let mut row = base_row(block, text, env);
            row.font_size = style.heading_sizes[idx];
            row.bold = true;
            row.color = style.heading_color;
            Box::new(row)
        }
        BlockKind::Bullet { text, children } => {
            item_widget(block, text, children, env, RowDecor::Bullet)
        }
        BlockKind::Numbered { number, text, children } => {
            item_widget(block, text, children, env, RowDecor::Number(*number))
        }
        BlockKind::Todo { checked, text, children } => {
            item_widget(block, text, children, env, RowDecor::Checkbox { checked: *checked })
        }
        BlockKind::Toggle { summary, children, collapsed } => {
            let row = text_row(
                block,
                summary,
                env,
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
                        .child(children_column(children, env)),
                )
            }
        }
        BlockKind::Quote(children) => Box::new(
            Chrome::new()
                .gap(style.child_spacing)
                .padding(style.quote_border_width + style.quote_padding_left, 4.0, 4.0, 4.0)
                .border_left(style.quote_border_width, style.quote_border_color)
                .children(children.iter().map(|b| block_widget(b, env))),
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
                let mut row = base_row(block, title, env);
                row.bold = true;
                row.color = accent;
                chrome = chrome.child(Box::new(row));
            }
            chrome = chrome.children(children.iter().map(|b| block_widget(b, env)));
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
            tables: Some(env.tables.clone()),
        }),
        BlockKind::Divider => Box::new(DividerView { style: style.clone() }),
        BlockKind::Media { media, url, alt } => media_widget(block, *media, url, alt, env),
        BlockKind::Embed { target } => {
            if let Some(factory) = &env.embeds {
                if let Some(inner) = factory.build(target, &env.embed_ctx) {
                    // Живая врезка в рамке.
                    return Box::new(
                        Chrome::new()
                            .padding(6.0, 6.0, 6.0, 6.0)
                            .radius(8.0)
                            .bg(style.embed_bg)
                            .border_left(2.0, style.embed_border_color)
                            .child(inner),
                    );
                }
            }
            Box::new(EmbedCard {
                block_id: block.id,
                target: target.clone(),
                style: style.clone(),
            })
        }
    }
}

/// Пункт списка: строка с маркером + (опционально) колонка детей с отступом.
fn item_widget(
    block: &DocBlock,
    text: &InlineText,
    children: &[DocBlock],
    env: &BuildEnv,
    decor: RowDecor,
) -> Box<dyn Widget> {
    let row = text_row(block, text, env, decor, env.style.indent, None);
    if children.is_empty() {
        return row;
    }
    Box::new(
        Chrome::new()
            .gap(env.style.child_spacing)
            .child(row)
            .child(children_column(children, env)),
    )
}

/// Колонка детей блока с отступом под гаттер родителя.
fn children_column(children: &[DocBlock], env: &BuildEnv) -> Box<dyn Widget> {
    Box::new(
        Chrome::new()
            .gap(env.style.child_spacing)
            .padding(env.style.indent, 0.0, 0.0, 0.0)
            .children(children.iter().map(|b| block_widget(b, env))),
    )
}

fn base_row(block: &DocBlock, text: &InlineText, env: &BuildEnv) -> TextRow {
    TextRow {
        block_id: block.id,
        text: text.clone(),
        font_size: env.style.text_size,
        bold: false,
        color: env.style.text_color,
        decor: RowDecor::None,
        gutter: 0.0,
        style: env.style.clone(),
        geom: Some(env.geom.clone()),
        links: env.links.clone(),
    }
}

fn text_row(
    block: &DocBlock,
    text: &InlineText,
    env: &BuildEnv,
    decor: RowDecor,
    gutter: f32,
    color: Option<Color>,
) -> Box<dyn Widget> {
    let mut row = base_row(block, text, env);
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
    env: &BuildEnv,
) -> Box<dyn Widget> {
    let style = &env.style;
    // Файл ещё загружается в хранилище хоста (drop/paste): карточка «…».
    if url.starts_with("pending:") {
        return Box::new(MediaCard {
            block_id: block.id,
            glyph: MediaGlyph::File,
            title: alt.to_string(),
            subtitle: "…".to_string(),
            style: style.clone(),
        });
    }
    let resolved = env.media.as_ref().and_then(|m| m.resolve(url));
    match media {
        MediaKind::Image => {
            #[cfg(feature = "image")]
            {
                use crate::widgets::visual::image::{Image, ImageFit};
                if let Some(r) = &resolved {
                    return Box::new(
                        Image::new(r.path.display().to_string()).fit(ImageFit::Contain),
                    );
                }
                if !url.starts_with("blob:") {
                    let img = if url.starts_with("http://") || url.starts_with("https://") {
                        Image::from_url(url)
                    } else {
                        Image::new(url)
                    };
                    return Box::new(img.fit(ImageFit::Contain));
                }
            }
        }
        MediaKind::Video | MediaKind::Audio => {
            // Живой плеер — когда есть резолвер, файл и ffmpeg.
            #[cfg(feature = "ffmpeg")]
            if let (Some(m), Some(_)) = (&env.media, &resolved) {
                return Box::new(super::media_block::MediaBlock {
                    block_id: block.id,
                    kind: media,
                    url: url.to_string(),
                    alt: alt.to_string(),
                    style: style.clone(),
                    media: m.clone(),
                });
            }
        }
        MediaKind::File => {}
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
