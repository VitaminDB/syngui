use crate::core::{Color, Point, Rect, Size};
use crate::gpu::image_store::ImageLoadState;
use crate::render::{Border, DisplayList, TextureId};
use crate::mss::{TextAlign, TextDecoration};
use crate::widget::context::TextMeasure;

use std::sync::Arc;

use super::highlight::CodeHighlighter;
use super::model::*;
use super::selection_map::SelectableRun;

#[derive(Clone, Copy, Debug)]
pub struct MdImageEntry {
    pub texture_id: u32,
    pub state: ImageLoadState,
    pub natural_w: u32,
    pub natural_h: u32,
}

pub trait MdImageProbe {
    fn entry(&self, url: &str) -> Option<MdImageEntry>;
}

#[derive(Clone, Debug)]
pub struct MdStyle {
    pub heading_sizes: [f32; 6],
    pub heading_color: Color,
    pub heading_bold_factor: f32,

    pub text_size: f32,
    pub text_color: Color,
    pub line_height: f32,

    pub code_bg: Color,
    pub code_color: Color,
    pub code_font_size: f32,
    pub code_padding_h: f32,
    pub code_radius: f32,
    pub code_block_bg: Color,
    pub code_block_color: Color,
    pub code_block_radius: f32,
    pub code_block_padding: f32,

    pub quote_border_color: Color,
    pub quote_border_width: f32,
    pub quote_bg: Color,
    pub quote_text_color: Color,
    pub quote_padding_left: f32,
    pub quote_padding_v: f32,
    pub quote_radius: f32,

    pub link_color: Color,

    pub list_indent: f32,
    pub bullet_radius: f32,
    pub bullet_color: Color,
    pub checkbox_size: f32,
    pub checkbox_color: Color,
    pub checkbox_check_color: Color,
    pub checkbox_radius: f32,

    pub table_border_color: Color,
    pub table_header_bg: Color,
    pub table_header_color: Color,
    pub table_stripe_bg: Color,
    pub table_cell_padding_h: f32,
    pub table_cell_padding_v: f32,

    pub hr_color: Color,
    pub hr_thickness: f32,

    pub block_spacing: f32,
    pub heading_spacing_above: f32,

    pub image_placeholder_bg: Color,
    pub image_placeholder_color: Color,
    pub image_placeholder_height: f32,

    pub strikethrough_color: Option<Color>,

    pub footnote_color: Color,
    pub footnote_divider_color: Color,
    pub footnote_ref_scale: f32,

    pub copy_btn_size: f32,
    pub copy_btn_margin: f32,
    pub copy_btn_radius: f32,
    pub copy_btn_bg: Color,
    pub copy_btn_bg_hover: Color,
    pub copy_btn_color: Color,
    pub copy_btn_flash_bg: Color,
}

impl Default for MdStyle {
    fn default() -> Self {
        Self {
            heading_sizes: [28.0, 24.0, 20.0, 18.0, 16.0, 14.0],
            heading_color: Color::from_hex("#1E293B"),
            heading_bold_factor: 1.1,

            text_size: 14.0,
            text_color: Color::from_hex("#374151"),
            line_height: 1.5,

            code_bg: Color::from_hex("#F1F5F9"),
            code_color: Color::from_hex("#E11D48"),
            code_font_size: 13.0,
            code_padding_h: 5.0,
            code_radius: 4.0,
            code_block_bg: Color::from_hex("#1E293B"),
            code_block_color: Color::from_hex("#E2E8F0"),
            code_block_radius: 8.0,
            code_block_padding: 16.0,

            quote_border_color: Color::from_hex("#3B82F6"),
            quote_border_width: 3.0,
            quote_bg: Color::from_hex("#F8FAFC"),
            quote_text_color: Color::from_hex("#64748B"),
            quote_padding_left: 16.0,
            quote_padding_v: 12.0,
            quote_radius: 4.0,

            link_color: Color::from_hex("#3B82F6"),

            list_indent: 24.0,
            bullet_radius: 3.0,
            bullet_color: Color::from_hex("#6B7280"),
            checkbox_size: 16.0,
            checkbox_color: Color::from_hex("#3B82F6"),
            checkbox_check_color: Color::WHITE,
            checkbox_radius: 3.0,

            table_border_color: Color::from_hex("#E5E7EB"),
            table_header_bg: Color::from_hex("#F8FAFC"),
            table_header_color: Color::from_hex("#1E293B"),
            table_stripe_bg: Color::from_hex("#FAFAFA"),
            table_cell_padding_h: 12.0,
            table_cell_padding_v: 8.0,

            hr_color: Color::from_hex("#E5E7EB"),
            hr_thickness: 1.0,

            block_spacing: 12.0,
            heading_spacing_above: 8.0,

            image_placeholder_bg: Color::from_hex("#F3F4F6"),
            image_placeholder_color: Color::from_hex("#9CA3AF"),
            image_placeholder_height: 120.0,

            strikethrough_color: None,

            footnote_color: Color::from_hex("#3B82F6"),
            footnote_divider_color: Color::from_hex("#E5E7EB"),
            footnote_ref_scale: 0.72,

            copy_btn_size: 28.0,
            copy_btn_margin: 8.0,
            copy_btn_radius: 6.0,
            copy_btn_bg: Color::from_srgb(255, 255, 255, 0.06),
            copy_btn_bg_hover: Color::from_srgb(255, 255, 255, 0.18),
            copy_btn_color: Color::from_hex("#E2E8F0"),
            copy_btn_flash_bg: Color::from_srgb(34, 197, 94, 0.85),
        }
    }
}

const CHAR_W: f32 = 0.6;
const BOLD_CHAR_W: f32 = 0.66;

fn text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * CHAR_W
}

fn bold_text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * BOLD_CHAR_W
}

#[derive(Clone)]
struct InlineStyle {
    color: Color,
    font_size: f32,
    bold: bool,
    #[allow(dead_code)]
    italic: bool,
    strikethrough: bool,
    link: bool,
    link_url: Option<String>,
}

struct FlatSpan {
    text: String,
    color: Color,
    font_size: f32,
    bold: bool,
    underline: bool,
    strikethrough: bool,
    is_code: bool,
    code_bg: Option<Color>,
    link: Option<String>,
}

