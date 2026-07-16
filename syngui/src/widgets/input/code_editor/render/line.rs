use super::super::syntax::LineSpans;
use super::super::theme::Theme;
use crate::core::{Color, Point, Rect, Size};
use crate::mss::{MssFields, TextAlign, TextDecoration};
use crate::render::DisplayList;
use crate::widget::context::TextMeasure;

pub fn paint_line(
    list: &mut DisplayList,
    text: &str,
    spans: &LineSpans,
    text_origin: Point,
    line_height: f32,
    font_size: f32,
    font_family: Option<&str>,
    theme: &Theme,
    mss: &MssFields,
    text_measure: Option<&dyn TextMeasure>,
) {
    let fg = theme.fg(mss);

    if spans.is_empty() {
        push_segment(
            list,
            text,
            text_origin,
            line_height,
            font_size,
            fg,
            font_family,
        );
        return;
    }

    let bytes = text.as_bytes();
    let mut cursor_byte: usize = 0;
    let mut x_offset: f32 = 0.0;

    for span in spans.iter() {
        let span_start = (span.byte_start as usize).min(bytes.len());
        let span_end = (span.byte_end as usize).min(bytes.len());
        if span_end <= span_start {
            continue;
        }

        if cursor_byte < span_start {
            let chunk = byte_substr(text, cursor_byte..span_start);
            let chunk_w =
                measure_chunk(text_measure, chunk, font_size, font_family);
            let pos = Point::new(text_origin.x + x_offset, text_origin.y);
            push_segment(
                list,
                chunk,
                pos,
                line_height,
                font_size,
                fg,
                font_family,
            );
            x_offset += chunk_w;
        }

        let chunk = byte_substr(text, span_start..span_end);
        let color = theme.token(span.class, mss);
        let chunk_w =
            measure_chunk(text_measure, chunk, font_size, font_family);
        let pos = Point::new(text_origin.x + x_offset, text_origin.y);
        push_segment(
            list,
            chunk,
            pos,
            line_height,
            font_size,
            color,
            font_family,
        );
        x_offset += chunk_w;

        cursor_byte = span_end;
    }

    if cursor_byte < bytes.len() {
        let chunk = byte_substr(text, cursor_byte..bytes.len());
        let pos = Point::new(text_origin.x + x_offset, text_origin.y);
        push_segment(
            list,
            chunk,
            pos,
            line_height,
            font_size,
            fg,
            font_family,
        );
    }
}

fn push_segment(
    list: &mut DisplayList,
    text: &str,
    pos: Point,
    line_height: f32,
    font_size: f32,
    color: Color,
    font_family: Option<&str>,
) {
    if text.is_empty() {
        return;
    }
    let rect = Rect::new(pos, Size::new(10_000.0, line_height));
    if let Some(family) = font_family {
        list.push_text_styled(
            text,
            rect,
            color,
            font_size,
            TextAlign::DEFAULT,
            TextDecoration::None,
            400,
            Some(family.to_string()),
        );
    } else {
        list.push_text_aligned(
            text,
            rect,
            color,
            font_size,
            TextAlign::DEFAULT,
            TextDecoration::None,
            400,
        );
    }
}

fn measure_chunk(
    tm: Option<&dyn TextMeasure>,
    chunk: &str,
    font_size: f32,
    font_family: Option<&str>,
) -> f32 {
    if chunk.is_empty() {
        return 0.0;
    }
    let char_count = chunk.chars().count();
    if let Some(tm) = tm {
        tm.measure_text_width_styled(chunk, font_size, char_count, false, font_family)
    } else {
        char_count as f32 * font_size * 0.6
    }
}

fn byte_substr(text: &str, range: std::ops::Range<usize>) -> &str {
    let start = clamp_to_char_boundary(text, range.start);
    let end = clamp_to_char_boundary(text, range.end);
    if start >= end {
        return "";
    }
    &text[start..end]
}

fn clamp_to_char_boundary(text: &str, byte: usize) -> usize {
    let byte = byte.min(text.len());
    if text.is_char_boundary(byte) {
        byte
    } else {
        (0..=byte).rev().find(|&b| text.is_char_boundary(b)).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_substr_handles_utf8() {
        let s = "Привет";
        assert_eq!(byte_substr(s, 0..8), "Прив");
        assert_eq!(byte_substr(s, 0..7), "При");
    }

    #[test]
    fn byte_substr_clamps_out_of_range() {
        assert_eq!(byte_substr("abc", 0..100), "abc");
        assert_eq!(byte_substr("abc", 5..10), "");
    }
}