pub struct MdRenderer<'a> {
    list: &'a mut DisplayList,
    style: &'a MdStyle,
    origin_x: f32,
    max_width: f32,
    y: f32,
    text_measure: Option<Arc<dyn TextMeasure>>,
    images: Option<&'a dyn MdImageProbe>,
    highlighter: Option<Arc<dyn CodeHighlighter>>,
    copy_hotspots: Option<&'a mut Vec<(Rect, String)>>,
    selection_sink: Option<&'a mut Vec<SelectableRun>>,
    current_block_id: u32,
    current_line_id: u32,
    footnotes: Vec<FootnoteCollected>,
}

struct FootnoteCollected {
    #[allow(dead_code)]
    label: String,
    blocks: Vec<MdBlock>,
}

impl<'a> MdRenderer<'a> {
    pub fn new(
        list: &'a mut DisplayList,
        style: &'a MdStyle,
        origin: Point,
        max_width: f32,
    ) -> Self {
        Self {
            list,
            style,
            origin_x: origin.x,
            max_width,
            y: origin.y,
            text_measure: None,
            images: None,
            highlighter: None,
            copy_hotspots: None,
            selection_sink: None,
            current_block_id: 0,
            current_line_id: 0,
            footnotes: Vec::new(),
        }
    }

    pub fn with_text_measure(mut self, tm: Option<Arc<dyn TextMeasure>>) -> Self {
        self.text_measure = tm;
        self
    }

    pub fn with_images(mut self, images: Option<&'a dyn MdImageProbe>) -> Self {
        self.images = images;
        self
    }

    pub fn with_highlighter(mut self, h: Option<Arc<dyn CodeHighlighter>>) -> Self {
        self.highlighter = h;
        self
    }

    pub fn with_copy_hotspots(mut self, sink: &'a mut Vec<(Rect, String)>) -> Self {
        sink.clear();
        self.copy_hotspots = Some(sink);
        self
    }

    pub fn with_selection_sink(mut self, sink: &'a mut Vec<SelectableRun>) -> Self {
        sink.clear();
        self.selection_sink = Some(sink);
        self
    }

    fn bump_line(&mut self) {
        self.current_line_id = self.current_line_id.wrapping_add(1);
    }

    fn bump_block(&mut self) {
        self.current_block_id = self.current_block_id.wrapping_add(1);
        self.current_line_id = self.current_line_id.wrapping_add(1);
    }

    fn emit_selectable(
        &mut self,
        rect: Rect,
        text: &str,
        font_size: f32,
        font_family: Option<String>,
        bold: bool,
        link: Option<String>,
    ) {
        if text.is_empty() {
            return;
        }
        let block_id = self.current_block_id;
        let line_id = self.current_line_id;
        if let Some(sink) = self.selection_sink.as_deref_mut() {
            sink.push(SelectableRun {
                rect,
                visible_text: text.to_string(),
                font_size,
                font_family,
                bold,
                block_id,
                line_id,
                link,
            });
        }
    }

    fn text_width(&self, text: &str, font_size: f32) -> f32 {
        self.text_measure.as_ref()
            .map(|tm| tm.measure_text_width(text, font_size, text.chars().count()))
            .unwrap_or_else(|| text.chars().count() as f32 * font_size * CHAR_W)
    }

    fn bold_text_width(&self, text: &str, font_size: f32) -> f32 {
        self.text_measure.as_ref()
            .map(|tm| tm.measure_text_width_styled(text, font_size, text.chars().count(), true, None))
            .unwrap_or_else(|| text.chars().count() as f32 * font_size * BOLD_CHAR_W)
    }

    pub fn render_blocks(&mut self, blocks: &[MdBlock]) {
        let mut first = true;
        for block in blocks.iter() {
            if let MdBlock::FootnoteDefinition { label, blocks } = block {
                self.footnotes.push(FootnoteCollected {
                    label: label.clone(),
                    blocks: blocks.clone(),
                });
                continue;
            }
            if !first {
                self.y += self.style.block_spacing;
                self.bump_block();
            }
            first = false;
            self.render_block(block);
        }

        if !self.footnotes.is_empty() {
            self.bump_block();
            self.render_footnotes_section();
        }
    }

    fn render_footnotes_section(&mut self) {
        self.y += self.style.block_spacing * 2.0;
        let divider_h = self.style.hr_thickness.max(1.0);
        let divider_rect = Rect::new(
            Point::new(self.origin_x, self.y),
            Size::new(self.max_width, divider_h),
        );
        self.list.push_rect(divider_rect, self.style.footnote_divider_color, [0.0; 4]);
        self.y += divider_h + self.style.block_spacing;

        let entries = std::mem::take(&mut self.footnotes);
        for (idx, entry) in entries.iter().enumerate() {
            let prefix = format!("{}. ", idx + 1);
            let prefix_w = self.text_width(&prefix, self.style.text_size);
            let prefix_rect = Rect::new(
                Point::new(self.origin_x, self.y),
                Size::new(prefix_w, self.style.text_size + 2.0),
            );
            self.list.push_text(&prefix, prefix_rect, self.style.footnote_color, self.style.text_size);

            let saved_x = self.origin_x;
            let saved_w = self.max_width;
            self.origin_x += prefix_w;
            self.max_width -= prefix_w;
            self.render_blocks_with_color(entry.blocks.as_slice(), self.style.text_color);
            self.origin_x = saved_x;
            self.max_width = saved_w;

            self.y += self.style.block_spacing;
        }
    }

    fn render_block(&mut self, block: &MdBlock) {
        match block {
            MdBlock::Heading { level, inlines, .. } => self.render_heading(*level, inlines),
            MdBlock::Paragraph { inlines } => self.render_paragraph(inlines),
            MdBlock::CodeBlock { language, code } => {
                self.render_code_block(language.as_deref(), code)
            }
            MdBlock::BlockQuote { blocks } => self.render_blockquote(blocks),
            MdBlock::UnorderedList { items } => self.render_unordered_list(items),
            MdBlock::OrderedList { start, items } => self.render_ordered_list(*start, items),
            MdBlock::TaskList { items } => self.render_task_list(items),
            MdBlock::Table { headers, rows, alignments } => self.render_table(headers, rows, alignments),
            MdBlock::HorizontalRule => self.render_hr(),
            MdBlock::FootnoteDefinition { .. } => {}
        }
    }

    fn render_heading(&mut self, level: u8, inlines: &[MdInline]) {
        let idx = (level as usize).saturating_sub(1).min(5);
        let font_size = self.style.heading_sizes[idx];

        self.y += self.style.heading_spacing_above;

        let is = InlineStyle {
            color: self.style.heading_color,
            font_size,
            bold: true,
            italic: false,
            strikethrough: false,
            link: false,
            link_url: None,
        };

        let flat = self.flatten_inlines(inlines, is);
        let height = self.render_flat_spans(&flat, self.origin_x, self.max_width);
        self.y += height;

        if level <= 2 {
            self.y += 4.0;
            let line_rect = Rect::new(
                Point::new(self.origin_x, self.y),
                Size::new(self.max_width, 1.0),
            );
            self.list.push_rect(line_rect, self.style.hr_color, [0.0; 4]);
            self.y += 1.0 + 4.0;
        }
    }

    fn render_paragraph(&mut self, inlines: &[MdInline]) {
        if self.images.is_none() {
            let is = InlineStyle {
                color: self.style.text_color,
                font_size: self.style.text_size,
                bold: false,
                italic: false,
                strikethrough: false,
                link: false,
                link_url: None,
            };
            let flat = self.flatten_inlines(inlines, is);
            let height = self.render_flat_spans(&flat, self.origin_x, self.max_width);
            self.y += height;
            return;
        }

        for part in split_para_parts(inlines) {
            match part {
                ParaPart::Inlines(items) => {
                    let is = InlineStyle {
                        color: self.style.text_color,
                        font_size: self.style.text_size,
                        bold: false,
                        italic: false,
                        strikethrough: false,
                        link: false,
                        link_url: None,
                    };
                    let flat = self.flatten_inlines(&items, is);
                    let height = self.render_flat_spans(&flat, self.origin_x, self.max_width);
                    self.y += height;
                }
                ParaPart::Image { alt, url } => {
                    self.render_image_part(&alt, &url, self.max_width);
                }
            }
        }
    }

    fn render_image_part(&mut self, alt: &str, url: &str, max_w: f32) {
        let entry = self.images.and_then(|p| p.entry(url));
        match entry {
            Some(MdImageEntry {
                state: ImageLoadState::Ready,
                texture_id,
                natural_w,
                natural_h,
            }) if natural_w > 0 && natural_h > 0 => {
                let nw = natural_w as f32;
                let nh = natural_h as f32;
                let aspect = nh / nw;
                let w = nw.min(max_w);
                let h = w * aspect;
                let rect = Rect::new(Point::new(self.origin_x, self.y), Size::new(w, h));
                let uv = Rect::new(Point::new(0.0, 0.0), Size::new(1.0, 1.0));
                self.list
                    .push_image(rect, TextureId(texture_id), uv, Color::WHITE);
                self.y += h;
            }
            Some(MdImageEntry {
                state: ImageLoadState::Failed,
                ..
            }) => {
                self.draw_image_placeholder(alt, max_w, true);
            }
            _ => {
                self.draw_image_placeholder(alt, max_w, false);
            }
        }
    }

    fn draw_image_placeholder(&mut self, alt: &str, max_w: f32, error: bool) {
        let h = self.style.image_placeholder_height.max(48.0);
        let bg = if error {
            Color::from_hex("#FEE2E2")
        } else {
            self.style.image_placeholder_bg
        };
        let rect = Rect::new(Point::new(self.origin_x, self.y), Size::new(max_w, h));
        self.list.push_rect(rect, bg, [8.0; 4]);
        let label = if error {
            format!("[Не загружено: {}]", if alt.is_empty() { "image" } else { alt })
        } else {
            format!("[{}]", if alt.is_empty() { "image" } else { alt })
        };
        let text_rect = Rect::new(
            Point::new(self.origin_x, self.y + h * 0.5 - self.style.text_size * 0.5),
            Size::new(max_w, self.style.text_size + 4.0),
        );
        let color = if error {
            Color::from_hex("#B91C1C")
        } else {
            self.style.image_placeholder_color
        };
        self.list
            .push_text_centered(&label, text_rect, color, self.style.text_size);
        self.y += h;
    }

    fn render_code_block(&mut self, language: Option<&str>, code: &str) {
        let padding = self.style.code_block_padding;
        let font_size = self.style.code_font_size;
        let line_h = font_size * self.style.line_height;

        let lines: Vec<&str> = code.lines().collect();
        let lines = if lines.last().map_or(false, |l| l.is_empty()) {
            &lines[..lines.len() - 1]
        } else {
            &lines[..]
        };

        let content_h = lines.len() as f32 * line_h;
        let total_h = content_h + padding * 2.0;

        let bg_rect = Rect::new(
            Point::new(self.origin_x, self.y),
            Size::new(self.max_width, total_h),
        );
        let r = self.style.code_block_radius;
        self.list.push_rect(bg_rect, self.style.code_block_bg, [r, r, r, r]);

        let tokens = self
            .highlighter
            .as_deref()
            .map(|h| h.highlight(code, language))
            .unwrap_or_default();

        let default_color = self.style.code_block_color;
        let mut line_y = self.y + padding;
        let mut line_byte_offset: usize = 0;
        let mut tok_idx: usize = 0;

        for (line_idx, line) in lines.iter().enumerate() {
            let line_start = line_byte_offset;
            let line_end = line_start + line.len();

            if line_idx > 0 {
                self.bump_line();
            }

            if tokens.is_empty() {
                let text_rect = Rect::new(
                    Point::new(self.origin_x + padding, line_y),
                    Size::new(self.max_width - padding * 2.0, font_size + 2.0),
                );
                self.list.push_text(line, text_rect, default_color, font_size);
            } else {
                let mut x = self.origin_x + padding;
                let mut cursor = line_start;
                while cursor < line_end {
                    while tok_idx < tokens.len() && tokens[tok_idx].range.end <= cursor {
                        tok_idx += 1;
                    }
                    if tok_idx >= tokens.len() {
                        break;
                    }
                    let tok = &tokens[tok_idx];
                    let span_start = tok.range.start.max(cursor);
                    let span_end = tok.range.end.min(line_end);
                    if span_start > cursor {
                        let frag = &code[cursor..span_start];
                        let w = self.text_width(frag, font_size);
                        let rect = Rect::new(
                            Point::new(x, line_y),
                            Size::new(w.max(1.0), font_size + 2.0),
                        );
                        self.list.push_text(frag, rect, default_color, font_size);
                        x += w;
                        cursor = span_start;
                    }
                    if span_end > span_start {
                        let frag = &code[span_start..span_end];
                        let w = self.text_width(frag, font_size);
                        let rect = Rect::new(
                            Point::new(x, line_y),
                            Size::new(w.max(1.0), font_size + 2.0),
                        );
                        self.list.push_text(frag, rect, tok.color, font_size);
                        x += w;
                        cursor = span_end;
                    } else {
                        tok_idx += 1;
                    }
                }
                if cursor < line_end {
                    let frag = &code[cursor..line_end];
                    let w = self.text_width(frag, font_size);
                    let rect = Rect::new(
                        Point::new(x, line_y),
                        Size::new(w.max(1.0), font_size + 2.0),
                    );
                    self.list.push_text(frag, rect, default_color, font_size);
                }
            }

            let line_w = self.text_width(line, font_size);
            let sel_rect = Rect::new(
                Point::new(self.origin_x + padding, line_y),
                Size::new(line_w, line_h),
            );
            self.emit_selectable(sel_rect, line, font_size, None, false, None);

            line_y += line_h;
            line_byte_offset = line_end + 1;
        }

        if let Some(hotspots) = self.copy_hotspots.as_deref_mut() {
            hotspots.push((bg_rect, code.to_string()));
        }

        self.y += total_h;
    }

    fn render_blockquote(&mut self, blocks: &[MdBlock]) {
        let pad_left = self.style.quote_padding_left;
        let border_w = self.style.quote_border_width;
        let pad_v = self.style.quote_padding_v;

        let content_h = measure_blocks(
            blocks,
            self.style,
            self.max_width - pad_left - border_w - 8.0,
            self.text_measure.as_deref(),
            self.images,
        );
        let total_h = content_h + pad_v * 2.0;

        let r = self.style.quote_radius;
        let bg_rect = Rect::new(
            Point::new(self.origin_x, self.y),
            Size::new(self.max_width, total_h),
        );
        self.list.push_rect(bg_rect, self.style.quote_bg, [r, r, r, r]);

        let border_rect = Rect::new(
            Point::new(self.origin_x, self.y),
            Size::new(border_w, total_h),
        );
        self.list.push_rect(border_rect, self.style.quote_border_color, [r, 0.0, 0.0, r]);

        let saved_x = self.origin_x;
        let saved_w = self.max_width;
        let saved_y = self.y;

        self.origin_x += border_w + pad_left;
        self.max_width -= border_w + pad_left + 8.0;
        self.y += pad_v;

        self.render_blocks_with_color(blocks, self.style.quote_text_color);

        self.origin_x = saved_x;
        self.max_width = saved_w;
        self.y = saved_y + total_h;
    }

    fn render_blocks_with_color(&mut self, blocks: &[MdBlock], color: Color) {
        for (i, block) in blocks.iter().enumerate() {
            if i > 0 {
                self.y += self.style.block_spacing;
            }
            match block {
                MdBlock::Paragraph { inlines } => {
                    let is = InlineStyle {
                        color,
                        font_size: self.style.text_size,
                        bold: false,
                        italic: false,
                        strikethrough: false,
                        link: false,
                        link_url: None,
                    };
                    let flat = self.flatten_inlines(inlines, is);
                    let height = self.render_flat_spans(&flat, self.origin_x, self.max_width);
                    self.y += height;
                }
                other => self.render_block(other),
            }
        }
    }

    fn render_unordered_list(&mut self, items: &[MdListItem]) {
        let indent = self.style.list_indent;
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.y += 4.0;
                self.bump_line();
            }
            let bullet_y = self.y + self.style.text_size * self.style.line_height / 2.0 - self.style.bullet_radius;
            let bullet_x = self.origin_x + indent / 2.0 - self.style.bullet_radius;
            let br = self.style.bullet_radius;
            let bullet_rect = Rect::new(
                Point::new(bullet_x, bullet_y),
                Size::new(br * 2.0, br * 2.0),
            );
            self.list.push_rect(bullet_rect, self.style.bullet_color, [br, br, br, br]);

            let saved_x = self.origin_x;
            let saved_w = self.max_width;
            self.origin_x += indent;
            self.max_width -= indent;
            self.render_list_item_blocks(&item.blocks);
            self.origin_x = saved_x;
            self.max_width = saved_w;
        }
    }

    fn render_ordered_list(&mut self, start: u64, items: &[MdListItem]) {
        let indent = self.style.list_indent;
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.y += 4.0;
                self.bump_line();
            }
            let num = format!("{}.", start + i as u64);
            let num_w = self.text_width(&num, self.style.text_size);
            let num_x = self.origin_x + indent - num_w - 4.0;
            let num_rect = Rect::new(
                Point::new(num_x, self.y),
                Size::new(num_w, self.style.text_size + 2.0),
            );
            self.list.push_text(&num, num_rect, self.style.bullet_color, self.style.text_size);

            let saved_x = self.origin_x;
            let saved_w = self.max_width;
            self.origin_x += indent;
            self.max_width -= indent;
            self.render_list_item_blocks(&item.blocks);
            self.origin_x = saved_x;
            self.max_width = saved_w;
        }
    }

    fn render_list_item_blocks(&mut self, blocks: &[MdBlock]) {
        for (i, block) in blocks.iter().enumerate() {
            if i > 0 {
                self.y += self.style.block_spacing * 0.5;
            }
            self.render_block(block);
        }
    }

    fn render_task_list(&mut self, items: &[MdTaskItem]) {
        let cb_size = self.style.checkbox_size;
        let indent = cb_size + 8.0;

        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.y += 6.0;
            }

            let cb_y = self.y + (self.style.text_size * self.style.line_height - cb_size) / 2.0;
            let cb_rect = Rect::new(
                Point::new(self.origin_x, cb_y),
                Size::new(cb_size, cb_size),
            );
            let r = self.style.checkbox_radius;

            if item.checked {
                self.list.push_rect(cb_rect, self.style.checkbox_color, [r, r, r, r]);
                let check_rect = Rect::new(
                    Point::new(self.origin_x + 1.0, cb_y - 1.0),
                    Size::new(cb_size, cb_size + 2.0),
                );
                self.list.push_text_centered(
                    "\u{2713}",
                    check_rect,
                    self.style.checkbox_check_color,
                    cb_size * 0.75,
                );
            } else {
                self.list.push_rect_bordered(
                    cb_rect,
                    Color::TRANSPARENT,
                    [r, r, r, r],
                    Border::new(1.5, self.style.table_border_color),
                );
            }

            let is = InlineStyle {
                color: if item.checked {
                    Color::from_hex("#9CA3AF")
                } else {
                    self.style.text_color
                },
                font_size: self.style.text_size,
                bold: false,
                italic: false,
                strikethrough: item.checked,
                link: false,
                link_url: None,
            };
            let flat = self.flatten_inlines(&item.inlines, is);
            let saved_x = self.origin_x;
            let saved_w = self.max_width;
            self.origin_x += indent;
            self.max_width -= indent;
            let height = self.render_flat_spans(&flat, self.origin_x, self.max_width);
            self.y += height;
            self.origin_x = saved_x;
            self.max_width = saved_w;
        }
    }

    fn render_table(
        &mut self,
        headers: &[MdTableCell],
        rows: &[Vec<MdTableCell>],
        alignments: &[MdAlign],
    ) {
        let num_cols = headers.len().max(1);
        let pad_h = self.style.table_cell_padding_h;
        let pad_v = self.style.table_cell_padding_v;
        let font_size = self.style.text_size;
        let row_h = font_size * self.style.line_height + pad_v * 2.0;
        let border_color = self.style.table_border_color;

        let mut col_widths = vec![60.0f32; num_cols];
        for (j, header) in headers.iter().enumerate() {
            let w = measure_inlines_width(&header.inlines, font_size) + pad_h * 2.0;
            col_widths[j] = col_widths[j].max(w);
        }
        for row in rows {
            for (j, cell) in row.iter().enumerate() {
                if j < num_cols {
                    let w = measure_inlines_width(&cell.inlines, font_size) + pad_h * 2.0;
                    col_widths[j] = col_widths[j].max(w);
                }
            }
        }

        let total_w: f32 = col_widths.iter().sum();
        let available = self.max_width;
        if total_w > available && total_w > 0.0 {
            let scale = available / total_w;
            for w in &mut col_widths {
                *w *= scale;
            }
        } else if total_w < available {
            let extra = (available - total_w) / num_cols as f32;
            for w in &mut col_widths {
                *w += extra;
            }
        }

        let table_w: f32 = col_widths.iter().sum();

        {
            let header_rect = Rect::new(
                Point::new(self.origin_x, self.y),
                Size::new(table_w, row_h),
            );
            self.list.push_rect(header_rect, self.style.table_header_bg, [4.0, 4.0, 0.0, 0.0]);

            let mut cx = self.origin_x;
            for (j, header) in headers.iter().enumerate() {
                let cw = col_widths.get(j).copied().unwrap_or(60.0);
                let align = alignments.get(j).copied().unwrap_or_default();
                self.render_table_cell(
                    &header.inlines,
                    cx, self.y, cw, row_h, pad_h, pad_v,
                    self.style.table_header_color,
                    font_size,
                    align,
                    true,
                );
                cx += cw;
            }

            let border_rect = Rect::new(
                Point::new(self.origin_x, self.y + row_h - 1.0),
                Size::new(table_w, 1.0),
            );
            self.list.push_rect(border_rect, border_color, [0.0; 4]);

            self.y += row_h;
        }

        for (ri, row) in rows.iter().enumerate() {
            if ri % 2 == 1 {
                let stripe_rect = Rect::new(
                    Point::new(self.origin_x, self.y),
                    Size::new(table_w, row_h),
                );
                let radius = if ri == rows.len() - 1 { [0.0, 0.0, 4.0, 4.0] } else { [0.0; 4] };
                self.list.push_rect(stripe_rect, self.style.table_stripe_bg, radius);
            }

            let mut cx = self.origin_x;
            for (j, cell) in row.iter().enumerate() {
                let cw = col_widths.get(j).copied().unwrap_or(60.0);
                let align = alignments.get(j).copied().unwrap_or_default();
                self.render_table_cell(
                    &cell.inlines,
                    cx, self.y, cw, row_h, pad_h, pad_v,
                    self.style.text_color,
                    font_size,
                    align,
                    false,
                );
                cx += cw;
            }

            let border_rect = Rect::new(
                Point::new(self.origin_x, self.y + row_h - 0.5),
                Size::new(table_w, 0.5),
            );
            self.list.push_rect(border_rect, border_color, [0.0; 4]);

            self.y += row_h;
        }
    }

    fn render_table_cell(
        &mut self,
        inlines: &[MdInline],
        x: f32, y: f32, width: f32, _height: f32,
        pad_h: f32, pad_v: f32,
        color: Color, font_size: f32,
        align: MdAlign,
        bold: bool,
    ) {
        let is = InlineStyle {
            color,
            font_size,
            bold,
            italic: false,
            strikethrough: false,
            link: false,
            link_url: None,
        };
        let flat = self.flatten_inlines(inlines, is);
        let content_w: f32 = flat.iter().map(|s| span_width(s)).sum();

        let text_x = match align {
            MdAlign::Left => x + pad_h,
            MdAlign::Center => x + (width - content_w) / 2.0,
            MdAlign::Right => x + width - pad_h - content_w,
        };
        let text_y = y + pad_v;

        self.render_flat_spans_single_line(&flat, text_x, text_y, width - pad_h * 2.0);
    }

    fn render_hr(&mut self) {
        let rect = Rect::new(
            Point::new(self.origin_x, self.y),
            Size::new(self.max_width, self.style.hr_thickness),
        );
        self.list.push_rect(rect, self.style.hr_color, [0.0; 4]);
        self.y += self.style.hr_thickness;
    }

    fn flatten_inlines(&self, inlines: &[MdInline], style: InlineStyle) -> Vec<FlatSpan> {
        let mut out = Vec::new();
        flatten_recursive(inlines, &style, self.style, &mut out);
        out
    }

    fn render_flat_spans(&mut self, spans: &[FlatSpan], x_start: f32, max_w: f32) -> f32 {
        if spans.is_empty() {
            return self.style.text_size * self.style.line_height;
        }

        let line_h = spans
            .iter()
            .map(|s| s.font_size)
            .fold(self.style.text_size, f32::max)
            * self.style.line_height;
        let mut x_rel = 0.0f32;
        let mut lines_height = 0.0f32;
        let start_y = self.y;

        for span in spans {
            let sw = if span.bold {
                self.bold_text_width(&span.text, span.font_size)
            } else {
                self.text_width(&span.text, span.font_size)
            };

            if x_rel + sw > max_w && !span.text.is_empty() {
                let words: Vec<&str> = span.text.split_inclusive(' ').collect();
                if words.len() > 1 {
                    for word in &words {
                        let ww = if span.bold {
                            self.bold_text_width(word, span.font_size)
                        } else {
                            self.text_width(word, span.font_size)
                        };

                        if x_rel + ww > max_w && x_rel > 0.0 {
                            self.y += line_h;
                            lines_height += line_h;
                            x_rel = 0.0;
                            self.bump_line();
                        }

                        self.draw_flat_span_text(word, span, x_start + x_rel);
                        x_rel += ww;
                    }
                    continue;
                }

                if x_rel > 0.0 {
                    self.y += line_h;
                    lines_height += line_h;
                    x_rel = 0.0;
                    self.bump_line();
                }
            }

            if span.text.is_empty() {
                continue;
            }

            self.draw_flat_span_text(&span.text, span, x_start + x_rel);
            x_rel += sw;
        }

        lines_height += line_h;
        self.y = start_y;
        lines_height
    }

    fn render_flat_spans_single_line(&mut self, spans: &[FlatSpan], mut x: f32, y: f32, _max_w: f32) {
        let saved_y = self.y;
        self.y = y;
        for span in spans {
            if span.text.is_empty() { continue; }
            self.draw_flat_span_text(&span.text, span, x);
            x += span_width(span);
        }
        self.y = saved_y;
    }

    fn draw_flat_span_text(&mut self, text: &str, span: &FlatSpan, x: f32) {
        let sw = if span.bold {
            self.bold_text_width(text, span.font_size)
        } else {
            self.text_width(text, span.font_size)
        };
        let text_rect = Rect::new(
            Point::new(x, self.y),
            Size::new(sw, 0.0),
        );

        if span.is_code {
            if let Some(bg) = span.code_bg {
                let bg_rect = Rect::new(
                    Point::new(x - self.style.code_padding_h, self.y - 1.0),
                    Size::new(sw + self.style.code_padding_h * 2.0, span.font_size + 4.0),
                );
                let r = self.style.code_radius;
                self.list.push_rect(bg_rect, bg, [r, r, r, r]);
            }
        }

        let font_weight: u16 = if span.bold { 700 } else { 400 };
        self.list.push_text_aligned(text, text_rect, span.color, span.font_size, TextAlign::DEFAULT, TextDecoration::None, font_weight);

        let row_h = span.font_size * self.style.line_height;
        let sel_rect = Rect::new(Point::new(x, self.y), Size::new(sw, row_h));
        self.emit_selectable(sel_rect, text, span.font_size, None, span.bold, span.link.clone());

        if span.underline {
            let underline_rect = Rect::new(
                Point::new(x, self.y + span.font_size + 1.0),
                Size::new(sw, 1.0),
            );
            self.list.push_rect(underline_rect, span.color, [0.0; 4]);
        }

        if span.strikethrough {
            let strike_y = self.y + span.font_size * 0.55;
            let strike_rect = Rect::new(
                Point::new(x, strike_y),
                Size::new(sw, 1.0),
            );
            let strike_color = self.style.strikethrough_color.unwrap_or(span.color);
            self.list.push_rect(strike_rect, strike_color, [0.0; 4]);
        }
    }
}

enum ParaPart {
    Inlines(Vec<MdInline>),
    Image { alt: String, url: String },
}

fn split_para_parts(inlines: &[MdInline]) -> Vec<ParaPart> {
    let mut out: Vec<ParaPart> = Vec::new();
    let mut buf: Vec<MdInline> = Vec::new();
    for inline in inlines {
        match inline {
            MdInline::Image { alt, url } => {
                if !buf.is_empty() {
                    out.push(ParaPart::Inlines(std::mem::take(&mut buf)));
                }
                out.push(ParaPart::Image {
                    alt: alt.clone(),
                    url: url.clone(),
                });
            }
            other => buf.push(other.clone()),
        }
    }
    if !buf.is_empty() {
        out.push(ParaPart::Inlines(buf));
    }
    out
}

fn flatten_recursive(
    inlines: &[MdInline],
    style: &InlineStyle,
    md_style: &MdStyle,
    out: &mut Vec<FlatSpan>,
) {
    for inline in inlines {
        match inline {
            MdInline::Text(t) => {
                out.push(FlatSpan {
                    text: t.clone(),
                    color: style.color,
                    font_size: style.font_size,
                    bold: style.bold,
                    underline: style.link,
                    strikethrough: style.strikethrough,
                    is_code: false,
                    code_bg: None,
                    link: style.link_url.clone(),
                });
            }
            MdInline::Bold(children) => {
                let is = InlineStyle { bold: true, ..style.clone() };
                flatten_recursive(children, &is, md_style, out);
            }
            MdInline::Italic(children) => {
                let is = InlineStyle { italic: true, ..style.clone() };
                flatten_recursive(children, &is, md_style, out);
            }
            MdInline::Strikethrough(children) => {
                let is = InlineStyle { strikethrough: true, ..style.clone() };
                flatten_recursive(children, &is, md_style, out);
            }
            MdInline::Code(text) => {
                out.push(FlatSpan {
                    text: text.clone(),
                    color: md_style.code_color,
                    font_size: md_style.code_font_size,
                    bold: false,
                    underline: false,
                    strikethrough: false,
                    is_code: true,
                    code_bg: Some(md_style.code_bg),
                    link: style.link_url.clone(),
                });
            }
            MdInline::Link { children, url } => {
                let is = InlineStyle {
                    color: md_style.link_color,
                    link: true,
                    link_url: Some(url.clone()),
                    ..style.clone()
                };
                flatten_recursive(children, &is, md_style, out);
            }
            MdInline::Image { alt, url: _ } => {
                out.push(FlatSpan {
                    text: format!("[Image: {}]", if alt.is_empty() { "image" } else { alt }),
                    color: md_style.image_placeholder_color,
                    font_size: style.font_size,
                    bold: false,
                    underline: false,
                    strikethrough: false,
                    is_code: false,
                    code_bg: None,
                    link: None,
                });
            }
            MdInline::SoftBreak => {
                out.push(FlatSpan {
                    text: " ".to_string(),
                    color: style.color,
                    font_size: style.font_size,
                    bold: style.bold,
                    underline: false,
                    strikethrough: false,
                    is_code: false,
                    code_bg: None,
                    link: style.link_url.clone(),
                });
            }
            MdInline::HardBreak => {
                out.push(FlatSpan {
                    text: "\n".to_string(),
                    color: style.color,
                    font_size: style.font_size,
                    bold: style.bold,
                    underline: false,
                    strikethrough: false,
                    is_code: false,
                    code_bg: None,
                    link: None,
                });
            }
            MdInline::FootnoteRef(label) => {
                let small = (style.font_size * md_style.footnote_ref_scale).max(8.0);
                out.push(FlatSpan {
                    text: format!("^{label}"),
                    color: md_style.footnote_color,
                    font_size: small,
                    bold: false,
                    underline: true,
                    strikethrough: false,
                    is_code: false,
                    code_bg: None,
                    link: None,
                });
            }
        }
    }
}

fn span_width(span: &FlatSpan) -> f32 {
    if span.bold {
        bold_text_width(&span.text, span.font_size)
    } else {
        text_width(&span.text, span.font_size)
    }
}

fn measure_inlines_width(inlines: &[MdInline], font_size: f32) -> f32 {
    measure_inlines_width_tm(inlines, font_size, None)
}

fn measure_inlines_width_tm(
    inlines: &[MdInline],
    font_size: f32,
    tm: Option<&dyn TextMeasure>,
) -> f32 {
    let tw = |t: &str| -> f32 {
        match tm {
            Some(tm) => tm.measure_text_width(t, font_size, t.chars().count()),
            None => text_width(t, font_size),
        }
    };
    let tw_bold = |t: &str| -> f32 {
        match tm {
            Some(tm) => tm.measure_text_width_styled(t, font_size, t.chars().count(), true, None),
            None => bold_text_width(t, font_size),
        }
    };
    let mut w = 0.0f32;
    for inline in inlines {
        w += match inline {
            MdInline::Text(t) => tw(t),
            MdInline::Bold(c) => {
                if c.len() == 1 {
                    if let MdInline::Text(bt) = &c[0] {
                        tw_bold(bt)
                    } else {
                        measure_inlines_width_tm(c, font_size, tm) * 1.05
                    }
                } else {
                    measure_inlines_width_tm(c, font_size, tm) * 1.05
                }
            }
            MdInline::Italic(c) | MdInline::Strikethrough(c) => measure_inlines_width_tm(c, font_size, tm),
            MdInline::Code(t) => tw(t),
            MdInline::Link { children, .. } => measure_inlines_width_tm(children, font_size, tm),
            MdInline::Image { alt, .. } => tw(alt) + tw("[Image: ]"),
            MdInline::SoftBreak => tw(" "),
            MdInline::HardBreak => 0.0,
            MdInline::FootnoteRef(label) => {
                let small = font_size * 0.72;
                tw(&format!("^{label}")) * (small / font_size)
            }
        };
    }
    w
}

pub fn measure_natural_width(
    blocks: &[MdBlock],
    style: &MdStyle,
    tm: Option<&dyn TextMeasure>,
) -> f32 {
    let mut w = 0.0f32;
    for block in blocks {
        w = w.max(natural_width_of_block(block, style, tm));
    }
    w
}

fn natural_width_of_block(
    block: &MdBlock,
    style: &MdStyle,
    tm: Option<&dyn TextMeasure>,
) -> f32 {
    match block {
        MdBlock::Heading { level, inlines, .. } => {
            let idx = (*level as usize).saturating_sub(1).min(5);
            measure_inlines_width_tm(inlines, style.heading_sizes[idx], tm)
        }
        MdBlock::Paragraph { inlines } => {
            measure_inlines_width_tm(inlines, style.text_size, tm)
        }
        MdBlock::CodeBlock { code, .. } => {
            let pad = style.code_block_padding * 2.0;
            let tw = |t: &str| -> f32 {
                match tm {
                    Some(tm) => tm.measure_text_width(t, style.code_font_size, t.chars().count()),
                    None => text_width(t, style.code_font_size),
                }
            };
            code.lines().map(tw).fold(0.0f32, f32::max) + pad
        }
        MdBlock::BlockQuote { blocks } => {
            let extra = style.quote_border_width + style.quote_padding_left + 8.0;
            measure_natural_width(blocks, style, tm) + extra
        }
        MdBlock::UnorderedList { items } | MdBlock::OrderedList { items, .. } => {
            items.iter()
                .map(|it| measure_natural_width(&it.blocks, style, tm))
                .fold(0.0f32, f32::max)
                + style.list_indent
        }
        MdBlock::TaskList { items } => {
            items.iter()
                .map(|it| measure_inlines_width_tm(&it.inlines, style.text_size, tm) + style.list_indent)
                .fold(0.0f32, f32::max)
        }
        MdBlock::Table { headers, rows, .. } => {
            let cell_pad = style.table_cell_padding_h * 2.0;
            let cols = headers.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
            if cols == 0 { return 0.0; }
            let mut col_w = vec![0.0f32; cols];
            for (i, cell) in headers.iter().enumerate() {
                if i < cols {
                    col_w[i] = col_w[i].max(measure_inlines_width_tm(&cell.inlines, style.text_size, tm) + cell_pad);
                }
            }
            for row in rows {
                for (i, cell) in row.iter().enumerate() {
                    if i < cols {
                        col_w[i] = col_w[i].max(measure_inlines_width_tm(&cell.inlines, style.text_size, tm) + cell_pad);
                    }
                }
            }
            col_w.iter().sum::<f32>()
        }
        MdBlock::HorizontalRule => 0.0,
        MdBlock::FootnoteDefinition { blocks, .. } => {
            measure_natural_width(blocks, style, tm)
        }
    }
}

pub fn measure_blocks(
    blocks: &[MdBlock],
    style: &MdStyle,
    max_width: f32,
    tm: Option<&dyn TextMeasure>,
    images: Option<&dyn MdImageProbe>,
) -> f32 {
    let mut y = 0.0f32;
    let mut first = true;
    let mut footnote_count: usize = 0;
    let mut footnotes_h: f32 = 0.0;
    for block in blocks.iter() {
        if let MdBlock::FootnoteDefinition { blocks: fb, .. } = block {
            footnote_count += 1;
            let prefix_w = style.text_size * 0.6 * 4.0;
            let inner_w = (max_width - prefix_w).max(40.0);
            footnotes_h += measure_blocks(fb, style, inner_w, tm, images)
                + style.block_spacing;
            continue;
        }
        if !first {
            y += style.block_spacing;
        }
        first = false;
        y += measure_block(block, style, max_width, tm, images);
    }
    if footnote_count > 0 {
        y += style.block_spacing * 2.0
            + style.hr_thickness.max(1.0)
            + style.block_spacing
            + footnotes_h;
    }
    y
}

fn measure_block(
    block: &MdBlock,
    style: &MdStyle,
    max_width: f32,
    tm: Option<&dyn TextMeasure>,
    images: Option<&dyn MdImageProbe>,
) -> f32 {
    match block {
        MdBlock::Heading { level, inlines, .. } => {
            let idx = (*level as usize).saturating_sub(1).min(5);
            let font_size = style.heading_sizes[idx];
            let content_w = measure_inlines_wrapped_height(inlines, font_size, max_width, style, tm);
            let mut h = style.heading_spacing_above + content_w;
            if *level <= 2 {
                h += 4.0 + 1.0 + 4.0;
            }
            h
        }
        MdBlock::Paragraph { inlines } => {
            if images.is_some() {
                measure_paragraph_with_images(inlines, style, max_width, tm, images)
            } else {
                measure_inlines_wrapped_height(inlines, style.text_size, max_width, style, tm)
            }
        }
        MdBlock::CodeBlock { code, .. } => {
            let lines = code.lines().count().max(1);
            let line_h = style.code_font_size * style.line_height;
            lines as f32 * line_h + style.code_block_padding * 2.0
        }
        MdBlock::BlockQuote { blocks } => {
            let inner_w = max_width - style.quote_border_width - style.quote_padding_left - 8.0;
            let content_h = measure_blocks(blocks, style, inner_w, tm, images);
            content_h + style.quote_padding_v * 2.0
        }
        MdBlock::UnorderedList { items } => {
            let inner_w = max_width - style.list_indent;
            let mut h = 0.0f32;
            for (i, item) in items.iter().enumerate() {
                if i > 0 { h += 4.0; }
                h += measure_blocks(&item.blocks, style, inner_w, tm, images);
            }
            h
        }
        MdBlock::OrderedList { items, .. } => {
            let inner_w = max_width - style.list_indent;
            let mut h = 0.0f32;
            for (i, item) in items.iter().enumerate() {
                if i > 0 { h += 4.0; }
                h += measure_blocks(&item.blocks, style, inner_w, tm, images);
            }
            h
        }
        MdBlock::TaskList { items } => {
            let line_h = style.text_size * style.line_height;
            let mut h = 0.0f32;
            for (i, _) in items.iter().enumerate() {
                if i > 0 { h += 6.0; }
                h += line_h;
            }
            h
        }
        MdBlock::Table { rows, .. } => {
            let row_h = style.text_size * style.line_height + style.table_cell_padding_v * 2.0;
            row_h * (1 + rows.len()) as f32
        }
        MdBlock::HorizontalRule => style.hr_thickness,
        MdBlock::FootnoteDefinition { blocks, .. } => {
            measure_blocks(blocks, style, max_width, tm, images)
                + style.text_size * style.line_height
        }
    }
}

fn measure_paragraph_with_images(
    inlines: &[MdInline],
    style: &MdStyle,
    max_width: f32,
    tm: Option<&dyn TextMeasure>,
    images: Option<&dyn MdImageProbe>,
) -> f32 {
    let parts = split_para_parts(inlines);
    let mut h = 0.0f32;
    for part in parts {
        match part {
            ParaPart::Inlines(items) => {
                h += measure_inlines_wrapped_height(&items, style.text_size, max_width, style, tm);
            }
            ParaPart::Image { url, .. } => {
                let entry = images.and_then(|p| p.entry(&url));
                match entry {
                    Some(MdImageEntry {
                        state: ImageLoadState::Ready,
                        natural_w,
                        natural_h,
                        ..
                    }) if natural_w > 0 && natural_h > 0 => {
                        let nw = natural_w as f32;
                        let nh = natural_h as f32;
                        let w = nw.min(max_width);
                        h += w * (nh / nw);
                    }
                    _ => h += style.image_placeholder_height.max(48.0),
                }
            }
        }
    }
    h
}

fn measure_inlines_wrapped_height(
    inlines: &[MdInline],
    font_size: f32,
    max_width: f32,
    style: &MdStyle,
    tm: Option<&dyn TextMeasure>,
) -> f32 {
    let line_h = font_size * style.line_height;
    if max_width <= 0.0 {
        return line_h;
    }
    let num_lines = simulate_wrap_lines(inlines, font_size, max_width, style, tm);
    (num_lines as f32) * line_h
}

fn simulate_wrap_lines(
    inlines: &[MdInline],
    font_size: f32,
    max_width: f32,
    md_style: &MdStyle,
    tm: Option<&dyn TextMeasure>,
) -> u32 {
    let root = InlineStyle {
        color: md_style.text_color,
        font_size,
        bold: false,
        italic: false,
        strikethrough: false,
        link: false,
        link_url: None,
    };
    let mut flat: Vec<FlatSpan> = Vec::new();
    flatten_recursive(inlines, &root, md_style, &mut flat);

    let measure_text = |text: &str, span_fs: f32, bold: bool| -> f32 {
        match tm {
            Some(tm) if bold => tm.measure_text_width_styled(text, span_fs, text.chars().count(), true, None),
            Some(tm) => tm.measure_text_width(text, span_fs, text.chars().count()),
            None => text.chars().count() as f32 * span_fs * if bold { BOLD_CHAR_W } else { CHAR_W },
        }
    };

    let mut x = 0.0f32;
    let mut lines = 1u32;

    for span in &flat {
        if span.text == "\n" {
            lines += 1;
            x = 0.0;
            continue;
        }

        let sw = measure_text(&span.text, span.font_size, span.bold);

        if x + sw > max_width && !span.text.is_empty() {
            let words: Vec<&str> = span.text.split_inclusive(' ').collect();
            if words.len() > 1 {
                for word in &words {
                    let ww = measure_text(word, span.font_size, span.bold);
                    if x + ww > max_width && x > 0.0 {
                        lines += 1;
                        x = 0.0;
                    }
                    x += ww;
                }
                continue;
            }

            if x > 0.0 {
                lines += 1;
                x = 0.0;
            }
        }

        if span.text.is_empty() {
            continue;
        }

        x += sw;
    }

    lines
}
